//! Trust, the directories a session may reach and requests for paths.

use super::*;

/// PATHREQ-4, SANDBOX-28: `/status` lists the paths the person let programs reach, read from the
/// workspace's grant when the report is built and worded as `/reach paths` words them. A session
/// that was granted none draws no heading. The fault it rejects is a report built from no
/// grant, which the hand-built facts of the status tests cannot notice.
#[test]
fn the_status_of_a_session_lists_the_paths_programs_were_let_reach() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-status-requested-paths");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let config = a_config_offering_three_tiers();
    let session = Session::new("none");
    let shown = || {
        status_report(&session, &stored, &workspace, None, &config, None)
            .lines
            .into_iter()
            .map(|line| (line.label, line.value))
            .collect::<Vec<_>>()
    };

    assert!(
        !shown()
            .iter()
            .any(|(label, _)| label == t!(status_requested_paths)),
        "a heading for paths nobody was asked about"
    );

    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/in"),
        false,
        "reads in".to_string(),
    );
    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/out"),
        true,
        "writes out".to_string(),
    );
    let first = t!(
        path_listed,
        number = "1",
        access = t!(reach_access_reads),
        path = "/data/in"
    );
    let second = t!(
        path_listed,
        number = "2",
        access = t!(reach_access_writes),
        path = "/data/out"
    );
    let lines = shown();
    let at = lines
        .iter()
        .position(|(label, _)| label == t!(status_requested_paths))
        .unwrap_or_else(|| panic!("no Requested paths heading in {lines:?}"));
    assert_eq!(
        lines[at].1,
        t!(count_requested_paths, count = 2),
        "the heading does not count the grant"
    );
    assert_eq!(
        lines[at + 1..at + 3]
            .iter()
            .map(|(_, row)| row.as_str())
            .collect::<Vec<_>>(),
        [first.as_str(), second.as_str()],
        "the rows are not the grant, numbered as /reach paths numbers them"
    );
}

/// PATHREQ-4. A yes the person gives to `request_path` inside a turn is the grant `/status`
/// lists afterwards, in the words `/reach paths` uses. The turn runs on a clone of the session's
/// workspace, as the terminal starts one, with the question crossing to this thread as it does
/// to the one that owns the screen. Every other status test grants by hand.
///
/// The faults it rejects are a grant a turn makes on a copy of the grant list the session does
/// not read, and a yes that is honoured for programs and listed by nothing. That `run_turn_animated`
/// hands the worker a clone of the session's workspace is read from its source, since the
/// function needs a terminal.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::panic, reason = "a test fails by panicking")]
#[expect(clippy::too_many_lines, reason = "a test of one clause with its setup")]
fn a_path_the_person_lets_a_turn_reach_is_the_one_status_lists_afterwards() {
    use crate::remote_confirm::{Interjections, RemoteConfirmer, Reply, ToMain};
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    // A turn is asked for reach only where programs are confined, as a request for a path is.
    if bravebot_sandbox::base::Prelude::current().is_none()
        || !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    let top = crate::testutil::scratch_dir("bravebot-app-status-turn-path");
    std::fs::remove_dir_all(&top).ok();
    for place in [".ssh", ".bravebot", ".aws"] {
        std::fs::create_dir_all(top.join("home").join(place)).expect("home directory");
    }
    for place in ["beside", "project"] {
        std::fs::create_dir_all(top.join(place)).expect("directory");
    }
    let home = top.join("home").canonicalize().expect("home");
    let beside = top.join("beside").canonicalize().expect("beside");
    let root = top.join("project").canonicalize().expect("project");
    let workspace = Workspace::new(&root).expect("a workspace");
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let session = Session::new("none");
    let (config, _requests, server) = undo_tests::endpoint::endpoint(
        vec![
            undo_tests::endpoint::tool(
                "load_tool",
                serde_json::json!({
                    "name": "request_path",
                    "why": "the build writes its output beside the project",
                }),
            ),
            undo_tests::endpoint::tool(
                "request_path",
                serde_json::json!({
                    "path": beside.display().to_string(),
                    "why": "the build writes its output there",
                }),
            ),
            undo_tests::endpoint::answer(),
        ],
        None,
    );
    let status_config = config.clone();
    let listed = |workspace: &Workspace| {
        status_report(&session, &stored, workspace, None, &status_config, None)
            .lines
            .into_iter()
            .map(|line| (line.label, line.value))
            .collect::<Vec<_>>()
    };
    assert!(
        !listed(&workspace)
            .iter()
            .any(|(label, _)| label == t!(status_requested_paths)),
        "a heading for paths nobody was asked about"
    );

    let mode = session.permission_mode();
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    let task = Task::new("reach it")
        .with_profile(Some(home))
        .with_permission_mode(mode)
        .with_confined_runs(true)
        .asking_about_unconfined(true)
        .with_sandbox_mode(session.live_sandbox_mode());
    let (to_main, from_worker) = mpsc::channel::<ToMain>();
    let (answer_tx, answer_rx) = mpsc::channel::<Reply>();
    let worker_workspace = workspace.clone();
    let worker = thread::spawn(move || {
        let mut asking = RemoteConfirmer::new(to_main, answer_rx, Interjections::new());
        let mut confirmer = bravebot_agent::Confining::new(&mut asking, mode, false);
        turn::resume(
            &config,
            &Egress::new(),
            &worker_workspace,
            &task,
            &mut Conversation::new(),
            &mut confirmer,
            &mut bravebot_agent::IgnoreReports,
            &mut Trail::new(),
            trust,
            TrustedPrograms::new(),
            None,
            &Cancel::new(),
        )
        .outcome
        .expect("the turn runs");
    });
    let mut asked = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match from_worker.recv_timeout(left) {
            Ok(ToMain::Path(request)) => {
                asked.push(request);
                answer_tx
                    .send(Reply::Path(bravebot_agent::confirm::Decision::Approve))
                    .expect("answered");
            }
            // Each of these blocks the worker until it is answered, and this loop answers only
            // a path.
            Ok(
                message @ (ToMain::Write(_)
                | ToMain::Run(_)
                | ToMain::ReadOutput(_)
                | ToMain::Vet(_)
                | ToMain::Fetch(_)
                | ToMain::Vouch(_)
                | ToMain::Exposure(_)
                | ToMain::Server(_)
                | ToMain::Manifest(_)
                | ToMain::ToolList(_)
                | ToMain::McpCall(_)
                | ToMain::Apply(_)
                | ToMain::Move(_)
                | ToMain::Host(_)
                | ToMain::Hook(_)
                | ToMain::Ask(_)),
            ) => panic!("the turn asked something other than a path: {message:?}"),
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("the turn did not finish within a minute")
            }
        }
    }
    worker.join().expect("the turn finished");
    server.join().expect("the model finished");

    assert_eq!(asked.len(), 1, "the turn did not ask about the path");
    let first = t!(
        path_listed,
        number = "1",
        access = t!(reach_access_reads),
        path = asked[0].path.display().to_string().as_str()
    );
    let lines = listed(&workspace);
    let at = lines
        .iter()
        .position(|(label, _)| label == t!(status_requested_paths))
        .unwrap_or_else(|| panic!("a yes given in a turn is not listed by /status: {lines:?}"));
    assert_eq!(lines[at].1, t!(count_requested_paths, count = 1));
    assert_eq!(
        lines[at + 1].1,
        first,
        "the row is not the grant the turn made, worded as /reach paths words it"
    );

    let turns = include_str!("../turns.rs");
    let start = turns
        .find("fn run_turn_animated(")
        .expect("`fn run_turn_animated(` is not in the source");
    let (_, run) = turns.split_at(start);
    let (run, _) = run
        .split_once("\n}\n")
        .expect("`fn run_turn_animated` is not closed");
    assert!(
        run.contains("let worker_workspace = workspace.clone();")
            && run.contains("&worker_workspace,")
            && !run.contains("Workspace::new("),
        "a turn is not handed a clone of the workspace `/status` reads"
    );
}

