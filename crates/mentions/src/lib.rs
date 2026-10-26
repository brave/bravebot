//! Naming a file with `@` in a prompt: what is offered while the name is typed, what Enter does
//! with it, and which files a sent line names.
//!
//! One implementation for every front end. The terminal calls it directly, and the desktop's
//! bridge calls it for the window, so the two cannot disagree about what a line names.
//!
//! # What an `@` reference means
//!
//! Naming a file with `@` puts its **contents into the turn as trusted input**, exactly as
//! `--file` does on the command line, because the user named it and the user is the one party whose
//! word makes something trusted. That is the whole point: a file the planner may read and compare
//! and act on, rather than a reference it can only carry.
//!
//! So this list exists to make that choice an informed one. It is drawn from the directory itself
//! rather than from anything a model said, it is shown to the person typing, and the file it names
//! becomes context only once they send the line. Sending is the grant, the same way it is for a
//! prompt recalled out of history.
//!
//! Nothing here is a decision derived from untrusted content, and nothing here reads a file's
//! contents. Filenames are content, and this walks the directory to show them to a person, which
//! is the release `bravebot_core::policy::Policy::names_for_display` already makes for the same
//! reason: the user owns the workspace, and an interface that will not tell them which files are in
//! it has protected them from nothing. No name reaches a model from here. This crate links neither
//! `bravebot-core` nor `bravebot-agent`, so none of it runs inside the driver.
#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// How many entries are offered at once.
///
/// A directory of ten thousand files would otherwise be a list nobody can read and a redraw for
/// every keystroke. Narrowing is what finds a file; the cap only bounds the first look.
pub const MAX_ENTRIES: usize = 40;

/// One thing in the workspace that a reference could name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Workspace-relative, with a trailing slash for a directory so it reads as one.
    pub path: String,
    /// Whether it is a directory, which completes to a path that can be typed further.
    pub is_directory: bool,
    /// What to show beside the path where it is not a file: a past session's age and the most a
    /// mention adds, then its title.
    pub note: Option<String>,
    /// What a typed word is looked for in besides the reference itself: a past session's title.
    pub title: Option<String>,
}

/// What follows the `@` of a reference to a past session, in place of a path: `@session:<id>`.
pub const SESSION_PREFIX: &str = "session:";

/// A row offering a past session of this directory, which completes to `@session:<id>`.
///
/// Not a file and not a directory, so completing it leaves a space after it and nothing here ever
/// reads it as a path ([`referenced`] leaves it out). `note` is what the person chooses it by.
pub fn session_entry(id: &str, title: &str, note: String) -> Entry {
    Entry {
        path: format!("{SESSION_PREFIX}{id}"),
        is_directory: false,
        note: Some(note),
        title: Some(title.to_string()),
    }
}

/// The directories the person's settings named as references, each with the alias it is written
/// under after an `@` (REFER-6).
///
/// Only what the workspace opened: a directory that could not be opened has no row here, so
/// nothing offered through it is a path the file tools would refuse.
pub type Sources = [(String, PathBuf)];

/// Where `typed` is looked for: the directory, what is left to find in it, and what is written
/// before each name offered there so the line still reads `@alias/...`.
///
/// A name under an alias is the alias's directory, unless the workspace holds an entry of that
/// name, in which case the workspace has it: a reference must not shadow a file the person can
/// already see and has always been able to name.
fn anchored<'a>(
    root: &'a Path,
    typed: &'a str,
    sources: &'a Sources,
) -> (&'a Path, &'a str, String) {
    if let Some((alias, rest)) = typed.split_once('/')
        && let Some((_, directory)) = sources.iter().find(|(name, _)| name == alias)
        && root.join(alias).symlink_metadata().is_err()
    {
        return (directory, rest, format!("{alias}/"));
    }
    (root, typed, String::new())
}

/// How long a directory must have gone unchanged before what was read from it is kept.
///
/// A modification time says only that something changed at that moment, so a listing taken in the
/// same tick as a change cannot be told from the one before it. A directory last touched longer ago
/// than this has no such change still to come in the clock's resolution, and a touched one is read
/// again every time.
const SETTLED: std::time::Duration = std::time::Duration::from_secs(2);

/// The most directories kept, so a session that walks a large tree holds a bounded amount.
///
/// Every directory a walk of the workspace reads holds at least one of the names it is bounded by,
/// so this keeps them all, with room for the few a person types into. A walk that filled the cache
/// would clear it and read the disk again on every key.
const MOST_LISTINGS: usize = MOST_WALKED + 32;

/// One name in a directory.
struct Named {
    name: String,
    is_directory: bool,
    /// Whether the directory held the name as text. A name that is not UTF-8 is held as a
    /// rendering of it that every name differing only in those bytes shares, so it stands for no
    /// one file and a line written from it names none.
    is_text: bool,
}

impl Named {
    fn new(name: std::ffi::OsString, is_directory: bool) -> Self {
        match name.into_string() {
            Ok(name) => Self {
                name,
                is_directory,
                is_text: true,
            },
            Err(name) => Self {
                name: name.to_string_lossy().to_string(),
                is_directory,
                is_text: false,
            },
        }
    }
}

type Listing = std::sync::Arc<Vec<Named>>;

/// The names in a directory, sorted, and whether each is a directory, read again only when it has
/// changed.
///
/// Typing towards a path asks about the same directory on every key, and a directory of many files
/// makes each of those a full read. One `stat` answers whether the read is still good.
fn listing(canonical: &Path) -> Option<Listing> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::SystemTime;
    static KEPT: Mutex<Option<HashMap<PathBuf, (SystemTime, Listing)>>> = Mutex::new(None);

    let modified = std::fs::metadata(canonical)
        .and_then(|meta| meta.modified())
        .ok();
    let settled = modified.is_some_and(|at| {
        SystemTime::now()
            .duration_since(at)
            .is_ok_and(|age| age >= SETTLED)
    });
    if let (true, Some(at)) = (settled, modified) {
        let kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((read_at, names)) = kept.as_ref().and_then(|kept| kept.get(canonical))
            && *read_at == at
        {
            return Some(names.clone());
        }
    }

    let mut read: Vec<Named> = std::fs::read_dir(canonical)
        .ok()?
        .flatten()
        .map(|entry| {
            let is_directory = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Named::new(entry.file_name(), is_directory)
        })
        .collect();
    // Sorted so that where the walk stops at its bound does not depend on the order the
    // filesystem hands names over.
    read.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Listing = std::sync::Arc::new(read);
    if let (true, Some(at)) = (settled, modified) {
        let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let kept = kept.get_or_insert_with(HashMap::new);
        if kept.len() >= MOST_LISTINGS {
            kept.clear();
        }
        kept.insert(canonical.to_path_buf(), (at, names.clone()));
    }
    Some(names)
}

/// Entries matching a half-typed reference: what one directory holds, then what is found by
/// looking further.
///
/// `typed` is what follows the `@`. An empty one lists the workspace root. Anything with a slash in
/// it lists the directory named up to the last slash, so `crates/t` offers what is in `crates`.
/// Directories come first and then files, each alphabetical, because a reference is usually typed
/// by walking into one, and being able to go deeper matters more than the file that happens to sort
/// first.
///
/// A fragment with no slash is also looked for in every directory below the root, and so is one
/// with a slash where its directory offers nothing (NAME-11). Up to [`MOST_FOUND`] of those names
/// follow, best match first, in whatever room the list has after what the directory offered.
///
/// A name no line can carry to the turn is not offered: a file or directory whose name holds a line
/// break, and a file whose name ends in whitespace (NAME-8).
pub fn matching(root: &Path, typed: &str, sources: &Sources) -> Vec<Entry> {
    // Refused rather than resolved: `..` would walk out of the workspace, and an absolute path
    // names something the reference syntax has no business reaching.
    if typed.contains("..") || typed.starts_with('/') {
        return Vec::new();
    }

    let anchor = anchored(root, typed, sources);
    // A name under an alias is the alias directory's to answer, which the walk does not cover.
    let under_an_alias = !anchor.2.is_empty();
    let mut entries = in_one_directory(typed, sources, anchor);
    if !typed.is_empty() && !under_an_alias && (!typed.contains('/') || entries.is_empty()) {
        let found = found_beneath(root, typed, &entries, MOST_WALKED);
        entries.extend(found);
    }
    entries
}

