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
        trust,
        egress: bravebot_net::Egress::new(),
        permissions: Default::default(),
        mode: bravebot_agent::PermissionMode::default(),
        attribution: Default::default(),
        output_cap: None,
        deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
        model: None,
        in_force: config.default_model.clone(),
        reads_effort: false,
        effort: None,
        complained: None,
        home: None,
        profile: None,
        conversation: Default::default(),
        programs: TrustedPrograms::new(),
        servers: None,
        mcp: None,
        asked_about: AskedAbout::new(),
        exposed: Default::default(),
        auto_vetting: false,
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
            session.trust.integrity_of("target.txt"),
            Some(bravebot_core::label::Integrity::Untrusted)
        );
    }
}

#[path = "../../agent/test-support/answers.rs"]
mod answers;

/// The real plain caller must adopt grants, advice and exposure answers even without an outcome.
#[test]
fn failed_plain_turn_keeps_exact_approvals_advice_and_exposure() {
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
