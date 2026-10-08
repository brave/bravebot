//! The four lists a person writes to move what a confined program reads and writes.
//!
//! `allowRead` lifts a refusal, `denyRead` adds one, `allowWrite` adds a write row that is read as
//! well, and `denyWrite` takes write access away under a path, a session directory included.
//! `docs/specs/sandboxing.md` decides what each does and which entries are refused; this is that
//! decision in code, from the entries as written to rows a [`SandboxPolicy`] holds.
//!
//! Nothing here reads a file's contents, the environment or a program's output. The inputs are the
//! entries a person wrote, the home directory and the directory relative entries are read from, and
//! what a glob names is decided by the directory entries' names alone.

use crate::policy::{SandboxPolicy, names_a_filesystem_root};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The most directory entries one glob looks at, so a pattern over a whole disk ends.
const GLOB_ENTRY_BOUND: usize = 200_000;

/// The deepest a glob goes below the directory its fixed part names.
const GLOB_DEPTH_BOUND: usize = 24;

/// Which of the four lists an entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum List {
    AllowRead,
    DenyRead,
    AllowWrite,
    DenyWrite,
}

impl List {
    /// The key a settings file and a report spell it with.
    pub fn key(self) -> &'static str {
        match self {
            Self::AllowRead => "allowRead",
            Self::DenyRead => "denyRead",
            Self::AllowWrite => "allowWrite",
            Self::DenyWrite => "denyWrite",
        }
    }

    /// The settings key, with the path to it, that `doctor` names it by.
    pub fn setting(self) -> &'static str {
        match self {
            Self::AllowRead => "sandbox.filesystem.allowRead",
            Self::DenyRead => "sandbox.filesystem.denyRead",
            Self::AllowWrite => "sandbox.filesystem.allowWrite",
            Self::DenyWrite => "sandbox.filesystem.denyWrite",
        }
    }

    /// Whether an entry in this list can only take reach away.
    pub fn is_a_denial(self) -> bool {
        matches!(self, Self::DenyRead | Self::DenyWrite)
    }

    fn is_a_write(self) -> bool {
        matches!(self, Self::AllowWrite | Self::DenyWrite)
    }
}

/// How many entries of each list are in force, which is what a report says of them: the entries and
/// never a path a glob turned up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub allow_read: usize,
    pub deny_read: usize,
    pub allow_write: usize,
    pub deny_write: usize,
}

impl Counts {
    /// Whether no entry is in force.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// One entry as a person wrote it, and where it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The spelling: absolute, `~/`-prefixed, or relative to the session's directory.
    pub path: String,
    /// The file that wrote it, or `None` for the command line.
    pub by: Option<PathBuf>,
    /// Whether the managed layer wrote it, which nothing a person wrote can lift.
    pub pinned: bool,
}

/// The four lists, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lists {
    pub allow_read: Vec<Entry>,
    pub deny_read: Vec<Entry>,
    pub allow_write: Vec<Entry>,
    pub deny_write: Vec<Entry>,
}

impl Lists {
    pub fn is_empty(&self) -> bool {
        self.allow_read.is_empty()
            && self.deny_read.is_empty()
            && self.allow_write.is_empty()
            && self.deny_write.is_empty()
    }

    /// The entries of one list.
    pub fn of(&self, list: List) -> &[Entry] {
        match list {
            List::AllowRead => &self.allow_read,
            List::DenyRead => &self.deny_read,
            List::AllowWrite => &self.allow_write,
            List::DenyWrite => &self.deny_write,
        }
    }
}