/// What the directory `typed` is being typed in holds that begins as it does.
fn in_one_directory(
    typed: &str,
    sources: &Sources,
    (root, typed_here, tag): (&Path, &str, String),
) -> Vec<Entry> {
    let (directory, prefix) = match typed_here.rsplit_once('/') {
        Some((directory, prefix)) => (directory, prefix),
        None => ("", typed_here),
    };

    let listed = root.join(directory);
    // Confined the same way every other path is: a symlinked subdirectory pointing out of the
    // workspace must not become a way to browse the filesystem.
    let Ok(canonical) = listed.canonicalize() else {
        return Vec::new();
    };
    let Ok(canonical_root) = root.canonicalize() else {
        return Vec::new();
    };
    if !canonical.starts_with(&canonical_root) {
        return Vec::new();
    }

    let Some(listing) = listing(&canonical) else {
        return Vec::new();
    };

    let mut entries: Vec<Entry> = listing
        .iter()
        .filter_map(|named| {
            let name = &named.name;
            if !name.starts_with(prefix) {
                return None;
            }
            // Not offered: a name that is not text has no spelling a line can carry. The one it is
            // held under is shared by every name that differs only in those bytes, so a line
            // written from it names another file, or none.
            if !named.is_text {
                return None;
            }
            // Skipped rather than offered: a reference into `.git` is never what was meant, and
            // offering the whole of it buries everything else. The names come from the walk's own
            // list so that what a person is shown and what a search covers are one idea of the
            // tree, rather than two that disagree about `.hg`.
            //
            // Decided from the name alone, without asking what the entry is. A worktree's `.git`
            // is a regular file holding a pointer to the real one, and a `node_modules` a person
            // symlinked elsewhere is a symlink, so a type test would offer both of them back.
            if bravebot_filetype::is_ignored_directory(name) {
                return None;
            }
            // Left out rather than offered as text that names something else (NAME-8).
            if !is_writable(name, named.is_directory) {
                return None;
            }
            let path = if directory.is_empty() {
                format!("{tag}{name}")
            } else {
                format!("{tag}{directory}/{name}")
            };
            Some(Entry {
                path: if named.is_directory {
                    format!("{path}/")
                } else {
                    path
                },
                is_directory: named.is_directory,
                note: None,
                title: None,
            })
        })
        .collect();

    // The aliases, offered beside the workspace's own entries while the first word is typed. An
    // alias the workspace has an entry of its name for is left out, as `anchored` does.
    if tag.is_empty() && !typed.contains('/') {
        for (alias, _) in sources {
            if alias.starts_with(typed) && root.join(alias).symlink_metadata().is_err() {
                entries.push(Entry {
                    path: format!("{alias}/"),
                    is_directory: true,
                    note: None,
                    title: None,
                });
            }
        }
    }

    // Sorted so the list is the same on every platform: `read_dir` returns whatever order the
    // filesystem holds, which would otherwise reshuffle the offered entries between machines.
    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then_with(|| a.path.cmp(&b.path))
    });
    entries.truncate(MAX_ENTRIES);
    entries
}

/// How many names found by the walk are offered at most.
///
/// A short fragment holds its characters in the path of most of a large tree, and the terminal
/// draws every row under the box, so forty of them would fill the screen at the first letter and
/// push the conversation out of it. The best ten are what a person chooses from, and typing more
/// narrows them.
const MOST_FOUND: usize = 10;

/// The most names the walk of the workspace reads before it stops.
///
/// A fragment is looked for in every directory, so a tree of a million files would make each key a
/// walk of all of them. The walk goes one level at a time from the root, so the names a stop leaves
/// out are the deepest ones, and a path typed to its end is still answered by its own directory.
const MOST_WALKED: usize = 10_000;

/// A name the walk found, and how well it matches.
struct Found {
    /// Whether the name or only its path matched, and then how closely: see [`closeness`].
    rank: (u8, u8),
    path: String,
    is_directory: bool,
}

/// The names below the workspace root that `typed` runs through, best match first, as many as the
/// list has room for after `offered`, which they do not repeat, and at most [`MOST_FOUND`]. At most
/// `most_read` names are read.
///
/// Case is not compared. A name holding the characters of `typed` in order ranks before one whose
/// path holds them, and within each, a match that starts the text ranks before one inside it, and
/// that before one with other characters between. Then the shorter path, then the alphabetical
/// one, so the order is the same on every platform.
///
/// Nothing is followed. A link is a name like any other, whatever it points at, so a link out of
/// the workspace leads nowhere here and a link back into it does not offer the same files twice.
/// The names skipped are the ones the directory listing skips (`is_ignored_directory`), and a name
/// that is not text is left out as it is there.
fn found_beneath(root: &Path, typed: &str, offered: &[Entry], most_read: usize) -> Vec<Entry> {
    let room = MAX_ENTRIES.saturating_sub(offered.len()).min(MOST_FOUND);
    let Ok(root) = root.canonicalize() else {
        return Vec::new();
    };
    if room == 0 {
        return Vec::new();
    }

    // Folded a character at a time, as the names are, since `str::to_lowercase` reads the word-final
    // sigma of `ΟΔΟΣ` as `ς` and the name is folded to `σ`.
    let wanted: String = typed.chars().flat_map(char::to_lowercase).collect();
    let mut found: Vec<Found> = Vec::new();
    let mut read = 0;
    // Each directory still to read, with the path from the root that names it.
    let mut pending = VecDeque::from([(root, String::new())]);
    'walk: while let Some((directory, relative)) = pending.pop_front() {
        let Some(names) = listing(&directory) else {
            continue;
        };
        for named in names.iter() {
            if read == most_read {
                break 'walk;
            }
            read += 1;
            if !named.is_text || bravebot_filetype::is_ignored_in(&directory, &named.name) {
                continue;
            }
            // Left out rather than offered as text that names something else (NAME-8).
            if !is_writable(&named.name, named.is_directory) {
                continue;
            }

            let slash = (!relative.is_empty()).then_some('/');
            let end = named.is_directory.then_some('/');
            let rank = closeness(&wanted, named.name.chars().chain(end))
                .map(|close| (0, close))
                .or_else(|| {
                    let path = relative.chars().chain(slash).chain(named.name.chars());
                    closeness(&wanted, path.chain(end)).map(|close| (1, close))
                });
            if rank.is_none() && !named.is_directory {
                continue;
            }

            let here = if relative.is_empty() {
                named.name.clone()
            } else {
                format!("{relative}/{}", named.name)
            };
            if named.is_directory {
                pending.push_back((directory.join(&named.name), here.clone()));
            }
            if let Some(rank) = rank {
                found.push(Found {
                    rank,
                    path: if named.is_directory {
                        format!("{here}/")
                    } else {
                        here
                    },
                    is_directory: named.is_directory,
                });
            }
        }
    }

    found.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.path.len().cmp(&b.path.len()))
            .then_with(|| a.path.cmp(&b.path))
    });
    found
        .into_iter()
        // A row that is what was typed completes to nothing, and `offered` has the ones the
        // directory held.
        .filter(|found| {
            found.path != typed && !offered.iter().any(|entry| entry.path == found.path)
        })
        .take(room)
        .map(|found| Entry {
            path: found.path,
            is_directory: found.is_directory,
            note: None,
            title: None,
        })
        .collect()
}

/// How closely `wanted`, which is lowercase, matches `text` without regard to case: `Some(0)` where
/// `text` starts with it, `Some(1)` where it is inside `text`, `Some(2)` where its characters are
/// in `text` in order with others between them, and `None` where they are not.
fn closeness(wanted: &str, text: impl Iterator<Item = char> + Clone) -> Option<u8> {
    let folded = || text.clone().flat_map(char::to_lowercase);
    let mut left = wanted.chars().peekable();
    for c in folded() {
        if left.peek() == Some(&c) {
            left.next();
        }
    }
    if left.peek().is_some() {
        return None;
    }
    let folded: String = folded().collect();
    Some(if folded.starts_with(wanted) {
        0
    } else if folded.contains(wanted) {
        1
    } else {
        2
    })
}

/// What follows an `@` at the end of the line, if the line is being typed towards a reference.
///
/// `None` unless the last word begins with `@`, so a reference already finished by a space is left
/// alone and an ordinary prompt offers nothing. That is what closes the list.
///
/// The path comes back unescaped, so a `\ ` the person typed to keep a space in the name is a plain
/// space here, the way [`matching`] and [`names_a_file`] expect it.
pub fn typed_reference(line: &str) -> Option<String> {
    let (start, end) = word_spans(line).pop()?;
    // Only while it is still being typed: a space after a reference means the user moved on.
    if end != line.len() {
        return None;
    }
    line[start..end].strip_prefix('@').map(unescape)
}

/// Whether `c` ends a line of text.
///
/// A reference lives on one line, so a backslash before one is an ordinary character: a prompt
/// whose line ends in a backslash does not take the first word of the next line into a name.
fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Whether a backslash in front of `c` is an escape, which keeps `c` in the word as itself.
///
/// A backslash before anything else is an ordinary character, which keeps a sentence containing
/// one from naming a path. [`word_spans`], [`unescape`] and [`escape`] all ask this, so the three
/// cannot disagree about where a name ends or what it holds.
fn is_escapable(c: char) -> bool {
    c == '\\' || (c.is_whitespace() && !is_line_break(c))
}

/// Whether a line can carry `name` to the turn, so that the file is the one the line names.
///
/// A line is one line, so a name holding a line break (U+000A, U+000D, U+2028 or U+2029) cannot be
/// written in it. A line is also trimmed when it is sent, in the terminal and in the
/// window, so a file whose name ends in whitespace would be sent without the end of its name. U+FEFF
/// is trimmed by the window and not by Rust's `trim`, so it counts as whitespace here. A directory
/// name is followed by a slash and loses nothing.
fn is_writable(name: &str, is_directory: bool) -> bool {
    !name.chars().any(is_line_break)
        && (is_directory || !name.ends_with(|c: char| c.is_whitespace() || c == '\u{feff}'))
}

