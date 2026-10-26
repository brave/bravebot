//! The lines of a file, released for a person's screen when they ask to see it.
//!
//! A person who clicks a path in the transcript asks to see what is at it. The file is read the way
//! a write's review reads the file it would replace ([`Workspace::peek_labelled_for_review`]): as
//! text nobody vouched for, whatever the trust map says of it, so what comes out is drawn inside the
//! margin of a quarantined block and never as the interface's own words (VIEW-3). The lines are cut
//! inside the kernel's gate for reshaping content ([`Policy::render_in_place`]) and released under
//! the witness for display ([`Policy::authorise_display_release`]), so the driver holds a few
//! numbered rows for a terminal and no more of the file than that.
//!
//! What decides whether anything is shown is the file system and the person's rules, never a byte of
//! the file: whether the path names a file inside the directories the session may read, whether a
//! `deny` rule covers it under any name the file has, and how large it is. A file whose bytes are not
//! valid UTF-8, and an empty one, come back as no lines, the same answer, so nothing here asks the
//! file what to say about it.
//!
//! Nothing reaches the planner. The rows go to a screen and the policy that released them is dropped
//! with them.

use crate::workspace::{BACKSLASH_SEPARATES, Unpreviewable, Workspace};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::permissions::Permissions;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::trust::TrustStore;

/// How many lines of a file a preview shows.
pub const PREVIEW_LINES: usize = 24;

/// The lines drawn above the one a person asked for, so it is not the first thing on the screen
/// with nothing before it to say where it is.
pub const LINES_BEFORE: usize = 3;

/// The largest file a preview reads, in bytes. A file is read whole to be cut, so a larger one is
/// refused by its size, which is the file system's answer about the open file and not the file's.
pub const MOST_BYTES: u64 = 1 << 20;

/// What a preview of a file came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    /// The file as the workspace names it: relative to the working directory where it is inside it,
    /// and where the name landed once links were followed.
    pub named: String,
    /// The label the bytes carry, in the short form the trail uses.
    pub label: String,
    /// The rows, each numbered with its place in the file and cut to the width of a preview.
    pub lines: Vec<String>,
    /// How many lines the file holds, which the screen says when it shows fewer.
    pub total: usize,
}

/// Why nothing was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The name does not resolve to a file inside the directories the session reads: it is outside
    /// them, it is a directory, or nothing is there.
    NotAFile,
    /// A `deny` rule the person wrote covers the file ([PERM-7](../../../docs/specs/permissions.md#PERM-7)).
    Denied,
    /// The file is larger than [`MOST_BYTES`].
    TooLarge(u64),
    /// The policy could not begin.
    Policy(String),
}

/// A policy for this release alone. It is not a turn: nothing is asked of a model, and the routing
/// is the click that asked.
fn policy<'sink, S: Sink>(
    workspace: &Workspace,
    trust: &TrustStore,
    permissions: &Permissions,
    sink: &'sink mut S,
) -> Result<Policy<'sink, S>, Refused> {
    let mut routing = Routing::new();
    routing.insert_trusted("command", "a click on a path");
    Ok(Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .map_err(|denial| Refused::Policy(denial.to_string()))?
    .with_trust(trust.clone())
    .with_permissions(permissions.clone())
    .with_root(workspace.root())
    .with_scratch(workspace.scratch())
    .with_backslash_separates(BACKSLASH_SEPARATES))
}

