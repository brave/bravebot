//! Where the command lines somebody asked to be remembered past the session are kept.
//!
//! One file per working directory under `~/.bravebot/remembered`, keyed the way the session store
//! is keyed ([`crate::home::key_for`]), so every session begun in a directory reads the answers
//! given in that directory and no others. What `make check` does depends on the tree it runs in,
//! and the person answered about one tree.
//!
//! [`bravebot_core::remembered`] is what an entry means and what it covers. This module is the
//! file: how an entry is spelled, when it is read, and what happens when it cannot be.
//!
//! # One line per entry, appended
//!
//! The file is JSON, one object per line. An entry is added rather than the file rewritten, so two
//! sessions open in one directory cannot lose each other's answers, and a turn running ten
//! delegates has ten writers of one file rather than ten copies of it.
//!
//! An unreadable line is skipped and the rest of the file still answers. A file written by a later
//! build, a half-written line from a disk that filled, a file somebody edited by hand: none of them
//! should turn every run in a directory into a prompt, and none of them should make a line cover
//! something it is not. Skipping is both.
//!
//! # Everything degrades to asking
//!
//! No home, an unreadable file, a write that fails: each of them means the record says nothing, and
//! a record that says nothing is a session that asks, which is what a session did before this
//! existed. Nothing here fails a run.
//!
//! # What is not written
//!
//! Nothing a program printed. What goes into the file is the argument list a person read at the
//! prompt, which is the driver's own compiled plan and trusted and public before it reaches any
//! gate, and the identifier of the session they pressed the key in.

use bravebot_core::command::{Route, Spelling};
use bravebot_core::remembered::{Remembered, RememberedLine, RememberedStep, Shape};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory the per-directory records live in, inside the state directory.
const REMEMBERED: &str = "remembered";

/// Whether this session may add a line to a record.
///
/// False in a session that adds nothing to `~/.bravebot`, which keeps a closed list of what still
/// reaches the filesystem and this is not on it. So the key that records an answer is not offered
/// there, and a front end answering with it anyway writes nothing.
///
/// Reading is unchanged, which is why this is about adding rather than about the record existing:
/// a private session still runs a line an earlier session recorded without asking, for the reason
/// it still reads the model and the theme somebody chose. The promise is about what survives a
/// session, not about what the session may know.
pub fn may_be_added_to() -> bool {
    !bravebot_core::incognito::engaged()
}

/// The record for one working directory.
///
/// Holds where the file is rather than what it says: the file belongs to every session begun in
/// the directory, so what it says is read at the moment the question is asked rather than kept.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    /// The directory the entries were answered about, written into each one.
    ///
    /// The key is lossy, so two directories whose names reduce to the same segment share a file.
    /// Recording the real path is what lets a reader see which one an entry was answered about,
    /// exactly as the session record holds the path its key was made from. It is also what decides
    /// which entries in a shared file answer for this directory, so it is written as a
    /// [`WrittenPath`] rather than rendered: a rendering of it collides exactly where the key does.
    directory: PathBuf,
}

impl Store {
    /// The record for `directory` inside `home`.
    ///
    /// Takes the state directory rather than resolving it, for the reason everything else in this
    /// crate takes it: a library that reached for `$HOME` behind its callers' backs would make
    /// every test depend on whatever the developer happened to have installed.
    pub fn new(home: &Path, directory: &Path) -> Self {
        Self {
            path: home
                .join(REMEMBERED)
                .join(format!("{}.jsonl", crate::home::key_for(directory))),
            directory: directory.to_path_buf(),
        }
    }

