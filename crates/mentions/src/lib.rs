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

use std::path::Path;

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
}

/// Entries matching a half-typed reference, directories first and then files, each alphabetical.
///
/// `typed` is what follows the `@`. An empty one lists the workspace root. Anything with a slash in
/// it lists the directory named up to the last slash, so `crates/t` offers what is in `crates`.
///
/// Directories come first because a reference is usually typed by walking into one, and being able
/// to go deeper matters more than the file that happens to sort first.
pub fn matching(root: &Path, typed: &str) -> Vec<Entry> {
    // Refused rather than resolved: `..` would walk out of the workspace, and an absolute path
    // names something the reference syntax has no business reaching.
    if typed.contains("..") || typed.starts_with('/') {
        return Vec::new();
    }

    let (directory, prefix) = match typed.rsplit_once('/') {
        Some((directory, prefix)) => (directory, prefix),
        None => ("", typed),
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

    let Ok(reading) = std::fs::read_dir(&canonical) else {
        return Vec::new();
    };

    let mut entries: Vec<Entry> = reading
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) {
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
            if bravebot_filetype::is_ignored_directory(&name) {
                return None;
            }
            let is_directory = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let path = if directory.is_empty() {
                name
            } else {
                format!("{directory}/{name}")
            };
            Some(Entry {
                path: if is_directory {
                    format!("{path}/")
                } else {
                    path
                },
                is_directory,
            })
        })
        .collect();

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

