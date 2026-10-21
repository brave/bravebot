//! Drafting a commit message from a diff, outside any turn (CMD-23).
//!
//! What each test pins is where the diff goes and what stops it: a processor is the only reader,
//! the request has no tool in it, and a diff the driver may not send, or that shows nothing, makes
//! no request at all.

#[allow(dead_code)]
mod repository;

#[path = "../test-support/mock_model.rs"]
mod mock_model;

use bravebot_agent::commit_message::{DraftError, Source};
use bravebot_agent::turn;
use bravebot_agent::workspace::Workspace;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::trust::TrustStore;
use mock_model::{Scratch, config_for, serve_sequence, trust_of};
use serde_json::json;

const MARKER: &str = "===== the document starts here =====";

/// A model reply with newlines in it, which `mock_model::reply_with` does not escape.
fn the_model_says(content: &str) -> String {
    json!({"model": "test-model", "choices": [{"message": {
        "role": "assistant", "content": content}}]})
    .to_string()
}

/// A repository whose `main` has two commits, the second changing `src/lib.rs`.
fn two_commits(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    repository::commit_files(
        &scratch.path,
        &[("src/lib.rs", "pub fn old() {}\n")],
        "first",
    );
    repository::commit_files(
        &scratch.path,
        &[("src/lib.rs", "pub fn renamed() {}\n")],
        "second",
    );
    scratch
}

fn draft(
    scratch: &Scratch,
    trust: impl FnOnce(&Workspace) -> TrustStore,
    source: &Source,
    format: Option<&str>,
    replies: Vec<String>,
) -> (
    Result<bravebot_agent::commit_message::Drafted, DraftError>,
    std::sync::mpsc::Receiver<String>,
) {
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let (endpoint, received) = serve_sequence(replies);
    let config = config_for(&endpoint);
    let trust = trust(&workspace);
    let done = turn::commit_message(
        &config,
        &bravebot_net::Egress::new(),
        &workspace,
        source,
        format,
        None,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut RecordingSink::new(),
        trust,
        &Cancel::new(),
    );
    (done, received)
}

/// The message is the document the processor marked, the diff reaches the model, and the request
/// carries no tool a reply could call: a model that was talked into acting has nothing to act with.
#[test]
fn a_range_is_drafted_from_its_diff_and_the_request_holds_no_tool() {
    let scratch = two_commits("commit-message-range");
    let reply = the_model_says(&format!(
        "It renames a function.\n{MARKER}\nRename old to renamed\n\nThe name said less than it should."
    ));
    let (done, received) = draft(
        &scratch,
        |w| trust_of(w, false),
        &Source::Range("HEAD~1..HEAD".to_string()),
        Some("use Conventional Commits"),
        vec![reply],
    );

    let drafted = done.expect("a draft");
    assert_eq!(
        drafted.message,
        "Rename old to renamed\n\nThe name said less than it should."
    );
    let body = received.recv().expect("the processor's request");
    assert!(
        body.contains("-pub fn old() {}") && body.contains("+pub fn renamed() {}"),
        "the diff did not reach the processor: {body}"
    );
    assert!(
        body.contains("use Conventional Commits"),
        "the person's format did not reach the processor: {body}"
    );
    assert!(
        !body.contains("\"tools\""),
        "the draft was requested with tools it could call: {body}"
    );
}

/// Writes `.git/index` holding `files` at stage 0, so `Staged` has an index to read. Each blob is
/// written to the object store as well, since the reader looks the staged side up there.
fn stage(root: &std::path::Path, files: &[(&str, &str)]) {
    use gix_object::Write as _;
    let store = gix_odb::loose::Store::at(root.join(".git/objects"), gix_hash::Kind::Sha1);
    let mut bytes = b"DIRC".to_vec();
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&(files.len() as u32).to_be_bytes());
    let mut sorted = files.to_vec();
    sorted.sort();
    for (path, text) in sorted {
        let id = store
            .write_buf(gix_object::Kind::Blob, text.as_bytes())
            .expect("blob");
        let start = bytes.len();
        for word in [0u32, 0, 0, 0, 0, 0, 0o100644, 0, 0, text.len() as u32] {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        bytes.extend_from_slice(id.as_bytes());
        bytes.extend_from_slice(&(path.len().min(0xfff) as u16).to_be_bytes());
        bytes.extend_from_slice(path.as_bytes());
        let length = bytes.len() - start;
        bytes.resize(start + ((length + 8) & !7), 0);
    }
    let mut hasher = gix_hash::hasher(gix_hash::Kind::Sha1);
    hasher.update(&bytes);
    let checksum = hasher.try_finalize().expect("hashed");
    bytes.extend_from_slice(checksum.as_bytes());
    std::fs::write(root.join(".git/index"), bytes).expect("index");
}

/// `Staged` is the index against `HEAD`, and a later edit to the working tree is not part of it.
/// A source that read the working tree, or compared two commits, would describe a change nobody
/// staged.
#[test]
fn the_staged_source_is_drafted_from_the_index_and_not_the_working_tree() {
    let scratch = two_commits("commit-message-staged");
    stage(&scratch.path, &[("src/lib.rs", "pub fn staged() {}\n")]);
    std::fs::create_dir_all(scratch.path.join("src")).expect("directory");
    std::fs::write(
        scratch.path.join("src/lib.rs"),
        "pub fn edited_later() {}\n",
    )
    .expect("edit");
    let reply = the_model_says(&format!("One function.\n{MARKER}\nStage a rename"));
    let (done, received) = draft(
        &scratch,
        |w| trust_of(w, false),
        &Source::Staged,
        None,
        vec![reply],
    );

    assert_eq!(done.expect("a draft").message, "Stage a rename");
    let body = received.recv().expect("the processor's request");
    assert!(
        body.contains("-pub fn renamed() {}") && body.contains("+pub fn staged() {}"),
        "the staged change did not reach the processor: {body}"
    );
    assert!(
        !body.contains("edited_later"),
        "a working-tree edit that was not staged reached the processor: {body}"
    );
}

