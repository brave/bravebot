//! `status`: HEAD's tree against the index, and the index against the working tree, read the way
//! git's own status reads them, with nothing written back: no index refresh, no lock.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use gix_glob::pattern::Case;
use gix_hash::{Kind as HashKind, ObjectId};
use gix_object::Kind;
use gix_object::bstr::ByteSlice;
use gix_object::tree::EntryKind;

use super::{Declined, Out, Repository, in_scope, join, parse_config, read_if_present};

const FLAG_ASSUME_VALID: u16 = 0x8000;
const FLAG_EXTENDED: u16 = 0x4000;
const EXTENDED_SKIP_WORKTREE: u16 = 0x4000;
const EXTENDED_INTENT_TO_ADD: u16 = 0x2000;

const MODE_FILE: u32 = 0o100644;
const MODE_EXECUTABLE: u32 = 0o100755;
const MODE_LINK: u32 = 0o120000;
const MODE_GITLINK: u32 = 0o160000;

/// Attribute names under which git converts a file's bytes on the way into the index, so that
/// comparing the file means running the conversion.
const CONVERTING: [&str; 6] = [
    "text",
    "eol",
    "crlf",
    "filter",
    "ident",
    "working-tree-encoding",
];

/// The stat data git records for an entry, each field as the index stores it: the low 32 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Stat {
    ctime: (u32, u32),
    mtime: (u32, u32),
    ino: u32,
    uid: u32,
    gid: u32,
    size: u32,
}

impl Stat {
    #[cfg(unix)]
    fn of(meta: &std::fs::Metadata) -> Stat {
        use std::os::unix::fs::MetadataExt;
        Stat {
            ctime: (meta.ctime() as u32, meta.ctime_nsec() as u32),
            mtime: (meta.mtime() as u32, meta.mtime_nsec() as u32),
            ino: meta.ino() as u32,
            uid: meta.uid(),
            gid: meta.gid(),
            size: meta.len() as u32,
        }
    }

    #[cfg(not(unix))]
    fn of(meta: &std::fs::Metadata) -> Stat {
        Stat {
            mtime: modified(meta),
            size: meta.len() as u32,
            ..Stat::default()
        }
    }

    /// Whether `now` shows no change from what the index recorded. Where this platform has no
    /// such field, it is not compared, as git for that platform leaves it zero.
    fn unchanged(&self, now: &Stat) -> bool {
        if cfg!(unix) {
            self == now
        } else {
            self.mtime == now.mtime && self.size == now.size
        }
    }
}

fn modified(meta: &std::fs::Metadata) -> (u32, u32) {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or((0, 0), |d| (d.as_secs() as u32, d.subsec_nanos()))
}