/// Where each word of a line begins and ends.
///
/// Words end at whitespace, except that a whitespace character written after a backslash belongs to
/// the word, so `@My\ Documents/a.md` is one. A backslash written after a backslash belongs to it
/// as well, so a name ending in a backslash can end a word. A line break always ends one.
fn word_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    let mut chars = line.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c.is_whitespace() {
            if let Some(begun) = start.take() {
                spans.push((begun, at));
            }
            continue;
        }
        start.get_or_insert(at);
        if c == '\\' && chars.next_if(|&(_, next)| is_escapable(next)).is_some() {
            continue;
        }
    }
    if let Some(begun) = start {
        spans.push((begun, line.len()));
    }
    spans
}

/// Where the last word of the line begins, which is where a completed reference is written.
pub fn last_word_starts_at(line: &str) -> usize {
    word_spans(line)
        .pop()
        .map_or(line.len(), |(start, _)| start)
}

/// A path as it is written in a line: each whitespace character and each backslash behind a
/// backslash, so the name stays in the word and ends where it ends ([`unescape`] reads it back).
///
/// A line break has no written form, so a name holding one is not offered ([`matching`]).
pub fn escape(path: &str) -> String {
    let mut written = String::with_capacity(path.len());
    for c in path.chars() {
        if is_escapable(c) {
            written.push('\\');
        }
        written.push(c);
    }
    written
}

/// The name a written reference stands for: each escape dropped and the character behind it kept.
fn unescape(written: &str) -> String {
    let mut name = String::with_capacity(written.len());
    let mut chars = written.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.next_if(|&next| c == '\\' && is_escapable(next)) {
            Some(escaped) => name.push(escaped),
            None => name.push(c),
        }
    }
    name
}

/// Whether what has been typed already names a file in the workspace.
///
/// Asked of the workspace rather than of [`matching`], because that list is capped at
/// [`MAX_ENTRIES`] for display and a name is no less finished for having been cut from it. Deciding
/// from the offered entries lets the number of siblings a directory happens to hold change what
/// Enter does: forty directories sharing a prefix sort above the file and push it out, so a
/// finished `@test` is completed away into a directory nobody chose.
///
/// A directory is not a file here, since it is somewhere to type through rather than something to
/// read, which is the same distinction [`referenced`] makes. Decided without following a symlink,
/// so the answer agrees with the way [`matching`] classifies the same entry: a name the list would
/// offer as a file is a finished name.
pub fn names_a_file(root: &Path, typed: &str, sources: &Sources) -> bool {
    // `..` and an absolute path are refused rather than resolved, the same string refusal
    // `matching` makes of what is being typed. Not a confinement check, and it cannot become one:
    // `matching` offers a symlink pointing out of the workspace as a file of its own, and a name
    // the list offers has to count as finished or Enter completes it away, which is the whole
    // thing this exists to stop.
    if typed.contains("..") || typed.starts_with('/') {
        return false;
    }
    // A session is named by its id and is no file, even where a file of that name exists: the
    // prefix is what the list offers it under, so it is what Enter reads it by.
    if typed.starts_with(SESSION_PREFIX) {
        return false;
    }
    let (root, typed, _) = anchored(root, typed, sources);
    root.join(typed)
        .symlink_metadata()
        .is_ok_and(|named| !named.is_dir())
}

/// Every file named with `@` in a line, in the order they were written.
///
/// This is what becomes a turn's context. A trailing slash is dropped, since a directory is a place
/// to type through rather than a file to read, and one named anyway is not a file to include.
pub fn referenced(line: &str) -> Vec<String> {
    words_after_at(line)
        .filter(|path| !path.is_empty() && !path.ends_with('/') && !is_a_session(path))
        .collect()
}

/// Every past session named with `@session:<id>` in a line, by id, in the order written and
/// without repeats.
///
/// Only the shape is decided here: whether the id names a record of this directory is for the
/// reader of the record to say, and a name that is none refuses the send rather than dropping out.
pub fn referenced_sessions(line: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for word in words_after_at(line) {
        if let Some(id) = word
            .strip_prefix(SESSION_PREFIX)
            .filter(|id| !id.is_empty())
            && !ids.iter().any(|seen| seen == id)
        {
            ids.push(id.to_string());
        }
    }
    ids
}

fn is_a_session(word: &str) -> bool {
    word.starts_with(SESSION_PREFIX)
}

fn words_after_at(line: &str) -> impl Iterator<Item = String> + '_ {
    word_spans(line)
        .into_iter()
        .filter_map(|(start, end)| line[start..end].strip_prefix('@'))
        .map(unescape)
}

/// A name written as `alias/path` as the file it means: the path under the alias's directory,
/// absolute, so the turn reads it the way it reads any file in a directory opened by name.
///
/// Every other name comes back as it was. A name with `..` in it is left for the read to refuse,
/// the way NAME-5 refuses it everywhere else.
///
/// The result is text, because a turn is handed its files as text. A reference directory whose path
/// is not valid UTF-8 has no spelling that names it, and a lossy one would name a different
/// directory, so a name under it is refused with [`NotText`] and nothing is resolved.
pub fn resolved(root: &Path, name: &str, sources: &Sources) -> Result<String, NotText> {
    let (base, rest, tag) = anchored(root, name, sources);
    if tag.is_empty() || rest.is_empty() || name.contains("..") {
        return Ok(name.to_string());
    }
    base.join(rest)
        .into_os_string()
        .into_string()
        .map_err(|_| NotText)
}

/// A reference directory whose path is not valid UTF-8, so a name under it cannot be written as the
/// text a turn takes its files in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotText;

impl std::fmt::Display for NotText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the reference directory's path is not valid UTF-8")
    }
}

impl std::error::Error for NotText {}

/// Whether Enter on a half-typed reference completes it rather than sending the line.
///
/// `offered` is what [`matching`] returned for `typed`, and `cursor` is the row the person moved to,
/// clamped to the list as it is drawn. A name the person finished typing is a finished sentence,
/// whatever the list happens to be highlighting: `@test` names a file of its own while a `tests/`
/// beside it sorts above. Walking the list with the arrows is a choice among the rows and still
/// wins, which is why this asks about the untouched cursor.
///
/// Asked of the workspace through [`names_a_file`] rather than of `offered`, which is capped for
/// display: forty directories sharing the prefix sort above the file and cut it from the list, and
/// scanning the list would then complete a finished name away into a directory nobody chose.
pub fn enter_completes(
    root: &Path,
    typed: &str,
    offered: &[Entry],
    cursor: usize,
    sources: &Sources,
) -> bool {
    if cursor == 0 && names_a_file(root, typed, sources) {
        return false;
    }
    offered
        .get(cursor.min(offered.len().saturating_sub(1)))
        .is_some_and(|entry| typed != entry.path)
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "`aliases.rs` is shared with the agent's tests, and the mentions tests call only its `held_names`"
)]
#[path = "../../agent/test-support/aliases.rs"]
mod aliases;

#[cfg(test)]
mod tests {
    use super::aliases;
    use super::*;