/// Why an entry is not in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// It starts with `~` and the session names no home directory.
    NoHome,
    /// It climbs out of the directory relative entries are read from, or holds `..` where it
    /// is absolute and so cannot be judged.
    Climbs,
    /// It holds a wildcard and is in a write list, which a glob does not apply to.
    GlobOnAWrite,
    /// It would be a write row over the whole filesystem or the home directory, which confines
    /// nothing.
    ConfinesNothing,
    /// It names `~/.ssh`, or a place inside it, in a list that adds reach. No row is a private key.
    PrivateKey,
    /// A glob looked at more entries or went deeper than a glob may, so what it names is not
    /// known.
    TooBroad,
    /// Another entry decides the path: a refusal at the same path, or one the managed layer wrote.
    Overridden,
}

/// What became of one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// In force, as these paths: one for a plain entry and one for each match of a glob, none where
    /// a glob matched nothing.
    InForce(Vec<PathBuf>),
    /// Not in force.
    Refused(Reason),
}

/// An entry with the list it is in and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub list: List,
    pub entry: Entry,
    pub state: State,
}

/// The lists resolved against a machine: every entry and what became of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    items: Vec<Item>,
}

impl Rules {
    /// No entry.
    pub fn none() -> Self {
        Self::default()
    }

    /// Every entry, in list order and then in the order written.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The entries in force.
    pub fn in_force(&self) -> impl Iterator<Item = &Item> {
        self.items
            .iter()
            .filter(|item| matches!(item.state, State::InForce(_)))
    }

    /// The entries that are not, with why.
    pub fn refused(&self) -> impl Iterator<Item = (&Item, Reason)> {
        self.items.iter().filter_map(|item| match item.state {
            State::Refused(reason) => Some((item, reason)),
            State::InForce(_) => None,
        })
    }

    /// How many entries of `list` are in force.
    pub fn count(&self, list: List) -> usize {
        self.in_force().filter(|item| item.list == list).count()
    }

    /// Whether no entry is in force.
    pub fn is_empty(&self) -> bool {
        self.in_force().next().is_none()
    }

    /// How many entries of each list are in force.
    pub fn counts(&self) -> Counts {
        Counts {
            allow_read: self.count(List::AllowRead),
            deny_read: self.count(List::DenyRead),
            allow_write: self.count(List::AllowWrite),
            deny_write: self.count(List::DenyWrite),
        }
    }

    fn paths(&self, list: List) -> Vec<&Path> {
        self.in_force()
            .filter(|item| item.list == list)
            .flat_map(|item| match &item.state {
                State::InForce(paths) => paths.iter().map(PathBuf::as_path).collect(),
                State::Refused(_) => Vec::new(),
            })
            .collect()
    }

    /// The first refusal that leaves a person's refusal of reach unapplied, if any.
    ///
    /// An entry of `denyRead` or `denyWrite` that is not in force is a path the person meant to
    /// hold back and that a stage would reach, so a caller starting a stage refuses it rather than
    /// start one. An allow entry that is not in force only leaves reach where it was.
    pub fn unapplied_denial(&self) -> Option<&Item> {
        self.refused()
            .map(|(item, _)| item)
            .find(|item| item.list.is_a_denial())
    }