#[derive(Debug, Clone)]
struct IndexEntry {
    path: Vec<u8>,
    stat: Stat,
    mode: u32,
    id: ObjectId,
    stage: u8,
    assume_valid: bool,
    skip_worktree: bool,
    intent_to_add: bool,
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// git's offset varint, as index version 4 prefixes each path with it.
fn varint(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut used = 0;
    let mut byte = *bytes.get(used)?;
    used += 1;
    let mut value = usize::from(byte & 0x7f);
    while byte & 0x80 != 0 {
        byte = *bytes.get(used)?;
        used += 1;
        value = value
            .checked_add(1)?
            .checked_mul(128)?
            .checked_add(usize::from(byte & 0x7f))?;
    }
    Some((value, used))
}

/// Whether git itself would check out `path`: relative, with no empty, `.` or `..` part and no
/// part that is `.git` in any case. One that is not could name a file outside the working tree.
fn safe_path(path: &[u8]) -> bool {
    !path.is_empty()
        && path.split(|b| *b == b'/').all(|part| {
            !part.is_empty()
                && part != b"."
                && part != b".."
                && !part.eq_ignore_ascii_case(b".git")
                && (!cfg!(windows) || !part.contains(&b'\\'))
        })
}

/// Parse `.git/index`, versions 2 to 4, declining the split and sparse layouts and any required
/// extension this reader does not know.
fn parse_index(bytes: &[u8]) -> Result<Vec<IndexEntry>, Declined> {
    let bad = || Declined::Unreadable;
    if bytes.len() < 12 + 20 || &bytes[..4] != b"DIRC" {
        return Err(bad());
    }
    let version = be32(&bytes[4..8]);
    if !(2..=4).contains(&version) {
        return Err(Declined::Format);
    }
    let count = be32(&bytes[8..12]) as usize;
    let end = bytes.len() - 20;
    let checksum = &bytes[end..];
    if checksum.iter().any(|b| *b != 0) {
        let mut hasher = gix_hash::hasher(HashKind::Sha1);
        hasher.update(&bytes[..end]);
        let sum = hasher.try_finalize().map_err(|_| bad())?;
        if sum.as_bytes() != checksum {
            return Err(bad());
        }
    }
    let body = &bytes[..end];
    let mut at = 12;
    let mut entries = Vec::with_capacity(count.min(1 << 16));
    let mut previous: Vec<u8> = Vec::new();
    for _ in 0..count {
        let start = at;
        let fixed = body.get(at..at + 62).ok_or_else(bad)?;
        let word = |i: usize| be32(&fixed[i * 4..i * 4 + 4]);
        let stat = Stat {
            ctime: (word(0), word(1)),
            mtime: (word(2), word(3)),
            ino: word(5),
            uid: word(7),
            gid: word(8),
            size: word(9),
        };
        let mode = word(6);
        let id = ObjectId::try_from(&fixed[40..60]).map_err(|_| bad())?;
        let flags = u16::from_be_bytes([fixed[60], fixed[61]]);
        at += 62;
        let mut extended = 0u16;
        if flags & FLAG_EXTENDED != 0 {
            if version < 3 {
                return Err(bad());
            }
            let two = body.get(at..at + 2).ok_or_else(bad)?;
            extended = u16::from_be_bytes([two[0], two[1]]);
            at += 2;
        }
        let path = if version == 4 {
            let (strip, used) = varint(body.get(at..).ok_or_else(bad)?).ok_or_else(bad)?;
            at += used;
            let nul = body[at..].iter().position(|b| *b == 0).ok_or_else(bad)?;
            let keep = previous.len().checked_sub(strip).ok_or_else(bad)?;
            let mut path = previous[..keep].to_vec();
            path.extend_from_slice(&body[at..at + nul]);
            at += nul + 1;
            path
        } else {
            let rest = body.get(at..).ok_or_else(bad)?;
            let nul = rest.iter().position(|b| *b == 0).ok_or_else(bad)?;
            let path = rest[..nul].to_vec();
            at = start + ((at - start + nul + 8) & !7);
            path
        };
        if !safe_path(&path) {
            return Err(bad());
        }
        if !matches!(mode, MODE_FILE | MODE_EXECUTABLE | MODE_LINK | MODE_GITLINK) {
            return Err(bad());
        }
        previous.clone_from(&path);
        entries.push(IndexEntry {
            path,
            stat,
            mode,
            id,
            stage: ((flags >> 12) & 3) as u8,
            assume_valid: flags & FLAG_ASSUME_VALID != 0,
            skip_worktree: extended & EXTENDED_SKIP_WORKTREE != 0,
            intent_to_add: extended & EXTENDED_INTENT_TO_ADD != 0,
        });
    }
    while at < end {
        let header = body.get(at..at + 8).ok_or_else(bad)?;
        let size = be32(&header[4..8]) as usize;
        match &header[..4] {
            b"link" => return Err(Declined::SplitIndex),
            b"sdir" => return Err(Declined::SparseIndex),
            signature if signature[0].is_ascii_uppercase() => {}
            _ => return Err(Declined::Format),
        }
        at = at.checked_add(8 + size).ok_or_else(bad)?;
    }
    if at != end {
        return Err(bad());
    }
    Ok(entries)
}

/// The keys of `.git/config` status reads. None of them names a program.
struct Settings {
    filemode: bool,
    ignorecase: bool,
    symlinks: bool,
    /// Set to anything but false, which has git convert line endings of any file no attribute
    /// excludes.
    autocrlf: bool,
    quote_path: bool,
    untracked: Untracked,
}

/// `status.showUntrackedFiles`: list nothing, collapse a directory to one line, or every file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Untracked {
    No,
    Normal,
    All,
}

fn boolean(value: Option<&[u8]>) -> Option<bool> {
    let Some(value) = value else {
        return Some(true);
    };
    match String::from_utf8_lossy(value)
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" | "" => Some(false),
        other => other.parse::<i64>().ok().map(|n| n != 0),
    }
}

