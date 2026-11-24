use super::*;
#[path = "plain_test_endpoint.rs"]
mod endpoint;

// The CLI integration tests use owned directories under target/test-scratch too.
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn running<'a>(config: &'a Config, workspace: &'a Workspace, trust: TrustStore) -> Running<'a> {
    Running {
        config,
        workspace,
        answers: Answers::opening(trust, TrustedPrograms::new(), ()),
        egress: bravebot_net::Egress::new(),
        permissions: Default::default(),
        mode: bravebot_agent::PermissionMode::default(),
        attribution: Default::default(),
        prompts: Default::default(),
        output_cap: None,
        download_cap: None,
        deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
        agent: None,
        model: None,
        in_force: config.default_model.clone(),
        reads_effort: false,
        effort: None,
        account: bravebot_agent::chatgpt::AccountSession::begin_with(None),
        complained: None,
        home: None,
        profile: None,
        conversation: Default::default(),
        mcp: None,
        beside: None,
        inbox: None,
        courier: None,
        auto_vetting: false,
        kept: None,
        injected_facts: Default::default(),
    }
}

/// A plain session continues after a returned error and must not reuse its pre-write grant.
#[test]
fn failed_plain_turn_excludes_replacement_from_the_next_request() {
    for success in [false, true] {
        const SENTINEL: &str = "This is untrusted replacement text number 47291.";
        let directory = Scratch::new("plain-retention-files");
        let workspace = Workspace::new(directory.path()).unwrap();
        std::fs::write(directory.path().join("source.txt"), SENTINEL).unwrap();
        std::fs::write(directory.path().join("target.txt"), "original").unwrap();
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        trust.distrust("source.txt");
        let (config, requests, server) = endpoint::endpoint(vec![
            endpoint::tool("read_file", r#"{"path":"source.txt"}"#),
            endpoint::tool(
                "write_file",
                r#"{"path":"target.txt","contents_ref":"ref:1"}"#,
            ),
            if success {
                endpoint::answer()
            } else {
                "fail".into()
            },
            endpoint::tool("read_file", r#"{"path":"target.txt"}"#),
            endpoint::answer(),
        ]);
        let mut session = running(&config, &workspace, trust);
        let said = session.take("copy", &mut bravebot_agent::confirm::ApproveWrites);
        assert_eq!(said.failure.is_none(), success);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("target.txt")).unwrap(),
            SENTINEL
        );
        let next = session.take("read", &mut bravebot_agent::confirm::ApproveWrites);
        assert!(next.failure.is_none());
        server.join().unwrap();
        let sent: Vec<_> = requests.try_iter().collect();
        assert_eq!(sent.len(), 5);
        assert!(
            !sent[4].contains(SENTINEL),
            "replacement reached the next planner request"
        );
        assert_eq!(
            session.answers.trust.integrity_of("target.txt"),
            Some(bravebot_core::label::Integrity::Untrusted)
        );
    }
}