    /// A scratch workspace, removed with the test.
    struct Scratch {
        path: std::path::PathBuf,
        _held: tempfile::TempDir,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let held = tempfile::Builder::new()
                .prefix(&format!("bravebot-mentions-{name}-"))
                .tempdir()
                .expect("scratch");
            let path = held.path().to_path_buf();
            std::fs::create_dir_all(path.join("crates/tui")).expect("create");
            std::fs::create_dir_all(path.join("target")).expect("create");
            std::fs::create_dir_all(path.join(".git")).expect("create");
            std::fs::create_dir_all(path.join("node_modules")).expect("create");
            std::fs::create_dir_all(path.join(".hg")).expect("create");
            std::fs::create_dir_all(path.join("dist")).expect("create");
            std::fs::create_dir_all(path.join(".worktrees/feature")).expect("create");
            std::fs::write(path.join("Cargo.toml"), "").expect("write");
            std::fs::write(path.join("Makefile"), "").expect("write");
            // What a worktree or a submodule has in place of the directory: a file naming the
            // real one.
            std::fs::write(path.join("crates/tui/.git"), "gitdir: /elsewhere\n").expect("write");
            std::fs::write(path.join("crates/tui/lib.rs"), "").expect("write");
            Self { path, _held: held }
        }
    }

    fn paths(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.path.as_str()).collect()
    }

    /// Back-dates a directory's modification time, so it reads as one nothing has touched lately.
    fn settled_at(path: &std::path::Path, hours_ago: u64) {
        let when = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(1_700_000_000 - hours_ago * 3600);
        std::fs::File::open(path)
            .expect("open")
            .set_modified(when)
            .expect("set modified");
    }

    /// A directory nothing has touched for a while is read once, however many keys ask about it.
    /// The directory is changed behind the cache's back and its modification time put back, so the
    /// only way the old names can still come out is that the directory was not read again.
    #[test]
    fn a_settled_directory_is_not_read_again_while_it_is_unchanged() {
        let scratch = Scratch::new("settled");
        let listed = scratch.path.join("crates/tui");
        settled_at(&listed, 5);
        let before = matching(&scratch.path, "crates/tui/", &[]);
        assert_eq!(paths(&before), ["crates/tui/lib.rs"]);

        std::fs::write(listed.join("extra.rs"), "").expect("write");
        settled_at(&listed, 5);
        let again = matching(&scratch.path, "crates/tui/", &[]);
        assert_eq!(
            paths(&again),
            ["crates/tui/lib.rs"],
            "an unchanged directory was read again"
        );
    }

    /// A directory whose modification time has moved is read again, and one touched just now is
    /// read every time, since a change in the same clock tick as a read cannot be told apart.
    #[test]
    fn a_changed_or_recently_touched_directory_is_read_afresh() {
        let scratch = Scratch::new("changed");
        let listed = scratch.path.join("crates/tui");
        settled_at(&listed, 5);
        let _ = matching(&scratch.path, "crates/tui/", &[]);

        std::fs::write(listed.join("extra.rs"), "").expect("write");
        settled_at(&listed, 6);
        let moved = matching(&scratch.path, "crates/tui/", &[]);
        assert_eq!(
            paths(&moved),
            ["crates/tui/extra.rs", "crates/tui/lib.rs"],
            "a directory whose time moved was served from the cache"
        );

        let now = std::time::SystemTime::now();
        let stamp = |path: &std::path::Path| {
            std::fs::File::open(path)
                .expect("open")
                .set_modified(now)
                .expect("set modified");
        };
        stamp(&listed);
        let _ = matching(&scratch.path, "crates/tui/", &[]);
        std::fs::write(listed.join("newest.rs"), "").expect("write");
        stamp(&listed);
        let touched = matching(&scratch.path, "crates/tui/", &[]);
        assert!(
            paths(&touched).contains(&"crates/tui/newest.rs"),
            "a directory touched just now was served from the cache"
        );
    }

    /// An empty reference lists the root, directories first, so walking deeper is the first thing
    /// offered.
    #[test]
    fn an_empty_reference_lists_the_root_with_directories_first() {
        let scratch = Scratch::new("root");
        let offered = matching(&scratch.path, "", &[]);
        assert_eq!(paths(&offered), vec!["crates/", "Cargo.toml", "Makefile"]);
    }

    /// A prefix narrows the list, which is what makes typing a path possible at all.
    #[test]
    fn a_prefix_narrows_the_list() {
        let scratch = Scratch::new("prefix");
        assert_eq!(paths(&matching(&scratch.path, "Ma", &[])), vec!["Makefile"]);
        assert!(matching(&scratch.path, "zz", &[]).is_empty());
    }

    /// A slash lists the directory it names, so a reference is typed by walking into one.
    #[test]
    fn a_slash_lists_what_is_inside_that_directory() {
        let scratch = Scratch::new("nested");
        // Whole paths, not just the last segment: what is offered is what gets typed.
        assert_eq!(
            paths(&matching(&scratch.path, "crates/", &[])),
            vec!["crates/tui/"]
        );
        assert_eq!(
            paths(&matching(&scratch.path, "crates/tui/l", &[])),
            vec!["crates/tui/lib.rs"]
        );
    }

    /// Build output and history are never what a reference means, and offering them buries the rest.
    ///
    /// The names withheld are the ones every walk of the tree steps over, so a Mercurial checkout
    /// and a JavaScript project get as readable a list as a git checkout of this repository does.
    #[test]
    fn noise_directories_are_not_offered() {
        let scratch = Scratch::new("noise");
        let offered = matching(&scratch.path, "", &[]);
        let root = paths(&offered);
        assert!(!root.contains(&"target/"), "build output was offered");
        assert!(!root.contains(&".git/"), "version control was offered");
        assert!(
            !root.contains(&"node_modules/"),
            "dependencies were offered"
        );
        assert!(!root.contains(&".hg/"), "version control was offered");
        assert!(!root.contains(&"dist/"), "build output was offered");
        assert!(
            !root.contains(&".worktrees/"),
            "a linked worktree's copy of the tree was offered"
        );
        // Withheld under the name a checkout of this shape gives it, which is a file.
        assert!(
            !paths(&matching(&scratch.path, "crates/tui/", &[])).contains(&"crates/tui/.git"),
            "version control was offered"
        );
    }

    /// The completion must not become a way to browse the filesystem: `..` and an absolute path are
    /// refused rather than resolved, exactly as the workspace refuses them.
    #[test]
    fn a_reference_cannot_climb_out_of_the_workspace() {
        let scratch = Scratch::new("escape");
        assert!(matching(&scratch.path, "../", &[]).is_empty());
        assert!(matching(&scratch.path, "../../etc/", &[]).is_empty());
        assert!(matching(&scratch.path, "/etc/", &[]).is_empty());
    }

    /// A reference is offered while the word is being typed, and left alone once a space says the
    /// user moved on.
    #[test]
    fn what_counts_as_a_reference_being_typed() {
        assert_eq!(typed_reference("look at @Car").as_deref(), Some("Car"));
        assert_eq!(typed_reference("@").as_deref(), Some(""));
        assert_eq!(typed_reference("@Cargo.toml "), None, "finished by a space");
        assert_eq!(typed_reference("an ordinary prompt"), None);
        assert_eq!(typed_reference(""), None);
        // An address is not a reference: only the last word counts, and this one is not it.
        assert_eq!(typed_reference("mail me@example.com now"), None);
    }

    /// Every referenced file becomes context, since each is one the user named.
    #[test]
    fn every_referenced_file_is_collected() {
        assert_eq!(
            referenced("compare @a.rs with @b.rs please"),
            vec!["a.rs".to_string(), "b.rs".to_string()]
        );
        assert!(referenced("no references here").is_empty());
    }

    /// A directory is a place to type through, not a file to read, so it is not collected.
    #[test]
    fn a_directory_is_not_collected_as_a_file() {
        assert!(referenced("look in @crates/").is_empty());
        assert_eq!(referenced("@crates/tui/lib.rs"), vec!["crates/tui/lib.rs"]);
    }

    /// An address inside a sentence is not a reference: only a word that begins with `@` is, so
    /// nothing in `me@example.com` is collected, and a real reference beside it still is.
    #[test]
    fn an_address_inside_a_sentence_is_not_collected_as_a_reference() {
        assert!(referenced("mail me@example.com now").is_empty());
        assert_eq!(
            referenced("mail me@example.com about @notes.md"),
            vec!["notes.md".to_string()]
        );
    }

    /// A finished name is decided from the workspace, so the cap that bounds what is displayed
    /// cannot change the answer, and a directory is still somewhere to type through.
    #[test]
    fn what_counts_as_already_naming_a_file() {
        let scratch = Scratch::new("finished");
        assert!(names_a_file(&scratch.path, "Makefile", &[]));
        assert!(names_a_file(&scratch.path, "crates/tui/lib.rs", &[]));
        assert!(!names_a_file(&scratch.path, "crates", &[]), "a directory");
        assert!(!names_a_file(&scratch.path, "Make", &[]), "half typed");
        assert!(!names_a_file(&scratch.path, "", &[]), "a bare `@`");

        // More siblings sharing the prefix than the list can hold. They sort above the file, so
        // the cut takes the file first and nothing about it having been typed has changed.
        for n in 0..MAX_ENTRIES + 5 {
            std::fs::create_dir_all(scratch.path.join(format!("Makefile{n:03}"))).expect("create");
        }
        let offered = matching(&scratch.path, "Makefile", &[]);
        assert!(
            !paths(&offered).contains(&"Makefile"),
            "the file was still offered, so the cap was never reached"
        );
        assert!(names_a_file(&scratch.path, "Makefile", &[]));
    }

    /// A symlink counts as whatever the offered list calls it. The list classifies without
    /// following one, so a symlink to a directory is offered as a file: following it here would
    /// mean a name the user can see offered stops counting as finished, which is the same
    /// completed-away failure from the other direction.
    #[cfg(unix)]
    #[test]
    fn a_symlink_is_a_finished_name_because_the_list_offers_it_as_one() {
        let scratch = Scratch::new("finished-symlink");
        std::os::unix::fs::symlink("crates", scratch.path.join("notes")).expect("link");
        assert_eq!(
            matching(&scratch.path, "notes", &[]),
            vec![Entry {
                path: "notes".to_string(),
                is_directory: false,
                note: None,
                title: None,
            }],
            "the list offers a symlink as a file, following nothing"
        );
        assert!(names_a_file(&scratch.path, "notes", &[]));
    }

    /// Nothing outside the workspace is a name this answers for, the same refusal `matching` makes.
    #[test]
    fn dots_and_an_absolute_path_are_not_finished_names() {
        let scratch = Scratch::new("finished-escape");
        // The workspace is the nested directory, so the file one level up is genuinely outside it
        // while still existing, which is what makes the refusal say anything.
        let root = scratch.path.join("crates");
        let outside = scratch.path.join("Makefile");
        assert!(names_a_file(&scratch.path, "Makefile", &[]), "it is there");
        assert!(!names_a_file(&root, "../Makefile", &[]), "climbing out");
        assert!(
            !names_a_file(&root, outside.to_str().expect("utf8"), &[]),
            "an absolute path"
        );
    }

    /// A bare `@` names nothing, so it is a character in a sentence rather than a file.
    #[test]
    fn a_bare_at_sign_names_nothing() {
        assert!(referenced("what does @ do").is_empty());
    }

    /// A backslash before a space keeps the space in the name, in what is being typed and in what
    /// is sent, and only there: a backslash anywhere else ends nothing and starts nothing.
    #[test]
    fn a_backslash_before_a_space_continues_a_reference() {
        assert_eq!(
            typed_reference(r"read @My\ Documents/no").as_deref(),
            Some("My Documents/no")
        );
        assert_eq!(
            typed_reference(r"read @My\ ").as_deref(),
            Some("My "),
            "an escaped space at the end is still being typed"
        );
        assert_eq!(
            typed_reference(r"read @My\ Documents/notes.md ").as_deref(),
            None,
            "an unescaped space finishes it"
        );
        assert_eq!(
            referenced(r"compare @My\ Documents/a.md with @b.md"),
            vec!["My Documents/a.md".to_string(), "b.md".to_string()]
        );
        assert!(referenced(r"@My\ Documents/").is_empty(), "a directory");
        // Ordinary prose with a backslash names nothing.
        assert!(referenced(r"a path like C:\dir\ and more").is_empty());
        assert_eq!(referenced(r"see @a\b.md now"), vec![r"a\b.md".to_string()]);
        assert_eq!(typed_reference(r"C:\ and so on"), None);
    }

    /// The escaped form of a path with a space reads back as the same path.
    #[test]
    fn an_escaped_path_reads_back_unchanged() {
        for path in ["a b/c d.md", r"odd\ name.md", "plain.md", r"back\slash.md"] {
            let line = format!("@{}", escape(path));
            assert_eq!(referenced(&line), vec![path.to_string()], "{line}");
            assert_eq!(typed_reference(&line).as_deref(), Some(path), "{line}");
        }
    }

    /// A backslash keeps the character behind it in the name when that character is whitespace or
    /// another backslash, and is an ordinary character before anything else (NAME-8). The name a
    /// completion writes is read back whole, and a line that ends in a backslash does not reach
    /// into the next one.
    #[test]
    fn a_backslash_before_whitespace_or_a_backslash_keeps_it_in_the_name() {
        assert_eq!(escape("a\tb.md"), "a\\\tb.md");
        assert_eq!(escape("a\u{a0}b.md"), "a\\\u{a0}b.md");
        assert_eq!(escape("a.md\\"), "a.md\\\\");
        assert_eq!(escape("a\\b.md"), "a\\\\b.md");

        for name in [
            "a\tb.md",
            "a\u{a0}b.md",
            "a\u{202f}b.md",
            "a\u{3000}b.md",
            "a\u{b}b.md",
            "a\u{c}b.md",
            "a\u{85}b.md",
            "\ta.md",
            " a.md",
            "a.md\\",
            "a.md\\\\",
            "a\\ b.md",
            "a\\\tb.md",
            "\\",
            "\\\\ b",
        ] {
            let written = escape(name);
            assert_eq!(
                referenced(&format!("see @{written} now")),
                vec![name.to_string()],
                "{name:?} written as {written:?}"
            );
            assert_eq!(
                typed_reference(&format!("see @{written}")).as_deref(),
                Some(name),
                "{name:?} written as {written:?} while it is typed"
            );
            assert_eq!(
                last_word_starts_at(&format!("see @{written}")),
                4,
                "{name:?} written as {written:?} is not one word"
            );
        }

        // What a person wrote before whitespace and backslashes were written behind one: a
        // backslash before an ordinary character, and an escaped space, read as they did.
        assert_eq!(referenced(r"see @a\b.md now"), vec![r"a\b.md".to_string()]);
        assert_eq!(referenced(r"see @a.md\ now"), vec!["a.md now".to_string()]);
        // Two backslashes are one now, which ends a name in a backslash instead of continuing it.
        assert_eq!(referenced(r"see @a.md\\ now"), vec![r"a.md\".to_string()]);
        assert_eq!(typed_reference(r"see @a.md\\").as_deref(), Some(r"a.md\"));

        for other in ["\t", "\n", "\u{a0}", "\u{3000}"] {
            assert_eq!(
                referenced(&format!("see @a.md{other}b now")),
                vec!["a.md".to_string()],
                "{other:?} not written behind a backslash ends the reference"
            );
        }
        // A line break is never kept: a line ending in a backslash, as a shell command or a path
        // does, leaves the next line's reference alone.
        for line_break in ["\n", "\r\n", "\u{2028}"] {
            assert_eq!(
                referenced(&format!("run C:\\dir\\{line_break}@notes.md summarise")),
                vec!["notes.md".to_string()],
                "{line_break:?} after a backslash"
            );
            assert_eq!(
                referenced(&format!("see @a.md\\{line_break}and @b.md")),
                vec![r"a.md\".to_string(), "b.md".to_string()],
                "{line_break:?} after a name ending in a backslash"
            );
        }
    }

    /// Names that differ only by the kind of whitespace inside them look alike in the list. The
    /// line a pick writes names that row's file and no other, so the file a rule is recorded for is
    /// the one chosen (NAME-2, TRUST-2).
    #[test]
    fn a_pick_among_names_that_look_alike_names_the_one_picked() {
        let scratch = Scratch::new("pick-look-alike");
        let names = [
            "a b.md",
            "a\u{a0}b.md",
            "a\u{202f}b.md",
            "a\u{3000}b.md",
            "a\tb.md",
            "a\\b.md",
            "a\\ b.md",
            "a.md\\",
        ];
        for name in names {
            let _ = std::fs::write(scratch.path.join(name), "");
        }
        let held = std::fs::read_dir(&scratch.path)
            .expect("read")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('a'))
            .count();
        assert!(held >= 5, "only {held} of the names could be made here");

        // The rows the directory holds for the prefix. Names found below it or by a fragment
        // follow them and are not what this asks about.
        let offered: Vec<Entry> = matching(&scratch.path, "a", &[])
            .into_iter()
            .filter(|entry| entry.path.starts_with('a'))
            .collect();
        assert_eq!(offered.len(), held, "{:?}", paths(&offered));
        let mut lines = Vec::new();
        for entry in &offered {
            let line = format!("see @{} now", escape(&entry.path));
            assert_eq!(
                referenced(&line),
                vec![entry.path.clone()],
                "picking {:?} writes {line:?}",
                entry.path
            );
            lines.push(line);
        }
        lines.sort();
        lines.dedup();
        assert_eq!(lines.len(), offered.len(), "two rows wrote the same line");
    }

    /// A name no line can carry to the turn is left out of the list, since offering it would write
    /// text that names another file or none: one holding a line break, and a file whose name ends
    /// in whitespace, which the trim made of a line when it is sent takes with it (NAME-8). A
    /// directory is followed by a slash and loses nothing at its end.
    #[test]
    fn a_name_a_line_cannot_carry_is_not_offered() {
        let scratch = Scratch::new("not-offered");
        let hidden = [
            "a.md ",
            "a.md\t",
            "a.md\n",
            "a.md\u{a0}",
            "a.md\u{3000}",
            "a.md\u{feff}",
            "a\nb.md",
            "a\rb.md",
            "a\u{2028}b.md",
            "a\u{2029}b.md",
        ];
        let offered_names = ["a.md", "a\u{a0}b.md", "a\tb.md", "a\u{b}b.md"];
        for name in hidden.iter().chain(&offered_names) {
            let _ = std::fs::write(scratch.path.join(name), "");
        }
        let present: Vec<String> = std::fs::read_dir(&scratch.path)
            .expect("read")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with('a'))
            .collect();
        let held_back = hidden
            .iter()
            .filter(|name| present.iter().any(|held| held == *name));
        assert!(
            held_back.count() >= 8,
            "only {present:?} could be made here"
        );

        let offered = matching(&scratch.path, "a", &[]);
        for name in hidden {
            assert!(
                !paths(&offered).contains(&name),
                "{name:?} was offered, and no line can carry it"
            );
        }
        for name in offered_names {
            assert!(
                present.iter().any(|held| held == name) == paths(&offered).contains(&name),
                "{name:?} is held and not offered, or offered and not held"
            );
        }

        if std::fs::create_dir_all(scratch.path.join("x\t")).is_ok() {
            assert_eq!(paths(&matching(&scratch.path, "x", &[])), vec!["x\t/"]);
        }
        if std::fs::create_dir_all(scratch.path.join("y\nz")).is_ok() {
            assert!(
                matching(&scratch.path, "y", &[]).is_empty(),
                "a directory holding a line break was offered"
            );
        }
    }

    /// Names the list leaves out because no line can carry them to the turn, each with why
    /// (NAME-8). The test below asks every other name to be offered and read back, and these to be
    /// absent, so a name moves between the two with the rule and not by accident.
    const NOT_OFFERED: &[(&str, &str)] = &[
        ("a.md ", "ends in a space"),
        ("a.md\t", "ends in a tab"),
        ("a.md\n", "ends in a line break"),
        ("a.md\u{a0}", "ends in a no-break space"),
        ("a.md\u{3000}", "ends in an ideographic space"),
        (" ", "is a space"),
        ("\t", "is a tab"),
        ("\u{a0}", "is a no-break space"),
        ("\u{3000}", "is an ideographic space"),
    ];

    /// What a person picks from the list is the file the sent line names, whatever the name holds
    /// (NAME-8, TRUST-2). The reference a naming records a rule for is read out of the line, so a
    /// step there that ends a name early names the file without the rest of it, which is another
    /// file where the volume holds both. A name the line cannot carry is not offered at all.
    #[test]
    fn every_awkward_name_is_offered_and_read_back_as_the_file_it_is() {
        let names = aliases::held_names();
        let mut asked = 0;
        for (index, (name, _)) in names.iter().enumerate() {
            let scratch = Scratch::new(&format!("awkward-{index}"));
            let file = scratch.path.join(name);
            std::fs::create_dir_all(file.parent().expect("a name is in a directory"))
                .expect("directory");
            if std::fs::write(&file, "").is_err() {
                continue;
            }
            asked += 1;

            assert!(
                names_a_file(&scratch.path, name, &[]),
                "{name:?} is not found as a finished name"
            );
            let offered = paths(&matching(&scratch.path, name, &[])).contains(name);
            if let Some((_, why)) = NOT_OFFERED.iter().find(|(listed, _)| listed == name) {
                assert!(!offered, "{name:?} was offered, and it {why}");
                continue;
            }
            assert!(offered, "{name:?} is not offered under its own name");
            let written = escape(name);
            assert_eq!(
                referenced(&format!("see @{written} now")),
                [name.to_string()],
                "the line a completion writes for {name:?} does not name that file"
            );
            assert_eq!(
                typed_reference(&format!("see @{written}")).as_deref(),
                Some(*name),
                "the line a completion writes for {name:?} is not read back while it is typed"
            );
            // What the completion leaves behind it: the space after a file closes the list, and the
            // name before it ends where it ends.
            assert_eq!(
                typed_reference(&format!("see @{written} ")),
                None,
                "the list stays open on {name:?} after it is completed"
            );
            // Sent, the line is trimmed, and the name is whole when it ends the line.
            assert_eq!(
                referenced(format!("  see @{written} \n").trim()),
                [name.to_string()],
                "{name:?} does not survive being the end of a sent line"
            );
        }
        assert!(asked > 20, "only {asked} names were made");
    }

    /// A name with a space is offered, found as finished, and is the word a completion replaces.
    #[test]
    fn a_name_with_a_space_is_listed_and_finished() {
        let scratch = Scratch::new("space");
        std::fs::create_dir_all(scratch.path.join("My Documents")).expect("create");
        std::fs::write(scratch.path.join("My Documents/notes.md"), "").expect("write");
        assert_eq!(
            paths(&matching(&scratch.path, "My Documents/", &[])),
            vec!["My Documents/notes.md"]
        );
        assert!(names_a_file(&scratch.path, "My Documents/notes.md", &[]));
        assert_eq!(last_word_starts_at(r"read @My\ Doc"), 5);
    }

    /// Enter completes a half-typed name, sends a finished one even where the list highlights a
    /// directory above it, and still completes to a row the person moved the cursor to.
    #[test]
    fn what_enter_does_with_a_half_typed_or_finished_name() {
        let scratch = Scratch::new("enter");
        let half = matching(&scratch.path, "Make", &[]);
        assert!(
            enter_completes(&scratch.path, "Make", &half, 0, &[]),
            "half typed"
        );

        std::fs::create_dir_all(scratch.path.join("Makefiles")).expect("create");
        let finished = matching(&scratch.path, "Makefile", &[]);
        assert_eq!(paths(&finished), vec!["Makefiles/", "Makefile"]);
        assert!(
            !enter_completes(&scratch.path, "Makefile", &finished, 0, &[]),
            "a finished name was completed away into the directory above it"
        );
        assert!(
            !enter_completes(&scratch.path, "Makefile", &finished, 1, &[]),
            "the row the cursor is on is what was typed"
        );
        assert!(
            enter_completes(&scratch.path, "Makefile", &[finished[0].clone()], 5, &[]),
            "a cursor past the end of the list chooses its last row"
        );
        assert!(
            !enter_completes(&scratch.path, "zz", &[], 0, &[]),
            "nothing offered"
        );
    }

    /// A session is named beside the files and read back as a session, never as a path: a file
    /// whose name looked like one would otherwise be read from disk by a line that meant a record.
    #[test]
    fn a_session_reference_is_not_a_file_and_is_read_back_by_its_id() {
        let line = "carry on from @session:abc-123 and @Cargo.toml @session:abc-123 again";
        assert_eq!(referenced(line), vec!["Cargo.toml"]);
        assert_eq!(referenced_sessions(line), vec!["abc-123"]);
        assert!(referenced_sessions("see @session: and @src/session:x").is_empty());
        assert!(referenced_sessions("mail me at a@session:abc").is_empty());
    }

    /// A finished session reference is a finished sentence even where a file of that name exists,
    /// and the row offered for a session completes to the form that names it.
    #[test]
    fn a_session_row_completes_to_its_reference_and_enter_sends_it_finished() {
        let scratch = Scratch::new("session-row");
        std::fs::write(scratch.path.join("session:abc"), "").expect("write");
        assert!(!names_a_file(&scratch.path, "session:abc", &[]));
        let row = session_entry("abc", "fix the build", "2 days ago".to_string());
        assert_eq!(row.path, "session:abc");
        assert!(!row.is_directory);
        assert!(!enter_completes(
            &scratch.path,
            "session:abc",
            std::slice::from_ref(&row),
            0,
            &[]
        ));
        assert!(enter_completes(&scratch.path, "session:a", &[row], 0, &[]));
    }

    /// Writes `path` under `scratch`, with the directories above it.
    fn put(scratch: &Scratch, path: &str) {
        let file = scratch.path.join(path);
        std::fs::create_dir_all(file.parent().expect("a name is in a directory")).expect("dir");
        std::fs::write(file, "").expect("write");
    }

    /// NAME-11: a fragment with no slash finds a name in a directory below the root, by its
    /// characters in order and without regard to case, so a person who remembers part of a name
    /// does not have to walk to it.
    #[test]
    fn a_bare_fragment_finds_a_name_in_a_directory_below() {
        let scratch = Scratch::new("found-below");
        for typed in ["lib", "lbrs", "LIB", "Lb.R"] {
            assert_eq!(
                paths(&matching(&scratch.path, typed, &[])),
                vec!["crates/tui/lib.rs"],
                "{typed}"
            );
        }
        assert!(
            matching(&scratch.path, "libx", &[]).is_empty(),
            "a name that lacks a character of the fragment was offered"
        );
    }

    /// NAME-11: case is ignored in every script, whatever follows a letter. The fragment and the
    /// names are folded the same way, so the capital sigma that ends `ΟΔΟΣ` matches the name it was
    /// typed from.
    #[test]
    fn a_fragment_matches_a_name_in_every_script_without_regard_to_case() {
        let scratch = Scratch::new("found-case");
        put(&scratch, "docs/ΟΔΟΣ.md");
        put(&scratch, "docs/Straße.md");
        for typed in ["ΟΔΟΣ", "οδοσ", "Οδοσ"] {
            assert_eq!(
                paths(&matching(&scratch.path, typed, &[])),
                vec!["docs/ΟΔΟΣ.md"],
                "{typed}"
            );
        }
        assert_eq!(
            paths(&matching(&scratch.path, "STRASSE", &[])),
            Vec::<&str>::new(),
            "a letter is folded to one letter, as the names are"
        );
    }

    /// NAME-11: every directory a walk reads is kept for the next key, so typing a fragment does not
    /// read the disk again for each letter, however many directories the tree has. The directories
    /// are changed behind the cache's back with their times put back, so the new name can only be
    /// missed if the walk's reading was kept, and more directories than the cache once held are
    /// read.
    #[test]
    fn the_directories_a_walk_read_are_kept_for_the_next_key() {
        let scratch = tempfile::tempdir().expect("scratch");
        let root = scratch.path();
        for n in 0..100 {
            std::fs::create_dir_all(root.join(format!("d{n:03}"))).expect("create");
            std::fs::write(root.join(format!("d{n:03}/f.txt")), "").expect("write");
        }
        let settle = |path: &std::path::Path| settled_at(path, 5);
        for n in 0..100 {
            settle(&root.join(format!("d{n:03}")));
        }
        settle(root);
        assert!(matching(root, "zzz", &[]).is_empty());

        std::fs::write(root.join("d000/needle.txt"), "").expect("write");
        settle(&root.join("d000"));
        assert!(
            matching(root, "needle", &[]).is_empty(),
            "a directory of the walk was read again on the next key"
        );
    }

    /// NAME-11: a directory is found as well as a file, and completes so typing can go on into it.
    #[test]
    fn a_bare_fragment_finds_a_directory_below_and_offers_it_as_one() {
        let scratch = Scratch::new("found-directory");
        let offered = matching(&scratch.path, "tui", &[]);
        assert_eq!(paths(&offered), vec!["crates/tui/", "crates/tui/lib.rs"]);
        assert!(offered[0].is_directory);
        assert!(!offered[1].is_directory);
    }

    /// NAME-11: a fragment with a slash is still answered by its own directory where that offers
    /// something, and looked for below only where it offers nothing. `crates/t` is a prefix in
    /// `crates`, so a walk beside it would add every name whose path holds those letters.
    #[test]
    fn a_fragment_with_a_slash_is_looked_for_below_only_where_its_directory_offers_nothing() {
        let scratch = Scratch::new("found-slash");
        assert_eq!(
            paths(&matching(&scratch.path, "crates/t", &[])),
            vec!["crates/tui/"]
        );
        assert_eq!(
            paths(&matching(&scratch.path, "tui/lib", &[])),
            vec!["crates/tui/lib.rs"],
            "no `tui` in the root, so the path was looked for below"
        );
        assert_eq!(
            paths(&matching(&scratch.path, "crates/lib", &[])),
            vec!["crates/tui/lib.rs"],
            "`crates` holds no name starting `lib`, so the path was looked for below"
        );
        assert!(matching(&scratch.path, "crates/zzz", &[]).is_empty());

        std::fs::create_dir_all(scratch.path.join("empty")).expect("create");
        assert!(
            matching(&scratch.path, "empty/", &[]).is_empty(),
            "a directory with nothing in it offered itself, which completes to what was typed"
        );
    }

    /// NAME-11: a name that matches ranks before one whose path matches, however closely the path
    /// matches, and a match that starts the text before one inside it before one with other
    /// characters between, whatever the paths' lengths and alphabetical order. Each fixture puts
    /// the worse match first by length or by alphabet, so an order that only sorted paths, or only
    /// weighed closeness, would reverse them.
    #[test]
    fn a_name_ranks_before_a_path_and_a_closer_match_before_a_looser_one() {
        let scratch = Scratch::new("found-ranked");
        put(&scratch, "a/notes/x.md");
        put(&scratch, "z/n-o-t-e-s.md");
        assert_eq!(
            paths(&matching(&scratch.path, "notes", &[])),
            vec!["a/notes/", "z/n-o-t-e-s.md", "a/notes/x.md"],
            "the directory called `notes`, the name spread through, then the path that holds it"
        );

        let scratch = Scratch::new("found-closer");
        put(&scratch, "a/m_a_i_n");
        put(&scratch, "b/xxxmain");
        put(&scratch, "c/mainxxx");
        assert_eq!(
            paths(&matching(&scratch.path, "main", &[])),
            vec!["c/mainxxx", "b/xxxmain", "a/m_a_i_n"],
            "starts the name, then inside it, then spread through it"
        );
    }

    /// NAME-11: what the directory offers keeps its rows and its order, and what is found follows
    /// without repeating it, so the prefix a person has always typed still comes first.
    #[test]
    fn what_a_directory_offers_comes_first_and_is_not_repeated_below() {
        let scratch = Scratch::new("found-after");
        put(&scratch, "lib.rs");
        put(&scratch, "crates/tui/lib.rs");
        put(&scratch, "docs/liberty.md");
        assert_eq!(
            paths(&matching(&scratch.path, "lib", &[])),
            vec!["lib.rs", "docs/liberty.md", "crates/tui/lib.rs"],
            "`lib.rs` once, from the root, then the two below by the length of their paths"
        );
    }

    /// NAME-11: no more than ten names found below are offered, however many match, and they follow
    /// what the directory offers. The terminal draws every row under the box, so more would fill
    /// the screen at the first letter.
    #[test]
    fn no_more_than_ten_names_found_below_follow_what_the_directory_offers() {
        let scratch = Scratch::new("found-ten");
        std::fs::create_dir_all(scratch.path.join("fa")).expect("create");
        for n in 0..MAX_ENTRIES + 10 {
            put(&scratch, &format!("x/file{n:02}.rs"));
        }
        let offered = matching(&scratch.path, "f", &[]);
        assert_eq!(offered.len(), 1 + MOST_FOUND);
        assert_eq!(offered[0].path, "fa/", "the directory's row, first");
        assert!(
            offered[1..]
                .iter()
                .all(|entry| entry.path.starts_with("x/file"))
        );
    }

    /// NAME-11: the list never holds more than forty rows, so names found below take only the room
    /// what the directory offers leaves, and none where it fills the list.
    #[test]
    fn the_list_never_holds_more_than_forty_rows() {
        let scratch = Scratch::new("found-room");
        for n in 0..MAX_ENTRIES - 5 {
            put(&scratch, &format!("g{n:02}"));
        }
        for letter in 'a'..='z' {
            put(&scratch, &format!("deep/g_{letter}"));
        }
        let offered = matching(&scratch.path, "g", &[]);
        assert_eq!(offered.len(), MAX_ENTRIES);
        assert_eq!(
            offered
                .iter()
                .filter(|e| e.path.starts_with("deep/"))
                .count(),
            5,
            "the room that was left"
        );

        let scratch = Scratch::new("found-full");
        for n in 0..MAX_ENTRIES {
            put(&scratch, &format!("g{n:02}"));
        }
        put(&scratch, "deep/g_extra");
        let offered = matching(&scratch.path, "g", &[]);
        assert_eq!(offered.len(), MAX_ENTRIES);
        assert!(
            offered.iter().all(|entry| !entry.path.starts_with("deep/")),
            "a name found below was offered past the cap"
        );
    }

    /// NAME-11: the names the directory listing leaves out are left out of the walk at every depth,
    /// and `.claude/worktrees` with them, while a `worktrees` anywhere else is the project's own.
    #[test]
    fn the_names_every_walk_skips_are_skipped_at_every_depth() {
        let scratch = Scratch::new("found-ignored");
        for skipped in [
            "target/needle.rs",
            ".git/needle.rs",
            "node_modules/needle.rs",
            ".hg/needle.rs",
            "dist/needle.rs",
            ".worktrees/feature/needle.rs",
            "crates/tui/node_modules/pkg/needle.rs",
            "crates/tui/vendor/needle.rs",
            ".claude/worktrees/pr-fix/needle.rs",
        ] {
            put(&scratch, skipped);
        }
        put(&scratch, "ci/worktrees/needle.rs");
        put(&scratch, ".claude/agents/needle.rs");
        assert_eq!(
            paths(&matching(&scratch.path, "needle", &[])),
            vec!["ci/worktrees/needle.rs", ".claude/agents/needle.rs"]
        );
    }

    /// NAME-5, NAME-11: the walk follows no link. A link out of the workspace leads nowhere, a link
    /// back into it does not offer the same files twice, and a link to the directory it sits in
    /// does not make the walk go round.
    #[cfg(unix)]
    #[test]
    fn the_walk_follows_no_link_out_of_the_workspace_or_back_into_it() {
        let scratch = Scratch::new("found-links");
        let outside = Scratch::new("found-links-outside");
        put(&outside, "needle.txt");
        std::os::unix::fs::symlink(&outside.path, scratch.path.join("link")).expect("link");
        std::os::unix::fs::symlink("crates", scratch.path.join("alias")).expect("link");
        std::os::unix::fs::symlink(".", scratch.path.join("round")).expect("link");

        assert!(
            matching(&scratch.path, "needle", &[]).is_empty(),
            "a name behind a link out of the workspace was offered"
        );
        assert!(matching(&scratch.path, "link/needle", &[]).is_empty());
        assert_eq!(
            paths(&matching(&scratch.path, "lib", &[])),
            vec!["crates/tui/lib.rs"],
            "the files were offered again under a link"
        );
    }

    /// PATH-002: a name the directory did not hold as text is held as a rendering that every name
    /// differing in those bytes shares, so it is told apart from the name that really is that text.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_text_is_told_apart_from_its_lookalike() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let invalid = Named::new(OsString::from_vec(b"needle-\xff".to_vec()), false);
        let lookalike = Named::new(OsString::from("needle-\u{FFFD}"), false);
        assert!(!invalid.is_text);
        assert!(lookalike.is_text);
        assert_eq!(invalid.name, lookalike.name, "the rendering they share");
    }

    /// PATH-002, NAME-4, NAME-11: a file whose name is not text is offered by neither the directory
    /// nor the walk, since the line a pick writes would name its lookalike, and the lookalike is
    /// offered once. Linux only: macOS cannot hold the name.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_text_is_not_offered_under_its_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let scratch = Scratch::new("not-text");
        std::fs::create_dir_all(scratch.path.join("sub")).expect("create");
        for directory in [&scratch.path, &scratch.path.join("sub")] {
            std::fs::write(directory.join(OsStr::from_bytes(b"needle-\xff.rs")), "")
                .expect("write");
            std::fs::write(directory.join("needle-\u{FFFD}.rs"), "").expect("write");
        }
        assert_eq!(
            paths(&matching(&scratch.path, "needle", &[])),
            vec!["needle-\u{FFFD}.rs", "sub/needle-\u{FFFD}.rs"],
            "the directory's row, then the one found below"
        );
    }

    /// NAME-11: the walk reads at most its bound and goes one level at a time, so the names a stop
    /// leaves out are the deepest. A walk that went deep first would read down the `z` chain and
    /// never reach `a`, which is a level from the root.
    #[test]
    fn the_walk_stops_at_its_bound_having_read_the_shallow_names_first() {
        let scratch = tempfile::tempdir().expect("scratch");
        let root = scratch.path();
        std::fs::create_dir_all(root.join("a")).expect("create");
        std::fs::create_dir_all(root.join("z/y/x")).expect("create");
        std::fs::write(root.join("a/needle_a.rs"), "").expect("write");
        std::fs::write(root.join("z/y/x/needle_deep.rs"), "").expect("write");
        std::fs::write(root.join("needle_root.rs"), "").expect("write");

        let names = |read: usize| -> Vec<String> {
            found_beneath(root, "needle", &[], read)
                .into_iter()
                .map(|entry| entry.path)
                .collect()
        };
        assert_eq!(
            names(5),
            vec!["a/needle_a.rs", "needle_root.rs"],
            "read past its bound, or went deep before it went wide"
        );
        assert_eq!(
            names(100),
            vec!["a/needle_a.rs", "needle_root.rs", "z/y/x/needle_deep.rs"],
            "the deep file is there to be found"
        );
    }

    /// NAME-11: a name made after the last walk is found by the next, because each directory is
    /// asked whether it has changed rather than the walk being kept.
    #[test]
    fn a_name_made_after_a_walk_is_found_by_the_next() {
        let scratch = Scratch::new("found-fresh");
        assert!(matching(&scratch.path, "newcomer", &[]).is_empty());
        put(&scratch, "crates/tui/newcomer.rs");
        assert_eq!(
            paths(&matching(&scratch.path, "newcomer", &[])),
            vec!["crates/tui/newcomer.rs"]
        );
    }

    /// NAME-8, NAME-11: a name found below that holds a space is written escaped and read back as
    /// the file it is, as one the directory offers is.
    #[test]
    fn a_name_with_a_space_found_below_reads_back_as_that_file() {
        let scratch = Scratch::new("found-space");
        put(&scratch, "My Documents/my notes.md");
        let offered = matching(&scratch.path, "notes", &[]);
        assert_eq!(paths(&offered), vec!["My Documents/my notes.md"]);
        let line = format!("read @{} now", escape(&offered[0].path));
        assert_eq!(referenced(&line), vec!["My Documents/my notes.md"]);
    }

    /// NAME-7, NAME-11: Enter still decides a finished name from the workspace and not from the
    /// rows offered. `lib.rs` is a file of the root and is sent, though another `lib.rs` is offered
    /// below it; a fragment completes to the row at the cursor, a found row included.
    #[test]
    fn enter_sends_a_finished_name_and_completes_a_fragment_to_a_found_row() {
        let scratch = Scratch::new("found-enter");
        put(&scratch, "lib.rs");
        put(&scratch, "crates/tui/lib.rs");

        let finished = matching(&scratch.path, "lib.rs", &[]);
        assert_eq!(paths(&finished), vec!["lib.rs", "crates/tui/lib.rs"]);
        assert!(
            !enter_completes(&scratch.path, "lib.rs", &finished, 0, &[]),
            "a finished name was completed away into a name found below it"
        );
        assert!(
            enter_completes(&scratch.path, "lib.rs", &finished, 1, &[]),
            "the arrows chose the found row"
        );

        let half = matching(&scratch.path, "lbrs", &[]);
        assert_eq!(paths(&half), vec!["lib.rs", "crates/tui/lib.rs"]);
        assert!(enter_completes(&scratch.path, "lbrs", &half, 0, &[]));
        assert!(!names_a_file(&scratch.path, "lbrs", &[]));
    }

    /// A scratch workspace with a reference directory beside it, and the alias list naming it.
    fn with_a_reference(name: &str) -> (Scratch, Scratch, Vec<(String, PathBuf)>) {
        let workspace = Scratch::new(name);
        let library = Scratch::new(&format!("{name}-library"));
        std::fs::create_dir_all(library.path.join("src")).expect("src");
        std::fs::write(library.path.join("src/lexer.rs"), "x").expect("file");
        std::fs::write(library.path.join("README.md"), "x").expect("file");
        let sources = vec![("parser".to_string(), library.path.clone())];
        (workspace, library, sources)
    }

    /// REFER-6: the alias is offered among the workspace's own entries, and a slash descends into
    /// the directory behind it with every name still written under the alias.
    #[test]
    fn an_alias_is_offered_and_a_slash_descends_into_its_directory() {
        let (workspace, _library, sources) = with_a_reference("alias-offered");
        assert_eq!(
            paths(&matching(&workspace.path, "", &sources)),
            vec!["crates/", "parser/", "Cargo.toml", "Makefile"],
        );
        assert_eq!(
            paths(&matching(&workspace.path, "par", &sources)),
            vec!["parser/"]
        );
        assert_eq!(
            paths(&matching(&workspace.path, "parser/", &sources)),
            vec![
                "parser/crates/",
                "parser/src/",
                "parser/Cargo.toml",
                "parser/Makefile",
                "parser/README.md"
            ]
        );
        assert_eq!(
            paths(&matching(&workspace.path, "parser/src/l", &sources)),
            vec!["parser/src/lexer.rs"]
        );
    }

    /// REFER-6: an entry of the workspace's own is not hidden by an alias of the same name.
    #[test]
    fn a_workspace_entry_is_not_shadowed_by_an_alias() {
        let (workspace, _library, sources) = with_a_reference("alias-shadow");
        std::fs::create_dir_all(workspace.path.join("parser")).expect("dir");
        std::fs::write(workspace.path.join("parser/own.rs"), "x").expect("file");
        assert_eq!(
            paths(&matching(&workspace.path, "par", &sources)),
            vec!["parser/", "parser/own.rs"],
            "one `parser/` row, the workspace's, then the file found inside it"
        );
        assert_eq!(
            paths(&matching(&workspace.path, "parser/", &sources)),
            vec!["parser/own.rs"]
        );
        assert_eq!(
            resolved(&workspace.path, "parser/own.rs", &sources).as_deref(),
            Ok("parser/own.rs")
        );
    }

    /// REFER-6, NAME-11: a name under an alias is answered by the alias's directory alone. The walk
    /// covers the workspace, so `parser/l` is not looked for there, where `pars/er/lib.rs` holds
    /// its characters in order.
    #[test]
    fn a_name_under_an_alias_is_not_looked_for_in_the_workspace() {
        let (workspace, _library, sources) = with_a_reference("alias-not-walked");
        put(&workspace, "pars/er/lib.rs");
        assert!(matching(&workspace.path, "parser/l", &sources).is_empty());
        assert_eq!(
            paths(&matching(&workspace.path, "lib", &sources)),
            vec!["pars/er/lib.rs", "crates/tui/lib.rs"],
            "the workspace is still walked for a name with no alias in it"
        );
    }

    /// REFER-6: a reference directory whose path is not UTF-8 is refused, and the directory whose
    /// name is its U+FFFD rendering is a different one that still resolves. A lossy rendering would
    /// resolve both to the second path.
    #[cfg(unix)]
    #[test]
    fn a_reference_directory_that_is_not_text_is_refused_not_rewritten() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let workspace = Scratch::new("alias-not-text");
        let beside = Scratch::new("alias-not-text-beside");
        let invalid = beside.path.join(OsStr::from_bytes(b"lib-\xff"));
        let lookalike = beside.path.join(OsStr::new("lib-\u{FFFD}"));
        let under = |directory: &PathBuf| vec![("parser".to_string(), directory.clone())];

        let refused = resolved(&workspace.path, "parser/a.rs", &under(&invalid));
        let named = resolved(&workspace.path, "parser/a.rs", &under(&lookalike));
        assert_eq!(refused, Err(NotText));
        assert_eq!(named, Ok(lookalike.join("a.rs").display().to_string()));
        assert_ne!(refused, named);
    }

    /// REFER-6: a name under an alias is the file in the directory, `..` leaves the reference, and
    /// a finished name sends rather than completes.
    #[test]
    fn a_name_under_an_alias_resolves_to_the_file_and_cannot_climb_out() {
        let (workspace, library, sources) = with_a_reference("alias-resolved");
        assert_eq!(
            resolved(&workspace.path, "parser/src/lexer.rs", &sources),
            Ok(library.path.join("src/lexer.rs").display().to_string())
        );
        assert_eq!(
            resolved(&workspace.path, "parser/../x", &sources).as_deref(),
            Ok("parser/../x")
        );
        assert!(matching(&workspace.path, "parser/../", &sources).is_empty());
        assert_eq!(
            resolved(&workspace.path, "other/a.rs", &sources).as_deref(),
            Ok("other/a.rs")
        );
        assert!(names_a_file(&workspace.path, "parser/README.md", &sources));
        assert!(!names_a_file(&workspace.path, "parser/src", &sources));
        let offered = matching(&workspace.path, "parser/README.md", &sources);
        assert!(!enter_completes(
            &workspace.path,
            "parser/README.md",
            &offered,
            0,
            &sources
        ));
    }
}