/// The rows of the file `named` names, from line `from` of it (counted from one), as a person is
/// shown them.
///
/// `named` is resolved by the file tools' own rules ([`Workspace::file_named`]), which hold it to the
/// working directory and the directories opened for the session. The window starts
/// [`LINES_BEFORE`] lines before `from` and holds [`PREVIEW_LINES`], and is moved back to end at the
/// file's last line where it would run past it.
pub fn preview<S: Sink>(
    workspace: &Workspace,
    trust: &TrustStore,
    permissions: &Permissions,
    sink: &mut S,
    named: &str,
    from: usize,
) -> Result<FilePreview, Refused> {
    let mut policy = policy(workspace, trust, permissions, sink)?;
    let resolved = workspace.file_named(named).ok_or(Refused::NotAFile)?;
    // The name the file was resolved from, which the rules are asked about: the spelling of the
    // resolved path below is for the screen, and is not the same file's name for a file whose name
    // is not text. The rules are asked as the file tools ask them, about every name the file has.
    let inside = workspace.name_inside(named);
    if crate::tools::read_is_refused_by_a_rule(&mut policy, workspace, inside) {
        return Err(Refused::Denied);
    }
    let shown_as = workspace.relative_display(&resolved);

    let body =
        Workspace::peek_labelled_for_a_preview(&resolved, MOST_BYTES).map_err(
            |unread| match unread {
                Unpreviewable::NotAFile => Refused::NotAFile,
                Unpreviewable::TooLarge(bytes) => Refused::TooLarge(bytes),
            },
        )?;
    let first = from.saturating_sub(1).saturating_sub(LINES_BEFORE);
    let shaped = policy.render_in_place("preview", &body, |text| {
        let total = text.lines().count();
        // Moved back from the end so the window is full where the file is long enough to fill it.
        let start = first.min(total.saturating_sub(PREVIEW_LINES));
        let width = (start + PREVIEW_LINES).min(total).max(1).to_string().len();
        let rows: Vec<String> = text
            .lines()
            .enumerate()
            .skip(start)
            .take(PREVIEW_LINES)
            .map(|(index, line)| {
                let mut row = format!("{:>width$}  ", index + 1);
                row.extend(line.chars().take(crate::turn::PREVIEW_WIDTH));
                if line.chars().count() > crate::turn::PREVIEW_WIDTH {
                    row.push('…');
                }
                row
            })
            .collect();
        (rows, total)
    });
    let label = shaped.label().to_string();
    let (lines, total) = {
        let proof = policy.authorise_display_release("the lines of a file a person asked to see");
        shaped.declassify(&proof)
    };
    Ok(FilePreview {
        named: shown_as,
        label,
        lines,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::event::NullSink;
    use std::path::PathBuf;

    /// A working directory holding what a test writes into it, removed with the test.
    struct Project {
        root: PathBuf,
        workspace: Workspace,
    }

    impl Project {
        fn new(name: &str) -> Self {
            // nosemgrep: rust.lang.security.temp-dir.temp-dir
            let root = std::env::temp_dir().join(format!(
                "bravebot-file-preview-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("a directory");
            let root = root.canonicalize().expect("a path");
            let workspace = Workspace::new(&root).expect("a workspace");
            Self { root, workspace }
        }

        fn write(&self, name: &str, text: &str) {
            let path = self.root.join(name);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(path, text).expect("a file");
        }

        fn preview(&self, named: &str, from: usize) -> Result<FilePreview, Refused> {
            preview(
                &self.workspace,
                &TrustStore::new(&self.root),
                &Permissions::new(),
                &mut NullSink,
                named,
                from,
            )
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn numbered(count: usize) -> String {
        (1..=count).map(|n| format!("line {n}\n")).collect()
    }

    /// The rows are the file's first lines, each with its number, and the label says that nobody
    /// vouched for them.
    #[test]
    fn a_file_in_the_working_directory_is_shown_numbered_and_untrusted() {
        let project = Project::new("numbered");
        project.write("src/main.rs", "fn main() {\n    run();\n}\n");
        let shown = project.preview("src/main.rs", 1).expect("a preview");
        assert_eq!(shown.named, "src/main.rs");
        assert_eq!(shown.lines, ["1  fn main() {", "2      run();", "3  }"]);
        assert_eq!(shown.total, 3);
        assert_eq!(shown.label, "(U,priv)");
    }

    /// A person who vouched for the whole directory still gets rows labelled as unvouched: what is
    /// drawn is a quarantined block either way, and a click has not asked for the file to be read by
    /// anything that acts on it.
    #[test]
    fn a_path_the_map_vouches_for_is_still_shown_as_untrusted() {
        let project = Project::new("vouched");
        project.write("notes.md", "# notes\n");
        let mut trust = TrustStore::new(&project.root);
        trust.trust(".");
        let shown = preview(
            &project.workspace,
            &trust,
            &Permissions::new(),
            &mut NullSink,
            "notes.md",
            1,
        )
        .expect("a preview");
        assert_eq!(shown.label, "(U,priv)");
        assert_eq!(shown.lines, ["1  # notes"]);
    }

    /// A long file shows a window of 24 lines, starting three before the one asked for and moved back
    /// so that it is full when the file has room for it, and the rows say where in the file they are.
    /// The numbers are the ones MOUSE-42 states, so a change to either shows here.
    #[test]
    fn the_window_starts_before_the_line_asked_for_and_ends_at_the_end_of_the_file() {
        let project = Project::new("window");
        project.write("long.txt", &numbered(100));

        let top = project.preview("long.txt", 1).expect("a preview");
        assert_eq!(top.lines.len(), 24);
        assert_eq!(top.lines[0], " 1  line 1");
        assert_eq!(top.lines[23], "24  line 24");
        assert_eq!(top.total, 100);

        let middle = project.preview("long.txt", 50).expect("a preview");
        assert_eq!(middle.lines[0], "47  line 47", "{:?}", middle.lines);
        assert_eq!(middle.lines.len(), 24);

        let end = project.preview("long.txt", 100).expect("a preview");
        assert_eq!(end.lines.last().map(String::as_str), Some("100  line 100"));
        assert_eq!(end.lines.len(), 24);

        let past = project.preview("long.txt", 5000).expect("a preview");
        assert_eq!(past.lines, end.lines);
    }

    /// A line wider than 160 characters is cut there and says so, so a minified file is not a row a
    /// screen wraps over a page, and one of exactly 160 is shown whole.
    #[test]
    fn a_long_line_is_cut() {
        let project = Project::new("wide");
        project.write("min.js", &"x".repeat(1000));
        project.write("exact.js", &"y".repeat(160));
        project.write("over.js", &"z".repeat(161));

        let shown = project.preview("min.js", 1).expect("a preview");
        assert_eq!(shown.lines, [format!("1  {}…", "x".repeat(160))]);
        let exact = project.preview("exact.js", 1).expect("a preview");
        assert_eq!(exact.lines, [format!("1  {}", "y".repeat(160))]);
        let over = project.preview("over.js", 1).expect("a preview");
        assert_eq!(over.lines, [format!("1  {}…", "z".repeat(160))]);
    }

    /// A name that is not a file inside the directories the session reads shows nothing: the same
    /// answer for each way it can fail, so a refusal says nothing about what is outside.
    #[test]
    fn a_name_that_is_not_a_file_in_reach_shows_nothing() {
        let project = Project::new("reach");
        project.write("a/b.txt", "inside\n");
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let outside = std::env::temp_dir().join(format!(
            "bravebot-file-preview-outside-{}.txt",
            std::process::id()
        ));
        std::fs::write(&outside, "outside\n").expect("a file");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, project.root.join("leads-out.txt")).expect("a link");
        let outside_name = outside.to_string_lossy().into_owned();

        for named in [
            "missing.txt",
            "a",
            "a/",
            "../outside.txt",
            "a/../../outside.txt",
            outside_name.as_str(),
            "/etc/hosts",
            "leads-out.txt",
            "",
        ] {
            assert_eq!(
                project.preview(named, 1),
                Err(Refused::NotAFile),
                "{named:?}"
            );
        }
        let _ = std::fs::remove_file(&outside);
    }

    /// An absolute name that lands inside the working directory is the same file as the name
    /// relative to it, and is shown as that.
    #[test]
    fn an_absolute_name_inside_the_working_directory_is_the_file_it_names() {
        let project = Project::new("absolute");
        project.write("src/lib.rs", "pub fn f() {}\n");
        let absolute = project.root.join("src/lib.rs");
        let shown = project
            .preview(absolute.to_str().expect("text"), 1)
            .expect("a preview");
        assert_eq!(shown.named, "src/lib.rs");
        assert_eq!(project.workspace.file_named("src"), None);
        assert_eq!(
            project.workspace.file_named("./src/lib.rs"),
            Some(project.root.join("src/lib.rs"))
        );
    }

    /// A rule the person wrote against reading a file covers this read as it covers every other.
    #[test]
    fn a_file_a_deny_rule_covers_is_not_read() {
        let project = Project::new("denied");
        project.write(".env", "TOKEN=1\n");
        project.write("notes.txt", "fine\n");
        let (permissions, rejected) = Permissions::parse(
            &["Read(.env)".to_string()],
            &[],
            &[],
            &bravebot_core::permissions::Anchors::none(),
        );
        assert!(rejected.is_empty());
        let read = |named: &str| {
            preview(
                &project.workspace,
                &TrustStore::new(&project.root),
                &permissions,
                &mut NullSink,
                named,
                1,
            )
        };
        assert_eq!(read(".env"), Err(Refused::Denied));
        assert_eq!(read("./.env"), Err(Refused::Denied));
        assert!(read("notes.txt").is_ok());
    }

    /// A name that is not UTF-8 has no spelling a rule can be written about, and its lossy rendering
    /// is the name of another file (PATH-003). Where a rule is in force, a file with such a second
    /// name is refused, since a rule may be about it. The file named like the rendering, and files
    /// whose names are all text, are refused only by a rule that names them.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_no_rule_can_be_written_about_refuses_only_the_file_that_has_it() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let project = Project::new("bytes");
        project.write("plain.txt", "one\n");
        project.write("link-\u{FFFD}", "the lookalike\n");
        project.write("pair-a.txt", "two\n");
        std::fs::hard_link(
            project.root.join("plain.txt"),
            project.root.join(OsStr::from_bytes(b"link-\xff")),
        )
        .expect("a second name that is not text");
        std::fs::hard_link(
            project.root.join("pair-a.txt"),
            project.root.join("pair-b.txt"),
        )
        .expect("a second name that is text");
        let read = |permissions: &Permissions, named: &str| {
            preview(
                &project.workspace,
                &TrustStore::new(&project.root),
                permissions,
                &mut NullSink,
                named,
                1,
            )
        };

        assert!(
            read(&Permissions::new(), "plain.txt").is_ok(),
            "with no rule in force a name no rule can spell refused a file"
        );

        let (permissions, rejected) = Permissions::parse(
            &["Read(link-\u{FFFD})".to_string()],
            &[],
            &[],
            &bravebot_core::permissions::Anchors::none(),
        );
        assert!(rejected.is_empty());
        assert_eq!(
            read(&permissions, "link-\u{FFFD}"),
            Err(Refused::Denied),
            "the rule did not refuse the file it names"
        );
        assert_eq!(
            read(&permissions, "plain.txt"),
            Err(Refused::Denied),
            "a file with a second name no rule can spell was passed"
        );
        for named in ["pair-a.txt", "pair-b.txt"] {
            assert!(
                read(&permissions, named).is_ok(),
                "{named}: a name that cannot be spelled elsewhere in the directory refused a file whose names can"
            );
        }
    }

    /// A file past the size a preview reads is refused by its size, and one that is not text has no
    /// rows, as an empty one has none: the same answer, since nothing here asks the file which it is.
    #[test]
    fn a_large_file_is_refused_and_one_that_is_not_text_has_no_rows() {
        let project = Project::new("size");
        std::fs::write(
            project.root.join("big.txt"),
            vec![b'a'; (MOST_BYTES + 1) as usize],
        )
        .expect("a file");
        std::fs::write(project.root.join("binary.bin"), [0xff, 0xfe, 0x00, 0x80]).expect("a file");
        project.write("empty.txt", "");

        assert_eq!(
            project.preview("big.txt", 1),
            Err(Refused::TooLarge(MOST_BYTES + 1))
        );
        for named in ["binary.bin", "empty.txt"] {
            let shown = project.preview(named, 1).expect("a preview");
            assert!(shown.lines.is_empty(), "{named}: {:?}", shown.lines);
            assert_eq!(shown.total, 0, "{named}");
        }
    }

    /// A pipe put where the file was, after the name was resolved and its kind asked, is refused and
    /// not waited on for a writer. Opened for reading it blocks until one comes, which in the
    /// terminal client is the whole session.
    #[cfg(unix)]
    #[test]
    fn a_pipe_put_where_the_file_was_is_refused_and_not_waited_on() {
        let project = Project::new("pipe");
        let pipe = project.root.join("pipe.txt");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        if !made.is_ok_and(|status| status.success()) {
            return;
        }
        let (sender, answer) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(Workspace::peek_labelled_for_a_preview(&pipe, MOST_BYTES).err());
        });

        let refused = answer
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the preview waited on a pipe for a writer");

        assert_eq!(refused, Some(Unpreviewable::NotAFile));
    }

    /// The size is the open file's: one byte over the most a preview reads is refused with its size,
    /// and a file of exactly that many is read.
    #[test]
    fn a_file_of_one_byte_over_the_most_is_refused_by_the_size_of_the_open_file() {
        let project = Project::new("most");
        project.write("at.txt", &"a".repeat(64));
        project.write("over.txt", &"a".repeat(65));

        let read =
            |name: &str| Workspace::peek_labelled_for_a_preview(&project.root.join(name), 64).err();

        assert_eq!(read("at.txt"), None);
        assert_eq!(read("over.txt"), Some(Unpreviewable::TooLarge(65)));
    }

    /// The release is recorded as one for a screen, so the trail says a file was shown and that it
    /// was reshaped on the way, and nothing in it holds a line of the file.
    #[test]
    fn the_release_is_recorded_as_one_for_a_screen() {
        use bravebot_core::event::{Event, RecordingSink};
        let project = Project::new("trail");
        project.write("a.txt", "a secret line\n");
        let mut trail = RecordingSink::new();
        preview(
            &project.workspace,
            &TrustStore::new(&project.root),
            &Permissions::new(),
            &mut trail,
            "a.txt",
            1,
        )
        .expect("a preview");
        let gates: Vec<&str> = trail
            .events()
            .iter()
            .filter_map(|event| match event {
                Event::GatePassed { gate, .. } => Some(*gate),
                _ => None,
            })
            .collect();
        assert_eq!(gates, ["precommit", "render", "display"]);
        assert!(
            !format!("{:?}", trail.events()).contains("secret"),
            "the trail holds a line of the file"
        );
    }
}
