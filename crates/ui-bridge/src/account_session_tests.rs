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
    take_a_turn_at(state, workspace, turn, None);
}

/// The same, asking for `effort`.
fn take_a_turn_at(
    state: &Arc<Mutex<State>>,
    workspace: &Workspace,
    turn: usize,
    effort: Option<bravebot_session::store::Effort>,
) {
    let (_answers, receiver) = mpsc::channel();
    work(Work {
        emitter: Emitter::new(Box::new(|_| {})),
        session: "account-session".into(),
        project: workspace.root().to_path_buf(),
        state: Arc::clone(state),
        config: config(),
        workspace: workspace.clone(),
        indexer: None,
        retrievers: None,
        attribution: Default::default(),
        effort,
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
    walk_a_run_at(state, workspace, run, None);
}

/// The same, asking for `effort`.
fn walk_a_run_at(
    state: &Arc<Mutex<State>>,
    workspace: &Workspace,
    run: usize,
    effort: Option<bravebot_session::store::Effort>,
) {
    let (_answers, receiver) = mpsc::channel();
    walk(Walk {
        emitter: Emitter::new(Box::new(|_| {})),
        session: "account-session".into(),
        project: workspace.root().to_path_buf(),
        state: Arc::clone(state),
        config: config(),
        attribution: Default::default(),
        effort,
        output_cap: None,
        deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
        sandbox: bravebot_sandbox::SandboxMode::default(),
        model: Some(ACCOUNT_MODEL.into()),
        permission_mode: PermissionMode::Ask.into(),
        mcp_requested: Vec::new(),
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

/// A session opened, one resumed from a record and a fork each start with the fields no record
/// holds empty, and each has an identifier and a scratch directory of its own. The regression it
/// rejects is a constructor that writes one of those fields at a value other than the blank one,
/// or leaves one out, which a state built in three places allows.
#[test]
fn a_fresh_resumed_and_forked_state_start_the_unsaved_fields_empty() {
    if !profile::in_isolated_profile() {
        return;
    }
    let (_directory, workspace, trust) = project();
    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(bravebot_aichat::protocol::Message::user("work"));
    let record: bravebot_session::sessions::Record = serde_json::from_value(serde_json::json!({
        "id": "unsaved-fields-resumed", "directory": workspace.root(), "title": "work",
        "started": 1, "updated": 1,
        "conversation": conversation.snapshot()
    }))
    .unwrap();

    let fresh = State::fresh(trust.clone());
    let resumed =
        State::resumed(workspace.root(), &record, trust.clone()).expect("the record is free");
    let forked = State::forked(
        Handle::begin(workspace.root(), crate::FRONT, crate::agent_build()),
        conversation.snapshot(),
        trust,
        bravebot_core::programs::TrustedPrograms::new(),
        Vec::new(),
        1,
        Default::default(),
        None,
    );

    for (which, state) in [
        ("fresh", &fresh),
        ("resumed", &resumed),
        ("forked", &forked),
    ] {
        assert!(!state.answered_by_mode, "{which}: answered_by_mode");
        assert!(state.kept.is_none(), "{which}: kept");
        assert_eq!(state.runs, 0, "{which}: runs");
        assert!(matches!(state.mcp, Mcp::Unstarted), "{which}: mcp");
        assert!(
            matches!(
                state.scratch,
                crate::running::Scratch::Held(_) | crate::running::Scratch::Unavailable(_)
            ),
            "{which}: scratch"
        );
        assert_eq!(
            state.question_ids.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "{which}: question_ids"
        );
        assert_eq!(
            *state.question_answers.lock().unwrap(),
            bravebot_core::ask::AnswerMemo::default(),
            "{which}: question_answers"
        );
        assert!(state.moments.is_none(), "{which}: moments");
        assert!(
            state.roster.lock().unwrap().agents().is_empty(),
            "{which}: roster"
        );
    }
    let identifiers = [
        fresh.account.identifier(),
        resumed.account.identifier(),
        forked.account.identifier(),
    ];
    assert_ne!(identifiers[0], identifiers[1]);
    assert_ne!(identifiers[0], identifiers[2]);
    assert_ne!(identifiers[1], identifiers[2]);
}

/// The level a window's turn and plan run resolve is the level the task they hand over asks for, so
/// a level picked in the window's model picker reaches the request. The regression it rejects is a
/// worker that resolves a level and builds the task without it, which is what every desktop turn did
/// before the window offered one.
#[test]
fn a_desktop_turn_and_plan_run_ask_for_the_level_they_were_given() {
    use bravebot_session::store::Effort;
    if !profile::in_isolated_profile() {
        return;
    }
    let (_directory, workspace, trust) = project();
    let state = Arc::new(Mutex::new(State::fresh(trust)));
    take_a_turn_at(&state, &workspace, 1, Some(Effort::Xhigh));
    walk_a_run_at(&state, &workspace, 1, Some(Effort::Low));
    take_a_turn_at(&state, &workspace, 2, None);
    assert_eq!(
        built_in::levels(),
        [
            ("turn", Some("xhigh")),
            ("manifest", Some("low")),
            ("turn", None)
        ]
    );
}

/// Send `method` to `bridge` and answer what it did, trying again while the session's earlier turn
/// is still ending, since a worker notices that its predecessor finished only when asked.
fn dispatch_when_idle(bridge: &mut Bridge, method: &str, params: Value) -> Value {
    let line = json!({"id": 1, "method": method, "params": params}).to_string();
    let request = Request::parse(&line).expect("a request");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match bridge.dispatch(&request) {
            Err(failure)
                if failure.code == crate::protocol::ErrorCode::TurnInFlight
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            other => {
                return other.unwrap_or_else(|failure| panic!("{method}: {}", failure.message));
            }
        }
    }
}

/// Wait until the workers have built `count` tasks and answer the levels they were built asking for.
fn levels_after(count: usize) -> Vec<(&'static str, Option<&'static str>)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while built_in::levels().len() < count && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    built_in::levels()
}

/// A turn and a plan run started through the bridge ask for the level `effort.set` recorded, and
/// for the level of the project's own settings where those outrank it (BACKEND-43). The regression
/// it rejects is a handler that stops reading the record, or reads it without the project's
/// settings, so that the picker shows one level and the request carries another. The workers are
/// tested with a level given to them above; this goes through the handlers that decide it.
#[test]
fn a_desktop_turn_and_plan_run_started_through_the_bridge_ask_for_the_recorded_level() {
    if !profile::in_isolated_profile() {
        return;
    }
    // A service that needs no credential, so that a request can be built; it is never answered.
    let home = bravebot_agent::home::directory().expect("the profile's state directory");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("settings.json"),
        r#"{"provider":{"local":{"options":{"baseURL":"http://127.0.0.1:9/v1"},"models":{"m":{}}}},"model":"local/m"}"#,
    )
    .unwrap();
    let (directory, workspace, trust) = project();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(
        workspace.root().to_path_buf(),
        State::fresh(trust),
        true,
        None,
        false,
    ));
    let mut call = |method: &str, params: Value| dispatch_when_idle(&mut bridge, method, params);

    call("effort.set", json!({"level": "high"}));
    call(
        "turn.send",
        crate::message_ids::with_fresh(json!({"session": handle, "prompt": "say hello"})),
    );
    assert_eq!(levels_after(1), [("turn", Some("high"))]);
    call(
        "manifest.run",
        json!({"session": handle, "task": "tidy the notes"}),
    );
    assert_eq!(
        levels_after(2),
        [("turn", Some("high")), ("manifest", Some("high"))]
    );

    // The project's own file outranks the pick, for the turn and the run alike.
    std::fs::create_dir_all(directory.path().join(".bravebot")).unwrap();
    std::fs::write(
        directory.path().join(".bravebot/settings.json"),
        r#"{"effort": "max"}"#,
    )
    .unwrap();
    call(
        "turn.send",
        crate::message_ids::with_fresh(json!({"session": handle, "prompt": "say hello"})),
    );
    call(
        "manifest.run",
        json!({"session": handle, "task": "tidy the notes"}),
    );
    assert_eq!(
        levels_after(4)[2..],
        [("turn", Some("max")), ("manifest", Some("max"))]
    );
}

/// The ChatGPT account is sent one level for a whole session (BACKEND-22), so a session opened in a
/// project whose own settings name a level begins with that level, and not with the pick the
/// picker reports as outranked. The regression it rejects is a session that begins with the record
/// alone, which holds the account at `low` while every task asks for `max`.
#[test]
fn a_session_opens_with_the_level_its_project_settles_on() {
    use bravebot_session::store::Effort;
    if !profile::in_isolated_profile() {
        return;
    }
    let outranking = tempfile::tempdir().expect("a project directory");
    std::fs::create_dir_all(outranking.path().join(".bravebot")).unwrap();
    std::fs::write(
        outranking.path().join(".bravebot/settings.json"),
        r#"{"effort": "max"}"#,
    )
    .unwrap();
    let plain = tempfile::tempdir().expect("a project directory");
    let mut bridge = Bridge::new(Box::new(|_| {}));
    bridge
        .dispatch(
            &Request::parse(r#"{"id":1,"method":"effort.set","params":{"level":"low"}}"#).unwrap(),
        )
        .unwrap();
    let mut open = |directory: &std::path::Path| {
        let line = json!({"id": 1, "method": "session.new", "params": {"directory": directory}});
        let made = bridge
            .dispatch(&Request::parse(&line.to_string()).unwrap())
            .expect("a session opens");
        made["session"].as_str().unwrap().to_string()
    };
    let outranked = open(outranking.path());
    let free = open(plain.path());
    let level = |handle: &str| bridge.open[handle].state.lock().unwrap().account.effort();
    assert_eq!(level(&outranked), Some(Effort::Max));
    assert_eq!(level(&free), Some(Effort::Low));
}
