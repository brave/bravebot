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

use bravebot_core::command::Route;
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
    /// exactly as the session record holds the path its key was made from.
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
    pub fn read(&self) -> Remembered {
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return Remembered::new();
        };
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Written>(line).ok())
            .filter(|entry| entry.directory == self.directory.to_string_lossy())
            .map(|entry| bravebot_core::remembered::Entry {
                line: entry.line.into(),
                answered_in: entry.session,
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
            directory: self.directory.to_string_lossy().to_string(),
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
    directory: String,
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
    resolved: String,
    args: Vec<String>,
    environment: Vec<(String, String)>,
    routes: Vec<WrittenRoute>,
}

/// Where one of a step's streams went.
///
/// Spelled out rather than rendered, for the reason every other field is: two routings that render
/// alike are two routings, and a record that could not tell them apart would cover a line whose
/// output went somewhere the person never saw.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "stream", rename_all = "lowercase", deny_unknown_fields)]
enum WrittenRoute {
    Stdout { path: String, append: bool },
    Stdin { path: String },
    Stderr { path: String, append: bool },
    Both { path: String },
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
    fn from(step: &RememberedStep) -> Self {
        Self {
            program: step.program.clone(),
            resolved: step.resolved.clone(),
            args: step.args.clone(),
            environment: step.environment.clone(),
            routes: step.routes.iter().map(WrittenRoute::from).collect(),
        }
    }
}

impl From<&Route> for WrittenRoute {
    fn from(route: &Route) -> Self {
        let named = |path: &std::path::Path| path.to_string_lossy().to_string();
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

impl From<WrittenLine> for RememberedLine {
    fn from(line: WrittenLine) -> Self {
        Self {
            steps: line.steps.into(),
        }
    }
}

impl From<WrittenShape> for Shape {
    fn from(shape: WrittenShape) -> Self {
        match shape {
            WrittenShape::Pipeline { steps } => {
                Self::Pipeline(steps.into_iter().map(RememberedStep::from).collect())
            }
            WrittenShape::Join {
                left,
                joiner,
                right,
            } => Self::Join {
                left: Box::new((*left).into()),
                joiner: match joiner {
                    WrittenJoiner::And => bravebot_core::command::Joiner::And,
                    WrittenJoiner::Or => bravebot_core::command::Joiner::Or,
                    WrittenJoiner::Then => bravebot_core::command::Joiner::Then,
                },
                right: Box::new((*right).into()),
            },
            WrittenShape::Group { inner } => Self::Group(Box::new((*inner).into())),
        }
    }
}

impl From<WrittenStep> for RememberedStep {
    fn from(step: WrittenStep) -> Self {
        Self {
            program: step.program,
            resolved: step.resolved,
            args: step.args,
            environment: step.environment,
            routes: step.routes.into_iter().map(Route::from).collect(),
        }
    }
}

impl From<WrittenRoute> for Route {
    fn from(route: WrittenRoute) -> Self {
        match route {
            WrittenRoute::Stdout { path, append } => Self::Stdout {
                path: PathBuf::from(path),
                append,
            },
            WrittenRoute::Stdin { path } => Self::Stdin {
                path: PathBuf::from(path),
            },
            WrittenRoute::Stderr { path, append } => Self::Stderr {
                path: PathBuf::from(path),
                append,
            },
            WrittenRoute::Both { path } => Self::Both {
                path: PathBuf::from(path),
            },
            WrittenRoute::StderrToStdout => Self::StderrToStdout,
        }
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