    /// `policy` with these rules in it.
    ///
    /// A refusal of a person's beats the rows a stage brings of its own at or beneath it, scope and
    /// toolchain rows included: the person said no program reads `~/.config/gh`, and a stage whose
    /// scope names it is still a program. A row of the person's own beneath a refusal of theirs is
    /// the narrower rule and stands. A write row is read as well, and a write row above a credential
    /// location the base refuses never reaches it.
    pub fn apply(&self, mut policy: SandboxPolicy) -> SandboxPolicy {
        if self.is_empty() {
            return policy;
        }
        let deny_read = self.paths(List::DenyRead);
        let deny_write = self.paths(List::DenyWrite);
        policy.readable.retain(|row| {
            !deny_read
                .iter()
                .any(|denied| row.starts_with(denied) || row == denied)
        });
        policy.writable.retain(|row| {
            !deny_read
                .iter()
                .chain(deny_write.iter())
                .any(|denied| row.path.starts_with(denied))
        });

        let built_in: Vec<PathBuf> = policy.unreadable.clone();
        for path in self.paths(List::AllowRead) {
            policy = policy.allow_read(path);
        }
        for path in self.paths(List::AllowWrite) {
            policy = policy.allow_write(path).allow_read(path);
        }
        // Seatbelt's refusal of a read does not stop a write, so a write row above a location the
        // base holds back is a write there unless that is refused too.
        for refused in built_in {
            let above = self
                .paths(List::AllowWrite)
                .into_iter()
                .any(|row| refused.starts_with(row) && refused != row);
            let lifted = self
                .paths(List::AllowRead)
                .into_iter()
                .chain(self.paths(List::AllowWrite))
                .any(|row| row.starts_with(&refused));
            if above && !lifted {
                policy = policy.deny_write(refused);
            }
        }
        for path in deny_read {
            let granted_above = policy
                .readable
                .iter()
                .any(|row| path.starts_with(row) && path != row);
            if granted_above {
                policy = policy.deny_read(path);
            }
        }
        for path in deny_write {
            let granted_above = policy
                .writable
                .iter()
                .any(|row| path.starts_with(&row.path) && path != row.path);
            if granted_above {
                policy = policy.deny_write(path);
            }
        }
        policy
    }
}

/// Resolve `lists` for a session whose home directory is `home` and whose relative entries are read
/// from `base`.
///
/// Every path is judged where it leads: the deepest part of it that is on disk is resolved through
/// its links, so a link into `~/.aws` is `~/.aws`. Globs are expanded here and not when a stage
/// starts, so a file made afterwards is outside what a glob named.
pub fn resolve(lists: &Lists, home: Option<&Path>, base: &Path) -> Rules {
    let home = home.map(resolved);
    let base = resolved(base);
    let mut items: Vec<Item> = Vec::new();
    for list in [
        List::AllowRead,
        List::DenyRead,
        List::AllowWrite,
        List::DenyWrite,
    ] {
        for entry in lists.of(list) {
            let state = match one(list, entry, home.as_deref(), &base) {
                Ok(paths) => State::InForce(paths),
                Err(reason) => State::Refused(reason),
            };
            items.push(Item {
                list,
                entry: entry.clone(),
                state,
            });
        }
    }
    let rules = Rules { items };
    let overridden = overridden(&rules);
    Rules {
        items: rules
            .items
            .into_iter()
            .enumerate()
            .map(|(at, mut item)| {
                if overridden.contains(&at) {
                    item.state = State::Refused(Reason::Overridden);
                }
                item
            })
            .collect(),
    }
}