    /// Where the record is, which is what a prompt offering to write it has to show.
    ///
    /// A person cannot endorse a record they were not shown, and deleting a line from the file is
    /// the way back from having pressed the key.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the record holds now.
    ///
    /// Read afresh on every call. The file is shared by every session begun in this directory, so a
    /// line recorded a minute ago in another one is covered by this call and a line deleted a
    /// minute ago is not.
    ///
    /// An empty record is the answer for a file that is not there, cannot be read, or holds nothing
    /// this build understands.
    ///
    /// An entry whose spelling of a path names no path is skipped, as an unreadable line is: see
    /// [`WrittenPath`].
    pub fn read(&self) -> Remembered {
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return Remembered::new();
        };
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Written>(line).ok())
            .filter_map(|entry| {
                if entry.directory.to_path()? != self.directory {
                    return None;
                }
                Some(bravebot_core::remembered::Entry {
                    line: entry.line.into_line()?,
                    answered_in: entry.session,
                })
            })
            .collect()
    }

    /// Add one line to the record, answered in the session named.
    ///
    /// Appended, never rewritten. Best effort: a home that is full or read-only means the answer
    /// does not last past this session, which is the state every answer was in before this existed,
    /// and is not a reason to refuse a run the person has just approved.
    pub fn remember(&self, line: &RememberedLine, session: &str) {
        if !may_be_added_to() {
            return;
        }
        let Some(parent) = self.path.parent() else {
            return;
        };
        if crate::home::create_directory(parent).is_err() {
            return;
        }
        let Ok(mut encoded) = serde_json::to_string(&Written {
            directory: WrittenPath::of(&self.directory),
            session: session.to_string(),
            line: line.into(),
        }) else {
            return;
        };
        encoded.push('\n');
        if let Ok(mut file) = crate::home::append_to_file(&self.path) {
            let _ = file.write_all(encoded.as_bytes());
        }
    }
}