impl Settings {
    fn read(git_dir: &Path) -> Result<Settings, Declined> {
        let mut settings = Settings {
            filemode: cfg!(unix),
            ignorecase: false,
            symlinks: true,
            // Git for Windows sets core.autocrlf in a system file this reader does not open.
            autocrlf: cfg!(windows),
            quote_path: true,
            untracked: Untracked::Normal,
        };
        for name in ["config", "config.worktree"] {
            let Some(bytes) = read_if_present(&git_dir.join(name))? else {
                continue;
            };
            let entries = parse_config(&bytes).ok_or(Declined::Format)?;
            for entry in entries {
                if entry.subsection.is_some() {
                    continue;
                }
                let value = entry.value.as_deref();
                let flag = || boolean(value).ok_or(Declined::Format);
                match (entry.section.as_str(), entry.key.as_str()) {
                    ("core", "bare") => {
                        if flag()? {
                            return Err(Declined::Bare);
                        }
                    }
                    ("core", "excludesfile" | "attributesfile") | ("attr", "tree") => {
                        return Err(Declined::Elsewhere);
                    }
                    ("core", "filemode") => settings.filemode = cfg!(unix) && flag()?,
                    ("core", "ignorecase") => settings.ignorecase = flag()?,
                    ("core", "symlinks") => settings.symlinks = flag()?,
                    ("core", "autocrlf") => settings.autocrlf = boolean(value) != Some(false),
                    ("core", "quotepath") => settings.quote_path = flag()?,
                    ("status", "showuntrackedfiles") => {
                        settings.untracked = match value {
                            Some(b"no") => Untracked::No,
                            Some(b"normal") => Untracked::Normal,
                            Some(b"all") => Untracked::All,
                            _ if flag()? => Untracked::Normal,
                            _ => Untracked::No,
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(settings)
    }
}

/// Where an attribute may have git convert a file, so a file there whose stat data changed is
/// reported as not compared rather than hashed as it stands on disk.
#[derive(Default)]
struct Attributes {
    /// Everything: a macro or a quoted pattern this reader does not resolve names a conversion.
    all: bool,
    /// Each pattern naming a conversion, with the directory of the file it came from.
    patterns: Vec<(String, gix_glob::Pattern)>,
    read_from: HashSet<String>,
}

impl Attributes {
    /// Read the `.gitattributes` of `dir`.
    fn add(&mut self, dir: &str, bytes: &[u8]) {
        self.read_from.insert(dir.to_owned());
        self.add_patterns(dir, bytes);
    }

    fn add_patterns(&mut self, dir: &str, bytes: &[u8]) {
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        for line in bytes.lines() {
            let line = line.trim_start();
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            let (pattern, rest) = match line.find_byteset(b" \t") {
                Some(at) => (&line[..at], &line[at..]),
                None => (line, &b""[..]),
            };
            let converts = rest.fields().any(|token| {
                if token.starts_with(b"-") || token.starts_with(b"!") {
                    return false;
                }
                let name = token.split(|b| *b == b'=').next().unwrap_or_default();
                CONVERTING.iter().any(|c| c.as_bytes() == name)
            });
            let macro_or_quoted = line.starts_with(b"[attr]") || line.starts_with(b"\"");
            if macro_or_quoted {
                if converts || line.starts_with(b"\"") {
                    self.all = true;
                }
                continue;
            }
            if !converts {
                continue;
            }
            match gix_glob::Pattern::from_bytes_without_negation(pattern) {
                Some(parsed) => self.patterns.push((dir.to_owned(), parsed)),
                None => self.all = true,
            }
        }
    }

    fn converts(&self, path: &str, case: Case) -> bool {
        self.all
            || self.patterns.iter().any(|(dir, pattern)| {
                let relative = if dir.is_empty() {
                    Some(path)
                } else {
                    path.strip_prefix(dir.as_str())
                        .and_then(|rest| rest.strip_prefix('/'))
                };
                relative.is_some_and(|relative| {
                    let basename = relative.rfind('/').map(|at| at + 1);
                    pattern.matches_repo_relative_path(
                        relative.as_bytes().as_bstr(),
                        basename,
                        Some(false),
                        case,
                        gix_glob::wildmatch::Mode::NO_MATCH_SLASH_LITERAL,
                    )
                })
            })
    }
}

/// What the working tree holds at a tracked path, against its index entry.
#[derive(Debug, PartialEq, Eq)]
enum Worktree {
    Same,
    Changed(char),
    /// Comparing it means running a conversion or reading a submodule.
    NotCompared,
}

struct Walk<'a> {
    root: &'a Path,
    tracked: HashSet<Vec<u8>>,
    tracked_dirs: HashSet<Vec<u8>>,
    ignore: gix_ignore::Search,
    attributes: Attributes,
    case: Case,
    filter: Option<&'a str>,
    mode: Untracked,
    untracked: Vec<String>,
}

impl Walk<'_> {
    fn key(&self, path: &[u8]) -> Vec<u8> {
        if self.case == Case::Fold {
            path.to_ascii_lowercase()
        } else {
            path.to_vec()
        }
    }

    fn ignored(&self, path: &str, is_dir: bool) -> bool {
        self.ignore
            .pattern_matching_relative_path(path.as_bytes().as_bstr(), Some(is_dir), self.case)
            .is_some_and(|found| !found.pattern.is_negative())
    }

    /// A regular file in the working tree, as long as it is one and not a link or a directory.
    fn regular(&self, out: &mut Out<'_>, path: &str) -> Result<Option<Vec<u8>>, Declined> {
        if (out.withheld)(path) {
            out.withheld_any = true;
            return Ok(None);
        }
        let full = self.root.join(path);
        match std::fs::symlink_metadata(&full) {
            Ok(meta) if meta.file_type().is_file() => {
                let mut file = open_as_seen(&full, &meta)?;
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut bytes)
                    .map_err(|_| Declined::Unreadable)?;
                Ok(Some(bytes))
            }
            _ => Ok(None),
        }
    }

    /// List what is untracked beneath `dir`, or with `probe`, say only whether anything is,
    /// stopping at the first. `ignored_above` is set beneath an ignored directory that holds
    /// tracked files: git reads no ignore file there and ignores everything untracked.
    fn dir(
        &mut self,
        out: &mut Out<'_>,
        dir: &str,
        ignored_above: bool,
        probe: bool,
    ) -> Result<bool, Declined> {
        if out.late() {
            return Ok(false);
        }
        let attributes = join(dir, ".gitattributes");
        if !probe && (out.withheld)(&attributes) {
            // What it says is unknown, so any file may be one git converts.
            self.attributes.all = true;
        }
        if !probe && let Some(bytes) = self.regular(out, &attributes)? {
            self.attributes.add(dir, &bytes);
        }
        let ignore = join(dir, ".gitignore");
        // Without the patterns, nothing beneath is listed rather than what they would hide.
        let ignored_above = ignored_above || (out.withheld)(&ignore);
        if ignored_above {
            out.withheld_any |= (out.withheld)(&ignore);
        }
        let mut pushed = false;
        if !ignored_above && let Some(bytes) = self.regular(out, &ignore)? {
            self.ignore.add_patterns_buffer(
                &bytes,
                self.root.join(dir).join(".gitignore"),
                Some(self.root),
                gix_ignore::search::Ignore::default(),
            );
            pushed = true;
        }
        let found = self.entries(out, dir, ignored_above, probe);
        if pushed {
            self.ignore.patterns.pop();
        }
        found
    }

    fn entries(
        &mut self,
        out: &mut Out<'_>,
        dir: &str,
        ignored_above: bool,
        probe: bool,
    ) -> Result<bool, Declined> {
        let listing = match std::fs::read_dir(self.root.join(dir)) {
            Ok(listing) => listing,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(Declined::Unreadable),
        };
        let mut children = Vec::new();
        for entry in listing {
            let entry = entry.map_err(|_| Declined::Unreadable)?;
            let kind = entry.file_type().map_err(|_| Declined::Unreadable)?;
            children.push((entry.file_name(), kind));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, kind) in children {
            let Some(name) = name.to_str() else {
                out.withheld_any = true;
                continue;
            };
            if name == ".git" || (self.case == Case::Fold && name.eq_ignore_ascii_case(".git")) {
                continue;
            }
            let child = join(dir, name);
            if !in_scope(&child, self.filter) {
                continue;
            }
            if (out.withheld)(&child) {
                out.withheld_any = true;
                continue;
            }
            let key = self.key(child.as_bytes());
            if self.mode == Untracked::No {
                if kind.is_dir() && self.tracked_dirs.contains(&key) {
                    let ignored = ignored_above || self.ignored(&child, true);
                    self.dir(out, &child, ignored, false)?;
                }
                continue;
            }
            if kind.is_dir() {
                if !probe && self.tracked.contains(&key) {
                    continue;
                }
                if !probe && self.tracked_dirs.contains(&key) {
                    let ignored = ignored_above || self.ignored(&child, true);
                    self.dir(out, &child, ignored, false)?;
                    continue;
                }
                if ignored_above || self.ignored(&child, true) {
                    continue;
                }
                let nested = is_repository(&self.root.join(&child));
                let inside_filter = self.filter.is_some_and(|f| {
                    f.strip_prefix(child.as_str())
                        .is_some_and(|r| r.starts_with('/'))
                });
                if !probe && !nested && (inside_filter || self.mode == Untracked::All) {
                    self.dir(out, &child, false, false)?;
                    continue;
                }
                if nested || self.dir(out, &child, false, true)? {
                    if probe {
                        return Ok(true);
                    }
                    if within(&child, self.filter) {
                        self.untracked.push(format!("{child}/"));
                    }
                }
            } else {
                // Git lists no fifo, socket or device.
                if !kind.is_file() && !kind.is_symlink() {
                    continue;
                }
                if !probe && self.tracked.contains(&key) {
                    continue;
                }
                if ignored_above || self.ignored(&child, false) {
                    continue;
                }
                if probe {
                    return Ok(true);
                }
                if within(&child, self.filter) {
                    self.untracked.push(child);
                }
            }
            if out.late() {
                break;
            }
        }
        Ok(false)
    }
}

/// Hash a file's bytes, or a link's target, as the blob git would store for it unconverted.
fn hash_blob(
    source: &mut dyn std::io::Read,
    len: u64,
    out: &mut Out<'_>,
) -> Result<Option<ObjectId>, Declined> {
    let mut hasher = gix_hash::hasher(HashKind::Sha1);
    hasher.update(&gix_object::encode::loose_header(Kind::Blob, len));
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if out.late() {
            return Ok(None);
        }
        let read = source.read(&mut buffer).map_err(|_| Declined::Unreadable)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > len {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if total != len {
        return Ok(Some(ObjectId::null(HashKind::Sha1)));
    }
    hasher
        .try_finalize()
        .map(Some)
        .map_err(|_| Declined::Unreadable)
}

struct Compare<'a> {
    root: &'a Path,
    settings: &'a Settings,
    attributes: &'a Attributes,
    case: Case,
    index_time: (u32, u32),
    real_dirs: HashSet<String>,
}

impl Compare<'_> {
    /// Whether each directory above `path` is a directory, not a link: git treats a file beneath a
    /// link as gone, and following one could read outside the working tree.
    fn leading_dirs_real(&mut self, path: &str) -> Result<bool, Declined> {
        let mut at = 0;
        while let Some(slash) = path[at..].find('/') {
            let dir = &path[..at + slash];
            at += slash + 1;
            if self.real_dirs.contains(dir) {
                continue;
            }
            match std::fs::symlink_metadata(self.root.join(dir)) {
                Ok(meta) if meta.file_type().is_dir() => {
                    self.real_dirs.insert(dir.to_owned());
                }
                Ok(_) => return Ok(false),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    return Ok(false);
                }
                Err(_) => return Err(Declined::Unreadable),
            }
        }
        Ok(true)
    }