/// Nothing to describe is said and nothing is sent: a request for a message about no change would
/// be answered with an invented one.
#[test]
fn an_index_that_matches_head_sends_nothing() {
    let scratch = two_commits("commit-message-nothing-staged");
    stage(&scratch.path, &[("src/lib.rs", "pub fn renamed() {}\n")]);
    let (done, received) = draft(
        &scratch,
        |w| trust_of(w, false),
        &Source::Staged,
        None,
        vec![the_model_says("unused")],
    );

    assert!(
        matches!(done.as_ref().err(), Some(DraftError::Nothing)),
        "{:?}",
        done.as_ref().err()
    );
    assert!(
        received.try_recv().is_err(),
        "a request went out for a diff that shows nothing"
    );
}

/// A repository the map does not trust is not opened, so none of its history is read or sent
/// (GIT-2). Asked of this path alone because the draft is the one read that happens with nobody
/// at the prompt to say yes.
#[test]
fn a_repository_the_map_does_not_trust_is_not_read_or_sent() {
    let scratch = two_commits("commit-message-untrusted");
    let (done, received) = draft(
        &scratch,
        |w| {
            let mut trust = TrustStore::new(w.root());
            trust.distrust("");
            trust
        },
        &Source::Range("HEAD~1..HEAD".to_string()),
        None,
        vec![the_model_says("unused")],
    );

    assert!(
        matches!(done, Err(DraftError::Read(_))),
        "{:?}",
        done.as_ref().err()
    );
    assert!(
        received.try_recv().is_err(),
        "an untrusted repository was sent"
    );
}

/// A diff holding a credential is never sent, since the person is not there to agree to it as they
/// are when the planner reads the same file.
#[test]
fn a_diff_holding_a_credential_is_not_sent() {
    let scratch = Scratch::new("commit-message-credential");
    let key = format!("AKIA{}", "QWERTYUIOPASDFGH");
    repository::commit_files(&scratch.path, &[("notes.txt", "one\n")], "first");
    repository::commit_files(
        &scratch.path,
        &[("notes.txt", &format!("one\nkey = {key}\n"))],
        "second",
    );
    let (done, received) = draft(
        &scratch,
        |w| trust_of(w, false),
        &Source::Range("HEAD~1..HEAD".to_string()),
        None,
        vec![the_model_says("unused")],
    );

    assert!(
        matches!(done, Err(DraftError::Credential)),
        "{:?}",
        done.as_ref().err()
    );
    assert!(
        received.try_recv().is_err(),
        "a credential was sent to a model"
    );
}

/// A diff of a file nobody vouched for is scanned too: it reaches the processor, where the planner's
/// read would have kept it back, so a key in it stops the draft the same way.
#[test]
fn an_untrusted_diff_holding_a_credential_is_not_sent() {
    let scratch = Scratch::new("commit-message-untrusted-credential");
    let key = format!("AKIA{}", "QWERTYUIOPASDFGH");
    repository::commit_files(&scratch.path, &[("notes.txt", "one\n")], "first");
    repository::commit_files(
        &scratch.path,
        &[("notes.txt", &format!("one\nkey = {key}\n"))],
        "second",
    );
    let (done, received) = draft(
        &scratch,
        |w| {
            let mut trust = trust_of(w, false);
            trust.distrust("notes.txt");
            trust
        },
        &Source::Range("HEAD~1..HEAD".to_string()),
        None,
        vec![the_model_says("unused")],
    );

    assert!(
        matches!(done, Err(DraftError::Credential)),
        "{:?}",
        done.as_ref().err()
    );
    assert!(
        received.try_recv().is_err(),
        "a credential was sent to a model"
    );
}

/// `Source::range` reads a range the way `read_git` reads a diff's revision, so a name git could
/// take as an option, a second operator or a missing side is no range.
#[test]
fn a_range_is_two_written_out_revisions() {
    for good in ["main..HEAD", "v1.0...HEAD~2", "a..b"] {
        assert_eq!(
            Source::range(good),
            Some(Source::Range(good.to_string())),
            "{good:?}"
        );
    }
    for bad in [
        "", "main", "..HEAD", "main..", "...HEAD", "-x..HEAD", "main..-x", "a..b..c", "a b",
    ] {
        assert_eq!(Source::range(bad), None, "{bad:?}");
    }
}

/// An answer that never marks where the message begins gives no draft, however much it said: the
/// driver does not pick a message out of prose it may not read.
#[test]
fn an_answer_with_no_marker_is_no_draft() {
    let scratch = two_commits("commit-message-no-marker");
    let (done, _received) = draft(
        &scratch,
        |w| trust_of(w, false),
        &Source::Range("HEAD~1..HEAD".to_string()),
        None,
        vec![the_model_says("Rename old to renamed")],
    );

    assert!(
        matches!(done, Err(DraftError::NoMessage)),
        "{:?}",
        done.as_ref().err()
    );
}
