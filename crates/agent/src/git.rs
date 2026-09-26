//! A repository's history, read from the files under `.git` without starting git.
//!
//! git runs programs its configuration names: `core.fsmonitor` on a status, `gpg.program` on a
//! log that shows signatures, and `include.path` pulls that configuration in from anywhere. So
//! [GIT-1](../../../docs/specs/tools/read-git.md#GIT-1) answers here instead, where what a request
//! means is fixed by this file and no key in `.git/config` can add a program to it. The
//! configuration is read only to decline what this reader would get wrong: an include it would not
//! follow, a working tree somewhere else, a repository format or extension it does not know.
//!
//! Nothing here reasons about labels or the trust map. The workspace decides whether a repository
//! may be opened at all, from [`survey`]'s list of every file a read could touch, and passes in
//! which paths are withheld; this module reports which paths it showed, so the answer can carry
//! their labels too.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use gix_hash::{Kind as HashKind, ObjectId, Prefix, oid};
use gix_object::tree::EntryKind;
use gix_object::{CommitRef, Kind, TagRef, TreeRef};
use gix_odb::pack::data::decode::header::ResolvedBase;

use crate::diff::{Change, Diff};

/// Commits a log shows when the planner names no count.
pub const DEFAULT_COUNT: usize = 20;

/// The most commits one log shows, however many the planner asks for.
pub const MAX_COUNT: usize = 200;

/// A blob larger than this is described rather than shown or diffed. Reading one means holding
/// it, twice for a diff, and a line diff over it is quadratic in the part that changed.
const MAX_BLOB: u64 = 1 << 20;

/// Lines one answer carries before it is cut.
const MAX_LINES: usize = 2000;

/// Characters one line carries before it is cut: a minified file is one line.
const MAX_LINE_CHARS: usize = 2000;

/// How many symbolic refs a name is followed through, as git does.
const SYMBOLIC_DEPTH: usize = 5;

/// How many tags one revision is peeled through before it is called unreadable.
const PEEL_DEPTH: usize = 16;

/// How deep a tree is walked before it is called unreadable, so a crafted one cannot exhaust the
/// stack.
const TREE_DEPTH: usize = 256;

/// Hex digits of an id a log line or a tree listing shows.
const SHORT: usize = 10;

/// Hex digits of an id an `index` or `Merge:` line shows, as git prints them.
const ABBREV: usize = 7;

/// Bytes looked at for a NUL, which is how git decides a blob is binary.
const BINARY_PROBE: usize = 8000;

/// How many more commits a range's walk takes once every commit left is one it will not list, as
/// git's `limit_list` does, in case a commit clocked earlier still leads back into the range.
const SLOP: usize = 5;

/// The questions `read_git` answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    Log,
    Show,
    Diff,
}

impl Query {
    pub fn named(name: &str) -> Option<Query> {
        match name {
            "log" => Some(Query::Log),
            "show" => Some(Query::Show),
            "diff" => Some(Query::Diff),
            _ => None,
        }
    }
}

/// One question, with every argument already a routing value the planner wrote.
pub struct Request<'a> {
    pub query: Query,
    pub revision: Option<&'a str>,
    /// Relative to the repository's root, not the workspace's.
    pub path: Option<&'a str>,
    pub count: usize,
    /// Seconds since the epoch; a commit older than this is not shown and not walked past.
    pub since: Option<i64>,
    /// Seconds since the epoch; a commit newer than this is not shown but is walked past.
    pub until: Option<i64>,
    pub deadline: Instant,
}

/// What a question produced.
#[derive(Debug, Default, Clone)]
pub struct Answer {
    pub text: String,
    /// Repository-relative paths whose contents or names the text shows.
    pub shown: Vec<String>,
    /// Whether a path was left out because the trust map withholds it.
    pub withheld: bool,
    /// Whether the text stops short of everything the question matched.
    pub cut: bool,
    pub timed_out: bool,
    /// Each run of a file's lines the text printed, so each is scanned as a read of that file.
    pub printed: Vec<Printed>,
    /// The text with every line of a file's contents left blank: the ids, names and messages
    /// around them, which are `.git`'s own.
    pub around: String,
}

/// Lines of one file, as an answer printed them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Printed {
    /// Relative to the repository's root.
    pub path: String,
    /// Which line of the file the first of them is.
    pub first_line: usize,
    pub text: String,
}

/// Why a repository or a question was not answered.
///
/// Each sentence is worded here and carries only what the planner wrote, never text read out of
/// the repository, so a refusal cannot be a route for a repository's bytes into the planner's
/// context without a label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declined {
    Untrusted,
    Fenced,
    NoRepository,
    LinkedGitDir,
    Linked,
    Alternates,
    CommonDir,
    Replaced,
    Include,
    Worktree,
    Format,
    Unreadable,
    NoCommits,
    TooSlow,
    Unknown(String),
    Ambiguous(String),
    Unsupported(String, &'static str),
    Kind {
        revision: String,
        wanted: &'static str,
    },
    NoSuchPath(String),
    PathInvalid(String),
    Withheld(String),
    ShowNeedsPath(String),
    ShowTakesOne(String),
    DiffNeedsTwo,
    PairIsForDiff(String),
}

impl Declined {
    /// The sentence the planner reads, for a repository it called `named`.
    pub fn describe(&self, named: &str) -> String {
        let fallback = "Use run to read it with git instead.";
        match self {
            Declined::Untrusted => format!(
                "{named}/.git is not a directory this session trusts in full, so read_git does \
                 not open it: reading history means following what its files say. {fallback}"
            ),
            Declined::Fenced => format!(
                "{named}/.git holds a file a deny rule covers, and read_git reads every file \
                 there or none, so it does not open it."
            ),
            Declined::NoRepository => format!(
                "{named} has no .git directory holding a repository. read_git reads a repository \
                 whose .git is a directory. {fallback}"
            ),
            Declined::LinkedGitDir => format!(
                "{named}/.git is a file pointing elsewhere, as a linked worktree or a submodule \
                 has, and read_git does not follow it. {fallback}"
            ),
            Declined::Linked => format!(
                "{named}/.git holds a symbolic link, and read_git does not follow one. {fallback}"
            ),
            Declined::Alternates => format!(
                "{named}/.git borrows objects from another repository through \
                 objects/info/alternates, which read_git does not read. {fallback}"
            ),
            Declined::CommonDir => format!(
                "{named}/.git shares its refs and objects with another repository through \
                 commondir, which read_git does not follow. {fallback}"
            ),
            Declined::Replaced => format!(
                "{named}/.git replaces objects or grafts parents, through refs/replace or \
                 info/grafts, and read_git does not apply either. {fallback}"
            ),
            Declined::Include => format!(
                "{named}/.git/config includes configuration from another file, which read_git \
                 does not read. {fallback}"
            ),
            Declined::Worktree => format!(
                "{named}/.git/config sets core.worktree, which read_git does not follow. \
                 {fallback}"
            ),
            Declined::Format => format!(
                "{named}/.git/config names a repository format or extension read_git does not \
                 read, or is not configuration read_git can parse. {fallback}"
            ),
            Declined::Unreadable => format!(
                "{named}/.git could not be read as a repository: something it names is missing \
                 or damaged. {fallback}"
            ),
            Declined::NoCommits => format!("{named} has no commits yet."),
            Declined::TooSlow => {
                format!("Reading {named}/.git took longer than read_git allows. {fallback}")
            }
            Declined::Unknown(revision) => {
                format!("{revision} names no commit, tag, branch or object in {named}.")
            }
            Declined::Ambiguous(revision) => format!(
                "{revision} is the start of more than one object id in {named}; give more of it."
            ),
            Declined::Unsupported(revision, what) => format!(
                "{revision} uses {what}, which read_git does not support. Use run to ask git for \
                 it."
            ),
            Declined::Kind { revision, wanted } => {
                format!("{revision} does not name a {wanted} in {named}.")
            }
            Declined::NoSuchPath(path) => {
                format!("{path} is not in that revision of {named}.")
            }
            Declined::PathInvalid(path) => format!(
                "{path} is not a path inside the repository: write it relative to the \
                 repository's root, without . or .. components."
            ),
            Declined::Withheld(path) => format!(
                "{path} in {named} is a path this session may not read, so read_git does not \
                 show it."
            ),
            Declined::ShowNeedsPath(revision) => format!(
                "{revision} names a tree or file by id alone, so where it sits in the repository \
                 is unknown and cannot be checked against what may be read. Name it as \
                 <commit>:<path>."
            ),
            Declined::ShowTakesOne(revision) => format!(
                "show takes one revision and {revision} is a range; use log to list its commits \
                 or diff to compare its ends."
            ),
            Declined::DiffNeedsTwo => "diff compares two commits, written as A..B or as \"A B\". \
                 To see what one commit changed, use show; to compare with the working tree, use \
                 run."
                .to_owned(),
            Declined::PairIsForDiff(revision) => format!(
                "{revision} names two revisions, which only diff takes; for a range of commits \
                 write A..B."
            ),
        }
    }
}

/// Every file under `git_dir` a question could read, or why none may be, from the directory
/// listings alone: no file is opened here.
///
/// The workspace holds each against the permission rules before [`Repository::open`] reads any of
/// them, which is how a repository with a file a rule withholds is never decoded at all. The list
/// covers what the ref store and the object finder read: `HEAD`, the configuration, `packed-refs`,
/// `shallow`, the pseudo refs at the top (`FETCH_HEAD`, `ORIG_HEAD`) a revision may name, every
/// file under `refs` a ref name can reach, and the loose objects and paired packs under `objects`.
/// A name is matched as a file system that ignores case would open it, and listed as the reader
/// spells it.
pub fn survey(git_dir: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Declined> {
    let meta = match std::fs::symlink_metadata(git_dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Declined::NoRepository),
        Err(_) => return Err(Declined::Unreadable),
    };
    if meta.file_type().is_symlink() {
        return Err(Declined::Linked);
    }
    if meta.is_file() {
        return Err(Declined::LinkedGitDir);
    }
    if !meta.is_dir() {
        return Err(Declined::NoRepository);
    }
    if present(&git_dir.join("commondir"))? {
        return Err(Declined::CommonDir);
    }
    if present(&git_dir.join("objects/info/alternates"))? {
        return Err(Declined::Alternates);
    }
    if present(&git_dir.join("info/grafts"))? {
        return Err(Declined::Replaced);
    }

    let mut files = Vec::new();
    let entries = std::fs::read_dir(git_dir).map_err(|_| Declined::Unreadable)?;
    for entry in entries {
        let entry = entry.map_err(|_| Declined::Unreadable)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let spelled = if matches!(
            lower.as_str(),
            "config" | "config.worktree" | "packed-refs" | "shallow"
        ) {
            lower
        } else if is_pseudo_ref(name) {
            name.to_owned()
        } else {
            continue;
        };
        let kind = entry.file_type().map_err(|_| Declined::Unreadable)?;
        if kind.is_symlink() {
            return Err(Declined::Linked);
        }
        if kind.is_file() {
            files.push(git_dir.join(spelled));
        }
    }
    if !files
        .iter()
        .any(|f| f.file_name().is_some_and(|n| n == "HEAD"))
    {
        return Err(Declined::NoRepository);
    }

    let refs = git_dir.join("refs");
    let mut found = Vec::new();
    walk(&refs, &mut found, deadline)?;
    let under = |file: &Path, root: &Path| -> Option<Vec<String>> {
        file.strip_prefix(root)
            .ok()?
            .iter()
            .map(|part| part.to_str().map(str::to_owned))
            .collect()
    };
    for file in found {
        let Some(parts) = under(&file, &refs) else {
            continue;
        };
        if parts
            .first()
            .is_some_and(|top| top.eq_ignore_ascii_case("replace"))
        {
            return Err(Declined::Replaced);
        }
        let named = parts.iter().all(|part| !part.starts_with('.'))
            && parts
                .last()
                .is_some_and(|last| !last.to_ascii_lowercase().ends_with(".lock"));
        if named {
            files.push(file);
        }
    }

    let objects = git_dir.join("objects");
    let mut found = Vec::new();
    walk(&objects, &mut found, deadline)?;
    for file in found {
        let loose = under(&file, &objects).is_some_and(|parts| match parts.as_slice() {
            [dir, name] => dir.len() == 2 && is_hex(dir) && name.len() == 38 && is_hex(name),
            _ => false,
        });
        if loose {
            files.push(file);
        }
    }
    for index in pack_indexes(&objects)? {
        files.push(index.with_extension("pack"));
        files.push(index);
    }
    Ok(files)
}

/// The pack indexes under `objects/pack` with their pack beside them, which are the packs a read
/// opens.
fn pack_indexes(objects: &Path) -> Result<Vec<PathBuf>, Declined> {
    let mut indexes = Vec::new();
    match std::fs::read_dir(objects.join("pack")) {
        Ok(entries) => {
            for entry in entries {
                let path = entry.map_err(|_| Declined::Unreadable)?.path();
                if path.extension().is_some_and(|e| e == "idx")
                    && path.is_file()
                    && path.with_extension("pack").is_file()
                {
                    indexes.push(path);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Declined::Unreadable),
    }
    indexes.sort();
    Ok(indexes)
}

fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, Declined> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Declined::Unreadable),
    }
}

fn present(path: &Path) -> Result<bool, Declined> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => Ok(false),
        Err(_) => Err(Declined::Unreadable),
    }
}

/// A name the ref store reads from the top of `.git` rather than under `refs`, as it does for
/// `HEAD` and `FETCH_HEAD`.
fn is_pseudo_ref(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
}

fn walk(root: &Path, files: &mut Vec<PathBuf>, deadline: Instant) -> Result<(), Declined> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if Instant::now() >= deadline {
            return Err(Declined::TooSlow);
        }
        let meta = match std::fs::symlink_metadata(&dir) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(Declined::Unreadable),
        };
        if meta.file_type().is_symlink() {
            return Err(Declined::Linked);
        }
        let entries = std::fs::read_dir(&dir).map_err(|_| Declined::Unreadable)?;
        for entry in entries {
            let entry = entry.map_err(|_| Declined::Unreadable)?;
            let kind = entry.file_type().map_err(|_| Declined::Unreadable)?;
            if kind.is_symlink() {
                return Err(Declined::Linked);
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    Ok(())
}

/// Decline a configuration this reader would answer differently from git under.
///
/// Parsed by hand, to git's syntax, because only three facts are wanted from it and a
/// configuration crate would read `include.path` by following it. Anything that does not parse is
/// declined rather than guessed at: a key misread here is a key git reads another way.
fn check_config(bytes: &[u8]) -> Result<(), Declined> {
    let entries = parse_config(bytes).ok_or(Declined::Format)?;
    for entry in entries {
        match (entry.section.as_str(), entry.subsection.is_some()) {
            ("include" | "includeif", _) => return Err(Declined::Include),
            ("core", false) if entry.key == "worktree" => return Err(Declined::Worktree),
            ("core", false) if entry.key == "repositoryformatversion" => {
                let value = entry.value.as_deref().unwrap_or_default();
                let version = std::str::from_utf8(value)
                    .ok()
                    .and_then(|v| v.trim().parse::<u32>().ok());
                if !matches!(version, Some(0 | 1)) {
                    return Err(Declined::Format);
                }
            }
            ("extensions", false) => {
                let value = entry
                    .value
                    .as_deref()
                    .map(|v| String::from_utf8_lossy(v).to_ascii_lowercase());
                let known = match entry.key.as_str() {
                    "noop" | "preciousobjects" | "partialclone" | "worktreeconfig" => true,
                    "objectformat" => value.as_deref() == Some("sha1"),
                    "refstorage" => value.as_deref() == Some("files"),
                    _ => false,
                };
                if !known {
                    return Err(Declined::Format);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct ConfigEntry {
    section: String,
    subsection: Option<Vec<u8>>,
    key: String,
    value: Option<Vec<u8>>,
}

/// A section's name, and its subsection where it has one.
type Section = (String, Option<Vec<u8>>);

fn parse_config(bytes: &[u8]) -> Option<Vec<ConfigEntry>> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut text = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        text.push(bytes[i]);
        i += 1;
    }
    let b = text.as_slice();
    let mut entries = Vec::new();
    let mut section: Option<Section> = None;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'#' | b';' => i = end_of_line(b, i),
            b'[' => {
                let (header, next) = parse_header(b, i + 1)?;
                section = Some(header);
                i = next;
            }
            c if c.is_ascii_alphabetic() => {
                let (name, subsection) = section.clone()?;
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-') {
                    i += 1;
                }
                let key = String::from_utf8_lossy(&b[start..i]).to_ascii_lowercase();
                while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r') {
                    i += 1;
                }
                let value = match b.get(i) {
                    None | Some(b'\n') => None,
                    Some(b'#' | b';') => {
                        i = end_of_line(b, i);
                        None
                    }
                    Some(b'=') => {
                        let (value, next) = parse_value(b, i + 1)?;
                        i = next;
                        Some(value)
                    }
                    Some(_) => return None,
                };
                entries.push(ConfigEntry {
                    section: name,
                    subsection,
                    key,
                    value,
                });
            }
            _ => return None,
        }
    }
    Some(entries)
}