    /// Whether anything stands at `path`, reached through real directories.
    fn present(&mut self, path: &str) -> Result<bool, Declined> {
        if !self.leading_dirs_real(path)? {
            return Ok(false);
        }
        match std::fs::symlink_metadata(self.root.join(path)) {
            Ok(_) => Ok(true),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(false)
            }
            Err(_) => Err(Declined::Unreadable),
        }
    }

    fn worktree(
        &mut self,
        out: &mut Out<'_>,
        path: &str,
        entry: &IndexEntry,
    ) -> Result<Option<Worktree>, Declined> {
        if entry.skip_worktree || entry.assume_valid {
            return Ok(Some(Worktree::Same));
        }
        if !self.leading_dirs_real(path)? {
            return Ok(Some(Worktree::Changed('D')));
        }
        let full = self.root.join(path);
        let meta = match std::fs::symlink_metadata(&full) {
            Ok(meta) => meta,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(Some(Worktree::Changed('D')));
            }
            Err(_) => return Err(Declined::Unreadable),
        };
        let kind = meta.file_type();
        if entry.mode == MODE_GITLINK {
            return Ok(Some(if kind.is_dir() {
                Worktree::NotCompared
            } else {
                Worktree::Changed('T')
            }));
        }
        if kind.is_symlink() {
            if entry.mode != MODE_LINK {
                return Ok(Some(Worktree::Changed('T')));
            }
            let target = std::fs::read_link(&full).map_err(|_| Declined::Unreadable)?;
            let target = target.as_os_str().as_encoded_bytes().to_vec();
            let len = target.len() as u64;
            return Ok(hash_blob(&mut target.as_slice(), len, out)?.map(|id| {
                if id == entry.id {
                    Worktree::Same
                } else {
                    Worktree::Changed('M')
                }
            }));
        }
        if !kind.is_file() {
            return Ok(Some(Worktree::Changed('D')));
        }
        if entry.mode == MODE_LINK && self.settings.symlinks {
            return Ok(Some(Worktree::Changed('T')));
        }
        if self.settings.filemode
            && entry.mode != MODE_LINK
            && executable(&meta) != (entry.mode == MODE_EXECUTABLE)
        {
            return Ok(Some(Worktree::Changed('M')));
        }
        let now = Stat::of(&meta);
        let racy = entry.stat.mtime >= self.index_time;
        // Git zeroes the recorded size of an entry it smudged as racy, so the stat says nothing.
        let smudged = entry.stat.size == 0 && entry.id != ObjectId::empty_blob(HashKind::Sha1);
        if entry.stat.unchanged(&now) && !racy && !smudged {
            return Ok(Some(Worktree::Same));
        }
        // As git does, a size other than the recorded one is a change without reading the file,
        // unless the recorded size is zero, which git writes for an entry it could not vouch for.
        if entry.stat.size != now.size && entry.stat.size != 0 {
            return Ok(Some(Worktree::Changed('M')));
        }
        if self.settings.autocrlf || self.attributes.converts(path, self.case) {
            return Ok(Some(Worktree::NotCompared));
        }
        let mut file = open_as_seen(&full, &meta)?;
        Ok(hash_blob(&mut file, meta.len(), out)?.map(|id| {
            if id == entry.id {
                Worktree::Same
            } else {
                Worktree::Changed('M')
            }
        }))
    }
}