/// The entries another entry decides: an allow at a path a refusal names, and an allow at or
/// beneath a refusal the managed layer wrote.
fn overridden(rules: &Rules) -> Vec<usize> {
    let paths = |item: &Item| match &item.state {
        State::InForce(paths) => paths.clone(),
        State::Refused(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for (at, item) in rules.items.iter().enumerate() {
        if item.list.is_a_denial() || !matches!(item.state, State::InForce(_)) {
            continue;
        }
        let mine = paths(item);
        let written = item.list.is_a_write();
        let beaten = rules.items.iter().any(|other| {
            if !other.list.is_a_denial() {
                return false;
            }
            // A write row is read as well, so a refusal of reads is a refusal of it; a read row is
            // not touched by a refusal of writes.
            let reaches = !matches!((other.list, written), (List::DenyWrite, false));
            reaches
                && paths(other).iter().any(|denied| {
                    mine.iter().any(|path| {
                        path == denied || (other.entry.pinned && path.starts_with(denied))
                    })
                })
        });
        if beaten {
            out.push(at);
        }
    }
    out
}

/// The paths one entry names, or why it names none.
fn one(
    list: List,
    entry: &Entry,
    home: Option<&Path>,
    base: &Path,
) -> Result<Vec<PathBuf>, Reason> {
    let spelled = entry.path.trim();
    let (anchor, rest, absolute) = match spelled.strip_prefix('~') {
        Some("") => (home.ok_or(Reason::NoHome)?.to_path_buf(), "", true),
        Some(after) if after.starts_with(['/', '\\']) => (
            home.ok_or(Reason::NoHome)?.to_path_buf(),
            after.trim_start_matches(['/', '\\']),
            true,
        ),
        _ if Path::new(spelled).is_absolute() => (PathBuf::new(), spelled, true),
        _ => (base.to_path_buf(), spelled, false),
    };
    let mut path = if anchor.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        anchor.clone()
    };
    let mut climbed = 0usize;
    for part in Path::new(rest).components() {
        match part {
            Component::Prefix(_) | Component::RootDir => path.push(part.as_os_str()),
            Component::CurDir => {}
            Component::Normal(name) => {
                path.push(name);
                climbed += 1;
            }
            Component::ParentDir => {
                // Judged against where the entry is read from, which is the only place a relative
                // entry can be said to escape; an absolute one has no such place.
                if absolute || climbed == 0 {
                    return Err(Reason::Climbs);
                }
                path.pop();
                climbed -= 1;
            }
        }
    }
    let wildcard = rest.contains(['*', '?']);
    if wildcard && list.is_a_write() {
        return Err(Reason::GlobOnAWrite);
    }
    let named = if wildcard {
        // What a pattern names is judged where it leads, as a path written out is.
        expand(&path, anchor.components().count())?
            .iter()
            .map(|found| resolved(found))
            .collect()
    } else {
        vec![resolved(&path)]
    };
    let mut kept = Vec::new();
    for path in named {
        let private_key = home.is_some_and(|home| path.starts_with(home.join(".ssh")));
        if matches!(list, List::AllowRead | List::AllowWrite) && private_key {
            if wildcard {
                continue;
            }
            return Err(Reason::PrivateKey);
        }
        if list == List::AllowWrite
            && (names_a_filesystem_root(&path) || home.is_some_and(|home| home.starts_with(&path)))
        {
            return Err(Reason::ConfinesNothing);
        }
        kept.push(path);
    }
    kept.sort();
    kept.dedup();
    Ok(kept)
}

/// `path` with the part of it that is on disk resolved through its links, which is where a program
/// opening it would end up.
pub fn resolved(path: &Path) -> PathBuf {
    let mut missing: Vec<&std::ffi::OsStr> = Vec::new();
    let mut here = path;
    loop {
        if let Ok(found) = fs::canonicalize(here) {
            let mut out = found;
            out.extend(missing.iter().rev());
            return out;
        }
        match (here.parent(), here.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                here = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Every path a pattern names.
///
/// The directory its fixed leading names reach is walked, listing each directory once and not
/// following a link, so the walk is the same listing the Linux backend makes when it grants entry
/// by entry. A pattern that is not met by anything is not an error and names nothing.
fn expand(pattern: &Path, anchored: usize) -> Result<Vec<PathBuf>, Reason> {
    let mut fixed = PathBuf::new();
    let mut rest: Vec<String> = Vec::new();
    for (at, part) in pattern.components().enumerate() {
        let text = part.as_os_str().to_string_lossy().into_owned();
        // The directory the pattern is read from is a name and not a pattern, whatever its name has
        // in it, so a `*` in it is a character and not a wildcard.
        if rest.is_empty() && (at < anchored || !text.contains(['*', '?'])) {
            fixed.push(part.as_os_str());
        } else {
            rest.push(text);
        }
    }
    let start = resolved(&fixed);
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut bound = false;
    walk(&start, &rest, 0, &mut seen, &mut bound, &mut out);
    match bound {
        true => Err(Reason::TooBroad),
        false => Ok(out),
    }
}

fn walk(
    directory: &Path,
    rest: &[String],
    depth: usize,
    seen: &mut usize,
    bound: &mut bool,
    out: &mut Vec<PathBuf>,
) {
    let Some((first, after)) = rest.split_first() else {
        return;
    };
    if *bound {
        return;
    }
    if first == "**" {
        // Everything beneath is the directory, as a subpath.
        if after.is_empty() {
            out.push(directory.to_path_buf());
            return;
        }
        walk(directory, after, depth, seen, bound, out);
    }
    if depth >= GLOB_DEPTH_BOUND {
        *bound = true;
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        *seen += 1;
        if *seen > GLOB_ENTRY_BOUND {
            *bound = true;
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = entry.path();
        let is_a_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if first == "**" {
            if is_a_directory {
                walk(&child, rest, depth + 1, seen, bound, out);
            }
        } else if matches_name(first, &name) {
            if after.is_empty() {
                out.push(child);
            } else if is_a_directory {
                walk(&child, after, depth + 1, seen, bound, out);
            }
        }
    }
}

/// Whether `name` is met by `pattern`, where `*` is any run of characters and `?` is one.
fn matches_name(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    let (mut star, mut resume) = (None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) && pattern[p] != '*' {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = n;
            p += 1;
        } else if let Some(at) = star {
            p = at + 1;
            resume += 1;
            n = resume;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    /// An empty directory of this name, made new so a leftover of an earlier run is not in it.
    fn fresh(name: &str) -> PathBuf {
        let dir = scratch_dir(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(path: &str) -> Entry {
        Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        }
    }

    fn pinned(path: &str) -> Entry {
        Entry {
            pinned: true,
            ..entry(path)
        }
    }

    fn only(list: List, path: &str) -> Lists {
        let mut lists = Lists::default();
        let entries = vec![entry(path)];
        match list {
            List::AllowRead => lists.allow_read = entries,
            List::DenyRead => lists.deny_read = entries,
            List::AllowWrite => lists.allow_write = entries,
            List::DenyWrite => lists.deny_write = entries,
        }
        lists
    }

    fn refusal_of(rules: &Rules) -> Option<Reason> {
        rules.refused().map(|(_, reason)| reason).next()
    }

    /// The pattern is the whole of what a name has to meet: `*` runs over any characters including
    /// none, `?` is exactly one, and neither is looser than that.
    #[test]
    fn a_name_is_met_by_a_star_and_a_question_mark_and_nothing_looser() {
        assert!(matches_name("*.env", ".env"));
        assert!(matches_name("*.env", "prod.env"));
        assert!(!matches_name("*.env", "prod.env.bak"));
        assert!(matches_name("id_?sa", "id_rsa"));
        assert!(!matches_name("id_?sa", "id_sa"));
        assert!(matches_name("a*b*c", "aXXbYYc"));
        assert!(!matches_name("a*b*c", "aXXbYY"));
        assert!(matches_name("*", ""));
    }

    /// An entry is read as absolute, from the home directory, or from the session's directory, and
    /// a relative one that climbs out of it is refused rather than read from somewhere else.
    #[test]
    fn an_entry_is_read_from_where_its_spelling_says() {
        let home = fresh("rules-spelling-home");
        let base = fresh("rules-spelling-base");
        let rules = |list, path: &str| resolve(&only(list, path), Some(&home), &base);
        let path_of = |rules: &Rules| match &rules.items()[0].state {
            State::InForce(paths) => paths[0].clone(),
            State::Refused(reason) => panic!("refused: {reason:?}"),
        };
        assert_eq!(
            path_of(&rules(List::DenyRead, "~/notes")),
            resolved(&home).join("notes")
        );
        assert_eq!(
            path_of(&rules(List::DenyRead, "sub/../secrets")),
            resolved(&base).join("secrets")
        );
        assert_eq!(
            refusal_of(&rules(List::DenyRead, "../outside")),
            Some(Reason::Climbs)
        );
        assert_eq!(
            refusal_of(&rules(List::DenyRead, "sub/../../outside")),
            Some(Reason::Climbs)
        );
        assert_eq!(
            refusal_of(&rules(List::DenyRead, "/etc/../root")),
            Some(Reason::Climbs)
        );
        assert_eq!(
            refusal_of(&resolve(&only(List::DenyRead, "~/x"), None, &base)),
            Some(Reason::NoHome)
        );
    }

    /// A glob that cannot be listed within the bound is refused whole, so a `denyRead` over a tree
    /// deeper than the walk goes is not read as the part of it that was listed. The same pattern over
    /// a shallow tree is the control that the refusal comes from the depth.
    #[test]
    fn a_glob_over_a_tree_deeper_than_the_walk_goes_is_refused_and_not_cut_short() {
        let shallow = fresh("rules-too-broad-shallow");
        fs::create_dir_all(shallow.join("a/b")).unwrap();
        fs::write(shallow.join("a/b/prod.env"), "").unwrap();
        let deep = fresh("rules-too-broad-deep");
        let mut at = deep.clone();
        for level in 0..=GLOB_DEPTH_BOUND {
            at = at.join(format!("d{level}"));
        }
        fs::create_dir_all(&at).unwrap();
        fs::write(at.join("prod.env"), "").unwrap();

        let kept = resolve(&only(List::DenyRead, "**/*.env"), None, &shallow);
        assert_eq!(refusal_of(&kept), None);
        assert!(
            matches!(&kept.items()[0].state, State::InForce(paths) if paths.len() == 1),
            "the control did not list the file"
        );

        let cut = resolve(&only(List::DenyRead, "**/*.env"), None, &deep);
        assert_eq!(refusal_of(&cut), Some(Reason::TooBroad));
        assert!(cut.unapplied_denial().is_some());

        let allowed = resolve(&only(List::AllowRead, "**/*.env"), None, &deep);
        assert_eq!(refusal_of(&allowed), Some(Reason::TooBroad));
        assert!(allowed.unapplied_denial().is_none());
    }

    /// A link is judged where it leads, so a link into a credential directory is that directory.
    #[cfg(unix)]
    #[test]
    fn a_link_is_judged_by_where_it_leads() {
        let home = fresh("rules-link-home");
        let base = fresh("rules-link-base");
        fs::create_dir_all(home.join(".aws")).unwrap();
        std::os::unix::fs::symlink(home.join(".aws"), base.join("shortcut")).unwrap();
        let rules = resolve(&only(List::DenyRead, "shortcut/config"), Some(&home), &base);
        let State::InForce(paths) = &rules.items()[0].state else {
            panic!("refused");
        };
        assert_eq!(paths, &vec![resolved(&home).join(".aws/config")]);
    }

    /// A write row over the home or above it, or over the whole filesystem, confines nothing, and
    /// a row beneath the home is fine.
    #[test]
    fn a_write_row_over_the_home_or_the_root_is_refused() {
        let home = fresh("rules-write-home");
        let base = fresh("rules-write-base");
        let home_text = home.to_string_lossy().into_owned();
        for spelled in ["~", "~/", "/", home_text.as_str()] {
            let rules = resolve(&only(List::AllowWrite, spelled), Some(&home), &base);
            assert_eq!(
                refusal_of(&rules),
                Some(Reason::ConfinesNothing),
                "{spelled}"
            );
        }
        let above = home.parent().unwrap().to_string_lossy().into_owned();
        let rules = resolve(&only(List::AllowWrite, &above), Some(&home), &base);
        assert_eq!(refusal_of(&rules), Some(Reason::ConfinesNothing));
        let rules = resolve(&only(List::AllowWrite, "~/notes"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
        // A refusal of writes at the home is a refusal, not a row.
        let rules = resolve(&only(List::DenyWrite, "~"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
    }

    /// No list that adds reach names a private key or the directory holding one.
    #[test]
    fn an_entry_that_adds_reach_inside_ssh_is_refused() {
        let home = fresh("rules-ssh-home");
        let base = fresh("rules-ssh-base");
        for list in [List::AllowRead, List::AllowWrite] {
            for spelled in ["~/.ssh", "~/.ssh/id_ed25519"] {
                let rules = resolve(&only(list, spelled), Some(&home), &base);
                assert_eq!(refusal_of(&rules), Some(Reason::PrivateKey), "{spelled}");
            }
        }
        // Holding it back is the point of the table, so a refusal of it stands.
        let rules = resolve(&only(List::DenyRead, "~/.ssh/id_rsa"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
    }

    /// A glob names what is on disk when the rules are resolved, matches nothing quietly, applies
    /// to reads only, and does not follow a link into a directory.
    #[test]
    fn a_glob_is_expanded_by_listing_and_applies_to_reads() {
        let home = fresh("rules-glob-home");
        let base = fresh("rules-glob-base");
        fs::create_dir_all(base.join("a/b")).unwrap();
        fs::write(base.join("one.env"), "").unwrap();
        fs::write(base.join("a/two.env"), "").unwrap();
        fs::write(base.join("a/b/three.env"), "").unwrap();
        fs::write(base.join("a/b/three.txt"), "").unwrap();
        let rules = resolve(&only(List::DenyRead, "**/*.env"), Some(&home), &base);
        let State::InForce(paths) = &rules.items()[0].state else {
            panic!("refused");
        };
        let root = resolved(&base);
        assert_eq!(
            paths,
            &vec![
                root.join("a/b/three.env"),
                root.join("a/two.env"),
                root.join("one.env")
            ]
        );
        let none = resolve(&only(List::DenyRead, "**/*.nothing"), Some(&home), &base);
        assert!(matches!(&none.items()[0].state, State::InForce(paths) if paths.is_empty()));
        let write = resolve(&only(List::AllowWrite, "**/*.env"), Some(&home), &base);
        assert_eq!(refusal_of(&write), Some(Reason::GlobOnAWrite));
    }

    /// What a pattern matches is judged where it leads, and the directory it is read from is a
    /// name and not a pattern: a session opened in a directory that has a `*` in its name is not
    /// opened on every sibling of it.
    #[cfg(unix)]
    #[test]
    fn a_match_is_judged_where_it_leads_and_the_directory_it_is_read_from_is_not_a_pattern() {
        let home = fresh("rules-glob-link-home");
        let top = fresh("rules-glob-link-top");
        let base = top.join("work*");
        let sibling = top.join("workshop");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::write(sibling.join("other.env"), "").unwrap();
        fs::write(base.join("mine.env"), "").unwrap();
        let elsewhere = top.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(elsewhere.join("target.txt"), "").unwrap();
        std::os::unix::fs::symlink(elsewhere.join("target.txt"), base.join("link.env")).unwrap();

        let rules = resolve(&only(List::DenyRead, "*.env"), Some(&home), &base);

        let State::InForce(paths) = &rules.items()[0].state else {
            panic!("refused");
        };
        let root = resolved(&top);
        assert_eq!(
            paths,
            &vec![
                root.join("elsewhere/target.txt"),
                root.join("work*/mine.env")
            ]
        );
    }

    /// A deny wins at the path an allow names, and the managed layer's wins over an allow beneath
    /// it as well, which a person's own deny does not (that allow is the narrower rule).
    #[test]
    fn a_denial_decides_the_path_it_names_and_a_pinned_one_what_is_beneath() {
        let home = fresh("rules-wins-home");
        let base = fresh("rules-wins-base");
        let lists = Lists {
            allow_read: vec![entry("/data/a"), entry("/data/a/b")],
            deny_read: vec![entry("/data/a")],
            ..Lists::default()
        };
        let rules = resolve(&lists, Some(&home), &base);
        let states: Vec<bool> = rules
            .items()
            .iter()
            .map(|item| matches!(item.state, State::InForce(_)))
            .collect();
        // allow /data/a: overridden; allow /data/a/b: stands as the narrower; deny: in force.
        assert_eq!(states, vec![false, true, true]);

        let lists = Lists {
            allow_read: vec![entry("/data/a/b")],
            deny_read: vec![pinned("/data/a")],
            ..Lists::default()
        };
        let rules = resolve(&lists, Some(&home), &base);
        assert_eq!(
            rules.items()[0].state,
            State::Refused(Reason::Overridden),
            "a pinned refusal was lifted by an entry beneath it"
        );
        // A refusal of writes does not touch a read row.
        let lists = Lists {
            allow_read: vec![entry("/data/a")],
            deny_write: vec![entry("/data/a")],
            ..Lists::default()
        };
        assert!(
            resolve(&lists, Some(&home), &base)
                .refused()
                .next()
                .is_none()
        );
    }

    /// The rows a stage brings of its own do not outlive a refusal of the person's, and the
    /// person's own row beneath it does.
    #[test]
    fn a_denial_beats_the_stages_own_rows_and_a_narrower_row_of_the_persons_stands() {
        let home = fresh("rules-apply-home");
        let base = fresh("rules-apply-base");
        let lists = Lists {
            deny_read: vec![entry("/h/.config/gh"), entry("/h/p/secrets")],
            allow_read: vec![entry("/h/p/secrets/public")],
            deny_write: vec![entry("/h/p/.env")],
            allow_write: vec![entry("/h/extra")],
        };
        let rules = resolve(&lists, Some(&home), &base);
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .allow_read("/h/.config/gh")
            .allow_write("/h/p")
            .allow_write("/h/.config/gh/hosts");
        let policy = rules.apply(policy);
        assert!(!policy.readable.contains(&PathBuf::from("/h/.config/gh")));
        assert!(policy.unreadable.contains(&PathBuf::from("/h/.config/gh")));
        assert!(policy.unreadable.contains(&PathBuf::from("/h/p/secrets")));
        assert!(
            policy
                .readable
                .contains(&PathBuf::from("/h/p/secrets/public"))
        );
        assert!(policy.unwritable.contains(&PathBuf::from("/h/p/.env")));
        assert!(
            !policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/h/.config/gh/hosts")),
            "a stage's write row survived a refusal of reads above it"
        );
        assert!(
            policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/h/extra"))
        );
        assert!(policy.readable.contains(&PathBuf::from("/h/extra")));
    }

    /// A refusal with nothing granted above it holds back nothing, so it is not added: a backend
    /// refuses a policy that carries one.
    #[test]
    fn a_denial_with_no_grant_above_it_is_not_added() {
        let home = fresh("rules-nogrant-home");
        let base = fresh("rules-nogrant-base");
        let lists = Lists {
            deny_read: vec![entry("/elsewhere/x")],
            deny_write: vec![entry("/elsewhere/y")],
            ..Lists::default()
        };
        let rules = resolve(&lists, Some(&home), &base);
        let policy = rules.apply(SandboxPolicy::strict().allow_read("/usr"));
        assert!(policy.unreadable.is_empty() && policy.unwritable.is_empty());
        assert!(policy.is_meaningful());
    }

    /// A write row above a location the base refuses is not a way to write there.
    #[test]
    fn a_write_row_above_a_credential_location_does_not_write_it() {
        let home = fresh("rules-cred-home");
        let base = fresh("rules-cred-base");
        let lists = only(List::AllowWrite, "/h/.config");
        let rules = resolve(&lists, Some(&home), &base);
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/h/.config/gcloud");
        let policy = rules.apply(policy);
        assert!(
            policy
                .unwritable
                .contains(&PathBuf::from("/h/.config/gcloud"))
        );
    }
}
