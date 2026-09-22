//! An incognito session adds no remembered line and no granted rule to `~/.bravebot`, and still
//! honours the ones already there.
//!
//! Two halves of INCOG-5 over two records. What the mode promises is about what survives a session,
//! so an answer given in this one does not reach either record, while an answer somebody gave in an
//! earlier one still stops a prompt: a private session still reads the model and the theme, and
//! this is the same kind of read.
//!
//! # Why a binary of its own
//!
//! Engaging is a one-way door for the life of a process, which is the property that makes it
//! trustworthy and also the reason these cannot share a binary with the tests that assert the
//! ordinary behaviour. An integration test is its own process, so this file engages once and no
//! other test in the workspace is affected.

use bravebot_agent::remembered::Store;
use bravebot_core::command::{Plan, Step, Steps};
use bravebot_core::remembered::RememberedLine;
use std::path::{Path, PathBuf};

/// A scratch state directory that removes itself.
struct Scratch {
    home: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let home = std::env::temp_dir().join(format!("bravebot-agent-incognito-{name}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create scratch");
        bravebot_core::incognito::engage();
        Self { home }
    }

    fn store(&self) -> Store {
        Store::new(&self.home, Path::new("/work"))
    }

    fn grants(&self) -> bravebot_agent::granted::Store {
        bravebot_agent::granted::Store::new(&self.home, Path::new("/work"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn a_line() -> RememberedLine {
    RememberedLine::of(&Plan {
        line: String::new(),
        directory: PathBuf::from("/work"),
        steps: Steps::Pipeline(vec![Step {
            program: "make".to_string(),
            resolved: PathBuf::from("/usr/bin/make"),
            args: vec!["check".to_string()],
            environment: Vec::new(),
            routes: Vec::new(),
        }]),
        writes: Vec::new(),
        reads: Vec::new(),
        stdin: None,
    })
}

/// INCOG-3, RUN-19: an answer given in a private session outlives nothing, so the key that would
/// record one is not offered and a front end answering with it anyway writes no file.
#[test]
fn no_remembered_line_is_written_down() {
    let scratch = Scratch::new("adds-nothing");
    assert!(
        !bravebot_agent::remembered::may_be_added_to(),
        "the prompt would still have offered the key"
    );

    scratch.store().remember(&a_line(), "a-private-session");

    assert!(
        !scratch.store().path().exists(),
        "an incognito session wrote a line into the record"
    );
    assert!(scratch.store().read().is_empty());
}

/// INCOG-5: reading is unchanged. A line an earlier session recorded still stops the asking here,
/// for the reason the chosen model and theme are still read: the promise is about what survives a
/// session rather than about what the session may know.
#[test]
fn a_line_an_earlier_session_recorded_is_still_honoured() {
    let scratch = Scratch::new("still-reads");

    // Seeded as an ordinary session would have left it, past the write this mode declines.
    let store = scratch.store();
    std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("seed the directory");
    std::fs::write(
        store.path(),
        concat!(
            r#"{"directory":"/work","session":"an-earlier-session","line":{"steps":"#,
            r#"{"shape":"pipeline","steps":[{"program":"make","resolved":"/usr/bin/make","#,
            r#""args":["check"],"environment":[],"routes":[]}]}}}"#,
            "\n"
        ),
    )
    .expect("seed a record");

    let read = store.read();
    assert_eq!(read.len(), 1, "an incognito session read nothing back");
    assert_eq!(
        read.iter().next().expect("an entry").line,
        a_line(),
        "the seeded line is not the one this test is about"
    );
}

/// The rule a checkout proposes in the two tests below.
fn a_proposed_rule() -> bravebot_agent::granted::Proposed {
    bravebot_agent::granted::Proposed::new(
        Path::new("/work/.bravebot/settings.json"),
        "Bash(bash scripts/check.sh)",
    )
}

/// INCOG-5, PERM-15: a rule granted in a private session holds for that session and outlives
/// nothing, so the question does not offer to remember it and a front end answering as though it had
/// writes no file. The grant is the same kind of answer a remembered command line is, and this
/// record is not on the list of what still reaches the filesystem.
#[test]
fn no_granted_rule_is_written_down() {
    let scratch = Scratch::new("grants-nothing");
    assert!(
        !bravebot_agent::granted::may_be_added_to(),
        "the question would still have offered to remember the answer"
    );

    let rule = a_proposed_rule();
    scratch.grants().grant(&[&rule], "a-private-session");

    assert!(
        !scratch.grants().path().exists(),
        "an incognito session wrote a grant into the record"
    );
    assert!(
        scratch
            .grants()
            .granted(std::slice::from_ref(&rule))
            .is_empty()
    );
}

/// INCOG-5, PERM-15: reading is unchanged here too. A rule an earlier ordinary session granted in
/// this workspace still stops the asking, so a private session is not one that has to answer every
/// question its user already answered.
#[test]
fn a_rule_an_earlier_session_granted_is_still_honoured() {
    let scratch = Scratch::new("still-reads-grants");

    // Seeded as an ordinary session would have left it, past the write this mode declines.
    let store = scratch.grants();
    std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("seed the directory");
    std::fs::write(
        store.path(),
        concat!(
            r#"{"workspace":"/work","session":"an-earlier-session","#,
            r#""rule":"Bash(bash scripts/check.sh)","#,
            r#""path":"/work/.bravebot/settings.json"}"#,
            "\n"
        ),
    )
    .expect("seed a record");

    let rule = a_proposed_rule();
    assert_eq!(
        store.granted(std::slice::from_ref(&rule)),
        [&rule],
        "an incognito session read no grant back"
    );
}