#[cfg(unix)]
fn executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
fn executable(_meta: &std::fs::Metadata) -> bool {
    false
}

fn tree_mode(kind: EntryKind) -> u32 {
    match kind {
        EntryKind::BlobExecutable => MODE_EXECUTABLE,
        EntryKind::Link => MODE_LINK,
        EntryKind::Commit => MODE_GITLINK,
        EntryKind::Blob | EntryKind::Tree => MODE_FILE,
    }
}

/// HEAD's tree as a flat map from each file's path to its mode and blob, descending only where the
/// filter reaches.
fn flatten(
    repo: &Repository,
    out: &mut Out<'_>,
    tree: ObjectId,
    prefix: &str,
    filter: Option<&str>,
    files: &mut BTreeMap<Vec<u8>, (u32, ObjectId)>,
) -> Result<(), Declined> {
    if out.late() {
        return Ok(());
    }
    for entry in repo.entries(Some(tree))?.into_values() {
        let Some(name) = entry.name else {
            out.withheld_any = true;
            continue;
        };
        let path = join(prefix, &name);
        if !in_scope(&path, filter) {
            continue;
        }
        if entry.kind == EntryKind::Tree {
            flatten(repo, out, entry.id, &path, filter, files)?;
        } else {
            files.insert(path.into_bytes(), (tree_mode(entry.kind), entry.id));
        }
    }
    Ok(())
}