/// TRUST-9: with no home directory known, a leading `~` is not a refusal, the way a file
/// tool's own read of one is; it is a name under the working directory, exactly as a name
/// with no leading `~` is. The home has to be a parameter rather than
/// [`bravebot_agent::home::profile`] read here, since a machine running this test may have
/// one, so there would be no way to hold "no home" fixed otherwise.
#[test]
fn a_leading_tilde_resolves_as_a_literal_name_with_no_home() {
    let root = std::path::Path::new("/w/proj");
    assert_eq!(
        named_directory(root, "~/x", None).ok(),
        Some(root.join("~").join("x").display().to_string())
    );
}

/// TRUST-9: `/add-dir close` takes back both halves `/add-dir` gave, the reach and the rule, for
/// the directory named and no other. A second close of it, and a close with no path, are
/// refused and say why, so nobody is told a directory closed when nothing did.
///
/// The failures this rejects are a close that leaves the rule, which would go on vouching for
/// files there that reach a turn some other way and be carried into a resume, a close of
/// the workspace's list alone that leaves the directory trusted, and a note saying trust was
/// withdrawn where the rule at the directory was a distrust that stays.
#[test]
fn add_dir_close_withdraws_the_reach_and_the_rule_together() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-close-test");
    let project = root.join("project");
    let notes = root.join("notes");
    let shared = root.join("shared");
    for directory in [&project, &notes, &shared] {
        std::fs::create_dir_all(directory).expect("scratch");
    }

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");
    for directory in [&notes, &shared] {
        add_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &directory.display().to_string(),
        );
    }
    let notes = notes.canonicalize().expect("canonical");
    let shared = shared.canonicalize().expect("canonical");
    let note = notes.join("todo.md");
    assert!(workspace.confines(&note).is_ok());
    assert!(trust.is_trusted(&note.display().to_string()));

    assert!(close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &notes.display().to_string(),
    ));

    assert!(
        workspace.confines(&note).is_err(),
        "the closed directory is still reachable"
    );
    assert!(
        !trust.is_trusted(&note.display().to_string()),
        "the closed directory is still trusted"
    );
    assert!(workspace.confines(&shared.join("todo.md")).is_ok());
    assert!(
        trust.is_trusted(&shared.join("todo.md").display().to_string()),
        "closing one directory withdrew another's rule"
    );
    let said = &session.transcript.last().expect("a note").text;
    assert!(said.contains("no longer trusting it"), "{said}");

    assert!(!close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &notes.display().to_string(),
    ));
    let said = &session.transcript.last().expect("a note").text;
    assert!(said.contains("could not close"), "{said}");
    assert!(!close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        ""
    ));
    let said = &session.transcript.last().expect("a note").text;
    assert!(said.contains("/add-dir close needs a directory"), "{said}");
    assert_eq!(workspace.added_directories(), std::slice::from_ref(&shared));

    let kept = shared.join("todo.md").display().to_string();
    trust.distrust(&bravebot_agent::workspace::key_of(&shared));
    assert!(close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &shared.display().to_string(),
    ));
    let said = &session.transcript.last().expect("a note").text;
    assert!(
        !said.contains("no longer trusting it"),
        "a close that withdrew nothing said it did: {said}"
    );
    assert_eq!(
        trust.integrity_of(&kept),
        Some(bravebot_core::label::Integrity::Untrusted)
    );

    std::fs::remove_dir_all(&root).ok();
}

/// `/add-dir close` rewrites the record at once where one is on disk, so a session that closed
/// a directory and then quit does not resume with it open and trusted. A session nobody has
/// asked anything in has no record, and closing a directory does not make one.
#[test]
fn closing_a_directory_rewrites_the_record_on_disk_and_writes_none_where_there_is_not() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let base = crate::test_profile::project("bravebot-app-close-directory-record");
    let root = base.join("project");
    let notes = base.join("notes");
    std::fs::create_dir_all(&root).expect("create");
    std::fs::create_dir_all(&notes).expect("create");
    let notes = notes.display().to_string();
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(TrustStore::new(&root), TrustedPrograms::new(), rules);
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut conversation = Conversation::new();
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    let mut held = Held {
        session: &mut session,
        conversation: &mut conversation,
        stored: &mut stored,
        answers: &mut answers,
        workspace: &mut workspace,
        scratch: &mut scratch,
        settings: &settings,
        run_settings: &run_settings,
    };

    held.add_directory_command(&notes);
    held.close_directory_command(&notes);
    assert_eq!(
        held.stored.resumable(),
        None,
        "closing a directory wrote a record for a session nobody asked anything in"
    );
    assert!(sessions::list(&root).is_empty());

    held.add_directory_command(&notes);
    held.save("tidy the notes");
    let id = held.stored.id().to_string();
    assert_eq!(
        sessions::load(&root, &id)
            .expect("the record just saved")
            .directories
            .len(),
        1,
        "the record did not name the directory that was open"
    );

    held.close_directory_command(&notes);
    assert!(
        sessions::load(&root, &id)
            .expect("the record")
            .directories
            .is_empty(),
        "the record on disk still names the directory that was closed"
    );
}

/// A session whose working directory is `project`, after `/add-dir <named>`.
fn after_add_dir(project: &Path, named: &str) -> (Workspace, TrustStore, Session) {
    let mut workspace = Workspace::new(project).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");
    add_directory(&mut session, &mut workspace, &mut trust, named);
    (workspace, trust, session)
}

