//! CRED-13 through the terminal front end. Bypassing answers a write that would create a credential
//! itself; accepting edits puts it to the person, whose `a` settles it for the one file the write
//! landed in until the session moves or begins again, and never for that name anywhere else.
use super::*;
use crate::remote_confirm::{Interjections, RemoteConfirmer, Reply, ToMain};
use bravebot_agent::PermissionMode;
use bravebot_agent::confirm::{WriteDecision, WriteRequest};
use serde_json::json;

use super::undo_tests::endpoint;

/// Forty hex characters and nothing else: a value the scan infers is a secret, which is the one a
/// person is asked about rather than one refused outright.
const GENERATED_SECRET: &str = "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7";

/// Another of the same shape, so a second write to the file puts a secret there that was not
/// there before rather than carrying the first one, which is reported and never asked about.
const ROTATED_SECRET: &str = "9e2d4b6f8a0c1e3d5f7b9a2c4e6d8f0a1b3c5d7e";

/// What a session opened in `root` holds, with `state` standing in for `~/.bravebot` and the root
/// vouched for.
fn opening_answers(
    session: &mut Session,
    state: &std::path::Path,
    root: &std::path::Path,
) -> Answers {
    let sources = RuleSources {
        state: Some(state.to_path_buf()),
        named: None,
        started: root.to_path_buf(),
    };
    let settings = sources.settings(root);
    let permissions = rules_from(
        session,
        &settings,
        sources.grants(root).as_ref(),
        Whence::Asked,
        "the-first-session",
        |_, _| Some(true),
    )
    .expect("nobody left");
    let mut trust = TrustStore::new(root);
    trust.trust(".");
    Answers::opening(
        trust,
        TrustedPrograms::new(),
        Rules {
            sources,
            permissions,
        },
    )
}

/// A session in `mode`, as the terminal starts one.
fn a_session(mode: PermissionMode) -> Session {
    match mode {
        PermissionMode::Bypass => Session::new("none").allowing_bypass(),
        PermissionMode::AcceptEdits => {
            let mut session = Session::new("none");
            session.cycle_permission_mode();
            session
        }
        _ => panic!("no test here starts a session in {mode:?}"),
    }
}

/// One turn writing `secret` to `config/master.key`, sent the way the terminal sends one: the mode
/// read off the session, what the session holds going in, every question crossing to this thread
/// as it crosses to the one that owns the screen, and what the turn agreed to carried back into the
/// session afterwards. `answer` is the key the person presses. Returns every write they were asked
/// about.
fn a_turn_the_session_sends(
    session: &Session,
    workspace: &Workspace,
    answers: &mut Answers,
    secret: &str,
    answer: WriteDecision,
) -> Vec<WriteRequest> {
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool(
                "write_file",
                json!({"path": "config/master.key", "contents": format!("{secret}\n")}),
            ),
            endpoint::answer(),
        ],
        None,
    );
    let mode = session.permission_mode();
    let task = Task::new("create the master key")
        .with_permissions(answers.rules.permissions.clone())
        .with_permission_mode(mode)
        .already_exposed(answers.exposed.clone());
    let (to_main, from_worker) = mpsc::channel::<ToMain>();
    let (answer_tx, answer_rx) = mpsc::channel::<Reply>();
    let worker_workspace = workspace.clone();
    let worker_trust = answers.trust.clone();
    let worker = thread::spawn(move || {
        let mut asking = RemoteConfirmer::new(to_main, answer_rx, Interjections::new());
        let mut confirmer = bravebot_agent::Confining::new(&mut asking, mode, false);
        let outcome = turn::resume(
            &config,
            &Egress::new(),
            &worker_workspace,
            &task,
            &mut Conversation::new(),
            &mut confirmer,
            &mut bravebot_agent::IgnoreReports,
            &mut Trail::new(),
            worker_trust,
            TrustedPrograms::new(),
            None,
            &Cancel::new(),
        )
        .outcome
        .expect("the turn runs");
        (outcome.trust, outcome.exposed)
    });
    let mut asked = Vec::new();
    for message in from_worker {
        match message {
            ToMain::Write(request) => {
                asked.push(request);
                answer_tx.send(Reply::Write(answer)).expect("answered");
            }
            ToMain::Run(_)
            | ToMain::ReadOutput(_)
            | ToMain::Vet(_)
            | ToMain::Fetch(_)
            | ToMain::Vouch(_)
            | ToMain::Exposure(_)
            | ToMain::Server(_)
            | ToMain::Manifest(_)
            | ToMain::ToolList(_)
            | ToMain::McpCall(_)
            | ToMain::Move(_)
            | ToMain::Ask(_) => {
                panic!("the turn asked something other than its write: {message:?}")
            }
            _ => {}
        }
    }
    (answers.trust, answers.exposed) = worker.join().expect("the turn finished");
    server.join().expect("the model finished");
    asked
}