/// Which side of a merge holds a path, as git's short status codes a conflict.
fn conflict(stages: u8) -> &'static str {
    match stages {
        0b001 => "DD",
        0b010 => "AU",
        0b011 => "UD",
        0b100 => "UA",
        0b101 => "DU",
        0b110 => "AA",
        _ => "UU",
    }
}

/// A path as git's short status writes it: in double quotes, with C escapes, where it holds a
/// space, a quote, a backslash, a control byte, or with `core.quotePath` on, a byte past ASCII.
fn quoted(path: &str, fully: bool) -> Cow<'_, str> {
    let escaped = |b: u8| b == b'"' || b == b'\\' || b < 0x20 || b == 0x7f || (fully && b >= 0x80);
    if !path.bytes().any(|b| b == b' ' || escaped(b)) {
        return Cow::Borrowed(path);
    }
    let mut text = String::from("\"");
    let mut plain = Vec::new();
    for b in path.bytes() {
        if !escaped(b) {
            plain.push(b);
            continue;
        }
        text.push_str(&String::from_utf8_lossy(&std::mem::take(&mut plain)));
        let named = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0x07 => "\\a",
            0x08 => "\\b",
            b'\t' => "\\t",
            b'\n' => "\\n",
            0x0b => "\\v",
            0x0c => "\\f",
            b'\r' => "\\r",
            _ => {
                let _ = write!(text, "\\{b:03o}");
                continue;
            }
        };
        text.push_str(named);
    }
    text.push_str(&String::from_utf8_lossy(&plain));
    text.push('"');
    Cow::Owned(text)
}

