//! The session a desktop turn and a manifest run are built in, for the ChatGPT account
//! ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
//!
//! Driven through the workers a window's requests start, `work` and `walk`, with the stop already
//! given so that nothing is asked of any service. The agent sends the identifier to a host that is
//! a constant, so what is read is the session the task was built in, which each worker records as it
//! hands the task over.

use super::test_profile as profile;
use super::*;
use crate::manifest::{Walk, walk};
use crate::running::built_in;

const ACCOUNT_MODEL: &str = "chatgpt/a-model";

/// A configuration whose service is never asked: the stop is given before a request is built.
fn config() -> Config {
    Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9/never-asked".into()),
        _ => None,
    })
    .unwrap()
}

/// A stop that was given before the work began.
fn stopped() -> Cancel {
    let cancel = Cancel::new();
    cancel.cancel();
    cancel
}

/// The identifier the account sees for the session `state` holds.
fn identifier_of(state: &Arc<Mutex<State>>) -> String {
    state.lock().unwrap().account.identifier().to_string()
}

/// One turn of the session `state` holds, in the worker a `turn.send` starts.
fn take_a_turn(state: &Arc<Mutex<State>>, workspace: &Workspace, turn: usize) {
    let (_answers, receiver) = mpsc::channel();
    work(Work {
        emitter: Emitter::new(Box::new(|_| {})),
        session: "account-session".into(),
        project: workspace.root().to_path_buf(),
        state: Arc::clone(state),
        config: config(),
        workspace: workspace.clone(),
        attribution: Default::default(),
        output_cap: None,
        download_cap: None,
        deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
        auto_vetting: false,
        sandbox: bravebot_sandbox::SandboxMode::default(),
        permission_mode: PermissionMode::Ask.into(),
        mcp_requested: Vec::new(),
        watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
        model: Some(ACCOUNT_MODEL.into()),
        addressing: None,
        addressing_standing: false,
        prompt: "say hello".into(),
        composed: None,
        files: Vec::new(),
        dropped: Vec::new(),
        attachments: Vec::new(),
        images: Vec::new(),
        recall: false,
        turn,
        cancel: stopped(),
        pending: Arc::new(Mutex::new(None)),
        answerers: Default::default(),
        answers: receiver,
        finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        interjections: Default::default(),
    });
}

/// One manifest run in the session `state` holds, in the worker a `manifest.run` starts.
fn walk_a_run(state: &Arc<Mutex<State>>, workspace: &Workspace, run: usize) {
    let (_answers, receiver) = mpsc::channel();
    walk(Walk {
        emitter: Emitter::new(Box::new(|_| {})),
        session: "account-session".into(),
        project: workspace.root().to_path_buf(),
        state: Arc::clone(state),
        config: config(),
        attribution: Default::default(),
        output_cap: None,
        deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
        sandbox: bravebot_sandbox::SandboxMode::default(),
        model: Some(ACCOUNT_MODEL.into()),
        permission_mode: PermissionMode::Ask.into(),
        workspace: workspace.clone(),
        task: "tidy the notes".into(),
        run,
        cancel: stopped(),
        pending: Arc::new(Mutex::new(None)),
        answerers: Default::default(),
        answers: receiver,
        finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    });
}

/// A project of the test's own, which goes when the test does, and a trust map that trusts it.
fn project() -> (tempfile::TempDir, Workspace, TrustStore) {
    let directory = tempfile::tempdir().expect("a project directory");
    let workspace = Workspace::new(directory.path()).unwrap();
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    (directory, workspace, trust)
}

/// Every turn of a desktop session is built in the session, so the requests of all of them carry its
/// identifier, and a second session open in the process carries its own. The regressions it rejects
/// are a task built with no session said, which would give each turn an identifier of its own, and
/// one built in a session other than the one the turn is in.
#[test]
fn every_turn_of_a_desktop_session_is_built_in_it_and_another_session_in_its_own() {
    if !profile::in_isolated_profile() {
        return;
    }
    let (_directory, workspace, trust) = project();
    let first = Arc::new(Mutex::new(State::fresh(trust.clone())));
    let second = Arc::new(Mutex::new(State::fresh(trust)));

    take_a_turn(&first, &workspace, 1);
    take_a_turn(&first, &workspace, 2);
    take_a_turn(&second, &workspace, 1);

    assert_ne!(identifier_of(&first), identifier_of(&second));
    assert_eq!(
        built_in::seen(),
        [
            ("turn", identifier_of(&first)),
            ("turn", identifier_of(&first)),
            ("turn", identifier_of(&second)),
        ]
    );
}

/// A manifest run is built in the session it was started from, as that session's turns are, so a
/// run and a turn of one session carry one identifier and the run of another session its own. The
/// regression it rejects is a run built with no session said.
#[test]
fn a_manifest_run_is_built_in_the_session_that_started_it() {
    if !profile::in_isolated_profile() {
        return;
    }
    let (_directory, workspace, trust) = project();
    let first = Arc::new(Mutex::new(State::fresh(trust.clone())));
    let second = Arc::new(Mutex::new(State::fresh(trust)));

    take_a_turn(&first, &workspace, 1);
    walk_a_run(&first, &workspace, 1);
    walk_a_run(&second, &workspace, 1);

    assert_eq!(
        built_in::seen(),
        [
            ("turn", identifier_of(&first)),
            ("manifest", identifier_of(&first)),
            ("manifest", identifier_of(&second)),
        ]
    );
}

/// A session resumed from a record is a session of its own to the account, each time it is
/// resumed: the record holds no identifier, because one in it would tie a record on this machine to
/// the requests the service logged for the account. The regression it rejects is an identifier
/// that comes from the record, which two resumptions of one record would then share.
#[test]
fn a_session_resumed_from_a_record_begins_an_identifier_each_time() {
    if !profile::in_isolated_profile() {
        return;
    }
    let (_directory, workspace, trust) = project();
    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(bravebot_aichat::protocol::Message::user("work"));
    let record: bravebot_session::sessions::Record = serde_json::from_value(serde_json::json!({
        "id": "account-session-resumed", "directory": workspace.root(), "title": "work",
        "started": 1, "updated": 1,
        "conversation": conversation.snapshot()
    }))
    .unwrap();

    let first = State::resumed(workspace.root(), &record, trust.clone())
        .expect("the record is free")
        .account
        .identifier()
        .to_string();
    let second = State::resumed(workspace.root(), &record, trust)
        .expect("the record was let go with the state that held it")
        .account
        .identifier()
        .to_string();

    assert_ne!(first, second);
}