/// CLI-15: a session in lines that asks releases a file nobody vouched for to the planner after a
/// vet only where `--vet` is given and the check finds nothing. The planner reads `notes.txt`,
/// asks to vet the reference, and the request after the vet either holds the file's words or does
/// not. With the flag a check that finds nothing answers the release question and one that objects
/// leaves the words out. Without the flag the question goes to the confirmer, which here declines,
/// so the words stay out whatever the check said. The regression it rejects is a task built with
/// `.with_auto_vetting(false)`: a check that finds nothing would not release the words. The
/// session under bypass is the next two tests.
#[test]
fn a_session_in_lines_that_asks_releases_a_vetted_file_only_with_the_vet_flag_and_a_safe_check() {
    const WORDS: &str = "NOTES-WORDS-47291";
    // (auto-vetting, what the check says, whether the planner is shown the file afterwards)
    let mode = bravebot_agent::PermissionMode::Ask;
    for (vetting, word, released) in [
        (true, "safe", true),
        (false, "safe", false),
        (true, "unsafe", false),
    ] {
        let directory = Scratch::new(&format!("plain-vet-{mode:?}-{vetting}-{word}"));
        let workspace = Workspace::new(directory.path()).unwrap();
        std::fs::write(directory.path().join("notes.txt"), WORDS).unwrap();
        let trust = TrustStore::new(workspace.root());
        let (config, requests, server) = endpoint::endpoint_answering_checks_with(
            endpoint::verdict(word),
            vec![
                endpoint::tool("read_file", r#"{"path":"notes.txt"}"#),
                endpoint::tool("vet_content", r#"{"ref":"ref:1","expects":"some notes"}"#),
                endpoint::answer(),
            ],
        );
        let mut session = running(&config, &workspace, trust);
        session.mode = mode;
        session.auto_vetting = vetting;
        let said = session.take(
            "look at the notes",
            &mut bravebot_agent::confirm::Unattended,
        );
        assert!(said.failure.is_none(), "{:?}", said.failure);
        server.join().unwrap();

        let sent: Vec<_> = requests.try_iter().collect();
        let case = format!("vetting {vetting}, check {word}");
        assert_eq!(sent.len(), 3, "{case}");
        assert!(
            !sent[1].contains(WORDS),
            "{case}: the planner was shown the file before any vet"
        );
        assert_eq!(sent[2].contains(WORDS), released, "{case}: {}", sent[2]);
    }
}

/// CLI-15: under bypass the flag is what lets a check refuse. The planner runs `cat notes.txt` and
/// asks to read the output back. With the flag and a check that objects the output is kept back,
/// with the flag and a check that finds nothing it is returned, and without the flag the prompt is
/// answered yes unshown, so the output is returned whatever a check would have said. The
/// regression it rejects is a task built with auto-vetting off, which makes no check and so
/// returns the words under a check that objects.
#[test]
fn a_session_in_lines_given_the_vet_flag_under_bypass_is_screened_by_the_check() {
    const WORDS: &str = "OUTPUT-WORDS-83716";
    // A `run` is refused where the platform has a base and no way to apply it.
    if bravebot_sandbox::base::Prelude::current().is_some()
        && !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    for (vetting, word, released) in [
        (true, "unsafe", false),
        (true, "safe", true),
        (false, "unsafe", true),
    ] {
        let directory = Scratch::new(&format!("plain-vet-bypass-{vetting}-{word}"));
        let workspace = Workspace::new(directory.path()).unwrap();
        std::fs::write(directory.path().join("notes.txt"), WORDS).unwrap();
        let (config, requests, server) = endpoint::endpoint_answering_checks_with(
            endpoint::verdict(word),
            vec![
                endpoint::tool("run", r#"{"command":"cat notes.txt","read":true}"#),
                endpoint::answer(),
            ],
        );
        let mut session = running(&config, &workspace, TrustStore::new(workspace.root()));
        session.mode = bravebot_agent::PermissionMode::Bypass;
        session.auto_vetting = vetting;
        let said = session.take("show the notes", &mut bravebot_agent::confirm::Unattended);
        assert!(said.failure.is_none(), "{:?}", said.failure);
        server.join().unwrap();

        let sent: Vec<_> = requests.try_iter().collect();
        let case = format!("vetting {vetting}, check {word}");
        assert_eq!(sent.len(), 2, "{case}");
        assert_eq!(sent[1].contains(WORDS), released, "{case}: {}", sent[1]);
    }
}

/// CLI-15: under bypass the confirmer a session in lines builds screens with the flag too. The
/// planner runs `cat notes.txt` without `read`, so the output is a reference, and then calls
/// `read_output` on it. With the flag and a check that objects, the question that call raises is
/// answered by the confirmer, which refuses it, so the output is kept back. With the flag and a
/// check that finds nothing the output is returned, and without the flag the question is answered
/// yes whatever the check said. The task's flag decides that the check is made and the confirmer's
/// flag decides what the check's word answers, so the regression it rejects is a confirmer built
/// with screening off (`Confining::new(.., false)`) beside a task built with it on: the check is
/// made and objects, and the confirmer approves the release anyway.
#[test]
fn a_session_in_lines_given_the_vet_flag_keeps_back_a_read_output_an_objecting_check_flagged_under_bypass()
 {
    const WORDS: &str = "OUTPUT-WORDS-52917";
    // A `run` is refused where the platform has a base and no way to apply it.
    if bravebot_sandbox::base::Prelude::current().is_some()
        && !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    for (vetting, word, released) in [
        (true, "unsafe", false),
        (true, "safe", true),
        (false, "unsafe", true),
    ] {
        let directory = Scratch::new(&format!("plain-vet-bypass-read-{vetting}-{word}"));
        let workspace = Workspace::new(directory.path()).unwrap();
        std::fs::write(directory.path().join("notes.txt"), WORDS).unwrap();
        let (config, requests, server) = endpoint::endpoint_answering_checks_with(
            endpoint::verdict(word),
            vec![
                endpoint::tool("run", r#"{"command":"cat notes.txt"}"#),
                endpoint::tool("read_output", r#"{"ref":"ref:1"}"#),
                endpoint::answer(),
            ],
        );
        let mut session = running(&config, &workspace, TrustStore::new(workspace.root()));
        session.mode = bravebot_agent::PermissionMode::Bypass;
        session.auto_vetting = vetting;
        let said = session.take("show the notes", &mut bravebot_agent::confirm::Unattended);
        assert!(said.failure.is_none(), "{:?}", said.failure);
        server.join().unwrap();

        let sent: Vec<_> = requests.try_iter().collect();
        let case = format!("vetting {vetting}, check {word}");
        assert_eq!(sent.len(), 3, "{case}");
        assert!(
            !sent[1].contains(WORDS),
            "{case}: the planner was shown the output before it asked to read it"
        );
        assert_eq!(sent[2].contains(WORDS), released, "{case}: {}", sent[2]);
    }
}

/// CHATGPT-10: every prompt of a plain session, which is what a background session runs, is built
/// in the session, so its requests to the ChatGPT account carry one identifier, and another session
/// carries its own. The regression it rejects is a task built with an identifier for the prompt.
#[test]
fn every_prompt_of_a_plain_session_is_built_in_it_and_another_session_has_its_own() {
    let directory = Scratch::new("plain-account-session");
    let workspace = Workspace::new(directory.path()).unwrap();
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9/never-asked".into()),
        _ => None,
    })
    .unwrap();
    let session = running(&config, &workspace, TrustStore::new(workspace.root()));
    let another = running(&config, &workspace, TrustStore::new(workspace.root()));

    let first = session.task("one");
    let second = session.task("two");

    assert_eq!(
        first.account_session().identifier(),
        second.account_session().identifier()
    );
    assert_eq!(
        first.account_session().identifier(),
        session.account.identifier()
    );
    assert_ne!(
        another.task("three").account_session().identifier(),
        first.account_session().identifier()
    );
}

#[path = "../../agent/test-support/answers.rs"]
mod answers;

/// The real plain caller must adopt grants, advice and exposure answers even without an outcome.
#[test]
fn failed_plain_turn_keeps_exact_approvals_advice_and_exposure() {
    // A `run` is refused where the platform has a base and no way to apply it.
    if bravebot_sandbox::base::Prelude::current().is_some()
        && !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    let directory = Scratch::new("plain-retention-decisions");
    let home = Scratch::new("plain-retention-home");
    let workspace = Workspace::new(directory.path()).unwrap();
    std::fs::create_dir(directory.path().join("sub")).unwrap();
    std::fs::write(
        directory.path().join(".env"),
        "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n",
    )
    .unwrap();
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    let (config, requests, server) = endpoint::endpoint(vec![
        endpoint::tool("read_file", r#"{"path":".env"}"#),
        endpoint::tool("run", r#"{"command":"touch approved.txt"}"#),
        "fail".into(),
        endpoint::tool("run", r#"{"command":"touch changed.txt"}"#),
        endpoint::tool(
            "run",
            r#"{"command":"touch approved.txt","directory":"sub"}"#,
        ),
        endpoint::tool("run", r#"{"command":"touch approved.txt","directory":"."}"#),
        endpoint::tool("read_file", r#"{"path":".env"}"#),
        endpoint::answer(),
    ]);
    let mut session = running(&config, &workspace, trust);
    session.home = Some(home.path().to_path_buf());
    let mut first = answers::Answers::new(bravebot_agent::RunDecision::approve_always());
    assert!(session.take("read and run", &mut first).failure.is_some());
    assert_eq!(first.exposures, 1);
    assert_eq!(first.runs.len(), 1);
    std::fs::remove_file(directory.path().join("approved.txt")).unwrap();
    let mut later = answers::Answers::new(bravebot_agent::RunDecision::reject());
    assert!(session.take("continue", &mut later).failure.is_none());
    server.join().unwrap();
    assert_eq!(later.exposures, 0);
    assert_eq!(later.runs.len(), 2);
    assert_eq!(
        later.runs[0].pattern,
        Some(home.path().join("settings.json"))
    );
    assert_eq!(later.runs[1].plan.directory, workspace.root().join("sub"));
    assert!(directory.path().join("approved.txt").exists());
    assert!(!directory.path().join("changed.txt").exists());
    assert!(!directory.path().join("sub/approved.txt").exists());
    assert_eq!(requests.try_iter().count(), 8);
}

/// MSG-6: a message waiting in the inbox of a hosted session is in front of the planner on the next
/// turn's first request, once, and is not a prompt: the turn that took it was started by the line
/// the person typed.
#[test]
fn a_message_waiting_for_a_hosted_session_reaches_the_planner_of_its_next_turn_once() {
    use bravebot_session::jobs::{Job, Mode, Roster};
    const SENDER: &str = "22222222-2222-4222-8222-222222222222";

    let directory = Scratch::new("plain-session-message");
    let roster = Scratch::new("plain-session-message-roster");
    let workspace = Workspace::new(directory.path()).unwrap();
    let roster = Roster::at(roster.path().to_path_buf());
    roster
        .publish(&Job::starting(
            SENDER.to_string(),
            std::path::Path::new("/elsewhere"),
            "fix the build",
            Mode::Ask,
        ))
        .unwrap();
    let hosted = Job::starting(
        "11111111-1111-4111-8111-111111111111".to_string(),
        directory.path(),
        "wait",
        Mode::Ask,
    );
    let shared = crate::host::Shared::new(roster, hosted, None);
    assert_eq!(
        shared.message(SENDER, "(T,pub)", "ADVICE-FROM-ELSEWHERE".to_string()),
        crate::host::Messaged::Queued
    );

    let (config, requests, server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()]);
    let mut session = running(&config, &workspace, TrustStore::new(workspace.root()));
    session.inbox = Some(shared);
    let first = session.take("what is new?", &mut bravebot_agent::confirm::Unattended);
    assert!(first.failure.is_none(), "{:?}", first.failure);
    let second = session.take("and now?", &mut bravebot_agent::confirm::Unattended);
    assert!(second.failure.is_none(), "{:?}", second.failure);
    server.join().unwrap();

    let sent: Vec<_> = requests.try_iter().collect();
    assert_eq!(sent.len(), 2);
    assert!(
        sent[0].contains("> ADVICE-FROM-ELSEWHERE")
            && sent[0].contains("Sender: session 22222222, named \\\"fix the build\\\"."),
        "{}",
        sent[0]
    );
    // The conversation keeps the first turn's, so what the second sends still holds it once.
    assert_eq!(
        sent[1].matches("> ADVICE-FROM-ELSEWHERE").count(),
        1,
        "{}",
        sent[1]
    );
}

/// A courier that finds one session and counts the lookups.
#[derive(Debug, Default)]
struct FindsBuild(std::sync::atomic::AtomicUsize);

impl bravebot_agent::messages::Courier for FindsBuild {
    fn own_id(&self) -> &str {
        "11111111-1111-4111-8111-111111111111"
    }

    fn find(&self, _name: &str) -> bravebot_agent::messages::Found {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        bravebot_agent::messages::Found::Session(bravebot_agent::messages::Target {
            id: "22222222-2222-4222-8222-222222222222".to_string(),
            name: "build".to_string(),
            state: "idle".to_string(),
            directory: "/work".to_string(),
        })
    }

    fn deliver(
        &self,
        _to: &bravebot_agent::messages::Target,
        _label: &str,
        _text: &str,
    ) -> bravebot_agent::messages::Delivery {
        bravebot_agent::messages::Delivery::Queued
    }
}

/// MSG-1, MSG-12: the planner of a hosted session is offered `message_session` and its call reaches
/// the session's courier, and a session without one is offered nothing. Nobody is at the end of this
/// session's input, so the person's question is declined and the planner is told only that the
/// message was not sent.
#[test]
fn the_planner_of_a_hosted_session_is_offered_message_session_and_its_call_reaches_the_courier() {
    for hosted in [true, false] {
        let directory = Scratch::new(if hosted {
            "plain-session-sends"
        } else {
            "plain-session-cannot-send"
        });
        let workspace = Workspace::new(directory.path()).unwrap();
        let (config, requests, server) = endpoint::endpoint(vec![
            endpoint::tool("message_session", r#"{"target":"build","text":"hi"}"#),
            endpoint::answer(),
        ]);
        let courier = std::sync::Arc::new(FindsBuild::default());
        let mut session = running(&config, &workspace, TrustStore::new(workspace.root()));
        session.courier = hosted.then(|| {
            std::sync::Arc::clone(&courier) as std::sync::Arc<dyn bravebot_agent::messages::Courier>
        });
        let turn = session.take("tell build", &mut bravebot_agent::confirm::Unattended);
        assert!(turn.failure.is_none(), "{:?}", turn.failure);
        server.join().unwrap();

        let sent: Vec<_> = requests.try_iter().collect();
        assert_eq!(sent.len(), 2, "hosted {hosted}");
        assert_eq!(
            sent[0].contains(r#""name":"message_session""#),
            hosted,
            "hosted {hosted}: {}",
            sent[0]
        );
        assert_eq!(
            courier.0.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(hosted),
            "hosted {hosted}"
        );
        assert_eq!(
            sent[1].contains("the message was not sent"),
            hosted,
            "hosted {hosted}: {}",
            sent[1]
        );
        assert_eq!(
            sent[1].contains("error: no such tool 'message_session'"),
            !hosted,
            "hosted {hosted}: {}",
            sent[1]
        );
    }
}