fn end_of_line(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    i
}

/// `[section]`, `[section "subsection"]` or the older `[section.subsection]`, from just past the
/// `[`. What follows the `]` on the same line is read as the rest of the file is, since git
/// allows a key there.
fn parse_header(b: &[u8], mut i: usize) -> Option<(Section, usize)> {
    let start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'.') {
        i += 1;
    }
    let name = String::from_utf8_lossy(&b[start..i]).to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    match b.get(i)? {
        b']' => {
            let header = match name.split_once('.') {
                Some((section, sub)) if !section.is_empty() => {
                    (section.to_owned(), Some(sub.as_bytes().to_vec()))
                }
                Some(_) => return None,
                None => (name, None),
            };
            Some((header, i + 1))
        }
        b' ' | b'\t' => {
            while i < b.len() && matches!(b[i], b' ' | b'\t') {
                i += 1;
            }
            if b.get(i) != Some(&b'"') {
                return None;
            }
            i += 1;
            let mut sub = Vec::new();
            loop {
                match *b.get(i)? {
                    b'"' => break,
                    b'\n' => return None,
                    b'\\' => {
                        let next = *b.get(i + 1)?;
                        if next == b'\n' {
                            return None;
                        }
                        sub.push(next);
                        i += 2;
                    }
                    c => {
                        sub.push(c);
                        i += 1;
                    }
                }
            }
            if b.get(i + 1) != Some(&b']') {
                return None;
            }
            Some(((name, Some(sub)), i + 2))
        }
        _ => None,
    }
}

/// A value from just past its `=`, to git's rules: quotes toggle, a backslash escapes `n`, `t`,
/// `b`, `"`, `\` or the end of the line, an unquoted `#` or `;` starts a comment, and whitespace
/// at either end outside quotes is dropped.
fn parse_value(b: &[u8], mut i: usize) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut spaces = Vec::new();
    loop {
        let Some(&c) = b.get(i) else {
            if quoted {
                return None;
            }
            return Some((out, i));
        };
        i += 1;
        if c == b'\n' {
            if quoted {
                return None;
            }
            return Some((out, i));
        }
        if !quoted && (c == b'#' || c == b';') {
            return Some((out, end_of_line(b, i)));
        }
        if !quoted && (c == b' ' || c == b'\t') {
            spaces.push(c);
            continue;
        }
        if !out.is_empty() {
            out.append(&mut spaces);
        }
        spaces.clear();
        match c {
            b'\\' => {
                let next = *b.get(i)?;
                i += 1;
                match next {
                    b'\n' => {}
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(8),
                    b'"' | b'\\' => out.push(next),
                    _ => return None,
                }
            }
            b'"' => quoted = !quoted,
            _ => out.push(c),
        }
    }
}

/// A revision as written: one, two for a diff, or a range.
#[derive(Debug, PartialEq, Eq)]
enum Spec<'a> {
    One(&'a str),
    Pair(&'a str, &'a str),
    Range(&'a str, &'a str),
}

/// Everything after the first `:` is a path, read as a name whatever it holds, so only the
/// revision before it is read for git's syntax.
fn spec(revision: &str) -> Result<Spec<'_>, Declined> {
    let text = revision.trim_start();
    let (rev, path) = match text.split_once(':') {
        Some((rev, path)) => (rev, Some(path)),
        None => (text.trim_end(), None),
    };
    let unsupported = |what| Err(Declined::Unsupported(text.trim_end().to_owned(), what));
    for (needle, what) in [
        ("...", "a symmetric difference (A...B)"),
        ("@{", "a reflog or upstream selector (@{...})"),
        ("^{/", "a message search (^{/...})"),
        ("^@", "the parents selector (^@)"),
        ("^!", "the ^! selector"),
        ("^-", "the ^- selector"),
    ] {
        if rev.contains(needle) {
            return unsupported(what);
        }
    }
    if rev.starts_with('-') {
        return unsupported("git's options");
    }
    if rev.trim().is_empty() {
        return match path {
            Some(_) => unsupported("the index (:path) or a message search (:/...)"),
            None => Err(Declined::Unknown(revision.to_owned())),
        };
    }
    let words: Vec<&str> = rev.split_whitespace().collect();
    if path.is_some() {
        return match words.as_slice() {
            [one] if one.contains("..") => unsupported("a path inside a range"),
            [_] => Ok(Spec::One(text)),
            _ => unsupported("a path inside a pair of revisions"),
        };
    }
    match words.as_slice() {
        [one] => match one.split_once("..") {
            Some((a, b)) => Ok(Spec::Range(
                if a.is_empty() { "HEAD" } else { a },
                if b.is_empty() { "HEAD" } else { b },
            )),
            None => Ok(Spec::One(one)),
        },
        [a, b] => {
            if a.contains("..") || b.contains("..") || a.starts_with('-') || b.starts_with('-') {
                return unsupported("a range or an option inside a pair of revisions");
            }
            Ok(Spec::Pair(a, b))
        }
        _ => unsupported("more than two revisions"),
    }
}

/// The path a single revision names after its `:`, relative to the repository's root, so the
/// caller can hold it against the trust map before anything is opened.
pub fn path_in(revision: &str) -> Option<&str> {
    let (_, path) = revision.trim_start().split_once(':')?;
    Some(path)
}

/// A path relative to the repository's root, with any trailing `/` dropped, or why it is not one.
/// The root itself is the empty string.
fn clean(path: &str) -> Result<String, Declined> {
    if path == "." {
        return Ok(String::new());
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() {
            Ok(String::new())
        } else {
            Err(Declined::PathInvalid(path.to_owned()))
        };
    }
    let bad = trimmed.split('/').any(|component| {
        component.is_empty() || component == "." || component == ".." || component.contains('\0')
    });
    if bad {
        return Err(Declined::PathInvalid(path.to_owned()));
    }
    Ok(trimmed.to_owned())
}

/// Whether `name` could be a ref, by the rules git checks a ref name against. The ref store checks
/// too; this runs first so a revision's own syntax is never handed to it as a name.
///
/// A name under `worktrees/` or `main-worktree/` is refused as well: the ref store reads those
/// from another worktree's directory, which [`survey`] never lists.
fn plausible_ref(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with("worktrees/")
        && !name.starts_with("main-worktree/")
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.to_ascii_lowercase().ends_with(".lock")
        && !name.contains("..")
        && !name.contains("@{")
        && name != "@"
        && name
            .chars()
            .all(|c| !c.is_control() && !" ~^:?*[\\".contains(c))
        && name
            .split('/')
            .all(|component| !component.is_empty() && !component.starts_with('.'))
}