/// One entry as it is spelled on disk.
///
/// Every field of the line is its own field here, which is the difference between this and a rule
/// in the settings file: nothing written down has a spelling that means "any text".
///
/// A field this build does not know refuses the whole entry rather than being passed over, here and
/// in everything below it. A later build that narrows what an entry covers does it by adding a
/// field, and an older build that read such an entry while ignoring that field would run a line
/// unasked that the newer one would have asked about. Refusing is the direction that asks.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    /// The working directory this answer was given about, in full.
    directory: WrittenPath,
    /// The session the key was pressed in. Decides nothing; it is what the reading back says.
    session: String,
    line: WrittenLine,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenLine {
    steps: WrittenShape,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase", deny_unknown_fields)]
enum WrittenShape {
    Pipeline {
        steps: Vec<WrittenStep>,
    },
    Join {
        left: Box<WrittenShape>,
        joiner: WrittenJoiner,
        right: Box<WrittenShape>,
    },
    Group {
        inner: Box<WrittenShape>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WrittenJoiner {
    And,
    Or,
    Then,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenStep {
    program: String,
    resolved: WrittenPath,
    args: Vec<String>,
    environment: Vec<(String, String)>,
    routes: Vec<WrittenRoute>,
}

/// A path as this record spells it.
///
/// A string for a path that has a text spelling, which is every path anybody types and every path
/// any entry written before this field could hold anything else. A list of bytes for a path that has
/// none. Untagged, because JSON already distinguishes a string from a list, so an entry from an
/// earlier build reads back as the path it always named and no spelling can be mistaken for the
/// other.
///
/// [`Spelling`] is why a rendering will not do here, and why a string holding a replacement
/// character names no path: an entry written that way by an earlier build held a rendering of some
/// binary nobody can now identify, and reading it as the path it renders to would answer for a
/// binary the person never approved. Such an entry covers nothing, and this build writes the bytes
/// instead, so nothing it writes is refused when it is read back.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum WrittenPath {
    Text(String),
    Bytes(Vec<u8>),
}

impl WrittenPath {
    fn of(path: &Path) -> Self {
        match Spelling::of(path) {
            Spelling::Text(text) => Self::Text(text),
            Spelling::Bytes(bytes) => Self::Bytes(bytes),
        }
    }

    fn to_path(&self) -> Option<PathBuf> {
        match self {
            Self::Text(text) => Spelling::Text(text.clone()),
            Self::Bytes(bytes) => Spelling::Bytes(bytes.clone()),
        }
        .into_path()
    }
}

/// Where one of a step's streams went.
///
/// Spelled out rather than rendered, for the reason every other field is: two routings that render
/// alike are two routings, and a record that could not tell them apart would cover a line whose
/// output went somewhere the person never saw.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "stream", rename_all = "lowercase", deny_unknown_fields)]
enum WrittenRoute {
    Stdout { path: WrittenPath, append: bool },
    Stdin { path: WrittenPath },
    Stderr { path: WrittenPath, append: bool },
    Both { path: WrittenPath },
    StderrToStdout,
}

impl From<&RememberedLine> for WrittenLine {
    fn from(line: &RememberedLine) -> Self {
        Self {
            steps: (&line.steps).into(),
        }
    }
}

impl From<&Shape> for WrittenShape {
    fn from(shape: &Shape) -> Self {
        match shape {
            Shape::Pipeline(steps) => Self::Pipeline {
                steps: steps.iter().map(WrittenStep::from).collect(),
            },
            Shape::Join {
                left,
                joiner,
                right,
            } => Self::Join {
                left: Box::new(left.as_ref().into()),
                joiner: match joiner {
                    bravebot_core::command::Joiner::And => WrittenJoiner::And,
                    bravebot_core::command::Joiner::Or => WrittenJoiner::Or,
                    bravebot_core::command::Joiner::Then => WrittenJoiner::Then,
                },
                right: Box::new(right.as_ref().into()),
            },
            Shape::Group(inner) => Self::Group {
                inner: Box::new(inner.as_ref().into()),
            },
        }
    }
}

impl From<&RememberedStep> for WrittenStep {
    /// Destructured rather than read field by field, so that a field added to the entry stops the
    /// build here instead of being left out of the file. What is written is what is read back and
    /// matched on, so a field missing from this conversion is one the record does not hold and an
    /// entry restored from it covers a line that differs in exactly that field ([RUN-8]).
    ///
    /// [RUN-8]: ../../../docs/specs/tools/run.md
    fn from(step: &RememberedStep) -> Self {
        let RememberedStep {
            program,
            resolved,
            args,
            environment,
            routes,
        } = step;
        Self {
            program: program.clone(),
            resolved: WrittenPath::of(resolved),
            args: args.clone(),
            environment: environment.clone(),
            routes: routes.iter().map(WrittenRoute::from).collect(),
        }
    }
}

impl From<&Route> for WrittenRoute {
    fn from(route: &Route) -> Self {
        let named = WrittenPath::of;
        match route {
            Route::Stdout { path, append } => Self::Stdout {
                path: named(path),
                append: *append,
            },
            Route::Stdin { path } => Self::Stdin { path: named(path) },
            Route::Stderr { path, append } => Self::Stderr {
                path: named(path),
                append: *append,
            },
            Route::Both { path } => Self::Both { path: named(path) },
            Route::StderrToStdout => Self::StderrToStdout,
        }
    }
}

impl WrittenLine {
    /// The line this entry holds, or nothing where it holds a path nobody can name.
    ///
    /// Nothing rather than a line with that path left out or rendered: either would be a shorter
    /// key, and a shorter key covers more than the person answered about.
    fn into_line(self) -> Option<RememberedLine> {
        Some(RememberedLine {
            steps: self.steps.into_shape()?,
        })
    }
}

impl WrittenShape {
    fn into_shape(self) -> Option<Shape> {
        Some(match self {
            Self::Pipeline { steps } => Shape::Pipeline(
                steps
                    .into_iter()
                    .map(WrittenStep::into_step)
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Join {
                left,
                joiner,
                right,
            } => Shape::Join {
                left: Box::new((*left).into_shape()?),
                joiner: match joiner {
                    WrittenJoiner::And => bravebot_core::command::Joiner::And,
                    WrittenJoiner::Or => bravebot_core::command::Joiner::Or,
                    WrittenJoiner::Then => bravebot_core::command::Joiner::Then,
                },
                right: Box::new((*right).into_shape()?),
            },
            Self::Group { inner } => Shape::Group(Box::new((*inner).into_shape()?)),
        })
    }
}

impl WrittenStep {
    fn into_step(self) -> Option<RememberedStep> {
        Some(RememberedStep {
            program: self.program,
            resolved: self.resolved.to_path()?,
            args: self.args,
            environment: self.environment,
            routes: self
                .routes
                .into_iter()
                .map(WrittenRoute::into_route)
                .collect::<Option<Vec<_>>>()?,
        })
    }
}

impl WrittenRoute {
    fn into_route(self) -> Option<Route> {
        Some(match self {
            Self::Stdout { path, append } => Route::Stdout {
                path: path.to_path()?,
                append,
            },
            Self::Stdin { path } => Route::Stdin {
                path: path.to_path()?,
            },
            Self::Stderr { path, append } => Route::Stderr {
                path: path.to_path()?,
                append,
            },
            Self::Both { path } => Route::Both {
                path: path.to_path()?,
            },
            Self::StderrToStdout => Route::StderrToStdout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::command::{Plan, Step, Steps};

    /// A state directory that removes itself, standing in for `~/.bravebot`.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self { path }
        }

        fn store(&self, directory: &str) -> Store {
            Store::new(&self.path, Path::new(directory))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// A line with something in every field an entry keys on, so a field lost on the way to disk
    /// shows up as a line covering more than the person read.
    fn plan_in(directory: &str, program: &str, args: &[&str]) -> Plan {
        Plan {
            line: String::new(),
            directory: PathBuf::from(directory),
            steps: Steps::Pipeline(vec![Step {
                program: program.to_string(),
                resolved: PathBuf::from(format!("/usr/bin/{program}")),
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
                environment: vec![("CI".to_string(), "1".to_string())],
                routes: vec![Route::StderrToStdout],
            }]),
            writes: Vec::new(),
            reads: Vec::new(),
            stdin: None,
        }
    }

    fn plan(program: &str, args: &[&str]) -> Plan {
        plan_in("/work", program, args)
    }

    /// Same line with one field emptied, which is a different line and must not be covered.
    fn without(plan: &Plan, strip: fn(&mut Step)) -> Plan {
        let mut step = (*plan.steps().first().expect("a step")).clone();
        strip(&mut step);
        Plan {
            steps: Steps::Pipeline(vec![step]),
            ..plan.clone()
        }
    }

    /// RUN-19: the answer outlives the session. A line written by one session is read back by the
    /// next, because the file is what carries it and the file is keyed by the directory.
    #[test]
    fn a_line_written_by_one_session_is_read_back_by_another() {
        let scratch = Scratch::new("remembered-across-sessions");
        scratch
            .store("/work")
            .remember(&RememberedLine::of(&plan("make", &["check"])), "the-first");

        let read = scratch.store("/work").read();
        assert!(read.covers(&plan("make", &["check"])));
        assert_eq!(read.len(), 1);
        assert_eq!(
            read.iter().next().expect("an entry").answered_in,
            "the-first"
        );
    }

    /// RUN-19: every field survives the round trip. A field lost on the way to disk is a field the
    /// entry no longer keys on, and an entry that covers more than the person read.
    #[test]
    fn every_field_of_a_line_survives_being_written_and_read() {
        let scratch = Scratch::new("remembered-fields");
        let recorded = plan("make", &["check"]);
        scratch
            .store("/work")
            .remember(&RememberedLine::of(&recorded), "a-session");
        let read = scratch.store("/work").read();

        assert!(read.covers(&recorded));
        for narrower in [
            plan("make", &["install"]),
            without(&recorded, |step| step.environment.clear()),
            without(&recorded, |step| step.routes.clear()),
            without(&recorded, |step| step.program = "gmake".to_string()),
            without(&recorded, |step| step.resolved = PathBuf::from("/tmp/make")),
        ] {
            assert!(
                !read.covers(&narrower),
                "a field was lost on the way to disk: {} was covered",
                narrower.display()
            );
        }
    }

    /// RUN-19: an entry is added rather than the record rewritten, so two sessions open in one
    /// directory cannot lose each other's answers.
    #[test]
    fn a_second_answer_is_added_rather_than_replacing_the_first() {
        let scratch = Scratch::new("remembered-appends");
        let store = scratch.store("/work");
        store.remember(&RememberedLine::of(&plan("make", &["check"])), "one");
        store.remember(&RememberedLine::of(&plan("cargo", &["test"])), "two");

        let read = store.read();
        assert_eq!(read.len(), 2);
        assert!(read.covers(&plan("make", &["check"])));
        assert!(read.covers(&plan("cargo", &["test"])));
    }

    /// RUN-19: the record is keyed by the directory, so a person working in two clones of one
    /// repository answers in each.
    #[test]
    fn a_line_answered_in_one_directory_does_not_answer_in_another() {
        let scratch = Scratch::new("remembered-per-directory");
        scratch
            .store("/work")
            .remember(&RememberedLine::of(&plan("make", &["check"])), "a-session");

        assert!(scratch.store("/other").read().is_empty());
    }

    /// RUN-19: the key a directory reduces to is lossy, so two directories can share a file. The
    /// full path is written into every entry, so a session is answered by its own directory's lines
    /// rather than by whatever else happened to reduce to the same name.
    #[test]
    fn a_directory_sharing_a_key_with_another_is_not_answered_by_its_lines() {
        let scratch = Scratch::new("remembered-lossy-key");
        let mine = "/a/b";
        let theirs = "/a-b";
        assert_eq!(
            crate::home::key_for(Path::new(mine)),
            crate::home::key_for(Path::new(theirs)),
            "this test needs two paths that reduce to one key"
        );

        scratch.store(theirs).remember(
            &RememberedLine::of(&plan_in(theirs, "make", &["check"])),
            "a-session",
        );

        assert!(scratch.store(mine).read().is_empty());
    }

    /// A path whose last byte is not valid UTF-8. `display` renders any two of them alike, and the
    /// file name a directory reduces to drops the byte entirely.
    #[cfg(unix)]
    fn unrenderable(prefix: &str, last: u8) -> PathBuf {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.push(last);
        PathBuf::from(std::ffi::OsStr::from_bytes(&bytes))
    }

    /// RUN-19: the binary is written as bytes, not as a rendering of them. `to_string_lossy` maps
    /// every byte it cannot read onto one replacement character, so a record holding a rendering
    /// answered for every binary whose path renders that way, in every session after the one that
    /// wrote it.
    #[cfg(unix)]
    #[test]
    fn a_binary_no_rendering_can_show_is_read_back_as_itself() {
        let scratch = Scratch::new("remembered-unrenderable-binary");
        let running = |last: u8| {
            let base = plan("make", &["check"]);
            let mut only = (*base.steps().first().expect("a step")).clone();
            only.resolved = unrenderable("/work/make-", last);
            Plan {
                steps: Steps::Pipeline(vec![only]),
                ..base
            }
        };
        scratch
            .store("/work")
            .remember(&RememberedLine::of(&running(0xff)), "a-session");

        let read = scratch.store("/work").read();
        assert!(
            read.covers(&running(0xff)),
            "the binary that was answered for was not read back"
        );
        assert!(
            !read.covers(&running(0xfe)),
            "an answer about one binary covered another that renders the same way"
        );
    }

    /// RUN-19: and so is the tree, for the reason the lossy file key already gives. Two trees
    /// differing only in a byte nothing can render reduce to one file name, so the full path written
    /// into every entry is the only thing keeping their answers apart.
    #[cfg(unix)]
    #[test]
    fn a_tree_no_rendering_can_show_is_answered_only_by_its_own_lines() {
        let scratch = Scratch::new("remembered-unrenderable-tree");
        let mine = unrenderable("/work-", 0xff);
        let theirs = unrenderable("/work-", 0xfe);
        assert_eq!(
            crate::home::key_for(&mine),
            crate::home::key_for(&theirs),
            "this test needs two trees that reduce to one key"
        );
        let line = |tree: &Path| Plan {
            directory: tree.to_path_buf(),
            ..plan("make", &["check"])
        };

        Store::new(&scratch.path, &theirs)
            .remember(&RememberedLine::of(&line(&theirs)), "a-session");

        assert!(
            Store::new(&scratch.path, &theirs)
                .read()
                .covers(&line(&theirs)),
            "the tree the answer was given in did not read it back"
        );
        assert!(
            Store::new(&scratch.path, &mine).read().is_empty(),
            "an answer given in one tree answered in another that renders the same way"
        );
    }

    /// RUN-19: an entry naming a rendering rather than a path covers nothing, which is what a record
    /// written by a build that keyed on `to_string_lossy` holds. The replacement character in it is
    /// equally consistent with every byte it could have stood for, so reading it as a path would
    /// answer for a file nobody was shown. Refused rather than repaired: the run asks, as it did
    /// before anything was remembered.
    #[test]
    fn an_entry_whose_recorded_binary_is_a_rendering_covers_nothing() {
        let scratch = Scratch::new("remembered-rendered-binary");
        let store = scratch.store("/work");
        store.remember(&RememberedLine::of(&plan("make", &["check"])), "a-session");

        let written = std::fs::read_to_string(store.path()).expect("the record");
        let rendered = written.replacen(
            r#""resolved":"/usr/bin/make""#,
            r#""resolved":"/usr/bin/make-�""#,
            1,
        );
        assert_ne!(
            rendered, written,
            "the entry was not rewritten, so this test proves nothing"
        );
        std::fs::write(store.path(), rendered).expect("rewritten");

        assert!(
            store.read().is_empty(),
            "an entry naming a rendering of a path still covered a line"
        );
    }

    /// Everything degrades to asking. A record nothing can read says nothing, which is what a
    /// session did before this existed.
    #[test]
    fn a_record_that_cannot_be_read_covers_nothing() {
        let scratch = Scratch::new("remembered-unreadable");
        let store = scratch.store("/work");
        assert!(store.read().is_empty(), "a missing file answered");

        std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("made");
        std::fs::write(store.path(), "{ not json at all\n").expect("written");
        assert!(store.read().is_empty());
    }

    /// An entry holding a field this build does not know is refused rather than read past. A later
    /// build narrows what an entry covers by adding a field, and an older one that ignored that
    /// field would run unasked a line the newer one would have asked about.
    #[test]
    fn an_entry_this_build_does_not_fully_understand_covers_nothing() {
        let scratch = Scratch::new("remembered-unknown-field");
        let store = scratch.store("/work");
        store.remember(&RememberedLine::of(&plan("make", &["check"])), "a-session");

        let written = std::fs::read_to_string(store.path()).expect("the record");
        let narrowed = written.replacen(
            r#"{"directory""#,
            r#"{"something-later-builds-key-on":"x","directory""#,
            1,
        );
        assert_ne!(
            narrowed, written,
            "the entry was not rewritten, so this test proves nothing"
        );
        std::fs::write(store.path(), narrowed).expect("rewritten");

        assert!(
            store.read().is_empty(),
            "an entry with a field this build cannot account for still covered a line"
        );
    }

    /// One unreadable line does not take the rest of the file with it: a half-written line from a
    /// disk that filled should not turn every run in a directory back into a prompt.
    #[test]
    fn a_line_nothing_can_read_leaves_the_rest_of_the_record_answering() {
        let scratch = Scratch::new("remembered-partial");
        let store = scratch.store("/work");
        store.remember(&RememberedLine::of(&plan("make", &["check"])), "a-session");

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(store.path())
            .expect("opened");
        file.write_all(b"{\"directory\":\"/work\",\"session\":\"truncated\"\n")
            .expect("written");
        drop(file);

        let read = store.read();
        assert_eq!(read.len(), 1);
        assert!(read.covers(&plan("make", &["check"])));
    }
}
