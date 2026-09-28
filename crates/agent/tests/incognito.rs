//! An incognito session adds no remembered line, no granted rule and no credential finding to
//! `~/.bravebot`, and still honours the ones already there.
//!
//! Two halves of INCOG-5 over three records. What the mode promises is about what survives a
//! session, so what this one produces does not reach any of them, while what somebody left in an
//! earlier one is still read: a private session still reads the model and the theme, and these are
//! the same kind of read.
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

    fn findings(&self) -> bravebot_agent::findings::Store {
        bravebot_agent::findings::Store::new(&self.home, Path::new("/work"))
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

/// The finding these two tests are about, taken from the scanner so it is the shape a real one has.
fn a_finding() -> bravebot_core::credentials::Finding {
    bravebot_core::credentials::scan(".env", "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE", 17)
        .into_iter()
        .next()
        .expect("a finding over the key")
}

/// INCOG-5, CRED-19: a private session scans what it writes, refuses what it would refuse and puts
/// the finding on the screen, and none of that reaches the record. The finding is a list of where
/// the credentials in this tree are, which is exactly the kind of thing a session that leaves
/// nothing behind may not leave behind.
#[test]
fn no_credential_finding_is_written_down() {
    let scratch = Scratch::new("finds-nothing");
    assert!(
        !bravebot_agent::findings::may_be_added_to(),
        "the record would still have been offered the finding"
    );

    let finding = a_finding();
    scratch
        .findings()
        .record(&[&finding], Some("a-private-session"));

    assert!(
        !scratch.findings().path().exists(),
        "an incognito session wrote a finding into the record"
    );
    assert!(scratch.findings().recorded().is_empty());
}

/// INCOG-5: reading is unchanged here too. A finding an earlier ordinary session recorded in this
/// workspace is still there to read, for the reason the chosen model and theme are: the promise is
/// about what survives a session rather than about what the session may know.
#[test]
fn a_finding_an_earlier_session_recorded_is_still_read() {
    let scratch = Scratch::new("still-reads-findings");

    // Seeded as an ordinary session would have left it, past the write this mode declines.
    let store = scratch.findings();
    std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("seed the directory");
    let finding = a_finding();
    std::fs::write(
        store.path(),
        format!(
            concat!(
                r#"{{"workspace":"/work","session":"an-earlier-session","#,
                r#""kind":"aws-access-key","path":".env","line":1,"#,
                r#""fingerprint":"{}","preview":"{}"}}"#,
                "\n"
            ),
            finding.fingerprint, finding.preview
        ),
    )
    .expect("seed a record");

    assert_eq!(
        store.recorded(),
        [finding],
        "an incognito session read no finding back"
    );
}

/// The record of a kept startup answer for a directory that exists, and that directory's identity.
///
/// `None` on a filesystem that cannot say when a directory was made, where nothing is ever kept.
fn a_kept_directory(
    scratch: &Scratch,
) -> Option<(
    bravebot_agent::trusted::Store,
    bravebot_agent::trusted::Identity,
)> {
    let identity = bravebot_agent::trusted::Identity::of(&scratch.home)?;
    Some((
        bravebot_agent::trusted::Store::new(&scratch.home, &scratch.home),
        identity,
    ))
}

/// INCOG-5, TRUST-23: a private session keeps no answer to the startup question, so the key that
/// would keep one is not offered and a front end answering with it anyway writes no file. Withdrawing
/// one is a write too, so it is refused rather than made.
#[test]
fn no_trusted_directory_is_written_down() {
    let scratch = Scratch::new("trusts-nothing");
    assert!(
        !bravebot_agent::trusted::may_be_written(),
        "the question would still have offered to keep the answer"
    );
    let Some((store, identity)) = a_kept_directory(&scratch) else {
        return;
    };

    assert!(!store.keep(&identity, "a-private-session", 1));
    assert!(
        !store.path().exists(),
        "an incognito session kept an answer in the record"
    );
    assert!(
        store.forget().is_err(),
        "an incognito session withdrew an answer"
    );
}

/// INCOG-5, TRUST-23: reading is unchanged here too. An answer an earlier ordinary session kept for
/// this directory still answers, and the session says so where it starts.
#[test]
fn a_directory_an_earlier_session_kept_is_still_trusted() {
    let scratch = Scratch::new("still-reads-trusted");
    let Some((store, identity)) = a_kept_directory(&scratch) else {
        return;
    };

    // Seeded as an ordinary session would have left it, past the write this mode declines.
    std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("seed the directory");
    std::fs::write(
        store.path(),
        format!(
            r#"{{"directory":{},"identity":{},"session":"an-earlier-session","at":1}}{}"#,
            serde_json::to_string(&scratch.home).expect("a path"),
            serde_json::to_string(&identity).expect("an identity"),
            "\n"
        ),
    )
    .expect("seed a record");

    assert_eq!(
        store.kept(&identity).map(|kept| kept.session),
        Some("an-earlier-session".to_string()),
        "an incognito session read no kept answer back"
    );
}