fn is_hex(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The object finder: every pack whose index sits beside it, then the loose objects. Written here
/// over gix's pack and loose stores, rather than taken as gix-odb's dynamic store, because that
/// store follows `objects/info/alternates` into whatever repository it names.
struct Objects {
    loose: gix_odb::loose::Store,
    packs: Vec<gix_odb::pack::Bundle>,
}

impl Objects {
    fn open(git_dir: &Path) -> Result<Self, Declined> {
        let objects = git_dir.join("objects");
        let loose = gix_odb::loose::Store::at(&objects, HashKind::Sha1);
        let packs = pack_indexes(&objects)?
            .iter()
            .map(|index| {
                gix_odb::pack::Bundle::at(index, HashKind::Sha1).map_err(|_| Declined::Unreadable)
            })
            .collect::<Result<_, _>>()?;
        Ok(Objects { loose, packs })
    }

    fn read(&self, id: &oid) -> Result<Option<(Kind, Vec<u8>)>, Declined> {
        let mut buf = Vec::new();
        for pack in &self.packs {
            if pack.index.lookup(id).is_none() {
                continue;
            }
            let found = pack
                .find(
                    id,
                    &mut buf,
                    &mut Default::default(),
                    &mut gix_odb::pack::cache::Never,
                )
                .map_err(|_| Declined::Unreadable)?;
            if let Some((data, _)) = found {
                return Ok(Some((data.kind, data.data.to_vec())));
            }
        }
        match self.loose.try_find(id, &mut buf) {
            Ok(Some(data)) => Ok(Some((data.kind, data.data.to_vec()))),
            Ok(None) => Ok(None),
            Err(_) => Err(Declined::Unreadable),
        }
    }

    /// An object's kind and size, without inflating more of it than the header.
    fn header(&self, id: &oid) -> Result<Option<(Kind, u64)>, Declined> {
        for pack in &self.packs {
            let Some(index) = pack.index.lookup(id) else {
                continue;
            };
            let entry = pack
                .pack
                .entry(pack.index.pack_offset_at_index(index))
                .map_err(|_| Declined::Unreadable)?;
            let resolve = |base: &oid| {
                let index = pack.index.lookup(base)?;
                pack.pack
                    .entry(pack.index.pack_offset_at_index(index))
                    .ok()
                    .map(ResolvedBase::InPack)
            };
            let outcome = pack
                .pack
                .decode_header(entry, &mut Default::default(), &resolve)
                .map_err(|_| Declined::Unreadable)?;
            return Ok(Some((outcome.kind, outcome.object_size)));
        }
        match self.loose.try_header(id) {
            Ok(Some((size, kind))) => Ok(Some((kind, size))),
            Ok(None) => Ok(None),
            Err(_) => Err(Declined::Unreadable),
        }
    }

    /// The one object whose id starts with `prefix`: `Some(Err(()))` when more than one does.
    fn by_prefix(&self, prefix: Prefix) -> Result<Option<Result<ObjectId, ()>>, Declined> {
        let mut found = Vec::new();
        for pack in &self.packs {
            match pack.index.lookup_prefix(prefix, None) {
                None => {}
                Some(Err(())) => return Ok(Some(Err(()))),
                Some(Ok(index)) => found.push(pack.index.oid_at_index(index).to_owned()),
            }
        }
        match self.loose.lookup_prefix(prefix, None) {
            Ok(None) => {}
            Ok(Some(Err(()))) => return Ok(Some(Err(()))),
            Ok(Some(Ok(id))) => found.push(id),
            Err(_) => return Err(Declined::Unreadable),
        }
        found.sort();
        found.dedup();
        Ok(match found.as_slice() {
            [] => None,
            [one] => Some(Ok(*one)),
            _ => Some(Err(())),
        })
    }
}

/// A commit's place in the walk.
struct Info {
    tree: ObjectId,
    parents: Vec<ObjectId>,
    time: i64,
}

/// A revision resolved to an object, and where in the repository it sits if that is known.
struct Resolved {
    id: ObjectId,
    kind: Kind,
    /// Relative to the repository's root; `Some("")` is the root tree.
    path: Option<String>,
    gitlink: bool,
}

type Side = Option<(EntryKind, ObjectId)>;

/// A repository opened for reading, once [`survey`] has passed it.
pub struct Repository {
    refs: gix_ref::file::Store,
    objects: Objects,
    shallow: HashSet<ObjectId>,
}

impl Repository {
    /// Open the repository at `git_dir`, declining one whose configuration or refs ask for what
    /// this reader does not do.
    pub fn open(git_dir: &Path) -> Result<Self, Declined> {
        for name in ["config", "config.worktree"] {
            if let Some(bytes) = read_if_present(&git_dir.join(name))? {
                check_config(&bytes)?;
            }
        }
        if let Some(bytes) = read_if_present(&git_dir.join("packed-refs"))?
            && bytes
                .split(|b| *b == b'\n')
                .any(|line| line.windows(14).any(|w| w == b" refs/replace/"))
        {
            return Err(Declined::Replaced);
        }
        let refs = gix_ref::file::Store::at(git_dir.to_path_buf(), HashKind::Sha1);
        let objects = Objects::open(git_dir)?;
        let mut shallow = HashSet::new();
        if let Some(bytes) = read_if_present(&git_dir.join("shallow"))? {
            for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
                let id = ObjectId::from_hex(line).map_err(|_| Declined::Unreadable)?;
                shallow.insert(id);
            }
        }
        Ok(Repository {
            refs,
            objects,
            shallow,
        })
    }

    /// Answer `request`. `withheld` says, for a repository-relative path, whether the trust map
    /// withholds it; such a path is left out of the answer, and the answer says one was.
    pub fn answer(
        &self,
        request: &Request<'_>,
        withheld: &dyn Fn(&str) -> bool,
    ) -> Result<Answer, Declined> {
        let mut out = Out {
            text: Text::default(),
            shown: Vec::new(),
            printed: Vec::new(),
            withheld_any: false,
            timed_out: false,
            withheld,
            deadline: request.deadline,
        };
        let filter = match request.path {
            None => None,
            Some(path) => {
                let path = clean(path)?;
                if path.is_empty() {
                    None
                } else if withheld(&path) {
                    return Err(Declined::Withheld(path));
                } else {
                    Some(path)
                }
            }
        };
        let mut cut = false;
        match request.query {
            Query::Log => cut = self.log(&mut out, request, filter.as_deref())?,
            Query::Show => self.show(&mut out, request, filter.as_deref())?,
            Query::Diff => self.diff(&mut out, request, filter.as_deref())?,
        }
        let mut shown = out.shown;
        shown.sort();
        shown.dedup();
        Ok(Answer {
            text: out.text.body,
            shown,
            withheld: out.withheld_any,
            cut: cut || out.text.cut,
            timed_out: out.timed_out,
            printed: out.printed,
            around: out.text.around,
        })
    }

    fn object(&self, id: &oid, want: Kind) -> Result<Vec<u8>, Declined> {
        match self.objects.read(id)? {
            Some((kind, data)) if kind == want => Ok(data),
            _ => Err(Declined::Unreadable),
        }
    }

    fn info(&self, id: &oid) -> Result<Info, Declined> {
        let data = self.object(id, Kind::Commit)?;
        let commit =
            CommitRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        let parents = if self.shallow.contains(id) {
            Vec::new()
        } else {
            commit.parents().collect()
        };
        let time = commit.committer().map(|c| c.seconds()).unwrap_or(0);
        Ok(Info {
            tree: commit.tree(),
            parents,
            time,
        })
    }

    /// Follow `name` through symbolic refs: `None` when there is no such ref, `Some(None)` when it
    /// points at a branch with no commits yet.
    fn follow(&self, name: &str) -> Result<Option<Option<ObjectId>>, Declined> {
        let mut current = name.to_owned();
        for depth in 0..=SYMBOLIC_DEPTH {
            // A symbolic ref's target is text from the repository, held to the same rule as a
            // name the planner wrote, so neither can reach a file the survey did not list.
            if depth > 0 && !plausible_ref(&current) {
                return Err(Declined::Unreadable);
            }
            let found = self
                .refs
                .try_find(current.as_str())
                .map_err(|_| Declined::Unreadable)?;
            let Some(reference) = found else {
                return Ok(if depth == 0 { None } else { Some(None) });
            };
            match reference.target {
                gix_ref::Target::Object(id) => return Ok(Some(Some(id))),
                gix_ref::Target::Symbolic(next) => {
                    let next: &[u8] = next.as_bstr();
                    current = std::str::from_utf8(next)
                        .map_err(|_| Declined::Unreadable)?
                        .to_owned();
                }
            }
        }
        Err(Declined::Unreadable)
    }

    fn head(&self) -> Result<ObjectId, Declined> {
        match self.follow("HEAD")? {
            Some(Some(id)) => Ok(id),
            Some(None) => Err(Declined::NoCommits),
            None => Err(Declined::Unreadable),
        }
    }

    /// The object a revision's first word names, before any `~`, `^` or `:`.
    fn base(&self, word: &str, text: &str) -> Result<ObjectId, Declined> {
        let unknown = || Declined::Unknown(text.to_owned());
        if word.is_empty() {
            return Err(unknown());
        }
        if word == "HEAD" || word == "@" {
            return self.head();
        }
        if word.len() == 40 && is_hex(word) {
            let id = ObjectId::from_hex(word.as_bytes()).map_err(|_| unknown())?;
            return match self.objects.header(&id)? {
                Some(_) => Ok(id),
                None => Err(unknown()),
            };
        }
        if plausible_ref(word) {
            match self.follow(word)? {
                Some(Some(id)) => return Ok(id),
                Some(None) => return Err(unknown()),
                None => {}
            }
        }
        if (4..40).contains(&word.len()) && is_hex(word) {
            let prefix = Prefix::from_hex(word).map_err(|_| unknown())?;
            return match self.objects.by_prefix(prefix)? {
                Some(Ok(id)) => Ok(id),
                Some(Err(())) => Err(Declined::Ambiguous(text.to_owned())),
                None => Err(unknown()),
            };
        }
        Err(unknown())
    }

    /// Peel `id` through tags, and a commit to its tree when a tree is wanted. The flag says a
    /// commit was peeled to its root tree, which is a tree whose place is known.
    fn peel(
        &self,
        mut id: ObjectId,
        want: Option<Kind>,
        text: &str,
    ) -> Result<(ObjectId, Kind, bool), Declined> {
        for _ in 0..PEEL_DEPTH {
            let (kind, _) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
            match (kind, want) {
                (kind, Some(want)) if kind == want => return Ok((id, kind, false)),
                (kind, None) if kind != Kind::Tag => return Ok((id, kind, false)),
                (Kind::Tag, _) => {
                    let data = self.object(&id, Kind::Tag)?;
                    let tag = TagRef::from_bytes(&data, HashKind::Sha1)
                        .map_err(|_| Declined::Unreadable)?;
                    id = tag.target();
                }
                (Kind::Commit, Some(Kind::Tree)) => {
                    return Ok((self.info(&id)?.tree, Kind::Tree, true));
                }
                _ => {
                    return Err(Declined::Kind {
                        revision: text.to_owned(),
                        wanted: want.map_or("object", kind_word),
                    });
                }
            }
        }
        Err(Declined::Unreadable)
    }

    /// Resolve one revision: a base, then `~N`, `^N` and `^{kind}` in order, then `:path`.
    fn resolve(
        &self,
        text: &str,
        deadline: Instant,
        withheld: &dyn Fn(&str) -> bool,
    ) -> Result<Resolved, Declined> {
        let (rev, path) = match text.split_once(':') {
            Some((rev, path)) => (rev, Some(path)),
            None => (text, None),
        };
        let cut = rev.find(['~', '^']).unwrap_or(rev.len());
        let (word, mut rest) = rev.split_at(cut);
        let mut id = self.base(word, text)?;
        let mut root = false;
        while !rest.is_empty() {
            let op = rest.as_bytes()[0];
            rest = &rest[1..];
            if op == b'^' && rest.starts_with('{') {
                let close = rest
                    .find('}')
                    .ok_or_else(|| Declined::Unknown(text.to_owned()))?;
                let want = match &rest[1..close] {
                    "" => None,
                    "commit" => Some(Kind::Commit),
                    "tree" => Some(Kind::Tree),
                    "blob" => Some(Kind::Blob),
                    "tag" => Some(Kind::Tag),
                    _ => {
                        return Err(Declined::Unsupported(
                            text.to_owned(),
                            "a peel other than ^{}, ^{commit}, ^{tree}, ^{blob} or ^{tag}",
                        ));
                    }
                };
                rest = &rest[close + 1..];
                let (peeled, _, to_root) = self.peel(id, want, text)?;
                id = peeled;
                root = to_root;
                continue;
            }
            let digits = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            let n: usize = if digits == 0 {
                1
            } else {
                rest[..digits]
                    .parse()
                    .map_err(|_| Declined::Unknown(text.to_owned()))?
            };
            rest = &rest[digits..];
            let (commit, _, _) = self.peel(id, Some(Kind::Commit), text)?;
            id = commit;
            root = false;
            if op == b'~' {
                for _ in 0..n {
                    if Instant::now() >= deadline {
                        return Err(Declined::TooSlow);
                    }
                    id = *self
                        .info(&id)?
                        .parents
                        .first()
                        .ok_or_else(|| Declined::Unknown(text.to_owned()))?;
                }
            } else if n > 0 {
                id = *self
                    .info(&id)?
                    .parents
                    .get(n - 1)
                    .ok_or_else(|| Declined::Unknown(text.to_owned()))?;
            }
        }
        let Some(path) = path else {
            let (kind, _) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
            return Ok(Resolved {
                id,
                kind,
                path: root.then(String::new),
                gitlink: false,
            });
        };
        let path = clean(path)?;
        if !path.is_empty() && withheld(&path) {
            return Err(Declined::Withheld(path));
        }
        let (commit, _, _) = self.peel(id, Some(Kind::Commit), text)?;
        let tree = self.info(&commit)?.tree;
        match self.entry_at(tree, &path)? {
            None => Err(Declined::NoSuchPath(path)),
            Some((kind, id)) => Ok(Resolved {
                id,
                kind: match kind {
                    EntryKind::Tree => Kind::Tree,
                    EntryKind::Commit => Kind::Commit,
                    _ => Kind::Blob,
                },
                gitlink: kind == EntryKind::Commit,
                path: Some(path),
            }),
        }
    }

    fn commit_of(&self, text: &str, out: &Out<'_>) -> Result<ObjectId, Declined> {
        let resolved = self.resolve(text, out.deadline, out.withheld)?;
        if resolved.gitlink {
            return Err(Declined::Kind {
                revision: text.to_owned(),
                wanted: "commit in this repository",
            });
        }
        Ok(self.peel(resolved.id, Some(Kind::Commit), text)?.0)
    }

    /// The entry at `path` beneath `tree`, or the tree itself for the empty path.
    fn entry_at(&self, tree: ObjectId, path: &str) -> Result<Side, Declined> {
        if path.is_empty() {
            return Ok(Some((EntryKind::Tree, tree)));
        }
        let mut current = tree;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            let data = self.object(&current, Kind::Tree)?;
            let listed =
                TreeRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
            let found = listed.entries.iter().find(|e| {
                let name: &[u8] = e.filename;
                name == part.as_bytes()
            });
            let Some(entry) = found else {
                return Ok(None);
            };
            if parts.peek().is_none() {
                return Ok(Some((entry.mode.kind(), entry.oid.to_owned())));
            }
            if !entry.mode.is_tree() {
                return Ok(None);
            }
            current = entry.oid.to_owned();
        }
        Ok(None)
    }

    /// A tree's entries, keyed so that iterating them visits paths in the order git's diff does:
    /// a tree sorts as though its name ended in `/`.
    fn entries(&self, tree: Option<ObjectId>) -> Result<BTreeMap<Vec<u8>, Entry>, Declined> {
        let mut entries = BTreeMap::new();
        let Some(tree) = tree else {
            return Ok(entries);
        };
        let data = self.object(&tree, Kind::Tree)?;
        let listed =
            TreeRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        for entry in listed.entries {
            let name: &[u8] = entry.filename;
            if name.is_empty()
                || name == b"."
                || name == b".."
                || name.contains(&b'/')
                || name.contains(&0)
            {
                return Err(Declined::Unreadable);
            }
            let kind = entry.mode.kind();
            let mut key = name.to_vec();
            if kind == EntryKind::Tree {
                key.push(b'/');
            }
            entries.insert(
                key,
                Entry {
                    name: std::str::from_utf8(name).ok().map(str::to_owned),
                    kind,
                    id: entry.oid.to_owned(),
                },
            );
        }
        Ok(entries)
    }

    fn log(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<bool, Declined> {
        let (tips, hidden_tips) = match spec(request.revision.unwrap_or("HEAD"))? {
            Spec::One(one) => (vec![self.commit_of(one, out)?], Vec::new()),
            Spec::Range(a, b) => (vec![self.commit_of(b, out)?], vec![self.commit_of(a, out)?]),
            Spec::Pair(..) => {
                return Err(Declined::PairIsForDiff(
                    request.revision.unwrap_or_default().trim().to_owned(),
                ));
            }
        };
        if let Some(path) = filter {
            out.shown.push(path.to_owned());
        }
        let mut walk = Walk::default();
        for &id in &hidden_tips {
            walk.ensure(self, id)?;
            walk.hidden.insert(id);
            walk.bottoms.insert(id);
            walk.hide_ancestors(walk.infos[&id].parents.clone());
        }
        for &id in hidden_tips.iter().chain(&tips) {
            walk.enqueue(self, id)?;
        }
        // A range is walked to its end before any of it is listed, because a commit is known to
        // be reachable from the range's start only once everything newer has been read.
        let limited = !hidden_tips.is_empty();
        let count = request.count.min(MAX_COUNT);
        let mut listed = Vec::new();
        let mut shown = 0;
        let mut date = i64::MAX;
        let mut slop = SLOP;
        while let Some(next) = walk.queue.pop() {
            if out.late() {
                break;
            }
            let id = next.id;
            if request.since.is_some_and(|since| next.time < since) {
                if !limited {
                    continue;
                }
                walk.hidden.insert(id);
            }
            if walk.hidden.contains(&id) {
                walk.hide_parents_of(self, id)?;
                slop = walk.still_interesting(date, slop);
                if slop == 0 {
                    break;
                }
                continue;
            }
            if let Some(path) = filter {
                walk.simplify(self, id, path)?;
            }
            for parent in walk.infos[&id].parents.clone() {
                walk.enqueue(self, parent)?;
            }
            if request.until.is_some_and(|until| next.time > until) {
                continue;
            }
            if limited {
                date = next.time;
                listed.push(id);
                continue;
            }
            if walk.treesame.contains(&id) {
                continue;
            }
            if shown == count {
                return Ok(true);
            }
            out.text.line(&self.log_line(&id)?);
            shown += 1;
        }
        // A range cut off by the deadline lists nothing: a commit its start reaches is known to
        // be one only once the walk is done.
        if limited && !out.timed_out {
            for id in listed {
                if walk.hidden.contains(&id) || walk.treesame.contains(&id) {
                    continue;
                }
                if shown == count {
                    return Ok(true);
                }
                out.text.line(&self.log_line(&id)?);
                shown += 1;
            }
        }
        if shown == 0 && !out.timed_out {
            out.text.line("(no commits match)");
        }
        Ok(false)
    }

    fn log_line(&self, id: &oid) -> Result<String, Declined> {
        let data = self.object(id, Kind::Commit)?;
        let commit =
            CommitRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        let (name, day) = match commit.author() {
            Ok(author) => {
                let (seconds, offset) = when(author.time().ok(), author.seconds());
                (one_line(author.name), day(seconds, offset))
            }
            Err(_) => (String::new(), String::new()),
        };
        let summary = one_line(commit.message().summary().as_ref());
        Ok(format!(
            "{} {day} {name} {summary}",
            id.to_hex_with_len(SHORT)
        ))
    }

    fn show(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let text = request.revision.unwrap_or("HEAD");
        let one = match spec(text)? {
            Spec::One(one) => one,
            Spec::Range(..) => return Err(Declined::ShowTakesOne(text.trim().to_owned())),
            Spec::Pair(..) => return Err(Declined::PairIsForDiff(text.trim().to_owned())),
        };
        let resolved = self.resolve(one, out.deadline, out.withheld)?;
        if resolved.gitlink {
            let path = resolved.path.unwrap_or_default();
            out.shown.push(path.clone());
            out.text.line(&format!(
                "{path} is a submodule, at commit {}; read_git does not read the submodule's own \
                 repository.",
                resolved.id.to_hex()
            ));
            return Ok(());
        }
        match resolved.kind {
            Kind::Commit => self.show_commit(out, resolved.id, filter),
            Kind::Tag => self.show_tag(out, resolved.id, filter),
            Kind::Tree => match resolved.path {
                Some(path) => self.show_tree(out, resolved.id, &path),
                None => Err(Declined::ShowNeedsPath(one.to_owned())),
            },
            Kind::Blob => match resolved.path {
                Some(path) => self.show_blob(out, resolved.id, &path),
                None => Err(Declined::ShowNeedsPath(one.to_owned())),
            },
        }
    }

    fn show_commit(
        &self,
        out: &mut Out<'_>,
        id: ObjectId,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let data = self.object(&id, Kind::Commit)?;
        let commit =
            CommitRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        let parents: Vec<ObjectId> = if self.shallow.contains(&id) {
            Vec::new()
        } else {
            commit.parents().collect()
        };
        out.text.line(&format!("commit {}", id.to_hex()));
        if parents.len() > 1 {
            let ids: Vec<String> = parents
                .iter()
                .map(|p| p.to_hex_with_len(ABBREV).to_string())
                .collect();
            out.text.line(&format!("Merge: {}", ids.join(" ")));
        }
        if let Ok(author) = commit.author() {
            let (seconds, offset) = when(author.time().ok(), author.seconds());
            out.text.line(&format!(
                "Author: {} <{}>",
                one_line(author.name),
                one_line(author.email)
            ));
            out.text
                .line(&format!("Date:   {}", stamp(seconds, offset)));
        }
        out.text.line("");
        message(out, commit.message);
        if parents.len() > 1 {
            let short = id.to_hex_with_len(ABBREV);
            out.text.line(&format!(
                "(a merge: read_git shows no diff for one; to compare it with a parent, use diff \
                 with the revision \"{short}^1 {short}\")"
            ));
            return Ok(());
        }
        let before = match parents.first() {
            Some(parent) => Some(self.info(parent)?.tree),
            None => None,
        };
        out.text.line("");
        self.diff_trees(out, before, Some(commit.tree()), "", filter, 0)
    }

    fn show_tag(
        &self,
        out: &mut Out<'_>,
        id: ObjectId,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let data = self.object(&id, Kind::Tag)?;
        let tag = TagRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        out.text.line(&format!("tag {}", one_line(tag.name)));
        if let Ok(Some(tagger)) = tag.tagger() {
            let (seconds, offset) = when(tagger.time().ok(), tagger.seconds());
            out.text.line(&format!(
                "Tagger: {} <{}>",
                one_line(tagger.name),
                one_line(tagger.email)
            ));
            out.text
                .line(&format!("Date:   {}", stamp(seconds, offset)));
        }
        out.text.line("");
        message(out, tag.message);
        out.text.line("");
        let text = id.to_hex().to_string();
        let (target, kind, _) = self.peel(tag.target(), None, &text)?;
        match kind {
            Kind::Commit => self.show_commit(out, target, filter),
            other => {
                out.text.line(&format!(
                    "(the tag points at a {} whose place in the repository is unknown, {})",
                    kind_word(other),
                    target.to_hex_with_len(SHORT)
                ));
                Ok(())
            }
        }
    }

    fn show_tree(&self, out: &mut Out<'_>, id: ObjectId, path: &str) -> Result<(), Declined> {
        out.shown.push(path.to_owned());
        for entry in self.entries(Some(id))?.into_values() {
            let Some(name) = entry.name else {
                out.withheld_any = true;
                continue;
            };
            let child = join(path, &name);
            if (out.withheld)(&child) {
                out.withheld_any = true;
                continue;
            }
            if !out.room() {
                break;
            }
            out.shown.push(child);
            let slash = if entry.kind == EntryKind::Tree {
                "/"
            } else {
                ""
            };
            out.text.line(&format!(
                "{} {} {}\t{name}{slash}",
                mode(entry.kind),
                entry_word(entry.kind),
                entry.id.to_hex_with_len(SHORT)
            ));
        }
        Ok(())
    }

    fn show_blob(&self, out: &mut Out<'_>, id: ObjectId, path: &str) -> Result<(), Declined> {
        out.shown.push(path.to_owned());
        let (_, size) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
        if size > MAX_BLOB {
            out.text.line(&format!(
                "({path} is {size} bytes, more than read_git shows; use read_file on the working \
                 copy, or run)"
            ));
            return Ok(());
        }
        let data = self.object(&id, Kind::Blob)?;
        if is_binary(&data) {
            out.text
                .line(&format!("({path} is binary, {} bytes)", data.len()));
            return Ok(());
        }
        let mut printed = String::new();
        for line in escaped(&data).lines() {
            let Some(end) = out.text.add(line, true) else {
                break;
            };
            printed.push_str(&line[..end]);
            printed.push('\n');
        }
        if !printed.is_empty() {
            out.printed.push(Printed {
                path: path.to_owned(),
                first_line: 1,
                text: printed,
            });
        }
        Ok(())
    }

    fn diff(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let text = request.revision.ok_or(Declined::DiffNeedsTwo)?;
        let (a, b) = match spec(text)? {
            Spec::Pair(a, b) | Spec::Range(a, b) => (a, b),
            Spec::One(_) => return Err(Declined::DiffNeedsTwo),
        };
        let old = self.commit_of(a, out)?;
        let new = self.commit_of(b, out)?;
        let old_tree = self.info(&old)?.tree;
        let new_tree = self.info(&new)?.tree;
        self.diff_trees(out, Some(old_tree), Some(new_tree), "", filter, 0)?;
        if out.text.lines == 0 && !out.timed_out {
            out.text.line("(no differences)");
        }
        Ok(())
    }

    fn diff_trees(
        &self,
        out: &mut Out<'_>,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
        prefix: &str,
        filter: Option<&str>,
        depth: usize,
    ) -> Result<(), Declined> {
        if depth > TREE_DEPTH {
            return Err(Declined::Unreadable);
        }
        let old_entries = self.entries(old)?;
        let new_entries = self.entries(new)?;
        let keys: BTreeSet<&Vec<u8>> = old_entries.keys().chain(new_entries.keys()).collect();
        for key in keys {
            // Full and already cut means a file below this tree found no room, so nothing after
            // it fits either. Full and not yet cut is an answer that may be complete: whether it
            // was cut is known only once another file it would show turns up.
            if out.late() || (out.text.cut && out.text.full()) {
                return Ok(());
            }
            let a = old_entries.get(key);
            let b = new_entries.get(key);
            let side = |e: Option<&Entry>| e.map(|e| (e.kind, e.id));
            if side(a) == side(b) {
                continue;
            }
            let Some(either) = a.or(b) else {
                continue;
            };
            let Some(name) = &either.name else {
                out.withheld_any = true;
                continue;
            };
            let path = join(prefix, name);
            if !in_scope(&path, filter) {
                continue;
            }
            if (out.withheld)(&path) {
                out.withheld_any = true;
                continue;
            }
            if either.kind == EntryKind::Tree {
                self.diff_trees(
                    out,
                    a.map(|e| e.id),
                    b.map(|e| e.id),
                    &path,
                    filter,
                    depth + 1,
                )?;
            } else {
                if !out.room() {
                    return Ok(());
                }
                match (side(a), side(b)) {
                    (Some(x), Some(y)) if family(x.0) != family(y.0) => {
                        self.patch(out, &path, Some(x), None)?;
                        self.patch(out, &path, None, Some(y))?;
                    }
                    (x, y) => self.patch(out, &path, x, y)?,
                }
            }
        }
        Ok(())
    }

    /// One file's change, in the form `git diff` prints it.
    fn patch(&self, out: &mut Out<'_>, path: &str, old: Side, new: Side) -> Result<(), Declined> {
        out.shown.push(path.to_owned());
        out.text.line(&format!("diff --git a/{path} b/{path}"));
        let zero = "0".repeat(ABBREV);
        let short = |id: ObjectId| id.to_hex_with_len(ABBREV).to_string();
        match (old, new) {
            (None, Some((kind, id))) => {
                out.text.line(&format!("new file mode {}", mode(kind)));
                out.text.line(&format!("index {zero}..{}", short(id)));
            }
            (Some((kind, id)), None) => {
                out.text.line(&format!("deleted file mode {}", mode(kind)));
                out.text.line(&format!("index {}..{zero}", short(id)));
            }
            (Some((old_kind, a)), Some((new_kind, b))) => {
                if old_kind != new_kind {
                    out.text.line(&format!("old mode {}", mode(old_kind)));
                    out.text.line(&format!("new mode {}", mode(new_kind)));
                    if a != b {
                        out.text.line(&format!("index {}..{}", short(a), short(b)));
                    }
                } else if a != b {
                    out.text.line(&format!(
                        "index {}..{} {}",
                        short(a),
                        short(b),
                        mode(old_kind)
                    ));
                }
                if a == b {
                    return Ok(());
                }
            }
            (None, None) => return Ok(()),
        }
        let before = if old.is_some() {
            format!("a/{path}")
        } else {
            "/dev/null".to_owned()
        };
        let after = if new.is_some() {
            format!("b/{path}")
        } else {
            "/dev/null".to_owned()
        };
        let gitlink = |side: Side| side.is_some_and(|(kind, _)| kind == EntryKind::Commit);
        if gitlink(old) || gitlink(new) {
            out.text.line(&format!("--- {before}"));
            out.text.line(&format!("+++ {after}"));
            let lines: Vec<String> = [(old, '-'), (new, '+')]
                .into_iter()
                .filter_map(|(side, sign)| {
                    side.map(|(_, id)| format!("{sign}Subproject commit {}", id.to_hex()))
                })
                .collect();
            let counts = |side: Side| if side.is_some() { "1" } else { "0,0" };
            out.text
                .line(&format!("@@ -{} +{} @@", counts(old), counts(new)));
            for line in lines {
                out.text.line(&line);
            }
            return Ok(());
        }
        for (_, id) in [old, new].into_iter().flatten() {
            let (_, size) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
            if size > MAX_BLOB {
                out.text.line(&format!(
                    "(not diffed: a side is {size} bytes, more than read_git diffs)"
                ));
                return Ok(());
            }
        }
        let read = |side: Side| -> Result<Vec<u8>, Declined> {
            match side {
                Some((_, id)) => self.object(&id, Kind::Blob),
                None => Ok(Vec::new()),
            }
        };
        let (a, b) = (read(old)?, read(new)?);
        // An empty file added or deleted ends at its index line, as git prints one.
        if a.is_empty() && b.is_empty() {
            return Ok(());
        }
        if is_binary(&a) || is_binary(&b) {
            out.text
                .line(&format!("Binary files {before} and {after} differ"));
            return Ok(());
        }
        out.text.line(&format!("--- {before}"));
        out.text.line(&format!("+++ {after}"));
        let diff = Diff::compute(&escaped(&a), &escaped(&b));
        if !diff.is_exact() {
            out.text.line(&format!(
                "(too different to diff line by line: {} lines removed, {} added)",
                diff.removed(),
                diff.added()
            ));
        } else if diff.is_empty() {
            out.text
                .line("(the two differ only in line endings or in a final newline)");
        } else {
            hunks(out, path, &diff);
        }
        Ok(())
    }
}