/// TRUST-9: `/add-dir` goes through the workspace's one opening, so a directory that is another
/// path to the working directory is refused with a note that names it and says why, nothing is
/// vouched for, and a directory that is not is added. The identity is injected so the pair exists
/// on any host. Rejects a UI door that opens the directory itself and skips the check, which
/// adds it and vouches for it.
#[cfg(unix)]
#[test]
fn add_dir_refuses_another_path_to_the_working_directory() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-second-path-test");
    let _ = std::fs::remove_dir_all(&root);
    for name in ["twin-a", "twin-b", "other"] {
        std::fs::create_dir_all(root.join(name)).expect("scratch");
    }
    let root = root.canonicalize().expect("canonical");
    let mut workspace = Workspace::new(root.join("twin-a"))
        .expect("workspace")
        .with_directory_identity(Some(|directory| {
            use std::os::unix::fs::MetadataExt;
            match directory.file_name().and_then(|name| name.to_str()) {
                Some("twin-a" | "twin-b") => Some((u64::MAX, 7)),
                _ => std::fs::metadata(directory)
                    .ok()
                    .map(|found| (found.dev(), found.ino())),
            }
        }));
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");

    let twin = root.join("twin-b").display().to_string();
    add_directory(&mut session, &mut workspace, &mut trust, &twin);
    let said = last_note(&session);
    assert!(
        said.starts_with(&format!("could not add {twin}")) && said.contains("another path"),
        "{said}"
    );
    assert!(workspace.added_directories().is_empty());
    assert!(trust.is_empty(), "a refused directory was vouched for");

    let other = root.join("other").display().to_string();
    add_directory(&mut session, &mut workspace, &mut trust, &other);
    assert_eq!(
        workspace.added_directories(),
        [root.join("other")],
        "{}",
        last_note(&session)
    );

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: `/add-dir` takes a path relative to the working directory, and opens, trusts and
/// names the directory the absolute spelling of it opens, trusts and names.
///
/// The failures this rejects are a name passed on for the workspace to refuse as not absolute,
/// and one resolved against the directory the program was started in, which is not what a
/// relative path means anywhere else in the session. The scratch directory is not the one the
/// tests run in, so the second reaches another directory than the one asked for.
#[test]
fn add_dir_opens_a_relative_name_as_it_opens_the_absolute_one() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-relative-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let notes = notes.canonicalize().expect("canonical");
    let todo = notes.join("todo.md").display().to_string();
    let opened = |named: &str| {
        let (workspace, trust, session) = after_add_dir(&project, named);
        (
            workspace.added_directories(),
            trust.is_trusted(&todo),
            last_note(&session).to_string(),
        )
    };

    let by_absolute = opened(&notes.display().to_string());
    assert_eq!(
        by_absolute,
        (
            vec![notes.clone()],
            true,
            t!(
                session_directory_added,
                directory = notes.display().to_string()
            )
        ),
        "the absolute spelling no longer opens, trusts and names the directory"
    );
    for named in ["../notes", "./../notes", "../notes/", "../project/../notes"] {
        assert_eq!(
            opened(named),
            by_absolute,
            "{named} did not open what the absolute spelling opens"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: the rule `/add-dir` records is under the directory a relative name resolves to, and
/// not under the name typed, and a name that goes through a link inside the project opens and
/// trusts what the link points to, as the absolute spelling of the link does, with the note
/// naming the target. `./link` is a name inside the project that leads out of it: a rule keyed
/// under the link would be a rule about a path in the project, where a person's answer about
/// the project already decides.
#[cfg(unix)]
#[test]
fn add_dir_records_trust_under_the_directory_a_relative_name_resolves_to() {
    use bravebot_core::label::Integrity;

    let root = crate::testutil::scratch_dir("bravebot-add-dir-relative-key-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("elsewhere/notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    std::os::unix::fs::symlink(&notes, project.join("link")).expect("a link");
    let notes = notes.canonicalize().expect("canonical");
    let project = project.canonicalize().expect("canonical");

    for named in ["./link", &project.join("link").display().to_string()] {
        let (workspace, trust, session) = after_add_dir(&project, named);

        assert_eq!(
            workspace.added_directories(),
            std::slice::from_ref(&notes),
            "{named}"
        );
        assert_eq!(
            trust.keyed().collect::<Vec<_>>(),
            [(
                bravebot_agent::workspace::key_of(&notes).as_str(),
                Some(Integrity::Trusted)
            )],
            "the rule for {named} was not recorded under the directory it resolves to"
        );
        assert!(trust.is_trusted(&notes.join("todo.md").display().to_string()));
        assert_eq!(
            last_note(&session),
            t!(
                session_directory_added,
                directory = notes.display().to_string()
            ),
            "{named}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: `/add-dir ../` opens the directory that holds the project and trusts it, naming that
/// directory and not the name typed. A directory holding the project is not refused.
#[test]
fn add_dir_of_the_parent_opens_and_trusts_the_directory_holding_the_project() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-parent-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let held = root.canonicalize().expect("canonical");

    let (workspace, trust, session) = after_add_dir(&project, "../");

    assert_eq!(workspace.added_directories(), std::slice::from_ref(&held));
    assert_eq!(
        session.transcript[0].text,
        t!(
            session_directory_added,
            directory = held.display().to_string()
        )
    );
    for file in ["notes/todo.md", "project/main.rs"] {
        assert!(
            trust.is_trusted(&held.join(file).display().to_string()),
            "the rule at the parent does not cover {file}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: a relative name means the working directory the session is in now. After `/cd` it
/// is the new one, and the directory beside the old one is not what `../notes` reaches.
///
/// The failure this rejects is a name resolved against the directory the session started in.
#[test]
fn a_relative_name_means_the_working_directory_after_a_cd() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-relative-cd-test");
    let _ = std::fs::remove_dir_all(&root);
    for directory in ["one/project", "one/notes", "two/project", "two/notes"] {
        std::fs::create_dir_all(root.join(directory)).expect("scratch");
    }
    let mut workspace = Workspace::new(root.join("one/project")).expect("workspace");
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(
        TrustStore::new(workspace.root()),
        TrustedPrograms::new(),
        rules,
    );
    let moved = change_directory(
        &mut session,
        &mut workspace,
        &mut answers,
        "a-session",
        &root.join("two/project").display().to_string(),
        |_, _| Some(true),
    );
    assert_eq!(moved, Changed::Moved);

    add_directory(&mut session, &mut workspace, &mut answers.trust, "../notes");

    assert_eq!(
        workspace.added_directories(),
        [root.join("two/notes").canonicalize().expect("canonical")]
    );

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: `/add-dir` refuses a relative name for the reasons it refuses the absolute spelling
/// of it, and nothing is opened or vouched for. The refusal begins with the name as it was typed
/// and then quotes the absolute path the name resolved to, so a person sees which directory was
/// refused. A name that is not a directory, one that is nowhere, and one inside the project each
/// keep the reason they have when spelled in full, and the last still says it can be named
/// relatively. No path at all says it needs one, and is not read as the working directory.
#[test]
fn add_dir_refuses_a_relative_name_as_it_refuses_the_absolute_one() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-relative-refused-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    std::fs::create_dir_all(project.join("sub")).expect("scratch");
    std::fs::write(root.join("file.txt"), "text").expect("a file");
    let project = project.canonicalize().expect("canonical");
    let root = root.canonicalize().expect("canonical");

    for (relative, absolute, reason) in [
        ("../nowhere", root.join("nowhere"), None),
        (
            "../file.txt",
            root.join("file.txt"),
            Some("must be a directory"),
        ),
        (
            "./sub",
            project.join("sub"),
            Some("is already inside the workspace, so it can be named relatively"),
        ),
        (
            "../project/sub",
            project.join("sub"),
            Some("is already inside the workspace, so it can be named relatively"),
        ),
        (
            ".",
            project.clone(),
            Some("is already inside the workspace, so it can be named relatively"),
        ),
    ] {
        let (workspace, trust, session) = after_add_dir(&project, relative);
        let said = last_note(&session);
        let absolute = absolute.display().to_string();
        let (_, _, by_absolute) = after_add_dir(&project, &absolute);
        let by_absolute = last_note(&by_absolute);

        let after_the_path = by_absolute
            .strip_prefix(&format!("could not add {absolute}: '{absolute}'"))
            .unwrap_or_else(|| panic!("the absolute spelling was not refused: {by_absolute}"));
        assert_eq!(
            said,
            format!("could not add {relative}: '{absolute}'{after_the_path}"),
            "{relative} was not refused as {absolute} is, under the name typed"
        );
        if let Some(reason) = reason {
            assert!(said.contains(reason), "{said}");
        }
        assert!(workspace.added_directories().is_empty(), "{relative}");
        assert!(trust.is_empty(), "{relative} was vouched for");
    }

    let (workspace, trust, session) = after_add_dir(&project, "");
    assert_eq!(last_note(&session), t!(session_add_dir_needs_a_path));
    assert!(workspace.added_directories().is_empty());
    assert!(trust.is_empty());

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9 and SANDBOX-18: a relative name that lands in a credential location is refused as
/// the absolute spelling is, and one that lands in the home directory, which holds a location,
/// is not. The refusal is decided on the directory the name resolves to, so a name that climbs
/// to a location is not a way past.
///
/// The home directory is a scratch one, so the test runs in a process of its own. Where this
/// process is not that one, `in_isolated_profile` runs the test there and fails this one if it
/// failed, so the assertions below are always run before the test passes.
#[test]
fn add_dir_refuses_a_relative_name_that_reaches_a_credential_location() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let home = bravebot_agent::home::profile()
        .expect("a profile")
        .canonicalize()
        .expect("canonical");
    let project = home.join("projects/project");
    for directory in [".ssh", "notes"] {
        std::fs::create_dir_all(home.join(directory)).expect("create");
    }
    std::fs::create_dir_all(&project).expect("create");

    let (workspace, trust, session) = after_add_dir(&project, "../../.ssh");
    assert!(
        last_note(&session).contains("credential location"),
        "{}",
        last_note(&session)
    );
    assert!(workspace.added_directories().is_empty());
    assert!(trust.is_empty());

    for (named, opened) in [("../../notes", home.join("notes")), ("../..", home.clone())] {
        let (workspace, ..) = after_add_dir(&project, named);
        assert_eq!(
            workspace.added_directories(),
            [opened],
            "{named} was refused"
        );
    }
}

/// TRUST-9: `/add-dir close` takes a relative name as it takes the absolute spelling, so a
/// directory opened under one spelling closes under the other, and both halves go. A
/// directory that has gone since closes under a relative name too, and a name that is not open
/// is refused under the spelling typed.
///
/// The failure this rejects is a close that leaves the name for the workspace to refuse as
/// not absolute, which would leave a directory a person opened with a short name open.
#[test]
fn add_dir_close_takes_a_relative_name_as_it_takes_the_absolute_one() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-close-relative-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes, &root.join("shared")] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let notes = notes.canonicalize().expect("canonical");
    let absolute = notes.display().to_string();
    let todo = notes.join("todo.md");

    for (opened_with, closed_with) in [
        (absolute.as_str(), "../notes"),
        ("../notes", absolute.as_str()),
        ("../notes", "../shared/../notes"),
        ("../notes", "./../notes/"),
    ] {
        let (mut workspace, mut trust, mut session) = after_add_dir(&project, opened_with);
        assert!(workspace.confines(&todo).is_ok(), "{opened_with}");

        assert!(
            close_directory(&mut session, &mut workspace, &mut trust, closed_with),
            "{closed_with} did not close what {opened_with} opened: {}",
            last_note(&session)
        );

        assert!(workspace.added_directories().is_empty());
        assert!(workspace.confines(&todo).is_err(), "{closed_with}");
        assert!(trust.is_empty(), "{closed_with} left the rule");
        assert_eq!(
            last_note(&session),
            t!(session_directory_withdrawn, directory = absolute.as_str()),
            "{closed_with}"
        );
    }

    let (mut workspace, mut trust, mut session) = after_add_dir(&project, "../notes");
    std::fs::remove_dir_all(&notes).expect("the directory goes");
    assert!(
        close_directory(&mut session, &mut workspace, &mut trust, "../notes"),
        "a directory that has gone did not close: {}",
        last_note(&session)
    );
    assert!(workspace.added_directories().is_empty());

    assert!(!close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        "../shared"
    ));
    assert!(
        last_note(&session).starts_with("could not close ../shared: '"),
        "{}",
        last_note(&session)
    );

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: a relative name closes the directory it names even when a link has been put where
/// that directory was. A relative name with no `..` after a name is made the absolute spelling
/// the directory was opened under before the workspace is asked, so the close matches it as
/// spelled, as it does the absolute name, and the link cannot send it to the link's target.
/// `notes` and `shared` are opened by absolute path and `notes` is then replaced by a link to
/// `shared`.
///
/// The failure this rejects is a relative name joined as typed, which is never equal to an
/// opened directory's recorded spelling and so is only ever followed through the link, closing
/// `shared` and leaving `notes` open.
#[cfg(unix)]
#[test]
fn add_dir_close_by_a_relative_name_is_not_redirected_by_a_link_put_where_the_directory_was() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-close-link-test");
    for by_absolute in [true, false] {
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("project");
        let notes = root.join("notes");
        let shared = root.join("shared");
        for directory in [&project, &notes, &shared] {
            std::fs::create_dir_all(directory).expect("scratch");
        }
        let notes = notes.canonicalize().expect("canonical");
        let shared = shared.canonicalize().expect("canonical");
        let closed_with = match by_absolute {
            true => notes.display().to_string(),
            false => "../notes".to_string(),
        };
        let (mut workspace, mut trust, mut session) =
            after_add_dir(&project, &notes.display().to_string());
        add_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &shared.display().to_string(),
        );
        std::fs::remove_dir_all(&notes).expect("the directory goes");
        std::os::unix::fs::symlink(&shared, &notes).expect("a link");

        assert!(
            close_directory(&mut session, &mut workspace, &mut trust, &closed_with),
            "{closed_with} closed nothing: {}",
            last_note(&session)
        );

        assert_eq!(
            workspace.added_directories(),
            std::slice::from_ref(&shared),
            "{closed_with} closed another directory than `notes`"
        );
        assert!(
            trust.is_trusted(&shared.join("todo.md").display().to_string()),
            "{closed_with} took back the rule of `shared`"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: a name that climbs out of the working directory and back into the directory beside
/// it opens that directory and closes it, under each spelling of the climb. The three are the
/// ones a person types: `../notes`, `./../notes` and one that goes back through the project's
/// own name.
///
/// The failure this rejects is a cleaning of the leading `..` that steps over a different
/// number of directories than it names, which opens or closes the wrong one.
#[test]
fn a_climbing_relative_name_opens_and_closes_the_directory_it_names() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-climb-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let notes = notes.canonicalize().expect("canonical");

    for named in ["../notes", "./../notes", "../project/../notes"] {
        let (mut workspace, mut trust, mut session) = after_add_dir(&project, named);
        assert_eq!(
            workspace.added_directories(),
            std::slice::from_ref(&notes),
            "{named} did not open `notes`"
        );

        assert!(
            close_directory(&mut session, &mut workspace, &mut trust, named),
            "{named} did not close what it opened: {}",
            last_note(&session)
        );

        assert!(workspace.added_directories().is_empty(), "{named}");
        assert!(trust.is_empty(), "{named} left the rule");
    }

    std::fs::remove_dir_all(&root).ok();
}

/// TRUST-9: a leading `~` is the home directory before it is a name under the working
/// directory, so `~/notes` opens and closes the home's `notes` and not the directory `notes`
/// inside a directory called `~` in the project, which is what the other order reads. Both
/// exist here, and the second is inside the project, which `/add-dir` refuses.
///
/// The home is a scratch one, so the test runs in a process of its own. Where this process is
/// not that one, `in_isolated_profile` runs the test there and fails this one if it failed, so
/// the assertions below are always run before the test passes.
#[test]
fn add_dir_reads_a_leading_tilde_as_the_home_before_a_name_under_the_project() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let home = bravebot_agent::home::profile()
        .expect("a profile")
        .canonicalize()
        .expect("canonical");
    let project = home.join("projects/project");
    std::fs::create_dir_all(home.join("notes")).expect("create");
    std::fs::create_dir_all(project.join("~/notes")).expect("a directory called ~");

    let (mut workspace, mut trust, mut session) = after_add_dir(&project, "~/notes");

    assert_eq!(
        workspace.added_directories(),
        [home.join("notes")],
        "`~/notes` did not open the home's notes: {}",
        last_note(&session)
    );
    assert!(close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        "~/notes"
    ));
    assert!(
        workspace.added_directories().is_empty(),
        "`~/notes` did not close the home's notes: {}",
        last_note(&session)
    );
}

/// TRUST-9 and PERM-16: a relative name is refused where the settings keep the file tools
/// inside the workspace, with the key named, as the absolute spelling is. The refusal is the
/// first thing the workspace decides about a directory it is asked to open, before it asks
/// whether the name is absolute, so this passes without the relative-name handling too. It
/// holds the claim that a relative name goes through the same refusals, and rejects a handler
/// that opens a relative name by a route that does not go through them.
#[test]
fn add_dir_refuses_a_relative_name_where_reads_stay_in_the_workspace() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-relative-confined-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(root.join("shared")).expect("scratch");
    let mut workspace = Workspace::new(&project)
        .expect("workspace")
        .with_reads_kept_inside(true);
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");

    add_directory(&mut session, &mut workspace, &mut trust, "../shared");

    assert!(workspace.added_directories().is_empty());
    assert!(trust.is_empty());
    assert!(
        last_note(&session).contains("permissions.readsStayInWorkspace"),
        "{}",
        last_note(&session)
    );

    std::fs::remove_dir_all(&root).ok();
}

/// A session that ran in this directory and vouched for it, which is what an earlier answer
/// of yes leaves behind in the directory's list of sessions.
fn a_record_that_answered_yes_here() -> Box<bravebot_session::sessions::Record> {
    Box::new(
        serde_json::from_value(serde_json::json!({
            "id": "1-2",
            "directory": "/tmp/x",
            "title": "an earlier session here",
            "started": 1,
            "updated": 1,
            "trust": [{"path": ".", "integrity": "trusted"}],
            "conversation": {"messages": [], "context": "trusted"},
        }))
        .expect("a record"),
    )
}

/// The opening map comes from how this session was started and from a yes that was kept, and
/// from nowhere else. A fresh start brings no record, so without a kept yes the question is put:
/// reading back what an earlier session here was left holding, a no included, would grant
/// standing permission over the tree on behalf of a user nobody asked. `/clear` begins a
/// session too, and reaches this with the same `Beginning::New`.
#[test]
fn a_fresh_session_is_asked_rather_than_inheriting_a_map() {
    use bravebot_agent::PermissionMode;

    assert!(
        matches!(
            opening_for(
                beginning_of(&Start::Fresh, here()),
                PermissionMode::Ask,
                here(),
                || None,
            ),
            Opening::Ask
        ),
        "a session started fresh took an answer its own user never gave",
    );
}

/// A yes an earlier session kept settles a fresh one, and `/clear` too, and the
/// map it settles is the one `y` would have given: the tree trusted, nothing more.
#[test]
fn a_remembered_answer_settles_a_fresh_session() {
    use bravebot_agent::PermissionMode;

    let kept = bravebot_agent::trusted::Kept {
        session: "1-2".to_string(),
        at: 7,
        root: here().to_path_buf(),
        confined_lines: false,
    };
    for beginning in [Beginning::New, Beginning::Resumed(None)] {
        match opening_for(beginning, PermissionMode::Ask, here(), || {
            Some(kept.clone())
        }) {
            Opening::Remembered(trust, from) => {
                assert_eq!(from, kept, "the session named another answer");
                assert!(trust.is_trusted("."));
                assert!(trust.is_trusted("src/main.rs"), "the rule covers the tree");
                assert!(
                    !trust.is_trusted("/etc/passwd"),
                    "a remembered answer reached outside the directory",
                );
            }
            opening => panic!("a remembered answer was not honoured: {opening:?}"),
        }
    }
}

/// A directory to answer about and the record a yes there is kept in, under a state directory
/// of its own. `None` on a filesystem that cannot say when a directory was made, which keeps
/// nothing.
fn a_record_to_keep_in(
    name: &str,
) -> Option<(
    std::path::PathBuf,
    std::path::PathBuf,
    (
        bravebot_agent::trusted::Store,
        bravebot_agent::trusted::Identity,
    ),
)> {
    let scratch = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&scratch);
    let home = scratch.join("home");
    let root = scratch.join("work");
    std::fs::create_dir_all(&root).expect("create");
    let identity = bravebot_agent::trusted::Identity::of(&root)?;
    let store = bravebot_agent::trusted::Store::new(&home, &root);
    Some((home, root, (store, identity)))
}

/// TRUST-23: a yes is the answer that is kept. The answer goes through the function
/// `opening_trust` calls once the question is closed, and the next session started in the
/// directory finds what it wrote, is not asked, and opens with the tree trusted. The regression
/// it rejects is the call being dropped, or a yes not reaching the record.
#[test]
fn a_yes_is_kept_and_the_next_session_there_is_not_asked() {
    use crate::trust_prompt::Answer;
    use bravebot_agent::PermissionMode;

    let Some((home, root, record)) = a_record_to_keep_in("bravebot-app-yes-kept") else {
        return;
    };

    assert_eq!(
        keep_the_answer(Answer::Trust, Some(&record), "1-2", &root),
        Some(t!(session_trust_kept, directory = root.display())),
        "a yes with a record did not say it was kept"
    );

    let kept = bravebot_agent::trusted::honoured(Some(&home), None, &root)
        .expect("the next session here would be asked after a yes")
        .kept;
    match opening_for(Beginning::New, PermissionMode::Ask, &root, || {
        Some(kept.clone())
    }) {
        Opening::Remembered(trust, _) => {
            assert!(
                trust.is_trusted("src/main.rs"),
                "the kept yes covers the tree"
            );
        }
        opening => panic!("a kept yes did not settle the next session: {opening:?}"),
    }
    let _ = std::fs::remove_dir_all(root.parent().expect("scratch"));
}

/// TRUST-23: a no is never kept. The next session started there asks, since a no grants nothing
/// and asking again is how a person changes their mind. Each answer the question can give goes
/// through the function `opening_trust` calls, so the reading of which of them is a yes is the
/// code's and not the test's: declining and leaving are both a no. The regression it rejects is
/// either of them reaching the record.
#[test]
fn a_no_is_not_kept_and_the_next_session_there_is_asked() {
    use crate::trust_prompt::Answer;
    use bravebot_agent::PermissionMode;

    let Some((home, root, record)) = a_record_to_keep_in("bravebot-app-no-not-kept") else {
        return;
    };

    for answer in [Answer::Decline, Answer::Leave] {
        assert_eq!(
            keep_the_answer(answer, Some(&record), "1-2", &root),
            None,
            "{answer:?} was kept"
        );
    }

    assert!(!record.0.path().exists(), "a record was written for no yes");
    assert!(matches!(
        opening_for(Beginning::New, PermissionMode::Ask, &root, || {
            bravebot_agent::trusted::honoured(Some(&home), None, &root).map(|h| h.kept)
        }),
        Opening::Ask
    ));
    let _ = std::fs::remove_dir_all(root.parent().expect("scratch"));
}

/// TRUST-23: a yes whose record cannot be written says it was not kept and names the file, so
/// the person knows the next session asks; a yes with no record has nothing to say, because the
/// question already said the answer cannot be kept. The state directory here is a file, which
/// nothing can be created inside.
#[test]
fn a_yes_the_record_refuses_says_so_and_a_yes_with_no_record_says_nothing() {
    use crate::trust_prompt::Answer;

    let Some((_, root, _)) = a_record_to_keep_in("bravebot-app-yes-refused") else {
        return;
    };
    let identity = bravebot_agent::trusted::Identity::of(&root).expect("a directory");
    let blocked = root.parent().expect("scratch").join("not-a-directory");
    std::fs::write(&blocked, b"").expect("a file in the way");
    let record = (
        bravebot_agent::trusted::Store::new(&blocked, &root),
        identity,
    );

    assert_eq!(
        keep_the_answer(Answer::Trust, Some(&record), "1-2", &root),
        Some(t!(
            session_trust_not_kept,
            directory = root.display(),
            path = record.0.path().display()
        ))
    );
    assert_eq!(keep_the_answer(Answer::Trust, None, "1-2", &root), None);
    let _ = std::fs::remove_dir_all(root.parent().expect("scratch"));
}

/// `/forget-trust` takes the kept answer back, so the next session started there asks, and
/// says so; this session keeps the map it already has.
#[test]
fn forgetting_trust_makes_the_next_session_here_ask() {
    use bravebot_agent::trusted::{Identity, Store};

    let scratch = crate::testutil::scratch_dir("bravebot-app-forget-trust");
    let _ = std::fs::remove_dir_all(&scratch);
    let home = scratch.join("home");
    let root = scratch.join("work");
    std::fs::create_dir_all(&root).expect("create");
    let Some(identity) = Identity::of(&root) else {
        // A filesystem with no birth time keeps nothing, which a sibling test covers.
        return;
    };
    let store = Store::new(&home, &root);
    assert!(
        store.keep(&identity, "1-2", 7, false),
        "the answer was not kept"
    );

    let said = forget_trust(Some(&home), &root);

    assert_eq!(
        store.kept(&identity),
        None,
        "the answer outlived /forget-trust"
    );
    assert_eq!(
        said,
        t!(session_trust_forgotten, directory = root.display()),
        "the line did not say the next session will ask",
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// `/forget-trust` in a directory below a remembered worktree root withdraws the root's answer,
/// which is the one settling this directory, and names the root, so the next session in either
/// asks.
#[test]
fn forgetting_trust_below_a_remembered_root_takes_the_roots_answer_back() {
    use bravebot_agent::trusted::{Identity, Store};

    let scratch = crate::testutil::scratch_dir("bravebot-app-forget-root-trust");
    let _ = std::fs::remove_dir_all(&scratch);
    let home = scratch.join("home");
    let root = scratch.join("repo");
    let package = root.join("packages/api");
    std::fs::create_dir_all(root.join(".git")).expect("create");
    std::fs::create_dir_all(&package).expect("create");
    let Some(identity) = Identity::of(&root) else {
        return;
    };
    let store = Store::new(&home, &root);
    assert!(
        store.keep(&identity, "1-2", 7, false),
        "the answer was not kept"
    );

    let said = forget_trust(Some(&home), &package);

    assert_eq!(store.kept(&identity), None, "the root's answer survived");
    assert_eq!(
        said,
        t!(
            session_trust_forgotten_root,
            directory = package.display(),
            root = root.canonicalize().expect("resolved").display()
        ),
        "the line did not name the root"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// With nothing kept there is nothing to take back, and the line says that rather than
/// claiming to have forgotten something.
#[test]
fn forgetting_trust_where_nothing_is_kept_says_so() {
    let scratch = crate::testutil::scratch_dir("bravebot-app-forget-nothing");
    let _ = std::fs::remove_dir_all(&scratch);
    let root = scratch.join("work");
    std::fs::create_dir_all(&root).expect("create");
    let nothing = t!(session_trust_nothing_to_forget, directory = root.display());

    assert_eq!(forget_trust(Some(&scratch.join("home")), &root), nothing);
    assert_eq!(forget_trust(None, &root), nothing, "no state directory");
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A resume is not an exception to that: the answer it honours is the one its own user gave,
/// and it comes with the rules that session's writes recorded, which is what stops a resumed
/// turn reading back a file an earlier turn of the same session poisoned.
#[test]
fn a_resume_starts_with_the_map_its_own_record_kept() {
    use bravebot_agent::PermissionMode;

    let record = a_record_that_answered_yes_here();
    match opening_for(
        beginning_of(&Start::Resuming(record), here()),
        PermissionMode::Ask,
        here(),
        || panic!("a resume read the kept answer over its own record's map"),
    ) {
        Opening::Settled(trust, Whence::Resumed) => {
            assert!(trust.is_trusted("."));
            assert!(trust.is_trusted("src/main.rs"), "the rule covers the tree");
        }
        settled => panic!("a resume did not take its own record's map: {settled:?}"),
    }
}

/// Nothing recorded is not the same as nothing trusted, so a record from before maps were
/// kept is asked about rather than resumed into a map that vouches for nothing.
#[test]
fn a_record_from_before_maps_were_kept_is_asked_about() {
    use bravebot_agent::PermissionMode;

    let mut record = a_record_that_answered_yes_here();
    record.trust = None;

    assert!(
        matches!(
            opening_for(
                beginning_of(&Start::Resuming(record), here()),
                PermissionMode::Ask,
                here(),
                || None,
            ),
            Opening::Ask
        ),
        "a record that answered nothing was resumed as an answer",
    );
}

/// Bypassing answers the question rather than putting it, but a resume is still the more
/// specific record: the question is not being put in either case, so the mode has nothing to
/// answer, and a no given inside a trusted tree must survive being picked up again.
#[test]
fn a_resume_keeps_its_own_map_even_where_the_mode_would_answer() {
    use bravebot_agent::PermissionMode;

    let mut record = a_record_that_answered_yes_here();
    record.trust = Some(vec![
        bravebot_session::sessions::StoredRule {
            path: ".".to_string(),
            integrity: "trusted".to_string(),
        },
        bravebot_session::sessions::StoredRule {
            path: "vendor".to_string(),
            integrity: "untrusted".to_string(),
        },
    ]);

    match opening_for(
        beginning_of(&Start::Resuming(record), here()),
        PermissionMode::Bypass,
        here(),
        || panic!("a resume read the kept answer over its own record's map"),
    ) {
        Opening::Settled(trust, Whence::Resumed) => assert!(
            !trust.is_trusted("vendor/lib.js"),
            "the mode's own answer replaced the one the record kept",
        ),
        settled => panic!("a resume was answered by the mode instead: {settled:?}"),
    }
}

/// Whether `path` has been removed within a second, since ending the session that held it waits
/// a quarter of a second at most for the removal.
fn removed_within_a_second(path: &std::path::Path) -> bool {
    (0..20).any(|_| {
        if !path.exists() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        false
    })
}

/// TRUST-15: `/branch` carries on in a copy and gives it a directory of its own, removing the one
/// of the session it followed with what was written in it, and a branch that was refused
/// carries on in the same session and keeps its directory. Rejects: a branch that keeps the
/// directory of the session it followed, one that replaces it where no copy was made, and one
/// that opens a new directory the workspace does not reach.
#[test]
fn branching_replaces_the_sessions_directory_only_where_a_copy_was_made() {
    use bravebot_agent::SessionScratch;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-scratch");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, written, mut conversation, mut answers) =
        a_session_to_branch(&root);
    let mut scratch = Some(SessionScratch::create().expect("a directory"));
    workspace.open_scratch(scratch.as_ref().and_then(SessionScratch::directory));
    let first = scratch.as_ref().expect("a directory").path().to_path_buf();
    std::fs::write(first.join("intermediate.txt"), "written by the session").expect("a file");
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);

    // A session nobody has written a record for has nothing to copy, so the branch is refused.
    let mut nothing_written =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    Held {
        session: &mut session,
        conversation: &mut conversation,
        stored: &mut nothing_written,
        answers: &mut answers,
        workspace: &mut workspace,
        scratch: &mut scratch,
        settings: &settings,
        run_settings: &run_settings,
    }
    .branch("");
    assert_eq!(
        scratch.as_ref().map(SessionScratch::path),
        Some(first.as_path()),
        "a refused branch replaced the directory of the session it stayed in"
    );
    assert!(
        first.join("intermediate.txt").exists(),
        "a refused branch removed what the session wrote"
    );
    assert_eq!(workspace.scratch(), Some(first.as_path()));
    drop(nothing_written);

    let mut stored = written;
    Held {
        session: &mut session,
        conversation: &mut conversation,
        stored: &mut stored,
        answers: &mut answers,
        workspace: &mut workspace,
        scratch: &mut scratch,
        settings: &settings,
        run_settings: &run_settings,
    }
    .branch("");

    let second = scratch
        .as_ref()
        .expect("the copy was given no directory")
        .path()
        .to_path_buf();
    assert_ne!(second, first, "the copy carried on in the old directory");
    assert!(second.is_dir(), "the copy's directory does not exist");
    assert!(
        removed_within_a_second(&first),
        "the directory of the session the copy followed was not removed"
    );
    assert_eq!(
        workspace.scratch(),
        Some(second.as_path()),
        "the workspace does not reach the copy's directory"
    );
}

/// TRUST-15: `/clear` and a handoff each begin a session with a directory of its own and remove
/// the one of the session they end, with what was written in it. Rejects: a clear that keeps the
/// directory of the cleared session, and a handoff that does.
#[test]
fn clearing_and_a_handoff_each_replace_the_sessions_directory() {
    use bravebot_agent::SessionScratch;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-clear-scratch");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_to_branch(&root);
    let mut scratch = Some(SessionScratch::create().expect("a directory"));
    workspace.open_scratch(scratch.as_ref().and_then(SessionScratch::directory));
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    // Never drawn on: a session bypassing every permission is not asked about the directory it
    // opens in, so the new session is settled without a question.
    let mut terminal = ratatui::Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(std::io::stdout()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 20, 5)),
        },
    )
    .expect("a terminal");

    for brief in [None, Some("carry on with the notes".to_string())] {
        for _ in 0..8 {
            if session.permission_mode() == bravebot_agent::PermissionMode::Bypass {
                break;
            }
            session.cycle_permission_mode();
        }
        assert_eq!(
            session.permission_mode(),
            bravebot_agent::PermissionMode::Bypass,
            "the fixture would put a question to the terminal"
        );
        let before = scratch.as_ref().expect("a directory").path().to_path_buf();
        std::fs::write(before.join("intermediate.txt"), "written by the session").expect("a file");

        let ended = Held {
            session: &mut session,
            conversation: &mut conversation,
            stored: &mut stored,
            answers: &mut answers,
            workspace: &mut workspace,
            scratch: &mut scratch,
            settings: &settings,
            run_settings: &run_settings,
        }
        .clear(&mut terminal, brief.clone());

        assert!(ended.is_none(), "{brief:?} ended the session: {ended:?}");
        let after = scratch
            .as_ref()
            .expect("the new session was given no directory")
            .path()
            .to_path_buf();
        assert_ne!(after, before, "{brief:?} carried on in the old directory");
        assert!(after.is_dir(), "{brief:?} left no directory");
        assert!(
            removed_within_a_second(&before),
            "{brief:?} did not remove the directory of the session it ended"
        );
        assert_eq!(
            workspace.scratch(),
            Some(after.as_path()),
            "{brief:?} left the workspace reaching the old directory"
        );
    }
}

/// TRUST-15: a session begun from a record, resumed or forked, is given a directory of its own
/// that its workspace reaches, and the directory goes when that session ends, as it does when
/// `event_loop` returns and drops what it began with. Each session is begun with the workspace
/// the one before it left, which still reaches the directory that session had. Rejects: a
/// session begun from a record that is given no directory, one whose workspace is left
/// reaching a directory that is not its own, and one that carries on in the directory of the
/// session it followed.
#[test]
fn a_session_begun_from_a_record_is_given_a_directory_of_its_own() {
    use bravebot_agent::SessionScratch;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-begin-scratch");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, _session, stored, _conversation, _answers) = a_session_to_branch(&root);
    let resumed = sessions::load(&root, stored.id()).expect("the record");
    let forked = sessions::fork(&root, stored.id()).expect("the session forks");
    drop(stored);

    // The directory of a session that has ended, still reached by the workspace it left.
    let ended = SessionScratch::create().expect("a directory");
    let mut followed = ended.path().to_path_buf();
    workspace.open_scratch(ended.directory());
    drop(ended);
    assert!(
        removed_within_a_second(&followed),
        "the fixture's earlier directory is still there"
    );

    // Never drawn on: a session opened bypassing every permission is not asked about the
    // directory it opens in.
    let mut terminal = ratatui::Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(std::io::stdout()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 20, 5)),
        },
    )
    .expect("a terminal");
    let mut config = a_config_needing_no_sign_in();
    let mut servers = crate::state::Servers::default();

    for (how, record) in [("resumed", resumed), ("forked", forked)] {
        let begun = begin_session(
            &mut terminal,
            &mut config,
            &workspace,
            &mut servers,
            Startup {
                confinement: String::new(),
                sockets_with_a_path_reachable: false,
                sandbox: crate::sandbox_command::Sandboxed::at_start(),
                start: Start::Resuming(Box::new(record)),
                named: None,
                links: Links::default(),
                skip_permissions: true,
                asked_for_auto: false,
                prompts: bravebot_agent::turn::SystemPrompts::default(),
            },
        )
        .unwrap_or_else(|ended| panic!("the {how} session did not begin: {ended:?}"));

        let own = begun
            .scratch
            .as_ref()
            .unwrap_or_else(|| panic!("the {how} session was given no directory"))
            .path()
            .to_path_buf();
        assert!(own.is_dir(), "the {how} session's directory does not exist");
        assert_ne!(
            own, followed,
            "the {how} session carried on in the directory of the session it followed"
        );
        assert_eq!(
            begun.workspace.scratch(),
            Some(own.as_path()),
            "the workspace does not reach the {how} session's directory"
        );
        std::fs::write(own.join("intermediate.txt"), "written by the session").expect("a file");

        // The session ends as `event_loop` returns, and the next begins with the workspace
        // this one left.
        workspace = begun.workspace.clone();
        drop(begun);
        assert!(
            removed_within_a_second(&own),
            "the {how} session left its directory behind when it ended"
        );
        followed = own;
    }
}