/// Whether the person was asked about a credential at `config/master.key`, and only that, with
/// `a` on offer.
fn asked_about_the_key(asked: &[WriteRequest]) -> bool {
    matches!(
        asked,
        [request] if request.path == "config/master.key"
            && !request.credentials.is_empty()
            && request.may_always
    )
}

fn the_key_in(checkout: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(checkout.join("config/master.key")).ok()
}

/// A checkout under `name` with somewhere for the key to go.
fn a_checkout(name: &str) -> std::path::PathBuf {
    let root = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("scratch");
    root
}

/// The flag is the person's answer to every question it covers, and this is one of them: nothing is
/// put to anybody, and the key is written.
#[test]
fn bypassing_writes_a_credential_without_asking() {
    let root = a_checkout("bravebot-credential-bypass");
    let mut session = a_session(PermissionMode::Bypass);
    let workspace = Workspace::new(&root).expect("workspace");
    let mut answers = opening_answers(&mut session, &root.join("state"), workspace.root());

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        GENERATED_SECRET,
        WriteDecision::reject(),
    );
    assert!(
        asked.is_empty(),
        "bypassing put the key to the person: {asked:?}"
    );
    assert_eq!(the_key_in(&root), Some(format!("{GENERATED_SECRET}\n")));

    std::fs::remove_dir_all(&root).ok();
}

/// `a` settles the question for the file it was given about, so a later secret in the same file
/// is written unasked. The control for the two tests after it: without it they would pass as well
/// for an `a` that settled nothing.
#[test]
fn always_for_a_credential_covers_a_later_write_to_the_same_file() {
    let root = a_checkout("bravebot-credential-always");
    let mut session = a_session(PermissionMode::AcceptEdits);
    let workspace = Workspace::new(&root).expect("workspace");
    let mut answers = opening_answers(&mut session, &root.join("state"), workspace.root());

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        GENERATED_SECRET,
        WriteDecision::approve_always(),
    );
    assert!(
        asked_about_the_key(&asked),
        "the key was not put to the person: {asked:?}"
    );

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        ROTATED_SECRET,
        WriteDecision::reject(),
    );
    assert!(
        asked.is_empty(),
        "`a` did not settle the file it was given about: {asked:?}"
    );
    assert_eq!(the_key_in(&root), Some(format!("{ROTATED_SECRET}\n")));

    std::fs::remove_dir_all(&root).ok();
}

/// `/cd` after an `a`. The person settles a generated key in one checkout, the session moves to a
/// second, and the same relative path is written there. That is another file, and everything the
/// session carries across the move is what the second turn is built from, so it is asked again and
/// refusing it leaves the second checkout without the key.
#[test]
fn always_for_a_credential_does_not_follow_the_session_through_cd() {
    let root = crate::testutil::scratch_dir("bravebot-credential-cd");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b) = (root.join("a"), root.join("b"));
    for checkout in [&a, &b] {
        std::fs::create_dir_all(checkout.join("config")).expect("scratch");
    }
    let mut session = a_session(PermissionMode::AcceptEdits);
    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut answers = opening_answers(&mut session, &root.join("state"), workspace.root());

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        GENERATED_SECRET,
        WriteDecision::approve_always(),
    );
    assert!(
        asked_about_the_key(&asked),
        "the first key was not put to the person: {asked:?}"
    );
    assert_eq!(the_key_in(&a), Some(format!("{GENERATED_SECRET}\n")));

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            |_, _| Some(true),
        ),
        Changed::Moved
    );
    assert_eq!(
        session.permission_mode(),
        PermissionMode::AcceptEdits,
        "the mode did not survive the move, so nothing here asks what it carries"
    );

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        GENERATED_SECRET,
        WriteDecision::reject(),
    );
    assert!(
        asked_about_the_key(&asked),
        "the second checkout's key was settled by the first one's `a`: {asked:?}"
    );
    assert_eq!(
        the_key_in(&b),
        None,
        "a refused key was written in the checkout the session moved to"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// `/clear` after an `a`. The session that gave it is over, so the same file is asked about again
/// in the one that begins, and a no leaves the file as the first write left it.
#[test]
fn always_for_a_credential_does_not_outlive_clear() {
    let root = a_checkout("bravebot-credential-clear");
    let mut session = a_session(PermissionMode::AcceptEdits);
    let workspace = Workspace::new(&root).expect("workspace");
    let mut answers = opening_answers(&mut session, &root.join("state"), workspace.root());

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        GENERATED_SECRET,
        WriteDecision::approve_always(),
    );
    assert!(
        asked_about_the_key(&asked),
        "the key was not put to the person: {asked:?}"
    );

    session.clear();
    answers.forget_what_was_agreed();

    let asked = a_turn_the_session_sends(
        &session,
        &workspace,
        &mut answers,
        ROTATED_SECRET,
        WriteDecision::reject(),
    );
    assert!(
        asked_about_the_key(&asked),
        "an `a` from the cleared session settled the file in the next one: {asked:?}"
    );
    assert_eq!(the_key_in(&root), Some(format!("{GENERATED_SECRET}\n")));

    std::fs::remove_dir_all(&root).ok();
}