/// A tree entry, with its name as UTF-8 when it is. A name that is not cannot be held against the
/// trust map, whose keys are strings, so it is withheld.
struct Entry {
    name: Option<String>,
    kind: EntryKind,
    id: ObjectId,
}

#[derive(PartialEq, Eq)]
struct Queued {
    time: i64,
    order: u64,
    id: ObjectId,
}

impl Ord for Queued {
    /// Newest first, and of two at the same time the one queued first, as git's date order does.
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then_with(|| other.order.cmp(&self.order))
    }
}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A log's walk through history, in date order as git's is. A hidden commit is one the start of a
/// range reaches, and a commit read is one whose parents are known, which is how far marking a
/// commit hidden reaches down at once.
#[derive(Default)]
struct Walk {
    infos: HashMap<ObjectId, Info>,
    hidden: HashSet<ObjectId>,
    /// The starts of a range, which stay relevant to simplifying a merge though hidden.
    bottoms: HashSet<ObjectId>,
    seen: HashSet<ObjectId>,
    queue: BinaryHeap<Queued>,
    order: u64,
    /// Commits that changed nothing under the log's path.
    treesame: HashSet<ObjectId>,
}

impl Walk {
    fn ensure(&mut self, repo: &Repository, id: ObjectId) -> Result<(), Declined> {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.infos.entry(id) {
            slot.insert(repo.info(&id)?);
        }
        Ok(())
    }

    fn enqueue(&mut self, repo: &Repository, id: ObjectId) -> Result<(), Declined> {
        self.ensure(repo, id)?;
        if self.seen.insert(id) {
            self.order += 1;
            self.queue.push(Queued {
                time: self.infos[&id].time,
                order: self.order,
                id,
            });
        }
        Ok(())
    }

    /// Hide a hidden commit's parents and queue them, as git's `process_parents` does for one.
    fn hide_parents_of(&mut self, repo: &Repository, id: ObjectId) -> Result<(), Declined> {
        for parent in self.infos[&id].parents.clone() {
            self.hidden.insert(parent);
            self.ensure(repo, parent)?;
            self.hide_ancestors(self.infos[&parent].parents.clone());
            self.enqueue(repo, parent)?;
        }
        Ok(())
    }

    /// Hide `pending` and, through commits already read, their ancestors, stopping at one already
    /// hidden, as git's `mark_parents_uninteresting` does.
    fn hide_ancestors(&mut self, mut pending: Vec<ObjectId>) {
        while let Some(id) = pending.pop() {
            if !self.hidden.insert(id) {
                continue;
            }
            if let Some(info) = self.infos.get(&id) {
                pending.extend(info.parents.iter().copied());
            }
        }
    }

    /// git's `still_interesting`: how many more hidden commits to take before stopping, where
    /// `date` is the time of the last commit listed.
    fn still_interesting(&self, date: i64, slop: usize) -> usize {
        let Some(head) = self.queue.peek() else {
            return 0;
        };
        if date <= head.time || self.queue.iter().any(|q| !self.hidden.contains(&q.id)) {
            return SLOP;
        }
        slop - 1
    }

    /// Whether a parent counts in simplifying a merge: one the range has not hidden, or its start.
    fn relevant(&self, id: ObjectId) -> bool {
        !self.hidden.contains(&id) || self.bottoms.contains(&id)
    }

    /// Mark `id` as changing nothing under `path`, and narrow its parents to the one it took
    /// `path` from, as git's `try_to_simplify_commit` does with its default history
    /// simplification. A merge the same as a relevant parent follows that parent alone, so a side
    /// whose changes to `path` the merge did not keep is never walked.
    fn simplify(&mut self, repo: &Repository, id: ObjectId, path: &str) -> Result<(), Declined> {
        let (tree, parents) = {
            let info = &self.infos[&id];
            (info.tree, info.parents.clone())
        };
        let mine = repo.entry_at(tree, path)?;
        if parents.is_empty() {
            if mine.is_none() {
                self.treesame.insert(id);
            }
            return Ok(());
        }
        let mut relevant_parents = 0;
        let (mut relevant_change, mut irrelevant_change) = (false, false);
        for parent in parents {
            let relevant = self.relevant(parent);
            if relevant {
                relevant_parents += 1;
            }
            self.ensure(repo, parent)?;
            let theirs = repo.entry_at(self.infos[&parent].tree, path)?;
            if theirs == mine {
                if relevant {
                    if let Some(info) = self.infos.get_mut(&id) {
                        info.parents = vec![parent];
                    }
                    self.treesame.insert(id);
                    return Ok(());
                }
                continue;
            }
            if relevant {
                relevant_change = true;
            } else {
                irrelevant_change = true;
            }
        }
        let changed = if relevant_parents > 0 {
            relevant_change
        } else {
            irrelevant_change
        };
        if !changed {
            self.treesame.insert(id);
        }
        Ok(())
    }
}

struct Out<'w> {
    text: Text,
    shown: Vec<String>,
    printed: Vec<Printed>,
    withheld_any: bool,
    timed_out: bool,
    withheld: &'w dyn Fn(&str) -> bool,
    deadline: Instant,
}

impl Out<'_> {
    /// Whether the deadline has passed, noting that it has.
    fn late(&mut self) -> bool {
        if Instant::now() >= self.deadline {
            self.timed_out = true;
        }
        self.timed_out
    }

    /// Whether another entry fits. One that does not is what makes a full answer a cut one.
    fn room(&mut self) -> bool {
        if self.text.full() {
            self.text.cut = true;
            return false;
        }
        true
    }
}

#[derive(Default)]
struct Text {
    body: String,
    /// The body with each line of a file's contents left blank, so its line numbers are the
    /// body's.
    around: String,
    lines: usize,
    cut: bool,
}

impl Text {
    /// Add a line, shortened past [`MAX_LINE_CHARS`], and say how many of its bytes were shown;
    /// `None` once the answer is full. `content` marks a line of a file's contents.
    fn add(&mut self, line: &str, content: bool) -> Option<usize> {
        if self.lines >= MAX_LINES {
            self.cut = true;
            return None;
        }
        let end = match line.char_indices().nth(MAX_LINE_CHARS) {
            Some((end, _)) => {
                self.cut = true;
                end
            }
            None => line.len(),
        };
        self.body.push_str(&line[..end]);
        if end < line.len() {
            self.body.push_str(" [line cut]");
        }
        self.body.push('\n');
        if !content {
            self.around.push_str(&line[..end]);
        }
        self.around.push('\n');
        self.lines += 1;
        Some(end)
    }

    /// Add a line that is not a file's contents; false once the answer is full.
    fn line(&mut self, line: &str) -> bool {
        self.add(line, false).is_some()
    }

    fn full(&self) -> bool {
        self.lines >= MAX_LINES
    }
}