/// Both halves of what `/cd` does, together: the working directory moves, and the directory
/// moved to is vouched for, which is what a relative path means and what decides a write there.
#[test]
fn changing_directory_moves_the_workspace_and_vouches_for_where_it_moved() {
    let root = crate::testutil::scratch_dir("bravebot-cd-test");
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(&other).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new("/work");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            other.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    let canonical = other.canonicalize().expect("canonical");
    assert_eq!(workspace.root(), canonical);
    assert!(
        answers.trust.is_trusted("."),
        "the directory moved to was not vouched for"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// The map goes with the session, so it has to keep saying what it said about the same files
/// once the working directory has moved. A yes given for the directory left behind must not
/// become a yes for the one arrived at, which is what carrying it across unchanged would do.
#[test]
fn changing_directory_leaves_the_previous_answer_where_it_was_given() {
    let root = crate::testutil::scratch_dir("bravebot-cd-rules-test");
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(project.join("vendor")).expect("scratch");
    std::fs::create_dir_all(&other).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    answers.trust.trust(".");
    answers.trust.distrust("vendor");

    let left = workspace.root().to_path_buf();
    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            other.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert!(
        answers.trust.is_trusted(&left.display().to_string()),
        "the answer given for the directory left behind was forgotten"
    );
    assert!(
        !answers
            .trust
            .is_trusted(&left.join("vendor").display().to_string()),
        "a no given inside the directory left behind was forgotten"
    );
    assert_eq!(
        answers.trust.integrity_of("vendor"),
        Some(bravebot_core::label::Integrity::Trusted),
        "a rule about the old vendor directory decided a path in the new one"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// Moving into a directory keeps the nos given inside it. They are about the same files after
/// the move as before, and vouching for the directory moved to is not vouching for a subtree
/// somebody has already refused.
#[test]
fn moving_into_a_directory_keeps_the_answers_given_inside_it() {
    let root = crate::testutil::scratch_dir("bravebot-cd-inner-test");
    let project = root.join("project");
    std::fs::create_dir_all(project.join("src/vendor")).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    answers.trust.trust(".");
    answers.trust.distrust("src/vendor");

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            "src",
            never_asked,
        ),
        Changed::Moved
    );

    assert!(
        answers.trust.is_trusted("main.rs"),
        "the directory moved into was not vouched for"
    );
    assert!(
        !answers.trust.is_trusted("vendor/lib.js"),
        "an untrusted subtree became trusted by moving into its parent"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// A program vouched for names the tree it was vouched in, so a move leaves it covering that
/// tree and no other, and the run prompts already drawn grant nothing. Both are kept.
#[test]
fn moving_keeps_the_programs_vouched_for_and_the_prompts_already_drawn() {
    use bravebot_core::programs::Command;
    let root = crate::testutil::scratch_dir("bravebot-cd-programs-test");
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(&other).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    let make = Command::new("/usr/bin/make", vec!["check".into()], workspace.root());
    answers.programs.trust(make.clone());
    answers.asked_about.record(make.clone());
    let (programs, asked_about) = (answers.programs.clone(), answers.asked_about.clone());

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            other.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );
    assert_eq!(
        answers.programs, programs,
        "moving forgot a program vouched for in the tree left behind"
    );
    assert_eq!(
        answers.asked_about, asked_about,
        "moving forgot a run prompt already drawn"
    );
    assert!(
        !answers.programs.contains(
            &make.program,
            &make.started_as,
            &make.args,
            workspace.root()
        ),
        "a program vouched for in one tree covered the tree moved to"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// A path that names nothing moves nothing, and says so. Half a move would leave the session
/// vouching for a directory it is not in, or asking again about files it has not left.
#[test]
fn a_directory_that_is_not_there_moves_nothing() {
    let root = crate::testutil::scratch_dir("bravebot-cd-missing-test");
    std::fs::create_dir_all(&root).expect("scratch");

    let mut workspace = Workspace::new(&root).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new("/work");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    answers.exposed.allow(".env");
    let before = workspace.root().to_path_buf();

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            "nowhere",
            never_asked,
        ),
        Changed::Stayed
    );
    assert_eq!(workspace.root(), before);
    assert!(
        answers.trust.is_empty(),
        "a refused move vouched for something"
    );
    assert!(
        answers.exposed.holds(".env"),
        "a refused move dropped an answer about a file the session has not left"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// An absolute name is left alone. Joining it onto the workspace would name a path inside the
/// project that nobody asked for.
#[test]
fn an_absolute_directory_in_a_settings_file_is_not_joined_onto_the_workspace() {
    assert_eq!(
        named_directory(std::path::Path::new("/tmp/project"), "/opt/other", None).ok(),
        Some("/opt/other".to_string())
    );
}

/// TRUST-9: the leading `.` and `..` of a relative name are cleaned against the working
/// directory and nothing after a name is. The working directory is canonical, so a leading `..`
/// is exact, and `../notes` becomes the spelling an opened directory is recorded under. A `..`
/// after a name is left alone because the name may be a link, whose target the file system
/// takes the `..` from.
///
/// The failure this rejects is a name joined as typed, which `/add-dir close` cannot match
/// against the directory it opened without asking where the name leads, and one cleaned
/// throughout, which reads `sub/../x` as `x` whether or not `sub` is a link.
#[cfg(unix)]
#[test]
fn a_relative_name_is_cleaned_of_its_leading_dots_and_of_nothing_after_a_name() {
    let root = std::path::Path::new("/w/proj");
    for (typed, resolved) in [
        ("../notes", "/w/notes"),
        ("./../notes", "/w/notes"),
        ("../../x", "/x"),
        ("../../../x", "/x"),
        (".", "/w/proj"),
        ("..", "/w"),
        ("./sub/dir", "/w/proj/sub/dir"),
        ("sub/../x", "/w/proj/sub/../x"),
        ("../project/../notes", "/w/project/../notes"),
        ("/opt/other", "/opt/other"),
    ] {
        assert_eq!(
            named_directory(root, typed, None).ok(),
            Some(resolved.to_string()),
            "{typed}"
        );
    }
}

/// TRUST-9 and PATH-003: a relative name is refused against a working directory whose name is
/// not text, and is resolved against its lookalike, the name that spells the replacement
/// character. The string made here becomes a reach grant and a trust key, and a rendering of
/// the first that substitutes for the bytes it cannot show would name the second. An absolute
/// name needs no working directory and resolves against either.
///
/// The names are built in memory, since the file system of a Mac cannot hold the first.
#[cfg(unix)]
#[test]
fn a_relative_name_is_refused_against_a_working_directory_whose_name_is_not_text() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let not_text = std::path::Path::new(OsStr::from_bytes(b"/w/proj-\xff"));
    let lookalike = std::path::Path::new("/w/proj-\u{FFFD}");

    for relative in ["notes", "../notes", ".", "./sub"] {
        match named_directory(not_text, relative, None) {
            Err(bravebot_agent::WorkspaceError::Invalid { path, reason }) => {
                assert_eq!(path, relative);
                assert_eq!(reason, t!(session_directory_root_not_text));
            }
            other => {
                panic!("{relative} was resolved against a name that is not text: {other:?}")
            }
        }
        assert!(
            named_directory(lookalike, relative, None).is_ok(),
            "{relative} was refused against a working directory that is text"
        );
    }
    assert_eq!(
        named_directory(lookalike, "../notes", None).ok(),
        Some("/w/notes".to_string())
    );
    for root in [not_text, lookalike] {
        assert_eq!(
            named_directory(root, "/opt/other", None).ok(),
            Some("/opt/other".to_string()),
            "an absolute name was refused for {root:?}"
        );
    }
}