/// Whether `path` is the filter or beneath it: what git's pathspec lists, where [`in_scope`] also
/// admits the directories above the filter so a walk can reach it.
fn within(path: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|filter| {
        path == filter
            || path
                .strip_prefix(filter)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether `dir` holds a repository of its own: a `.git` file pointing at one, or a `.git`
/// directory with a HEAD.
fn is_repository(dir: &Path) -> bool {
    let git = dir.join(".git");
    match std::fs::symlink_metadata(&git) {
        Ok(meta) if meta.file_type().is_file() => true,
        Ok(meta) if meta.file_type().is_dir() => {
            std::fs::symlink_metadata(git.join("HEAD")).is_ok()
        }
        _ => false,
    }
}

/// Open the file `seen` describes, declining if what opens is another file: one swapped for a
/// link since it was looked at would otherwise be read wherever the link points.
fn open_as_seen(full: &Path, seen: &std::fs::Metadata) -> Result<std::fs::File, Declined> {
    let file = std::fs::File::open(full).map_err(|_| Declined::Unreadable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata().map_err(|_| Declined::Unreadable)?;
        if (opened.dev(), opened.ino()) != (seen.dev(), seen.ino()) {
            return Err(Declined::Unreadable);
        }
    }
    #[cfg(not(unix))]
    let _ = seen;
    Ok(file)
}

#[derive(Default)]
struct Tracked {
    zero: Option<IndexEntry>,
    stages: u8,
}

/// Answer `status` for the repository at `repo`, whose working tree is the directory holding its
/// `.git`.
pub(super) fn answer(
    repo: &Repository,
    out: &mut Out<'_>,
    filter: Option<&str>,
) -> Result<(), Declined> {
    let git_dir = repo.git_dir.as_path();
    let root = git_dir.parent().ok_or(Declined::NoRepository)?;
    let settings = Settings::read(git_dir)?;
    let case = if settings.ignorecase {
        Case::Fold
    } else {
        Case::Sensitive
    };

    let index_file = git_dir.join("index");
    let (entries, index_time) = match std::fs::symlink_metadata(&index_file) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(Declined::Linked),
        Ok(meta) => {
            let bytes = std::fs::read(&index_file).map_err(|_| Declined::Unreadable)?;
            (parse_index(&bytes)?, modified(&meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), (0, 0)),
        Err(_) => return Err(Declined::Unreadable),
    };

    let mut head = BTreeMap::new();
    match repo.head() {
        Ok(commit) => {
            let tree = repo.info(&commit)?.tree;
            flatten(repo, out, tree, "", filter, &mut head)?;
        }
        Err(Declined::NoCommits) => {}
        Err(other) => return Err(other),
    }

    let mut tracked: BTreeMap<Vec<u8>, Tracked> = BTreeMap::new();
    for entry in entries {
        let slot = tracked.entry(entry.path.clone()).or_default();
        if entry.stage == 0 {
            slot.zero = Some(entry);
        } else {
            slot.stages |= 1 << (entry.stage - 1);
        }
    }

    let mut walk = Walk {
        root,
        tracked: HashSet::new(),
        tracked_dirs: HashSet::new(),
        ignore: gix_ignore::Search::default(),
        attributes: Attributes::default(),
        case,
        filter,
        mode: settings.untracked,
        untracked: Vec::new(),
    };
    for path in tracked.keys() {
        walk.tracked.insert(walk.key(path));
        let mut at = 0;
        while let Some(slash) = path[at..].iter().position(|b| *b == b'/') {
            walk.tracked_dirs.insert(walk.key(&path[..at + slash]));
            at += slash + 1;
        }
    }
    if let Some(bytes) = read_if_present(&git_dir.join("info").join("exclude"))? {
        walk.ignore.add_patterns_buffer(
            &bytes,
            git_dir.join("info").join("exclude"),
            None,
            gix_ignore::search::Ignore::default(),
        );
    }
    if let Some(bytes) = read_if_present(&git_dir.join("info").join("attributes"))? {
        walk.attributes.add_patterns("", &bytes);
    }
    walk.dir(out, "", false, false)?;

    // A tracked attributes file gone from the working tree still applies, from the index.
    for (path, slot) in &tracked {
        let Some(entry) = &slot.zero else { continue };
        let Ok(path) = std::str::from_utf8(path) else {
            continue;
        };
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        if name == ".gitattributes"
            && (dir.is_empty() || in_scope(dir, filter))
            && !walk.attributes.read_from.contains(dir)
            && !(out.withheld)(path)
        {
            let bytes = repo.object(&entry.id, Kind::Blob)?;
            walk.attributes.add(dir, &bytes);
        }
    }

    let mut compare = Compare {
        root,
        settings: &settings,
        attributes: &walk.attributes,
        case,
        index_time,
        real_dirs: HashSet::new(),
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut not_compared: Vec<String> = Vec::new();
    let mut paths: Vec<&Vec<u8>> = head.keys().chain(tracked.keys()).collect();
    paths.sort();
    paths.dedup();
    for bytes in paths {
        if out.late() {
            break;
        }
        let Ok(path) = std::str::from_utf8(bytes) else {
            out.withheld_any = true;
            continue;
        };
        if !within(path, filter) {
            continue;
        }
        if (out.withheld)(path) {
            out.withheld_any = true;
            continue;
        }
        let slot = tracked.get(bytes);
        if let Some(slot) = slot
            && slot.stages != 0
        {
            rows.push((conflict(slot.stages).to_owned(), path.to_owned()));
            continue;
        }
        let entry = slot.and_then(|s| s.zero.as_ref());
        let staged = match (head.get(bytes), entry) {
            (None, Some(e)) if e.intent_to_add => ' ',
            (None, Some(_)) => 'A',
            (Some(_), None) => 'D',
            (Some((mode, id)), Some(e)) if *mode != e.mode || *id != e.id => {
                if (*mode & 0o170000) != (e.mode & 0o170000) {
                    'T'
                } else {
                    'M'
                }
            }
            _ => ' ',
        };
        let unstaged = match entry {
            None => ' ',
            Some(e) if e.intent_to_add => match compare.present(path)? {
                true => 'A',
                false => 'D',
            },
            Some(e) => match compare.worktree(out, path, e)? {
                None => break,
                Some(Worktree::Same) => ' ',
                Some(Worktree::Changed(code)) => code,
                Some(Worktree::NotCompared) => {
                    not_compared.push(path.to_owned());
                    ' '
                }
            },
        };
        if staged != ' ' || unstaged != ' ' {
            rows.push((format!("{staged}{unstaged}"), path.to_owned()));
        }
    }

    let mut untracked = std::mem::take(&mut walk.untracked);
    untracked.sort();
    if rows.is_empty() && untracked.is_empty() && not_compared.is_empty() && !out.timed_out {
        out.text.line(
            "Nothing to commit: the index matches HEAD and the working tree matches the index.",
        );
        return Ok(());
    }
    for (code, path) in rows {
        if !out.room() {
            return Ok(());
        }
        out.text
            .line(&format!("{code} {}", quoted(&path, settings.quote_path)));
        out.shown.push(path);
    }
    for path in untracked {
        if !out.room() {
            return Ok(());
        }
        out.text
            .line(&format!("?? {}", quoted(&path, settings.quote_path)));
        out.shown.push(path.trim_end_matches('/').to_owned());
    }
    if !not_compared.is_empty() && out.room() {
        out.text.line(
            "Not compared, since git would convert them on the way into the index (an attribute \
             or core.autocrlf asks for a filter or line-ending conversion) or they are submodules; \
             use run with git status for these:",
        );
        for path in not_compared {
            if !out.room() {
                return Ok(());
            }
            out.text
                .line(&format!("   {}", quoted(&path, settings.quote_path)));
            out.shown.push(path);
        }
    }
    Ok(())
}
