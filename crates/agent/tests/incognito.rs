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

#[allow(dead_code)]
mod repository;

use bravebot_agent::remembered::Store;
use bravebot_core::command::{Plan, Step, Steps};
use bravebot_core::remembered::RememberedLine;
use std::io::{BufRead, BufReader, Read, Write};
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
            started_as: PathBuf::from("/usr/bin/make"),
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

/// INCOG-3, CRED-13: the same for a file a write may create a credential in. The key that would
/// record one is not offered, and a front end answering with it anyway writes no file.
#[test]
fn no_file_a_credential_may_be_created_in_is_written_down() {
    let scratch = Scratch::new("adds-no-file");
    assert!(
        !bravebot_agent::remembered::may_be_added_to(),
        "the write prompt would still have offered the key"
    );

    scratch
        .store()
        .remember_file(Path::new("/work/.env"), "a-private-session");

    assert!(
        !scratch.store().path().exists(),
        "an incognito session wrote a file into the record"
    );
    assert!(!scratch.store().read().covers_file(Path::new("/work/.env")));
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

/// INCOG-5, CRED-21: a person who asks a private session to accept a finding is told it was not
/// kept, and no acceptance file or directory is made. Keeping it would leave behind a list of
/// where this tree's credentials are.
#[test]
fn no_acceptance_is_written_down() {
    let scratch = Scratch::new("accepts-nothing");

    let result = scratch
        .findings()
        .accept(&a_finding(), "a development key", u64::MAX);

    assert_eq!(
        result.expect_err("the acceptance was kept").kind(),
        std::io::ErrorKind::PermissionDenied,
        "an incognito session did not refuse the acceptance"
    );
    assert!(
        !scratch.home.join("findings").exists(),
        "an incognito session wrote an acceptance"
    );
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

/// MEMORY-5, INCOG-8: the record of the memories a write left untrusted is kept in a private
/// session too. It names a path and nothing a person typed, and without it the next session would
/// read what this one left untrusted as trusted.
#[test]
fn a_memory_left_untrusted_is_still_recorded() {
    let scratch = Scratch::new("records-untrusted-memory");
    let record = bravebot_agent::memory::Record::new(&scratch.home, "/work");

    record
        .keep("/work/.bravebot/memory/notes.md")
        .expect("an incognito session refused to record an untrusted memory");

    assert_eq!(
        record.paths(),
        vec!["/work/.bravebot/memory/notes.md".to_string()]
    );
}

/// One reply as the SSE stream the turn loop reads: `content`, or a call to `spawn_agent`.
fn sse(content: Option<&str>, spawn: Option<&str>) -> String {
    let mut frames = vec![serde_json::json!({"choices": [{"delta": {"role": "assistant"}}]})];
    if let Some(content) = content {
        frames.push(serde_json::json!({"choices": [{"delta": {"content": content}}]}));
    }
    if let Some(arguments) = spawn {
        frames.push(serde_json::json!({"choices": [{"delta": {"tool_calls": [
            {"index": 0, "id": "c1", "function": {"name": "spawn_agent", "arguments": arguments}}
        ]}}]}));
    }
    frames.push(serde_json::json!({"choices": [{"finish_reason": "stop"}]}));
    let mut text: String = frames
        .iter()
        .map(|frame| format!("data: {frame}\n\n"))
        .collect();
    text.push_str("data: [DONE]\n\n");
    text
}

/// A model that starts one delegate asking for a checkout, and records every request it is sent.
///
/// Told apart by what a request holds rather than by the order they arrive in, since the delegate
/// and the turn that started it ask at the same time: the delegate's is the one carrying its task
/// and no tool result, and the turn's later ones carry the result.
fn a_model_starting_a_delegate_in_a_checkout() -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let sender = sender.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut length = 0usize;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 && line.trim() != "" {
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    line.clear();
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).to_string();
                let _ = sender.send(body.clone());
                let reply = if body.contains("\"role\":\"tool\"") {
                    sse(Some("done"), None)
                } else if body.contains("THE-DELEGATES-TASK") {
                    sse(Some("ran"), None)
                } else {
                    sse(
                        None,
                        Some(
                            r#"{"kind":"worker","task":"THE-DELEGATES-TASK","isolation":"checkout"}"#,
                        ),
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), received)
}

/// CHECKOUT-6, INCOG-5: a private session with a state directory is given its delegate's checkout
/// in the system temporary directory, and nothing is made under that state directory, which is
/// where a checkout would outlast the session.
#[test]
fn a_checkout_is_not_made_under_the_state_directory() {
    let scratch = Scratch::new("checkout-not-in-state");
    let work = std::env::temp_dir().join("bravebot-agent-incognito-checkout-work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("create the working directory");
    repository::commit_files(&work, &[("README", "committed\n")], "first");
    let workspace = bravebot_agent::Workspace::new(&work).expect("workspace");
    let (endpoint, received) = a_model_starting_a_delegate_in_a_checkout();
    let config = bravebot_config::Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.clone()),
        _ => None,
    })
    .expect("config");
    let mut trust = bravebot_core::trust::TrustStore::new(workspace.root());
    trust.trust(".");

    let outcome = bravebot_agent::turn::run_cancellable(
        &config,
        &bravebot_net::Egress::new(),
        &workspace,
        &bravebot_agent::turn::Task::new("START-THE-DELEGATE")
            .with_home(Some(scratch.home.clone())),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut bravebot_core::event::RecordingSink::new(),
        trust,
        &bravebot_core::cancel::Cancel::new(),
    );
    drop(workspace);
    let _ = std::fs::remove_dir_all(&work);
    outcome.expect("turn runs");

    let asked: Vec<String> = received.try_iter().collect();
    assert!(
        asked
            .iter()
            .any(|body| body.contains("It works in a checkout of commit")),
        "the delegate was not given a checkout"
    );
    assert!(
        !scratch.home.join("checkouts").exists(),
        "a private session made a checkout under the state directory"
    );
}