/// Print a diff's hunks, and record each hunk's shown lines as the file's on each side: the kept
/// and added ones from the new file, the kept and removed ones from the old.
fn hunks(out: &mut Out<'_>, path: &str, diff: &Diff) {
    struct Hunk {
        old_start: usize,
        new_start: usize,
        old_count: usize,
        new_count: usize,
        lines: Vec<(char, String)>,
    }
    fn range(start: usize, count: usize) -> String {
        match count {
            0 => format!("{},0", start.saturating_sub(1)),
            1 => start.to_string(),
            n => format!("{start},{n}"),
        }
    }
    fn flush(out: &mut Out<'_>, path: &str, hunk: &mut Option<Hunk>) {
        let Some(h) = hunk.take() else {
            return;
        };
        if !out.text.line(&format!(
            "@@ -{} +{} @@",
            range(h.old_start, h.old_count),
            range(h.new_start, h.new_count)
        )) {
            return;
        }
        let (mut old, mut new) = (String::new(), String::new());
        for (sign, line) in &h.lines {
            let Some(end) = out.text.add(&format!("{sign}{line}"), true) else {
                break;
            };
            let shown = &line[..end - sign.len_utf8()];
            for (side, left_out) in [(&mut old, '+'), (&mut new, '-')] {
                if *sign != left_out {
                    side.push_str(shown);
                    side.push('\n');
                }
            }
        }
        for (first_line, text) in [(h.new_start, new), (h.old_start, old)] {
            if !text.is_empty() {
                out.printed.push(Printed {
                    path: path.to_owned(),
                    first_line,
                    text,
                });
            }
        }
    }
    let (mut old_line, mut new_line) = (1usize, 1usize);
    let mut hunk: Option<Hunk> = None;
    for change in diff.condensed(3) {
        if let Change::Elided(n) = change {
            flush(out, path, &mut hunk);
            old_line += n;
            new_line += n;
            continue;
        }
        let h = hunk.get_or_insert_with(|| Hunk {
            old_start: old_line,
            new_start: new_line,
            old_count: 0,
            new_count: 0,
            lines: Vec::new(),
        });
        match change {
            Change::Kept(line) => {
                h.lines.push((' ', line.to_string()));
                h.old_count += 1;
                h.new_count += 1;
                old_line += 1;
                new_line += 1;
            }
            Change::Removed(line) => {
                h.lines.push(('-', line.to_string()));
                h.old_count += 1;
                old_line += 1;
            }
            Change::Added(line) => {
                h.lines.push(('+', line.to_string()));
                h.new_count += 1;
                new_line += 1;
            }
            Change::Elided(_) => {}
        }
    }
    flush(out, path, &mut hunk);
}

/// Bytes as text, with each byte that is not part of UTF-8 written as `\xNN` rather than all of
/// them replaced by one character, so two lines that differ only there still differ.
fn escaped(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => {
            let mut text = String::with_capacity(bytes.len());
            for chunk in bytes.utf8_chunks() {
                text.push_str(chunk.valid());
                for byte in chunk.invalid() {
                    let _ = write!(text, "\\x{byte:02x}");
                }
            }
            Cow::Owned(text)
        }
    }
}

fn message(out: &mut Out<'_>, message: &[u8]) {
    let text = String::from_utf8_lossy(message);
    for line in text.trim_end().lines() {
        if !out.text.line(&format!("    {line}")) {
            return;
        }
    }
}

/// Whether `path` is inside the filter, or a directory the filter is inside of.
fn in_scope(path: &str, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
        || filter
            .strip_prefix(path)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE)].contains(&0)
}

/// Which kinds a diff can compare line by line; a change between two of these is a delete and an
/// add, as git prints a type change.
fn family(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Blob | EntryKind::BlobExecutable => 0,
        EntryKind::Link => 1,
        EntryKind::Commit => 2,
        EntryKind::Tree => 3,
    }
}

fn mode(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Tree => "040000",
        EntryKind::Blob => "100644",
        EntryKind::BlobExecutable => "100755",
        EntryKind::Link => "120000",
        EntryKind::Commit => "160000",
    }
}

fn entry_word(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Tree => "tree",
        EntryKind::Commit => "commit",
        _ => "blob",
    }
}

fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Commit => "commit",
        Kind::Tree => "tree",
        Kind::Blob => "blob",
        Kind::Tag => "tag",
    }
}

/// A field shown on one line, with anything that would break the line replaced.
fn one_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

/// A signature's time and zone, or its time in UTC when the zone does not parse.
fn when(time: Option<gix_object::date::Time>, seconds: i64) -> (i64, i32) {
    time.map_or((seconds, 0), |time| (time.seconds, time.offset))
}