/// Where each word of a line begins and ends.
///
/// Words end at whitespace, except that a space written after a backslash belongs to the word, so
/// `@My\ Documents/a.md` is one. A backslash anywhere else is an ordinary character, which keeps a
/// sentence containing one from naming a path.
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
        if c == '\\' && chars.next_if(|&(_, next)| next == ' ').is_some() {
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

/// A path as it is written in a line: each space behind a backslash, so it stays in the word.
pub fn escape(path: &str) -> String {
    path.replace(' ', "\\ ")
}

fn unescape(written: &str) -> String {
    written.replace("\\ ", " ")
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
pub fn names_a_file(root: &Path, typed: &str) -> bool {
    // `..` and an absolute path are refused rather than resolved, the same string refusal
    // `matching` makes of what is being typed. Not a confinement check, and it cannot become one:
    // `matching` offers a symlink pointing out of the workspace as a file of its own, and a name
    // the list offers has to count as finished or Enter completes it away, which is the whole
    // thing this exists to stop.
    if typed.contains("..") || typed.starts_with('/') {
        return false;
    }
    root.join(typed)
        .symlink_metadata()
        .is_ok_and(|named| !named.is_dir())
}

/// Every file named with `@` in a line, in the order they were written.
///
/// This is what becomes a turn's context. A trailing slash is dropped, since a directory is a place
/// to type through rather than a file to read, and one named anyway is not a file to include.
pub fn referenced(line: &str) -> Vec<String> {
    word_spans(line)
        .into_iter()
        .filter_map(|(start, end)| line[start..end].strip_prefix('@'))
        .map(unescape)
        .filter(|path| !path.is_empty() && !path.ends_with('/'))
        .collect()
}

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
pub fn enter_completes(root: &Path, typed: &str, offered: &[Entry], cursor: usize) -> bool {
    if cursor == 0 && names_a_file(root, typed) {
        return false;
    }
    offered
        .get(cursor.min(offered.len().saturating_sub(1)))
        .is_some_and(|entry| typed != entry.path)
}

#[cfg(test)]
mod tests {
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

    /// An empty reference lists the root, directories first, so walking deeper is the first thing
    /// offered.
    #[test]
    fn an_empty_reference_lists_the_root_with_directories_first() {
        let scratch = Scratch::new("root");
        let offered = matching(&scratch.path, "");
        assert_eq!(paths(&offered), vec!["crates/", "Cargo.toml", "Makefile"]);
    }

    /// A prefix narrows the list, which is what makes typing a path possible at all.
    #[test]
    fn a_prefix_narrows_the_list() {
        let scratch = Scratch::new("prefix");
        assert_eq!(paths(&matching(&scratch.path, "Ma")), vec!["Makefile"]);
        assert!(matching(&scratch.path, "zz").is_empty());
    }

    /// A slash lists the directory it names, so a reference is typed by walking into one.
    #[test]
    fn a_slash_lists_what_is_inside_that_directory() {
        let scratch = Scratch::new("nested");
        // Whole paths, not just the last segment: what is offered is what gets typed.
        assert_eq!(
            paths(&matching(&scratch.path, "crates/")),
            vec!["crates/tui/"]
        );
        assert_eq!(
            paths(&matching(&scratch.path, "crates/tui/l")),
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
        let offered = matching(&scratch.path, "");
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
            !paths(&matching(&scratch.path, "crates/tui/")).contains(&"crates/tui/.git"),
            "version control was offered"
        );
    }

    /// The completion must not become a way to browse the filesystem: `..` and an absolute path are
    /// refused rather than resolved, exactly as the workspace refuses them.
    #[test]
    fn a_reference_cannot_climb_out_of_the_workspace() {
        let scratch = Scratch::new("escape");
        assert!(matching(&scratch.path, "../").is_empty());
        assert!(matching(&scratch.path, "../../etc/").is_empty());
        assert!(matching(&scratch.path, "/etc/").is_empty());
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

    /// A finished name is decided from the workspace, so the cap that bounds what is displayed
    /// cannot change the answer, and a directory is still somewhere to type through.
    #[test]
    fn what_counts_as_already_naming_a_file() {
        let scratch = Scratch::new("finished");
        assert!(names_a_file(&scratch.path, "Makefile"));
        assert!(names_a_file(&scratch.path, "crates/tui/lib.rs"));
        assert!(!names_a_file(&scratch.path, "crates"), "a directory");
        assert!(!names_a_file(&scratch.path, "Make"), "half typed");
        assert!(!names_a_file(&scratch.path, ""), "a bare `@`");

        // More siblings sharing the prefix than the list can hold. They sort above the file, so
        // the cut takes the file first and nothing about it having been typed has changed.
        for n in 0..MAX_ENTRIES + 5 {
            std::fs::create_dir_all(scratch.path.join(format!("Makefile{n:03}"))).expect("create");
        }
        let offered = matching(&scratch.path, "Makefile");
        assert!(
            !paths(&offered).contains(&"Makefile"),
            "the file was still offered, so the cap was never reached"
        );
        assert!(names_a_file(&scratch.path, "Makefile"));
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
            matching(&scratch.path, "notes"),
            vec![Entry {
                path: "notes".to_string(),
                is_directory: false,
            }],
            "the list offers a symlink as a file, following nothing"
        );
        assert!(names_a_file(&scratch.path, "notes"));
    }

    /// Nothing outside the workspace is a name this answers for, the same refusal `matching` makes.
    #[test]
    fn dots_and_an_absolute_path_are_not_finished_names() {
        let scratch = Scratch::new("finished-escape");
        // The workspace is the nested directory, so the file one level up is genuinely outside it
        // while still existing, which is what makes the refusal say anything.
        let root = scratch.path.join("crates");
        let outside = scratch.path.join("Makefile");
        assert!(names_a_file(&scratch.path, "Makefile"), "it is there");
        assert!(!names_a_file(&root, "../Makefile"), "climbing out");
        assert!(
            !names_a_file(&root, outside.to_str().expect("utf8")),
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

    /// A name with a space is offered, found as finished, and is the word a completion replaces.
    #[test]
    fn a_name_with_a_space_is_listed_and_finished() {
        let scratch = Scratch::new("space");
        std::fs::create_dir_all(scratch.path.join("My Documents")).expect("create");
        std::fs::write(scratch.path.join("My Documents/notes.md"), "").expect("write");
        assert_eq!(
            paths(&matching(&scratch.path, "My Documents/")),
            vec!["My Documents/notes.md"]
        );
        assert!(names_a_file(&scratch.path, "My Documents/notes.md"));
        assert_eq!(last_word_starts_at(r"read @My\ Doc"), 5);
    }

    /// Enter completes a half-typed name, sends a finished one even where the list highlights a
    /// directory above it, and still completes to a row the person moved the cursor to.
    #[test]
    fn what_enter_does_with_a_half_typed_or_finished_name() {
        let scratch = Scratch::new("enter");
        let half = matching(&scratch.path, "Make");
        assert!(
            enter_completes(&scratch.path, "Make", &half, 0),
            "half typed"
        );

        std::fs::create_dir_all(scratch.path.join("Makefiles")).expect("create");
        let finished = matching(&scratch.path, "Makefile");
        assert_eq!(paths(&finished), vec!["Makefiles/", "Makefile"]);
        assert!(
            !enter_completes(&scratch.path, "Makefile", &finished, 0),
            "a finished name was completed away into the directory above it"
        );
        assert!(
            !enter_completes(&scratch.path, "Makefile", &finished, 1),
            "the row the cursor is on is what was typed"
        );
        assert!(
            enter_completes(&scratch.path, "Makefile", &[finished[0].clone()], 5),
            "a cursor past the end of the list chooses its last row"
        );
        assert!(
            !enter_completes(&scratch.path, "zz", &[], 0),
            "nothing offered"
        );
    }
}