/// `YYYY-MM-DD` in the signature's own offset, as `git log --date=short` prints it.
fn day(seconds: i64, offset: i32) -> String {
    let (y, m, d) = civil_from_days((seconds + i64::from(offset)).div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DD HH:MM:SS +zzzz` in the signature's own offset.
fn stamp(seconds: i64, offset: i32) -> String {
    let local = seconds + i64::from(offset);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    let within = local.rem_euclid(86_400);
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} {sign}{:02}{:02}",
        within / 3600,
        within % 3600 / 60,
        within % 60,
        minutes / 60,
        minutes % 60
    )
}

/// The civil date of a day counted from 1970-01-01, by Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The start of a `YYYY-MM-DD` day in UTC, in seconds since the epoch.
pub fn parse_day(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    if !shaped {
        return None;
    }
    let y: i64 = text[..4].parse().ok()?;
    let m: u32 = text[5..7].parse().ok()?;
    let d: u32 = text[8..].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    let (next_y, next_m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let length = days_from_civil(next_y, next_m, 1) - days_from_civil(y, m, 1);
    if i64::from(d) > length {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gix_object::Write as _;

    use super::*;

    /// 2023-11-14 22:13:20 UTC.
    const T1: i64 = 1_700_000_000;
    const DAY: i64 = 86_400;

    /// A repository built object by object, so no test needs git installed.
    struct Repo {
        root: PathBuf,
        git: PathBuf,
        loose: gix_odb::loose::Store,
    }

    impl Repo {
        fn new(name: &str) -> Repo {
            let root = crate::testutil::scratch_dir(&format!(
                "bravebot-git-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            let git = root.join(".git");
            for dir in ["objects", "refs/heads", "refs/tags"] {
                std::fs::create_dir_all(git.join(dir)).expect("scratch repository");
            }
            std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
            std::fs::write(
                git.join("config"),
                "[core]\n\trepositoryformatversion = 0\n",
            )
            .expect("config");
            let loose = gix_odb::loose::Store::at(git.join("objects"), HashKind::Sha1);
            Repo { root, git, loose }
        }

        fn object(&self, kind: Kind, bytes: &[u8]) -> ObjectId {
            self.loose.write_buf(kind, bytes).expect("object written")
        }

        fn blob(&self, text: &str) -> ObjectId {
            self.object(Kind::Blob, text.as_bytes())
        }

        /// A tree of `(mode, name, id)` entries, in any order.
        fn tree(&self, entries: &[(&str, &str, ObjectId)]) -> ObjectId {
            self.object(Kind::Tree, &tree_bytes(entries))
        }

        fn commit(
            &self,
            tree: ObjectId,
            parents: &[ObjectId],
            time: i64,
            message: &str,
        ) -> ObjectId {
            self.object(Kind::Commit, &commit_bytes(tree, parents, time, message))
        }

        /// Write `text` to `name` under `.git`.
        fn put(&self, name: &str, text: &str) {
            let path = self.git.join(name);
            std::fs::create_dir_all(path.parent().expect("inside .git")).expect("directory");
            std::fs::write(path, text).expect("written");
        }

        fn point(&self, name: &str, id: ObjectId) {
            self.put(name, &format!("{}\n", id.to_hex()));
        }

        fn opened(&self) -> Result<Repository, Declined> {
            survey(&self.git, later())?;
            Repository::open(&self.git)
        }

        fn ask(
            &self,
            query: Query,
            revision: Option<&str>,
            path: Option<&str>,
        ) -> Result<Answer, Declined> {
            self.opened()?
                .answer(&request(query, revision, path), &|_| false)
        }

        fn text(&self, query: Query, revision: &str) -> String {
            self.ask(query, Some(revision), None)
                .expect("answered")
                .text
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    fn request<'a>(query: Query, revision: Option<&'a str>, path: Option<&'a str>) -> Request<'a> {
        Request {
            query,
            revision,
            path,
            count: DEFAULT_COUNT,
            since: None,
            until: None,
            deadline: later(),
        }
    }

    fn tree_bytes(entries: &[(&str, &str, ObjectId)]) -> Vec<u8> {
        let mut sorted = entries.to_vec();
        sorted.sort_by_key(|(mode, name, _)| {
            let mut key = name.as_bytes().to_vec();
            if *mode == "40000" {
                key.push(b'/');
            }
            key
        });
        let mut out = Vec::new();
        for (mode, name, id) in sorted {
            out.extend_from_slice(format!("{mode} {name}\0").as_bytes());
            out.extend_from_slice(id.as_bytes());
        }
        out
    }

    fn commit_bytes(tree: ObjectId, parents: &[ObjectId], time: i64, message: &str) -> Vec<u8> {
        let mut text = format!("tree {}\n", tree.to_hex());
        for parent in parents {
            text.push_str(&format!("parent {}\n", parent.to_hex()));
        }
        text.push_str(&format!(
            "author A U Thor <author@example.com> {time} +0000\n\
             committer C O Mitter <committer@example.com> {time} +0000\n\n{message}\n"
        ));
        text.into_bytes()
    }

    fn short(id: ObjectId, len: usize) -> String {
        id.to_hex_with_len(len).to_string()
    }

    /// Three commits a day apart on `main`: README added, then `src/lib.rs`, then README changed.
    struct History {
        repo: Repo,
        first: ObjectId,
        second: ObjectId,
        third: ObjectId,
        readme: ObjectId,
        changed: ObjectId,
        lib: ObjectId,
        src: ObjectId,
    }

    fn history(name: &str) -> History {
        let repo = Repo::new(name);
        let readme = repo.blob("hello\n");
        let first = repo.commit(repo.tree(&[("100644", "README", readme)]), &[], T1, "first");
        let lib = repo.blob("pub fn f() {}\n");
        let src = repo.tree(&[("100644", "lib.rs", lib)]);
        let second = repo.commit(
            repo.tree(&[("100644", "README", readme), ("40000", "src", src)]),
            &[first],
            T1 + DAY,
            "second",
        );
        let changed = repo.blob("hello\nworld\n");
        let third = repo.commit(
            repo.tree(&[("100644", "README", changed), ("40000", "src", src)]),
            &[second],
            T1 + 2 * DAY,
            "third",
        );
        repo.point("refs/heads/main", third);
        History {
            repo,
            first,
            second,
            third,
            readme,
            changed,
            lib,
            src,
        }
    }

    fn relative(git: &Path, files: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = files
            .iter()
            .map(|f| {
                f.strip_prefix(git)
                    .expect("under .git")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        names.sort();
        names
    }

    /// The workspace holds each file this lists against the deny rules and the trust map, and
    /// holds nothing else: a file a read opens that is missing here is read unchecked, and a file
    /// listed that no read opens closes a repository to a rule about something read_git never
    /// shows.
    #[test]
    fn survey_lists_the_files_a_read_opens_and_none_it_does_not() {
        let h = history("survey");
        h.repo.put(
            "packed-refs",
            &format!(
                "# pack-refs with: peeled\n{} refs/tags/old\n",
                h.first.to_hex()
            ),
        );
        h.repo.point("FETCH_HEAD", h.second);
        h.repo.point("ORIG_HEAD", h.second);
        h.repo.put("objects/pack/pack-a.idx", "an index\n");
        h.repo.put("objects/pack/pack-a.pack", "its pack\n");
        let unread = [
            "hooks/pre-commit",
            "index",
            "description",
            "info/exclude",
            "logs/HEAD",
            "COMMIT_EDITMSG.lock",
            "refs/heads/main.lock",
            "refs/heads/.hidden",
            "objects/info/packs",
            "objects/pack/orphan.idx",
            "objects/pack/pack-a.keep",
            "objects/ab/not-an-object",
        ];
        for name in unread {
            h.repo.put(name, "not a ref\n");
        }

        let listed = relative(
            &h.repo.git,
            &survey(&h.repo.git, later()).expect("surveyed"),
        );

        for read in [
            "HEAD",
            "FETCH_HEAD",
            "ORIG_HEAD",
            "config",
            "packed-refs",
            "refs/heads/main",
            "objects/pack/pack-a.idx",
            "objects/pack/pack-a.pack",
        ] {
            assert!(
                listed.iter().any(|f| f == read),
                "{read} is read but not listed"
            );
        }
        let objects = listed.iter().filter(|f| f.starts_with("objects/")).count();
        assert_eq!(
            objects, 12,
            "every loose object and paired pack is listed: {listed:?}"
        );
        for unread in unread {
            assert!(
                !listed.iter().any(|f| f == unread),
                "{unread} is never read but is listed"
            );
        }
        // A tree of refs too large to list in time is declined rather than listed in part.
        assert_eq!(survey(&h.repo.git, Instant::now()), Err(Declined::TooSlow));
    }

    /// Each of these sends a read to files outside the ones [`survey`] listed, or answers with
    /// objects other than the ones stored, so a repository laid out this way is declined before
    /// anything in it is decoded.
    #[test]
    fn a_repository_whose_files_send_a_read_elsewhere_is_declined() {
        let file = Repo::new("declined-git-file");
        std::fs::remove_dir_all(&file.git).expect("removed");
        std::fs::write(&file.git, "gitdir: /elsewhere/.git\n").expect("git file");
        assert_eq!(survey(&file.git, later()), Err(Declined::LinkedGitDir));

        let cases: [(&str, &str, Declined); 4] = [
            ("commondir", "../other/.git\n", Declined::CommonDir),
            (
                "objects/info/alternates",
                "/other/.git/objects\n",
                Declined::Alternates,
            ),
            (
                "info/grafts",
                "0000000000000000000000000000000000000000\n",
                Declined::Replaced,
            ),
            (
                "refs/replace/0000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000\n",
                Declined::Replaced,
            ),
        ];
        for (name, text, declined) in cases {
            let h = history(&format!("declined-{}", name.replace('/', "-")));
            h.repo.put(name, text);
            assert_eq!(
                h.repo.ask(Query::Log, None, None).map(|a| a.text),
                Err(declined),
                "{name}"
            );
        }

        let packed = history("declined-packed-replace");
        packed.repo.put(
            "packed-refs",
            &format!(
                "# pack-refs with: peeled\n{} refs/replace/{}\n",
                packed.first.to_hex(),
                packed.third.to_hex()
            ),
        );
        assert_eq!(
            packed.repo.ask(Query::Log, None, None).map(|a| a.text),
            Err(Declined::Replaced)
        );
    }

    /// A link is how a trusted directory reaches an untrusted one, and the trust map judges the
    /// link's own path, so a link anywhere a read goes is declined rather than followed.
    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_where_a_read_goes_is_declined_rather_than_followed() {
        let h = history("linked-ref");
        std::os::unix::fs::symlink(
            h.repo.git.join("refs/heads/main"),
            h.repo.git.join("refs/heads/alias"),
        )
        .expect("link");
        assert_eq!(survey(&h.repo.git, later()), Err(Declined::Linked));

        let top = history("linked-top");
        std::os::unix::fs::symlink(top.repo.git.join("HEAD"), top.repo.git.join("ORIG_HEAD"))
            .expect("link");
        assert_eq!(survey(&top.repo.git, later()), Err(Declined::Linked));

        let whole = history("linked-whole");
        let moved = whole.repo.root.join("real-git");
        std::fs::rename(&whole.repo.git, &moved).expect("moved");
        std::os::unix::fs::symlink(&moved, &whole.repo.git).expect("link");
        assert_eq!(survey(&whole.repo.git, later()), Err(Declined::Linked));
    }

    /// git reads these keys to decide what a repository means: another file's configuration, a
    /// working tree elsewhere, an object or ref format this reader does not decode. Answering
    /// past any of them answers a different question from the one git would.
    #[test]
    fn configuration_that_changes_what_a_read_means_is_declined() {
        let cases: [(&[u8], Declined); 11] = [
            (b"[include]\n\tpath = other\n", Declined::Include),
            (b"[Include]\n\tPath = other\n", Declined::Include),
            (
                b"[includeIf \"gitdir:~/work/\"]\n\tpath = other\n",
                Declined::Include,
            ),
            (b"[core]\n\tworktree = /elsewhere\n", Declined::Worktree),
            (b"[core] worktree = /elsewhere\n", Declined::Worktree),
            (b"[core]\n\trepositoryformatversion = 2\n", Declined::Format),
            (b"[extensions]\n\tobjectFormat = sha256\n", Declined::Format),
            (b"[extensions]\n\trefStorage = reftable\n", Declined::Format),
            (b"[extensions]\n\tsomethingNew = true\n", Declined::Format),
            (b"[core\n\tbare = false\n", Declined::Format),
            (b"[core]\n\tname = \"never closed\n", Declined::Format),
        ];
        for (config, declined) in cases {
            assert_eq!(
                check_config(config),
                Err(declined),
                "{}",
                String::from_utf8_lossy(config)
            );
        }
    }

    /// A configuration git reads without changing what history means is read, whatever its
    /// comments, quoting and line endings, and a continued line is read as the rest of its
    /// value rather than as the section it looks like: a parser that did either would decline
    /// most real repositories or find a key git never sees.
    #[test]
    fn configuration_git_reads_to_the_same_meaning_is_accepted() {
        let config = b"\xef\xbb\xbf# a comment\r\n\
            [core]\r\n\
            \trepositoryformatversion = 1\r\n\
            \tbare = false ; a trailing comment\r\n\
            \tfilemode\r\n\
            [remote \"or\\\"igin\"]\n\
            \turl = https://example.com/repo.git\n\
            \tfetch = +refs/heads/*:refs/remotes/origin/*\n\
            [alias]\n\
            \tlast = \"log -1 # not a comment\"\n\
            \tlong = one \\\n[include] path = continued\n\
            [core.ignored]\n\
            \tworktree = a subsection, not core\n\
            [extensions]\n\
            \tworktreeConfig = true\n\
            \tobjectFormat = sha1\n";
        assert_eq!(check_config(config), Ok(()));
    }

    /// A ref under `worktrees/` or `main-worktree/` is read from another worktree's directory,
    /// which [`survey`] never lists, so neither a name the planner wrote nor a symbolic ref
    /// the repository holds may reach one.
    #[test]
    fn a_ref_in_another_worktrees_directory_is_never_read() {
        let h = history("worktree-ref");
        h.repo.point("worktrees/other/HEAD", h.first);

        assert_eq!(
            h.repo
                .ask(Query::Log, Some("worktrees/other/HEAD"), None)
                .map(|a| a.text),
            Err(Declined::Unknown("worktrees/other/HEAD".to_owned()))
        );

        h.repo.put("HEAD", "ref: worktrees/other/HEAD\n");
        assert_eq!(
            h.repo.ask(Query::Log, None, None).map(|a| a.text),
            Err(Declined::Unreadable)
        );
    }

    /// A revision form read_git does not implement would otherwise be read as a form it does:
    /// `A...B` as the range from `A.` to `.B`, `HEAD^@` as `HEAD^`. Refusing it by name tells
    /// the planner to ask git instead.
    #[test]
    fn revision_syntax_read_git_does_not_implement_is_refused_by_name() {
        for revision in [
            "main...topic",
            "@{-1}",
            "main@{upstream}",
            "HEAD^{/fix}",
            "HEAD^@",
            "HEAD^!",
            "HEAD^-",
            ":README",
            ":/fix",
            "--all",
            "-n",
            "a b c",
            "a..b:README",
            "a b:README",
            "a..b c",
        ] {
            assert!(
                matches!(spec(revision), Err(Declined::Unsupported(ref text, _)) if text == revision),
                "{revision} was read as {:?}",
                spec(revision)
            );
        }
    }

    /// An end left off a range is HEAD, as git reads it, and two words are the two sides of a
    /// diff.
    #[test]
    fn ranges_and_pairs_are_read_as_git_reads_them() {
        assert_eq!(spec("a..b"), Ok(Spec::Range("a", "b")));
        assert_eq!(spec("..b"), Ok(Spec::Range("HEAD", "b")));
        assert_eq!(spec("a.."), Ok(Spec::Range("a", "HEAD")));
        assert_eq!(spec("  a   b "), Ok(Spec::Pair("a", "b")));
        assert_eq!(
            spec("HEAD~2:src/lib.rs"),
            Ok(Spec::One("HEAD~2:src/lib.rs"))
        );
        assert_eq!(spec(" "), Err(Declined::Unknown(" ".to_owned())));
    }

    /// What follows the colon is a file's name, and a name may hold what would be revision syntax
    /// anywhere else: a space, two or three dots, `@{`.
    #[test]
    fn a_path_after_a_revision_is_read_as_a_name_whatever_it_holds() {
        let repo = Repo::new("path-names");
        let blob = repo.blob("named\n");
        let docs = repo.tree(&[("100644", "a b.md", blob)]);
        let commit = repo.commit(
            repo.tree(&[
                ("40000", "docs", docs),
                ("100644", "notes...md", blob),
                ("100644", "x@{y}", blob),
            ]),
            &[],
            T1,
            "names",
        );
        repo.point("refs/heads/main", commit);
        for text in ["HEAD:docs/a b.md", "HEAD:notes...md", "HEAD:x@{y}"] {
            assert_eq!(spec(text), Ok(Spec::One(text)), "{text}");
            assert_eq!(path_in(text), text.strip_prefix("HEAD:"), "{text}");
            assert_eq!(repo.text(Query::Show, text), "named\n", "{text}");
        }
    }

    /// The trust map is keyed by paths as written, so a path that names the same file another
    /// way, through `..`, `.` or a doubled slash, would be checked under a key it does not
    /// read.
    #[test]
    fn a_path_not_plainly_inside_the_repository_is_refused() {
        for (path, cleaned) in [
            ("src", "src"),
            ("src/", "src"),
            (".", ""),
            ("", ""),
            ("a/b.rs", "a/b.rs"),
        ] {
            assert_eq!(clean(path), Ok(cleaned.to_owned()), "{path}");
        }
        for path in [
            "../x",
            "src/../../x",
            "./src",
            "/etc/passwd",
            "src//lib.rs",
            "/",
            "src/./a",
            "a\0b",
        ] {
            assert_eq!(
                clean(path),
                Err(Declined::PathInvalid(path.to_owned())),
                "{path}"
            );
        }
    }

    fn branchy(name: &str) -> (History, ObjectId, ObjectId, ObjectId) {
        let h = history(name);
        let side_readme = h.repo.blob("hello\nside\n");
        let side = h.repo.commit(
            h.repo
                .tree(&[("100644", "README", side_readme), ("40000", "src", h.src)]),
            &[h.second],
            T1 + DAY + 3600,
            "side",
        );
        let merged = h.repo.blob("hello\nworld\nside\n");
        let merge = h.repo.commit(
            h.repo
                .tree(&[("100644", "README", merged), ("40000", "src", h.src)]),
            &[h.third, side],
            T1 + 3 * DAY,
            "merge",
        );
        let tag = h.repo.object(
            Kind::Tag,
            format!(
                "object {}\ntype commit\ntag v1\ntagger T A Gger <tagger@example.com> {} +0000\n\n\
                 release one\n",
                h.third.to_hex(),
                T1 + 2 * DAY
            )
            .as_bytes(),
        );
        h.repo.point("refs/heads/main", merge);
        h.repo.point("refs/tags/v1", tag);
        (h, side, merge, tag)
    }

    fn resolved(repository: &Repository, text: &str) -> Result<ObjectId, Declined> {
        repository
            .resolve(text, Instant::now() + Duration::from_secs(60), &|_| false)
            .map(|r| r.id)
    }

    /// A planner writes revisions the way git's documentation does, so each form read_git
    /// accepts has to land on the commit git would pick: a wrong parent here is a wrong answer
    /// shown as a right one.
    #[test]
    fn revisions_resolve_through_names_ancestry_and_peels_as_git_does() {
        let (h, side, merge, tag) = branchy("resolve");
        let repository = h.repo.opened().expect("opened");
        let full = h.third.to_hex().to_string();
        for (text, want) in [
            ("HEAD", merge),
            ("@", merge),
            ("main", merge),
            ("heads/main", merge),
            ("refs/heads/main", merge),
            ("HEAD^0", merge),
            ("HEAD~1", h.third),
            ("HEAD^", h.third),
            ("HEAD^1", h.third),
            ("HEAD^2", side),
            ("HEAD^^", h.second),
            ("HEAD~3", h.first),
            ("HEAD^2~1", h.second),
            ("v1", tag),
            ("tags/v1", tag),
            ("v1^{}", h.third),
            ("v1^{commit}", h.third),
            ("v1~1", h.second),
            (&full[..7], h.third),
            (&full, h.third),
        ] {
            assert_eq!(resolved(&repository, text), Ok(want), "{text}");
        }
        assert_eq!(
            resolved(&repository, &full.to_ascii_uppercase()),
            Ok(h.third),
            "a full id in capitals"
        );
        assert_eq!(
            resolved(&repository, &full[..7].to_ascii_uppercase()),
            Ok(h.third),
            "a prefix in capitals"
        );
        for missing in ["HEAD~9", "HEAD^3", "nosuch", "v2"] {
            assert_eq!(
                resolved(&repository, missing),
                Err(Declined::Unknown(missing.to_owned()))
            );
        }
        assert_eq!(
            resolved(&repository, "HEAD^{blob}"),
            Err(Declined::Kind {
                revision: "HEAD^{blob}".to_owned(),
                wanted: "blob"
            })
        );
        assert!(matches!(
            resolved(&repository, "HEAD^{object}"),
            Err(Declined::Unsupported(..))
        ));
    }

    /// One commit per line, newest first, carries what a planner picks a commit by, in the
    /// shape `git log --format='%h %ad %an %s' --date=short --abbrev=10` prints. Across a merge,
    /// newest first interleaves the two lines of history by date rather than walking one of them
    /// to its end.
    #[test]
    fn log_lists_commits_newest_first_one_line_each() {
        let (h, side, merge, _) = branchy("log");
        assert_eq!(
            h.repo.ask(Query::Log, None, None).expect("answered").text,
            format!(
                "{} 2023-11-17 A U Thor merge\n{} 2023-11-16 A U Thor third\n{} 2023-11-15 A U \
                 Thor side\n{} 2023-11-15 A U Thor second\n{} 2023-11-14 A U Thor first\n",
                short(merge, 10),
                short(h.third, 10),
                short(side, 10),
                short(h.second, 10),
                short(h.first, 10)
            )
        );
    }

    /// `A..B` is what B has that A lacks, which is how a planner asks what a branch adds: a
    /// range that listed A's history too would answer that it adds everything.
    #[test]
    fn a_range_leaves_out_what_its_start_already_reaches() {
        let (h, side, merge, _) = branchy("range");
        let listed = |revision: &str| -> Vec<String> {
            h.repo
                .text(Query::Log, revision)
                .lines()
                .map(|line| line[..10].to_owned())
                .collect()
        };
        assert_eq!(
            listed(&format!("{}..HEAD", h.first.to_hex())),
            [merge, h.third, side, h.second].map(|id| short(id, 10))
        );
        assert_eq!(
            listed("main~1..main"),
            [merge, side].map(|id| short(id, 10))
        );
        assert_eq!(
            h.repo.text(Query::Log, "HEAD..HEAD~1"),
            "(no commits match)\n"
        );
    }

    /// A commit's time is whatever its author's clock said, so a parent can be dated after its
    /// child. The start of a range still hides it, whether that is known when the walk first meets
    /// it or only once the walk reaches it again through the start.
    #[test]
    fn a_range_leaves_out_its_start_whatever_the_commit_times() {
        let repo = Repo::new("range-times");
        let tree = repo.tree(&[("100644", "README", repo.blob("hello\n"))]);
        let root = repo.commit(tree, &[], T1, "root");
        let late = repo.commit(tree, &[root], T1 + 3 * DAY, "late");
        let early = repo.commit(tree, &[late], T1 + DAY / 2, "early");
        let start = repo.commit(tree, &[late], T1 + DAY, "start");
        let behind = repo.commit(tree, &[early], T1 + DAY, "behind");
        let end = repo.commit(tree, &[late], T1 + 2 * DAY, "end");
        repo.point("refs/heads/main", end);
        for from in [start, behind] {
            let range = format!("{}..{}", from.to_hex(), end.to_hex());
            let listed: Vec<String> = repo
                .text(Query::Log, &range)
                .lines()
                .map(|line| line[..10].to_owned())
                .collect();
            assert_eq!(listed, [short(end, 10)], "{range}");
        }
    }

    /// The bounds are whole days in UTC, both ends included, which is what a planner asking for
    /// "since the 15th" means.
    #[test]
    fn since_and_until_bound_the_log_by_whole_days() {
        let h = history("dates");
        let day = |text| parse_day(text).expect("a day");
        let listed = |since: Option<i64>, until: Option<i64>| -> Vec<String> {
            let repository = h.repo.opened().expect("opened");
            let mut asked = request(Query::Log, None, None);
            asked.since = since;
            asked.until = until;
            repository
                .answer(&asked, &|_| false)
                .expect("answered")
                .text
                .lines()
                .map(|line| line[..10].to_owned())
                .collect()
        };
        assert_eq!(
            listed(Some(day("2023-11-15")), None),
            [h.third, h.second].map(|id| short(id, 10))
        );
        assert_eq!(
            listed(None, Some(day("2023-11-15") + DAY - 1)),
            [h.second, h.first].map(|id| short(id, 10))
        );
        assert_eq!(
            listed(Some(day("2023-11-15")), Some(day("2023-11-15") + DAY - 1)),
            [h.second].map(|id| short(id, 10))
        );
    }

    /// A path narrows the log to the commits that changed something at or under it, as
    /// `git log -- <path>` does; listing every commit would bury the ones asked about.
    #[test]
    fn a_path_keeps_only_the_commits_that_changed_it() {
        let h = history("log-path");
        let listed = |path| -> Vec<String> {
            h.repo
                .ask(Query::Log, None, Some(path))
                .expect("answered")
                .text
                .lines()
                .map(|line| line[..10].to_owned())
                .collect()
        };
        assert_eq!(listed("src"), [short(h.second, 10)]);
        assert_eq!(listed("src/lib.rs"), [short(h.second, 10)]);
        assert_eq!(listed("README"), [h.third, h.first].map(|id| short(id, 10)));
        assert_eq!(
            h.repo
                .ask(Query::Log, None, Some("nowhere"))
                .expect("answered")
                .text,
            "(no commits match)\n"
        );
    }

    /// Narrowed to a path, a merge that kept one side's version of it is followed down that side
    /// alone, as git's default simplification does: the other side's commits changed a version
    /// the merge threw away. A merge that kept neither is listed, and both sides with it.
    #[test]
    fn a_log_narrowed_to_a_path_follows_the_side_a_merge_kept_it_from() {
        let repo = Repo::new("log-merge-path");
        let base = ("100644", "base", repo.blob("base\n"));
        let with = |text: &str| repo.tree(&[base, ("100644", "README", repo.blob(text))]);
        let root = repo.commit(repo.tree(&[base]), &[], T1, "root");
        let x = repo.commit(with("x\n"), &[root], T1 + DAY, "x");
        let y = repo.commit(with("y\n"), &[x], T1 + 2 * DAY, "y");
        let z = repo.commit(with("z\n"), &[root], T1 + 3 * DAY, "z");
        let log = |parents: &[ObjectId], text: &str| {
            let merge = repo.commit(with(text), parents, T1 + 4 * DAY, "merge");
            repo.point("refs/heads/main", merge);
            let listed: Vec<String> = repo
                .ask(Query::Log, None, Some("README"))
                .expect("answered")
                .text
                .lines()
                .map(|line| line[..10].to_owned())
                .collect();
            (merge, listed)
        };
        assert_eq!(log(&[z, y], "z\n").1, [short(z, 10)]);
        assert_eq!(log(&[y, z], "z\n").1, [short(z, 10)]);
        let (merge, listed) = log(&[z, y], "w\n");
        assert_eq!(listed, [merge, z, y, x].map(|id| short(id, 10)));
    }

    /// A log stops at its count, and says it stopped, so the planner can tell "these are all
    /// the commits" from "these are the first of them".
    #[test]
    fn a_log_that_reaches_its_count_stops_and_says_it_was_cut() {
        let h = history("count");
        let repository = h.repo.opened().expect("opened");
        let mut asked = request(Query::Log, None, None);
        asked.count = 2;
        let answer = repository.answer(&asked, &|_| false).expect("answered");
        assert_eq!(answer.text.lines().count(), 2);
        assert!(answer.cut);

        asked.count = 3;
        let answer = repository.answer(&asked, &|_| false).expect("answered");
        assert_eq!(answer.text.lines().count(), 3);
        assert!(!answer.cut, "every commit was shown");
    }

    /// `show` is how a planner reads what one commit did, so it prints the commit and its
    /// change in the shape `git show` does, a root commit as files added.
    #[test]
    fn show_prints_a_commit_its_message_and_its_diff_as_git_does() {
        let h = history("show");
        assert_eq!(
            h.repo.text(Query::Show, "HEAD"),
            format!(
                "commit {}\nAuthor: A U Thor <author@example.com>\nDate:   2023-11-16 22:13:20 \
                 +0000\n\n    third\n\ndiff --git a/README b/README\nindex {}..{} 100644\n--- \
                 a/README\n+++ b/README\n@@ -1 +1,2 @@\n hello\n+world\n",
                h.third.to_hex(),
                short(h.readme, 7),
                short(h.changed, 7)
            )
        );
        assert_eq!(
            h.repo.text(Query::Show, "HEAD~2"),
            format!(
                "commit {}\nAuthor: A U Thor <author@example.com>\nDate:   2023-11-14 22:13:20 \
                 +0000\n\n    first\n\ndiff --git a/README b/README\nnew file mode 100644\nindex \
                 0000000..{}\n--- /dev/null\n+++ b/README\n@@ -0,0 +1 @@\n+hello\n",
                h.first.to_hex(),
                short(h.readme, 7)
            )
        );
    }

    /// `<revision>:<path>` reads a directory or a file as it stood then, and the answer names
    /// every path it showed so the workspace can label it by them.
    #[test]
    fn show_lists_a_directory_and_prints_a_file_at_a_revision() {
        let h = history("show-path");
        let root = h
            .repo
            .ask(Query::Show, Some("HEAD:"), None)
            .expect("answered");
        assert_eq!(
            root.text,
            format!(
                "100644 blob {}\tREADME\n040000 tree {}\tsrc/\n",
                short(h.changed, 10),
                short(h.src, 10)
            )
        );
        assert_eq!(root.shown, ["", "README", "src"]);
        assert_eq!(
            h.repo.text(Query::Show, "HEAD:src"),
            format!("100644 blob {}\tlib.rs\n", short(h.lib, 10))
        );
        assert_eq!(h.repo.text(Query::Show, "HEAD^{tree}"), root.text);
        assert_eq!(h.repo.text(Query::Show, "HEAD:README"), "hello\nworld\n");
        let old = h
            .repo
            .ask(Query::Show, Some("HEAD~2:README"), None)
            .expect("answered");
        assert_eq!(
            (old.text.as_str(), old.shown.as_slice()),
            ("hello\n", ["README".to_owned()].as_slice())
        );
        assert_eq!(
            h.repo
                .ask(Query::Show, Some("HEAD~2:src"), None)
                .map(|a| a.text),
            Err(Declined::NoSuchPath("src".to_owned()))
        );
    }

    /// An object named by its id alone has no path, and a path is what the trust map and the
    /// deny rules are held against: showing it would show a file no rule was asked about.
    #[test]
    fn a_tree_or_file_named_by_its_id_alone_is_refused() {
        let h = history("show-by-id");
        for id in [h.lib, h.src] {
            let text = id.to_hex().to_string();
            assert_eq!(
                h.repo.ask(Query::Show, Some(&text), None).map(|a| a.text),
                Err(Declined::ShowNeedsPath(text.clone()))
            );
        }
    }

    /// A path the trust map withholds is left out of whatever would show it, a diff, a listing
    /// or a log narrowed to it, and the answer says something was left out: a read of history
    /// must not be a way round a deny rule on the file it is history of.
    #[test]
    fn a_withheld_path_is_left_out_of_every_answer_and_the_answer_says_so() {
        let h = history("withheld");
        let repository = h.repo.opened().expect("opened");
        let withheld = |path: &str| path == "src" || path.starts_with("src/");
        let ask =
            |query, revision, path| repository.answer(&request(query, revision, path), &withheld);

        for (query, revision) in [
            (Query::Show, "HEAD~1"),
            (Query::Show, "HEAD:"),
            (Query::Diff, "HEAD~2 HEAD"),
        ] {
            let answer = ask(query, Some(revision), None).expect("answered");
            assert!(
                !answer.text.contains("lib.rs"),
                "{revision}: {}",
                answer.text
            );
            assert!(!answer.text.contains("src"), "{revision}: {}", answer.text);
            assert!(
                answer.withheld,
                "{revision} does not say a path was left out"
            );
            assert!(
                answer.shown.iter().all(|path| !withheld(path)),
                "{revision} reports showing {:?}",
                answer.shown
            );
        }
        for revision in ["HEAD:src/lib.rs", "HEAD:src"] {
            let path = path_in(revision).expect("a path");
            assert_eq!(
                ask(Query::Show, Some(revision), None).map(|a| a.text),
                Err(Declined::Withheld(path.to_owned()))
            );
        }
        assert_eq!(
            ask(Query::Log, None, Some("src/")).map(|a| a.text),
            Err(Declined::Withheld("src".to_owned()))
        );
    }

    /// A name that is not UTF-8 cannot be held against the trust map, whose keys are strings,
    /// so it is left out the way a withheld one is.
    #[test]
    fn a_path_whose_name_is_not_utf8_is_left_out_as_withheld() {
        let repo = Repo::new("not-utf8");
        let blob = repo.blob("hidden\n");
        let mut tree = b"100644 \xffname\0".to_vec();
        tree.extend_from_slice(blob.as_bytes());
        let tree = repo.object(Kind::Tree, &tree);
        repo.point("refs/heads/main", repo.commit(tree, &[], T1, "odd"));
        for revision in ["HEAD", "HEAD:"] {
            let answer = repo
                .ask(Query::Show, Some(revision), None)
                .expect("answered");
            assert!(!answer.text.contains("hidden"), "{}", answer.text);
            assert!(!answer.text.contains("name"), "{}", answer.text);
            assert!(answer.withheld, "{revision}");
        }
    }

    /// A merge has more than one diff, and git's combined form is not one a planner can act
    /// on, so the merge says which diff to ask for instead of printing one.
    #[test]
    fn a_merge_is_shown_without_a_diff_and_names_the_diff_that_gives_one() {
        let (h, side, merge, _) = branchy("merge");
        let text = h.repo.text(Query::Show, "HEAD");
        assert!(
            text.starts_with(&format!(
                "commit {}\nMerge: {} {}\n",
                merge.to_hex(),
                short(h.third, 7),
                short(side, 7)
            )),
            "{text}"
        );
        assert!(!text.contains("diff --git"), "{text}");
        let pair = format!("{}^1 {}", short(merge, 7), short(merge, 7));
        assert!(text.contains(&format!("\"{pair}\"")), "{text}");
        assert!(
            h.repo
                .text(Query::Diff, &pair)
                .ends_with(" hello\n world\n+side\n")
        );
    }

    /// An annotated tag is shown with its own message and then the commit it names, which is
    /// what `git show v1` prints.
    #[test]
    fn a_tag_is_shown_with_its_message_then_its_commit() {
        let (h, _, _, _) = branchy("tag");
        let text = h.repo.text(Query::Show, "v1");
        assert!(
            text.starts_with(&format!(
                "tag v1\nTagger: T A Gger <tagger@example.com>\nDate:   2023-11-16 22:13:20 \
                 +0000\n\n    release one\n\ncommit {}\n",
                h.third.to_hex()
            )),
            "{text}"
        );
        assert!(text.contains("+world\n"), "{text}");
    }

    /// Bytes that are not text would arrive as noise, and a NUL is how git tells them apart,
    /// so a binary file is described by its size rather than printed.
    #[test]
    fn a_binary_file_is_described_rather_than_printed() {
        let repo = Repo::new("binary");
        let before = repo.object(Kind::Blob, b"\x00\x01before");
        let after = repo.object(Kind::Blob, b"\x00\x01after!");
        let first = repo.commit(
            repo.tree(&[("100644", "image.bin", before)]),
            &[],
            T1,
            "one",
        );
        let second = repo.commit(
            repo.tree(&[("100644", "image.bin", after)]),
            &[first],
            T1 + DAY,
            "two",
        );
        repo.point("refs/heads/main", second);
        assert_eq!(
            repo.text(Query::Show, "HEAD:image.bin"),
            "(image.bin is binary, 8 bytes)\n"
        );
        assert!(
            repo.text(Query::Diff, "HEAD~1..HEAD")
                .ends_with("Binary files a/image.bin and b/image.bin differ\n")
        );
    }

    /// `diff` compares two commits' trees, every file that differs in the shape `git diff`
    /// prints, and says so when nothing does rather than answering with nothing.
    #[test]
    fn diff_compares_two_commits_as_git_diff_prints_them() {
        let h = history("diff");
        let expected = format!(
            "diff --git a/README b/README\nindex {}..{} 100644\n--- a/README\n+++ b/README\n@@ -1 \
             +1,2 @@\n hello\n+world\ndiff --git a/src/lib.rs b/src/lib.rs\nnew file mode \
             100644\nindex 0000000..{}\n--- /dev/null\n+++ b/src/lib.rs\n@@ -0,0 +1 @@\n+pub fn \
             f() {{}}\n",
            short(h.readme, 7),
            short(h.changed, 7),
            short(h.lib, 7)
        );
        assert_eq!(h.repo.text(Query::Diff, "HEAD~2..HEAD"), expected);
        assert_eq!(h.repo.text(Query::Diff, "HEAD~2 HEAD"), expected);
        let narrowed = h
            .repo
            .ask(Query::Diff, Some("HEAD~2 HEAD"), Some("src"))
            .expect("answered");
        assert!(
            narrowed.text.starts_with("diff --git a/src/lib.rs"),
            "{}",
            narrowed.text
        );
        assert_eq!(narrowed.shown, ["src/lib.rs"]);
        assert_eq!(h.repo.text(Query::Diff, "HEAD HEAD"), "(no differences)\n");
    }

    /// Each query takes the revisions it can answer for, and a planner who writes the other
    /// shape is told which query takes it rather than given half an answer.
    #[test]
    fn a_query_given_the_wrong_shape_of_revision_says_which_query_takes_it() {
        let h = history("shapes");
        assert_eq!(
            h.repo.ask(Query::Diff, Some("HEAD"), None).map(|a| a.text),
            Err(Declined::DiffNeedsTwo)
        );
        assert_eq!(
            h.repo.ask(Query::Diff, None, None).map(|a| a.text),
            Err(Declined::DiffNeedsTwo)
        );
        assert_eq!(
            h.repo
                .ask(Query::Show, Some("HEAD~1..HEAD"), None)
                .map(|a| a.text),
            Err(Declined::ShowTakesOne("HEAD~1..HEAD".to_owned()))
        );
        assert_eq!(
            h.repo
                .ask(Query::Log, Some("HEAD~1 HEAD"), None)
                .map(|a| a.text),
            Err(Declined::PairIsForDiff("HEAD~1 HEAD".to_owned()))
        );
    }

    /// A shallow clone stops at commits whose parents it never fetched, and git reads those
    /// commits as roots: following the parent would find nothing and fail the whole log.
    #[test]
    fn a_shallow_commit_is_read_as_having_no_parents() {
        let h = history("shallow");
        let hex = h.first.to_hex().to_string();
        std::fs::remove_file(h.repo.git.join("objects").join(&hex[..2]).join(&hex[2..]))
            .expect("first commit removed");
        assert_eq!(
            h.repo.ask(Query::Log, None, None).map(|a| a.text),
            Err(Declined::Unreadable)
        );

        h.repo.point("shallow", h.second);
        assert_eq!(
            h.repo.text(Query::Log, "HEAD"),
            format!(
                "{} 2023-11-16 A U Thor third\n{} 2023-11-15 A U Thor second\n",
                short(h.third, 10),
                short(h.second, 10)
            )
        );
    }

    /// A deadline already past stops the walk at once and the answer says it ran out of time,
    /// so a planner is not handed a partial log as though it were the whole one.
    #[test]
    fn a_read_out_of_time_says_so() {
        let h = history("deadline");
        let repository = h.repo.opened().expect("opened");
        let mut asked = request(Query::Log, None, None);
        asked.deadline = Instant::now();
        let answer = repository.answer(&asked, &|_| false).expect("answered");
        assert!(answer.timed_out);
        assert_eq!(answer.text, "");
    }

    /// One answer is held to a number of lines and each line to a width, a minified file
    /// being one line, and either cut is reported so the answer is not taken as whole.
    #[test]
    fn an_answer_past_its_lines_or_a_line_past_its_width_is_cut() {
        let mut text = Text::default();
        for n in 0..MAX_LINES {
            assert!(text.line(&n.to_string()));
        }
        assert!(!text.cut, "exactly the cap is not a cut");
        assert!(!text.line("one more"));
        assert!(text.cut);
        assert_eq!(text.body.lines().count(), MAX_LINES);

        let mut wide = Text::default();
        wide.line(&"é".repeat(MAX_LINE_CHARS + 1));
        assert!(wide.cut);
        assert_eq!(
            wide.body,
            format!("{} [line cut]\n", "é".repeat(MAX_LINE_CHARS))
        );
    }

    /// A listing or a diff that exactly fills the answer is whole; the entry after it is what
    /// makes it cut. A planner told a whole answer was cut narrows a question that needed none.
    #[test]
    fn a_listing_or_a_diff_that_fills_the_answer_says_it_was_cut_only_when_more_was_left() {
        let repo = Repo::new("fills");
        let empty = repo.blob("");
        let one = repo.blob("one\n");
        let names: Vec<String> = (0..=MAX_LINES).map(|n| format!("f{n:04}")).collect();
        let files = |count: usize| -> Vec<(&str, &str, ObjectId)> {
            names[..count]
                .iter()
                .map(|name| ("100644", name.as_str(), empty))
                .collect()
        };
        for (count, cut) in [(MAX_LINES, false), (MAX_LINES + 1, true)] {
            let commit = repo.commit(repo.tree(&files(count)), &[], T1, "listing");
            repo.point("refs/heads/main", commit);
            let answer = repo
                .ask(Query::Show, Some("HEAD:"), None)
                .expect("answered");
            assert_eq!((answer.text.lines().count(), answer.cut), (MAX_LINES, cut));
            assert_eq!(
                answer.shown.len(),
                MAX_LINES + 1,
                "a file left out was shown"
            );
        }

        // An empty file added is three lines and a one-line file seven, so this fills it exactly.
        let whole = files((MAX_LINES - 2 * 7) / 3);
        let nothing = repo.commit(repo.tree(&[]), &[], T1, "nothing");
        for cut in [false, true] {
            let mut entries = whole.clone();
            entries.extend([("100644", "one-a", one), ("100644", "one-b", one)]);
            if cut {
                entries.push(("100644", "z", empty));
            }
            let commit = repo.commit(repo.tree(&entries), &[nothing], T1 + DAY, "files");
            repo.point("refs/heads/main", commit);
            let answer = repo
                .ask(Query::Diff, Some("HEAD~1..HEAD"), None)
                .expect("answered");
            assert_eq!((answer.text.lines().count(), answer.cut), (MAX_LINES, cut));
            assert!(
                answer.text.starts_with(&format!(
                    "diff --git a/f0000 b/f0000\nnew file mode 100644\nindex 0000000..{}\ndiff \
                     --git a/f0001",
                    short(empty, 7)
                )),
                "an empty file added is more than its index line"
            );
            assert!(
                !answer.shown.iter().any(|path| path == "z"),
                "a file left out was shown"
            );
        }
    }

    /// Each file's lines an answer holds are kept apart with the line of the file they start at,
    /// the new side and the old side of a diff each whole, and the rest of the answer is kept with
    /// those lines left blank: a credential is found in a file as a read of that file would find
    /// it, and at the line the person would open.
    #[test]
    fn each_file_an_answer_shows_is_kept_with_the_line_it_starts_at() {
        let repo = Repo::new("printed");
        let lines: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
        let before = repo.blob(&format!("{}\n", lines.join("\n")));
        let mut changed = lines.clone();
        changed[7] = "eight".to_owned();
        let after = repo.blob(&format!("{}\n", changed.join("\n")));
        let first = repo.commit(repo.tree(&[("100644", "notes", before)]), &[], T1, "one");
        let second = repo.commit(
            repo.tree(&[("100644", "notes", after)]),
            &[first],
            T1 + DAY,
            "two",
        );
        repo.point("refs/heads/main", second);

        let answer = repo.ask(Query::Show, Some("HEAD"), None).expect("answered");
        let printed = |first_line, text: &str| Printed {
            path: "notes".to_owned(),
            first_line,
            text: text.to_owned(),
        };
        assert_eq!(
            answer.printed,
            [
                printed(5, "5\n6\n7\neight\n9\n10\n"),
                printed(5, "5\n6\n7\n8\n9\n10\n"),
            ]
        );
        assert_eq!(answer.around.lines().count(), answer.text.lines().count());
        assert!(
            answer.around.contains("\n    two\n") && answer.around.contains("@@ -5,6 +5,6 @@\n\n"),
            "{}",
            answer.around
        );
        assert!(!answer.around.contains("eight"), "{}", answer.around);

        let file = repo
            .ask(Query::Show, Some("HEAD:notes"), None)
            .expect("answered");
        assert_eq!(
            file.printed,
            [printed(1, &format!("{}\n", changed.join("\n")))]
        );
    }

    /// A file's bytes that are not UTF-8 are shown as `\xNN`, each on its own, so two versions that
    /// differ only in such a byte are shown differing rather than as one line twice.
    #[test]
    fn bytes_that_are_not_utf8_are_shown_escaped_and_diffed_as_bytes() {
        let repo = Repo::new("not-utf8");
        let before = repo.object(Kind::Blob, b"caf\xe9\n");
        let after = repo.object(Kind::Blob, b"caf\xe8\n");
        let first = repo.commit(repo.tree(&[("100644", "menu", before)]), &[], T1, "one");
        let second = repo.commit(
            repo.tree(&[("100644", "menu", after)]),
            &[first],
            T1 + DAY,
            "two",
        );
        repo.point("refs/heads/main", second);
        assert_eq!(repo.text(Query::Show, "HEAD:menu"), "caf\\xe8\n");
        let diff = repo.text(Query::Diff, "HEAD~1..HEAD");
        assert!(
            diff.ends_with("@@ -1 +1 @@\n-caf\\xe9\n+caf\\xe8\n"),
            "{diff}"
        );
    }

    /// `since` and `until` come from the planner, and a day that is not on the calendar would
    /// otherwise land on a neighbouring one without anyone being told.
    #[test]
    fn only_a_real_calendar_day_is_read_as_one() {
        assert_eq!(parse_day("1970-01-01"), Some(0));
        assert_eq!(parse_day("1970-01-02"), Some(DAY));
        assert_eq!(parse_day("2023-11-14"), Some(T1 - T1 % DAY));
        for real in ["2024-02-29", "2000-02-29", "2023-12-31"] {
            let start = parse_day(real).expect(real);
            assert_eq!(day(start, 0), real);
        }
        for unreal in [
            "2023-02-29",
            "1900-02-29",
            "2023-13-01",
            "2023-00-10",
            "2023-01-00",
            "2023-04-31",
            "2023-1-01",
            "2023-01-01T00",
            "+023-01-01",
            "2023/01/01",
            "",
        ] {
            assert_eq!(parse_day(unreal), None, "{unreal}");
        }
    }

    /// A signature's time is shown in the zone it was made in, as git shows it, so a date a
    /// planner reads matches the one the author saw.
    #[test]
    fn a_signature_time_is_shown_in_its_own_zone() {
        assert_eq!(stamp(T1, 0), "2023-11-14 22:13:20 +0000");
        assert_eq!(stamp(T1, -5 * 3600), "2023-11-14 17:13:20 -0500");
        assert_eq!(stamp(T1, 5 * 3600 + 1800), "2023-11-15 03:43:20 +0530");
        assert_eq!(day(T1, 2 * 3600), "2023-11-15");
    }

    /// The per-worktree configuration changes what a read means as the shared one does, so it
    /// is held to the same rules.
    #[test]
    fn the_per_worktree_configuration_is_held_to_the_same_rules() {
        let h = history("config-worktree");
        h.repo
            .put("config.worktree", "[core]\n\tworktree = /elsewhere\n");
        assert_eq!(
            h.repo.ask(Query::Log, None, None).map(|a| a.text),
            Err(Declined::Worktree)
        );
    }

    /// Symbolic refs that point at each other would otherwise be followed forever.
    #[test]
    fn a_loop_of_symbolic_refs_is_declined() {
        let h = history("symbolic-loop");
        h.repo.put("refs/heads/a", "ref: refs/heads/b\n");
        h.repo.put("refs/heads/b", "ref: refs/heads/a\n");
        h.repo.put("HEAD", "ref: refs/heads/a\n");
        assert_eq!(
            h.repo.ask(Query::Log, None, None).map(|a| a.text),
            Err(Declined::Unreadable)
        );
    }

    /// A tree entry git itself refuses to check out, `..` or a name holding a `/`, would join
    /// into a path the trust map holds under another key, so a tree holding one is not read.
    #[test]
    fn a_tree_entry_whose_name_would_leave_its_directory_is_not_read() {
        for name in ["..", ".", "a/b"] {
            let repo = Repo::new(&format!("entry-{}", name.replace(['/', '.'], "x")));
            let secret = repo.blob("secret\n");
            let mut tree = format!("100644 {name}\0").into_bytes();
            tree.extend_from_slice(secret.as_bytes());
            let tree = repo.object(Kind::Tree, &tree);
            repo.point("refs/heads/main", repo.commit(tree, &[], T1, "odd"));
            for revision in ["HEAD", "HEAD:"] {
                assert_eq!(
                    repo.ask(Query::Show, Some(revision), None).map(|a| a.text),
                    Err(Declined::Unreadable),
                    "{name} in {revision}"
                );
            }
        }
    }

    /// A file past the size cap would be inflated whole to print a part of it, so it is
    /// described by its size, read from the object's header alone.
    #[test]
    fn a_file_past_the_size_cap_is_described_by_its_size() {
        let repo = Repo::new("large");
        let size = usize::try_from(MAX_BLOB).expect("fits") + 1;
        let large = repo.object(Kind::Blob, &vec![b'x'; size]);
        let small = repo.blob("x\n");
        let first = repo.commit(repo.tree(&[("100644", "big", small)]), &[], T1, "small");
        let second = repo.commit(
            repo.tree(&[("100644", "big", large)]),
            &[first],
            T1 + DAY,
            "big",
        );
        repo.point("refs/heads/main", second);
        assert_eq!(
            repo.text(Query::Show, "HEAD:big"),
            format!(
                "(big is {size} bytes, more than read_git shows; use read_file on the working copy, or run)\n"
            )
        );
        assert!(repo.text(Query::Diff, "HEAD~1 HEAD").ends_with(&format!(
            "(not diffed: a side is {size} bytes, more than read_git diffs)\n"
        )));
    }

    /// A submodule's commit lives in another repository, which read_git does not open, so it is
    /// shown as the id git records for it and never read as a commit here.
    #[test]
    fn a_submodule_is_shown_as_the_commit_it_records_and_not_read() {
        let repo = Repo::new("gitlink");
        let pinned = ObjectId::from_hex(b"1111111111111111111111111111111111111111").expect("id");
        let moved = ObjectId::from_hex(b"2222222222222222222222222222222222222222").expect("id");
        let first = repo.commit(repo.tree(&[("160000", "vendor", pinned)]), &[], T1, "pin");
        let second = repo.commit(
            repo.tree(&[("160000", "vendor", moved)]),
            &[first],
            T1 + DAY,
            "bump",
        );
        repo.point("refs/heads/main", second);
        assert_eq!(
            repo.text(Query::Show, "HEAD:vendor"),
            format!(
                "vendor is a submodule, at commit {}; read_git does not read the submodule's own \
                 repository.\n",
                moved.to_hex()
            )
        );
        assert!(
            repo.text(Query::Diff, "HEAD~1 HEAD").ends_with(&format!(
                "--- a/vendor\n+++ b/vendor\n@@ -1 +1 @@\n-Subproject commit {}\n+Subproject commit {}\n",
                pinned.to_hex(),
                moved.to_hex()
            ))
        );
        assert_eq!(
            repo.ask(Query::Log, Some("HEAD:vendor"), None)
                .map(|a| a.text),
            Err(Declined::Kind {
                revision: "HEAD:vendor".to_owned(),
                wanted: "commit in this repository"
            })
        );
    }

    fn sha1(bytes: &[u8]) -> ObjectId {
        let mut hasher = gix_hash::hasher(HashKind::Sha1);
        hasher.update(bytes);
        hasher.try_finalize().expect("hashed")
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// zlib's format with one stored block, which every inflater reads, so the fixture needs
    /// no compressor.
    fn stored(data: &[u8]) -> Vec<u8> {
        let len = u16::try_from(data.len()).expect("one stored block");
        let mut out = vec![0x78, 0x01, 0x01];
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in data {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }

    fn entry_header(code: u8, size: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut byte = (code << 4) | (size & 0x0f) as u8;
        let mut rest = size >> 4;
        while rest > 0 {
            out.push(byte | 0x80);
            byte = (rest & 0x7f) as u8;
            rest >>= 7;
        }
        out.push(byte);
        out
    }

    fn varint(mut n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    /// Write a version 2 pack of `base`, then `target` stored as an offset delta against it,
    /// then `rest` whole, with its version 2 index beside it.
    fn write_pack(git: &Path, base: &[u8], target: &[u8], rest: &[(Kind, Vec<u8>)]) {
        fn code(kind: Kind) -> u8 {
            match kind {
                Kind::Commit => 1,
                Kind::Tree => 2,
                Kind::Blob => 3,
                Kind::Tag => 4,
            }
        }
        let id = |kind, data: &[u8]| {
            gix_object::compute_hash(HashKind::Sha1, kind, data).expect("hashed")
        };
        let mut pack = b"PACK".to_vec();
        pack.extend_from_slice(&2u32.to_be_bytes());
        pack.extend_from_slice(&(2 + rest.len() as u32).to_be_bytes());
        let mut entries: Vec<(ObjectId, u32, u32)> = Vec::new();
        let mut add = |pack: &mut Vec<u8>, id: ObjectId, raw: Vec<u8>| -> usize {
            let offset = pack.len();
            entries.push((id, offset as u32, crc32(&raw)));
            pack.extend_from_slice(&raw);
            offset
        };

        let mut raw = entry_header(code(Kind::Blob), base.len());
        raw.extend(stored(base));
        let base_at = add(&mut pack, id(Kind::Blob, base), raw);

        assert!(target.starts_with(base) && base.len() < 256 && target.len() - base.len() < 128);
        let mut delta = varint(base.len());
        delta.extend(varint(target.len()));
        delta.extend([0x80 | 0x10, base.len() as u8]);
        delta.push((target.len() - base.len()) as u8);
        delta.extend_from_slice(&target[base.len()..]);
        let mut raw = entry_header(6, delta.len());
        let mut distance = pack.len() - base_at;
        let mut back = vec![(distance & 0x7f) as u8];
        distance >>= 7;
        while distance > 0 {
            distance -= 1;
            back.push(0x80 | (distance & 0x7f) as u8);
            distance >>= 7;
        }
        back.reverse();
        raw.extend(back);
        raw.extend(stored(&delta));
        add(&mut pack, id(Kind::Blob, target), raw);

        for (kind, data) in rest {
            let mut raw = entry_header(code(*kind), data.len());
            raw.extend(stored(data));
            add(&mut pack, id(*kind, data), raw);
        }
        let checksum = sha1(&pack);
        pack.extend_from_slice(checksum.as_bytes());

        entries.sort();
        let mut index = vec![0xff, b't', b'O', b'c', 0, 0, 0, 2];
        for first in 0..=255u8 {
            let below = entries
                .iter()
                .filter(|e| e.0.as_bytes()[0] <= first)
                .count() as u32;
            index.extend_from_slice(&below.to_be_bytes());
        }
        for (id, _, _) in &entries {
            index.extend_from_slice(id.as_bytes());
        }
        for (_, _, crc) in &entries {
            index.extend_from_slice(&crc.to_be_bytes());
        }
        for (_, offset, _) in &entries {
            index.extend_from_slice(&offset.to_be_bytes());
        }
        index.extend_from_slice(checksum.as_bytes());
        let own = sha1(&index);
        index.extend_from_slice(own.as_bytes());

        let dir = git.join("objects/pack");
        std::fs::create_dir_all(&dir).expect("pack directory");
        let name = format!("pack-{}", checksum.to_hex());
        std::fs::write(dir.join(format!("{name}.pack")), pack).expect("pack");
        std::fs::write(dir.join(format!("{name}.idx")), index).expect("index");
    }

    /// Most of a real repository is packed, much of it as deltas, so a reader of loose objects
    /// alone would decline or misread nearly every repository it is pointed at.
    #[test]
    fn objects_in_a_pack_and_stored_as_deltas_read_as_loose_ones_do() {
        let repo = Repo::new("packed");
        let base = b"one\ntwo\nthree\n";
        let target = b"one\ntwo\nthree\nfour\n";
        let notes = gix_object::compute_hash(HashKind::Sha1, Kind::Blob, target).expect("hashed");
        let tree = tree_bytes(&[("100644", "notes", notes)]);
        let tree_id = gix_object::compute_hash(HashKind::Sha1, Kind::Tree, &tree).expect("hashed");
        let commit = commit_bytes(tree_id, &[], T1, "packed");
        let commit_id =
            gix_object::compute_hash(HashKind::Sha1, Kind::Commit, &commit).expect("hashed");
        write_pack(
            &repo.git,
            base,
            target,
            &[(Kind::Tree, tree), (Kind::Commit, commit)],
        );
        repo.point("refs/heads/main", commit_id);

        assert_eq!(
            repo.text(Query::Show, "HEAD:notes"),
            "one\ntwo\nthree\nfour\n"
        );
        let abbreviated = short(commit_id, 7);
        assert_eq!(
            repo.text(Query::Log, &abbreviated),
            format!("{} 2023-11-14 A U Thor packed\n", short(commit_id, 10))
        );
        assert!(
            repo.text(Query::Show, "HEAD")
                .ends_with("@@ -0,0 +1,4 @@\n+one\n+two\n+three\n+four\n")
        );
    }
}
