//! Permission modes, plan mode and the sandbox setting.

use super::*;

/// MODE-1, MODE-5, AUTO-1: a session opens asking, and opens in auto where `--auto` was typed
/// or the person's own settings say `permissions.startInAuto`. A mode typed on the line beats
/// the setting, `defaultMode` still selects nothing, and where a layer made bypass unreachable
/// the key from the opening mode no longer reaches it. Each row rejects one fault: opening in
/// auto without being asked (the default flipped), a setting nobody reads, `--auto` ignored, a
/// `false` setting that beats the flag, the setting that overrides the bypass flag, and
/// `defaultMode` read as a mode.
#[test]
fn a_session_opens_asking_unless_the_line_or_the_persons_setting_says_auto() {
    use bravebot_agent::PermissionMode::{Ask, Auto, Bypass};
    let on = r#"{"permissions": {"startInAuto": true}}"#;
    let off = r#"{"permissions": {"startInAuto": false}}"#;
    let rows: [(bool, bool, &str, bravebot_agent::PermissionMode); 9] = [
        (false, false, "{}", Ask),
        (false, false, off, Ask),
        (
            false,
            false,
            r#"{"permissions": {"defaultMode": "auto"}}"#,
            Ask,
        ),
        (false, false, on, Auto),
        (false, true, "{}", Auto),
        (false, true, off, Auto),
        (true, false, "{}", Bypass),
        (true, false, on, Bypass),
        (
            false,
            false,
            r#"{"permissions": {"startInAuto": "true"}}"#,
            Ask,
        ),
    ];
    for (skip_permissions, asked_for_auto, text, expected) in rows {
        let settings = bravebot_config::Settings::parse(text);
        let session = open_in_the_mode_asked_for(
            Session::new("none"),
            skip_permissions,
            asked_for_auto,
            false,
            &settings,
        );
        assert_eq!(
            session.permission_mode(),
            expected,
            "skip {skip_permissions}, --auto {asked_for_auto}, settings {text}"
        );
        assert_eq!(session.began_in_bypass(), expected == Bypass);
    }

    // Where a layer made bypass unreachable, the key from the opening mode goes to asking.
    let unreachable = bravebot_config::Settings::parse(
        r#"{"permissions": {"startInAuto": true, "bypassUnreachable": true}}"#,
    );
    let mut session =
        open_in_the_mode_asked_for(Session::new("none"), false, false, false, &unreachable);
    assert_eq!(session.permission_mode(), Auto);
    session.cycle_permission_mode();
    assert_eq!(session.permission_mode(), Ask);
}

/// AUTO-1: the settings a session reads are the layered ones, so a checkout's file that carries
/// `permissions.startInAuto` opens nothing in auto where the person's own file says nothing, and
/// the person's own file and the file `--settings` names open the session in it. The fault it
/// rejects is the setting being read from a layer that arrives with a clone.
#[test]
fn only_the_persons_own_files_open_a_session_in_auto() {
    let root = crate::testutil::scratch_dir("bravebot-app-start-in-auto-layers");
    let _ = std::fs::remove_dir_all(&root);
    let (home, work) = (root.join("home"), root.join("work"));
    std::fs::create_dir_all(work.join(".bravebot")).expect("a checkout");
    std::fs::create_dir_all(work.join(".git")).expect("a checkout");
    std::fs::create_dir_all(&home).expect("a home");
    let on = r#"{"permissions": {"startInAuto": true}}"#;
    std::fs::write(work.join(".bravebot/settings.json"), on).expect("project");
    std::fs::write(work.join(".bravebot/settings.local.json"), on).expect("local");
    let opens_in = |home: Option<std::path::PathBuf>, named: Option<&std::path::Path>| {
        let settings = bravebot_config::Settings::layered(home, Some(&work), named);
        open_in_the_mode_asked_for(Session::new("none"), false, false, false, &settings)
            .permission_mode()
    };
    assert_eq!(
        opens_in(Some(home.clone()), None),
        bravebot_agent::PermissionMode::Ask,
        "a checkout's files opened the session in auto"
    );

    let file = root.join("named.json");
    std::fs::write(&file, on).expect("named");
    assert_eq!(
        opens_in(Some(home.clone()), Some(&file)),
        bravebot_agent::PermissionMode::Auto,
        "the file --settings names was not read"
    );

    std::fs::write(home.join("settings.json"), on).expect("home");
    assert_eq!(
        opens_in(Some(home), None),
        bravebot_agent::PermissionMode::Auto,
        "the person's own file was not read"
    );
}

/// AUTO-1, MODE-1: a session opened in auto says so on the line under the box, as one the key
/// moved there does, and one that asks says nothing. The fault it rejects is a session put in
/// auto in the handle only, so that the line under the box and what a turn does disagree.
#[test]
fn a_session_that_opens_in_auto_names_it_on_the_line_under_the_box() {
    let settings = bravebot_config::Settings::parse("{}");
    let mut session =
        open_in_the_mode_asked_for(Session::new("none"), false, true, false, &settings);
    assert_eq!(
        session.permission_mode(),
        bravebot_agent::PermissionMode::Auto
    );
    let name = crate::status::named_mode(bravebot_agent::PermissionMode::Auto, false)
        .expect("auto is named");
    let buffer = drawn(&mut session);
    // Panics when the line is not on the screen.
    find_on(&buffer, name);

    let mut asking =
        open_in_the_mode_asked_for(Session::new("none"), false, false, false, &settings);
    let buffer = drawn(&mut asking);
    assert!(
        !buffer_text(&buffer).contains(name),
        "a session that asks drew the auto line"
    );
}

/// The turn is sent the task and not the line, and it begins in plan mode (MODE-12). A wrong
/// implementation sends `/plan fix the auth bug`, or starts the turn before the mode is set.
#[test]
fn the_plan_command_sets_plan_mode_and_sends_only_its_task() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    assert_eq!(session.permission_mode(), PermissionMode::Ask);
    for c in "/plan fix the auth bug".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("fix the auth bug".to_string())
    );
    assert_eq!(session.permission_mode(), PermissionMode::Plan);
    assert_eq!(session.status, Status::Working);
    assert_eq!(
        session.transcript.last().expect("the task is said").text,
        "fix the auth bug"
    );
}

/// Without a task there is nothing to send, so the word sets the mode and the session stays
/// at rest. Typed from bypassing it narrows to planning.
#[test]
fn the_plan_command_without_a_task_sets_the_mode_and_sends_nothing() {
    use bravebot_agent::PermissionMode;
    for session in [
        Session::new("none"),
        Session::new("none").starting_in_bypass(),
    ] {
        let mut session = session;
        assert_eq!(
            dispatch_command(&mut session, commanded("/plan")),
            Action::Redraw
        );
        assert_eq!(session.permission_mode(), PermissionMode::Plan);
        assert_eq!(session.status, Status::Idle);
        assert!(session.transcript.is_empty(), "something was said");
    }
}

/// The word names plan mode, not the next rung: a session already planning stays there, and one
/// asking is not moved to accepting edits. A longer word is a prompt.
#[test]
fn the_plan_command_sets_plan_mode_rather_than_cycling_to_the_next() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    for _ in 0..2 {
        dispatch_command(&mut session, commanded("/plan"));
        assert_eq!(session.permission_mode(), PermissionMode::Plan);
    }

    assert_eq!(command_typed("/planning the release"), None);
}

/// The word carries a prompt, which is a turn of its own, so it waits for the turn in flight
/// and sets the mode when that prompt is run (MODE-12).
#[test]
fn the_plan_command_waits_for_the_turn_in_flight() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "/plan look at it".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(session.permission_mode(), PermissionMode::Ask);
    assert_eq!(session.queued.len(), 1, "the command did not wait");
    assert_eq!(session.queued[0].prompt, "/plan look at it");
}

/// Bypass is on the ladder of a session that did not start in it, and reaching it that way does
/// not make the line under the box name asking once the key leaves it: that is said only of a
/// session the flag opened in bypass.
#[test]
fn the_key_reaches_bypass_without_the_flag() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    assert!(!session.began_in_bypass());
    let mut reached = false;
    for _ in 0..5 {
        handle_key(&mut session, shift(KeyCode::Tab));
        reached |= session.permission_mode() == PermissionMode::Bypass;
    }
    assert!(reached, "five presses never reached bypass");
    assert_eq!(session.permission_mode(), PermissionMode::Ask);
    assert!(!session.began_in_bypass());
}

/// PERM-17: where a layer wrote `permissions.bypassUnreachable` the rung is off the ladder, and
/// the key cannot reach it however many times it is pressed.
#[test]
fn the_key_cannot_reach_bypass_where_a_layer_made_it_unreachable() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    session.make_bypass_unreachable();
    for _ in 0..12 {
        handle_key(&mut session, shift(KeyCode::Tab));
        assert_ne!(session.permission_mode(), PermissionMode::Bypass);
    }
}

/// A session already in bypass when a layer takes it away goes back to asking, is told so, and
/// does not get the rung back.
#[test]
fn a_session_in_bypass_is_put_back_to_asking_when_a_layer_takes_bypass_away() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none").starting_in_bypass();
    let notes = session.transcript.len();
    session.make_bypass_unreachable();
    assert_eq!(session.permission_mode(), PermissionMode::Ask);
    assert!(session.transcript.len() > notes, "the person was not told");
    for _ in 0..12 {
        handle_key(&mut session, shift(KeyCode::Tab));
        assert_ne!(session.permission_mode(), PermissionMode::Bypass);
    }
}

/// `/cd` reads the destination's layers, and a checkout that forbids bypassing holds in a session
/// that was in bypass when it moved there, whatever the person answers about its rules.
#[test]
fn reading_the_rules_of_a_checkout_that_forbids_bypassing_takes_bypass_away() {
    use bravebot_agent::PermissionMode;
    let root = crate::testutil::scratch_dir("bravebot-bypass-unreachable-test");
    let _ = std::fs::remove_dir_all(&root);
    let a = root.join("a");
    let b = root.join("b");
    checkout_with_settings(&a, r#"{"permissions": {}}"#);
    checkout_with_settings(&b, r#"{"permissions": {"bypassUnreachable": true}}"#);

    let workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let mut rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    session.cycle_permission_mode();
    session.cycle_permission_mode();
    session.cycle_permission_mode();
    session.cycle_permission_mode();
    assert_eq!(session.permission_mode(), PermissionMode::Bypass);

    rules.read_for(
        &mut session,
        &b,
        Whence::Asked,
        "the-next-session",
        never_asked,
    );

    assert_eq!(session.permission_mode(), PermissionMode::Ask);
    for _ in 0..4 {
        session.cycle_permission_mode();
    }
    assert_eq!(session.permission_mode(), PermissionMode::Ask);

    std::fs::remove_dir_all(&root).ok();
}

/// The flag opens the session in bypass, and the key walks out of it and back round.
#[test]
fn the_flag_opens_the_session_in_bypass_and_can_be_cycled_out_of() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none").starting_in_bypass();
    assert_eq!(session.permission_mode(), PermissionMode::Bypass);

    // Out of it, round the ladder, and back: the key means the same thing wherever it started.
    for expected in [
        PermissionMode::Ask,
        PermissionMode::AcceptEdits,
        PermissionMode::Plan,
        PermissionMode::Auto,
        PermissionMode::Bypass,
    ] {
        handle_key(&mut session, shift(KeyCode::Tab));
        assert_eq!(session.permission_mode(), expected);
    }
}

/// Most wanted mid-turn, which is when somebody watching a turn edit the wrong files decides it
/// should stop and ask. The handle the running turn holds is the session's own, so the turn
/// reads the mode the key left (MODE-8).
#[test]
fn the_mode_can_be_changed_while_a_turn_runs() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);
    let held_by_the_turn = session.live_permission_mode();

    handle_key(&mut session, shift(KeyCode::Tab));
    assert_eq!(session.permission_mode(), PermissionMode::AcceptEdits);
    assert_eq!(held_by_the_turn.get(), PermissionMode::AcceptEdits);
}

/// Keys that mean nothing here must be ignored rather than mishandled.
#[test]
fn unknown_keys_are_ignored() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, key(KeyCode::F(5))), Action::None);
    assert_eq!(handle_key(&mut session, key(KeyCode::Insert)), Action::None);
}

/// A second submission cannot start while a turn is in flight.
#[test]
fn enter_is_inert_while_working() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    handle_key(&mut session, key(KeyCode::Char('b')));
    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
}

/// A directory the person accepted is opened and vouched for by the same route `/add-dir`
/// takes, so neither has a way in that the other lacks. A relative name means a path under the
/// workspace, which is what `../shared` in a file about a project says.
#[test]
fn an_accepted_directory_is_opened_and_trusted_like_one_typed() {
    let root = crate::testutil::scratch_dir("bravebot-settings-dir-test");
    let outside = root.join("shared");
    let project = root.join("project");
    std::fs::create_dir_all(&outside).expect("scratch");
    std::fs::create_dir_all(&project).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let mut trust = TrustStore::new("/work");

    // Named the way a settings file names it, relative to the project.
    let asked = named_to_open(&mut session, &workspace, &["../shared".to_string()]);
    open_named(&mut session, &mut workspace, &mut trust, &asked);

    // Both halves, since either alone is useless: reach without trust asks about every write
    // there, and trust without reach is a rule about files nothing can open.
    let canonical = outside.canonicalize().expect("canonical");
    assert!(
        workspace.added_directories().contains(&canonical),
        "a directory the person accepted was not opened"
    );
    assert!(
        trust.is_trusted(&canonical.display().to_string()),
        "a directory the person accepted was not vouched for"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// The other half of the answer, asserted where a refusal bites rather than in the list that
/// was asked about: a name nobody accepted leaves its path unreachable and out of the map, so
/// reading or writing there is refused as it is anywhere else outside the workspace.
#[test]
fn a_declined_directory_is_left_unreachable_and_unvouched_for() {
    let root = crate::testutil::scratch_dir("bravebot-declined-dir-test");
    let accepted = root.join("accepted");
    let declined = root.join("declined");
    let project = root.join("project");
    for directory in [&accepted, &declined, &project] {
        std::fs::create_dir_all(directory).expect("scratch");
    }

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let mut trust = TrustStore::new("/work");

    let asked = named_to_open(
        &mut session,
        &workspace,
        &["../accepted".to_string(), "../declined".to_string()],
    );
    assert_eq!(asked.len(), 2, "both names were asked about");
    // One question answered yes, the other no.
    open_named(&mut session, &mut workspace, &mut trust, &asked[..1]);

    let yes = accepted.canonicalize().expect("canonical");
    let no = declined.canonicalize().expect("canonical");
    assert!(workspace.confines(&yes.join("notes.md")).is_ok());
    assert!(
        workspace.confines(&no.join("notes.md")).is_err(),
        "a declined directory is still reachable"
    );
    assert!(
        !trust.is_trusted(&no.display().to_string()),
        "a declined directory is still vouched for"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// What the question shows has to be what accepting it would reach. A name is canonicalized
/// when it is opened, so a settings file naming a link inside the project would otherwise put
/// a path beginning with the person's own project root in front of them and grant the tree at
/// the other end of it.
#[test]
#[cfg(unix)]
fn a_named_directory_is_resolved_before_it_is_asked_about() {
    let root = crate::testutil::scratch_dir("bravebot-named-link-test");
    let outside = root.join("outside");
    let project = root.join("project");
    std::fs::create_dir_all(&outside).expect("scratch");
    std::fs::create_dir_all(&project).expect("scratch");
    std::os::unix::fs::symlink(&outside, project.join("link")).expect("symlink");

    let workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");

    let asked = named_to_open(&mut session, &workspace, &["link".to_string()]);

    assert_eq!(
        asked,
        vec![
            outside
                .canonicalize()
                .expect("canonical")
                .display()
                .to_string()
        ],
        "the question would have shown the name rather than the directory"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// Two layers of settings both naming a directory is one request. A second box for a path
/// already answered is a question whose answer is already known, and answering the same
/// question twice differently is a contradiction nothing could resolve.
#[test]
fn a_directory_two_layers_both_named_is_asked_about_once() {
    let root = crate::testutil::scratch_dir("bravebot-named-twice-test");
    let outside = root.join("shared");
    let project = root.join("project");
    std::fs::create_dir_all(&outside).expect("scratch");
    std::fs::create_dir_all(&project).expect("scratch");

    let workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");

    let both = vec!["../shared".to_string(), outside.display().to_string()];
    let asked = named_to_open(&mut session, &workspace, &both);

    assert_eq!(asked.len(), 1, "the same directory was asked about twice");

    std::fs::remove_dir_all(&root).ok();
}

/// A name that could not be opened whatever the answer is reported rather than put as a
/// question: a directory already inside the project is reachable by its relative path, so a
/// box asking about it collects an answer that changes nothing either way.
#[test]
fn a_name_that_cannot_be_opened_is_said_so_rather_than_asked_about() {
    let root = crate::testutil::scratch_dir("bravebot-named-inside-test");
    let project = root.join("project");
    std::fs::create_dir_all(project.join("docs")).expect("scratch");

    let workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");

    let asked = named_to_open(&mut session, &workspace, &["docs".to_string()]);

    assert!(
        asked.is_empty(),
        "a name that cannot be opened was asked about"
    );
    assert!(
        session.transcript[0].text.contains("could not add"),
        "nothing said why the directory was not opened: {}",
        session.transcript[0].text
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-16: where a layer asked for the file tools to stay inside the workspace, a name in
/// `additionalDirectories` is refused rather than put to anybody, and the refusal names the key.
/// That is PERM-13's existing treatment of a name that cannot be opened whatever the answer, and
/// the whole point of the key is that no answer opens one.
///
/// The failure this rejects is a question asked anyway and refused after it is answered, which is
/// the box that trains answering without reading: the person is shown a decision they do not
/// have.
#[test]
fn a_named_directory_is_refused_rather_than_asked_about_where_reads_stay_in_the_workspace() {
    let root = crate::testutil::scratch_dir("bravebot-named-confined-test");
    let project = root.join("project");
    let outside = root.join("shared");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(&outside).expect("scratch");

    let workspace = Workspace::new(&project)
        .expect("workspace")
        .with_reads_kept_inside(true);
    let mut session = Session::new("none");

    let asked = named_to_open(&mut session, &workspace, &[outside.display().to_string()]);

    assert!(
        asked.is_empty(),
        "a directory the settings keep shut was put to the person"
    );
    assert!(
        session.transcript[0]
            .text
            .contains("permissions.readsStayInWorkspace"),
        "the refusal did not name the key that made it: {}",
        session.transcript[0].text
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-16 at the other door onto the same reach: `/add-dir` is refused with the key named, and
/// nothing is vouched for, where a settings layer asked for the tools to stay inside.
#[test]
fn add_dir_is_refused_where_reads_stay_in_the_workspace() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-confined-test");
    let project = root.join("project");
    let outside = root.join("shared");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(&outside).expect("scratch");

    let mut workspace = Workspace::new(&project)
        .expect("workspace")
        .with_reads_kept_inside(true);
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");

    add_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &outside.display().to_string(),
    );

    assert!(
        workspace.added_directories().is_empty(),
        "a directory the settings keep shut was opened"
    );
    assert!(
        trust.is_empty(),
        "a directory that was not opened was vouched for"
    );
    assert!(
        session.transcript[0]
            .text
            .contains("permissions.readsStayInWorkspace"),
        "the refusal did not name the key that made it: {}",
        session.transcript[0].text
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-11, PERM-15: the list this interface builds holds the entries that are rules and not
/// the ones that are not. A line nothing can act on is reported for what is wrong with it, so
/// offering it in the box would ask somebody to grant a rule that would still decide nothing
/// once they had, and telling them it was withheld sends them to the wrong fix.
///
/// From files rather than from [`bravebot_config::Settings::parse`], because which layer wrote
/// an entry is what puts it in this list at all. The tests in `bravebot_agent::opening` start
/// from a list already built, so this is the one that pins where the interface's own comes from.
#[test]
fn a_checkouts_entry_is_proposed_here_only_where_it_is_a_rule() {
    let root = crate::testutil::scratch_dir("bravebot-app-proposed-rules");
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let cwd = root.join("cwd");
    std::fs::create_dir_all(&home).expect("scratch home");
    std::fs::create_dir_all(cwd.join(".bravebot")).expect("scratch project");
    std::fs::write(
        cwd.join(".bravebot").join("settings.json"),
        r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)", "Nonsense"]}}"#,
    )
    .expect("write the project layer");
    let settings = bravebot_config::Settings::layered(Some(home), Some(&cwd), None);

    let offered: Vec<String> = proposed_rules(&settings)
        .into_iter()
        .map(|rule| rule.rule)
        .collect();
    assert_eq!(
        offered,
        ["Bash(bash scripts/check.sh)"],
        "the entries this session offers are not the ones that are rules"
    );
}

/// Bypassing every permission answers before the kept record is read, so the session says the
/// flag answered, which is what is in force, and never touches the record (MODE-4).
#[test]
fn bypass_answers_before_a_remembered_answer_is_read() {
    use bravebot_agent::PermissionMode;

    match opening_for(
        beginning_of(&Start::Fresh, here()),
        PermissionMode::Bypass,
        here(),
        || panic!("bypass read the kept answer"),
    ) {
        Opening::Settled(_, Whence::Unasked) => {}
        opening => panic!("bypass did not answer: {opening:?}"),
    }
}

/// MODE-4, TRUST-6. A session that had bypass on at any point writes a record whose map and
/// opened directories a resume without the flag does not take back, so the startup question
/// is put; one that never had it keeps its map. The faults it rejects: the record not saying
/// so, and a resume that reads the map back as the person's answer.
#[test]
fn a_resume_without_the_flag_asks_where_bypass_answered_for_the_record() {
    use bravebot_agent::PermissionMode;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-bypass-record");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let written = |mode_during_the_sitting: &[PermissionMode]| {
        let session = Session::new("none");
        for mode in mode_during_the_sitting {
            session.live_permission_mode().set(*mode);
        }
        let trust = crate::trust_prompt::answered_by(PermissionMode::Bypass, &root)
            .expect("bypass answers the question");
        let mut stored =
            sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
        with_standing(
            &session,
            &Conversation::new(),
            &trust,
            &TrustedPrograms::new(),
            &workspace,
            |standing| stored.save("a session", standing),
        );
        sessions::load(&root, stored.id()).expect("the record")
    };

    // Left again before the record is written: the answers it holds were still the mode's.
    for during in [
        &[PermissionMode::Bypass][..],
        &[PermissionMode::Bypass, PermissionMode::Ask],
    ] {
        let record = written(during);
        assert!(record.trust_answered_by_mode, "{during:?}");
        assert!(
            matches!(
                opening_for(
                    beginning_of(&Start::Resuming(Box::new(record.clone())), &root),
                    PermissionMode::Ask,
                    &root,
                    || None,
                ),
                Opening::Ask
            ),
            "a resume without the flag took the map bypass wrote, {during:?}",
        );
        assert!(
            matches!(
                opening_for(
                    beginning_of(&Start::Resuming(Box::new(record)), &root),
                    PermissionMode::Bypass,
                    &root,
                    || None,
                ),
                Opening::Settled(_, Whence::Unasked)
            ),
            "a resume given the flag again was asked",
        );
    }

    let record = written(&[PermissionMode::AcceptEdits]);
    assert!(!record.trust_answered_by_mode);
    assert!(
        matches!(
            opening_for(
                beginning_of(&Start::Resuming(Box::new(record)), &root),
                PermissionMode::Ask,
                &root,
                || None,
            ),
            Opening::Settled(_, Whence::Resumed)
        ),
        "a map a person answered was not taken back on a resume",
    );
}

/// SANDBOX-24, SANDBOX-28: `/clear` and a handoff end the paths programs were let reach and the
/// hosts answered, and begin a record of their own.
#[test]
fn clearing_ends_the_reach_and_the_host_answers_and_begins_a_new_record() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-clear-reach");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, stored, _conversation, _answers) = a_session_to_branch(&root);
    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/in"),
        false,
        "reads in".to_string(),
    );
    workspace.grant_hosts(&["yes.example".to_string()]);
    workspace.decline_hosts(&["no.example".to_string()]);

    let (conversation, fresh) = clear_session(&mut session, &mut workspace, None);

    assert!(
        workspace.path_reach().is_empty(),
        "the cleared session kept a path programs were let reach"
    );
    assert_eq!(
        workspace.unanswered_hosts(&["yes.example".to_string(), "no.example".to_string()]),
        ["yes.example".to_string(), "no.example".to_string()],
        "the cleared session kept a host answer"
    );
    assert_ne!(
        fresh.id(),
        stored.id(),
        "the cleared session kept the record"
    );
    assert!(
        conversation.recounted().is_empty(),
        "the cleared session kept the context"
    );
}

/// PERM-10: a directory a settings file named and the person accepted is closed by `/clear` and
/// by a handoff, since the answer that opened it was given by the session being cleared. The
/// regression it rejects is a directory staying reachable, with no answer left behind it,
/// into a session that was never asked.
#[test]
fn clearing_closes_the_directories_a_file_named_and_opens_none() {
    let root = crate::testutil::scratch_dir("bravebot-clear-named-dir-test");
    let outside = root.join("shared");
    let project = root.join("project");
    std::fs::create_dir_all(&outside).expect("scratch");
    std::fs::create_dir_all(&project).expect("scratch");
    let canonical = outside.canonicalize().expect("canonical");

    for handed_from in [None, Some("an-earlier-id")] {
        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new("/work");
        let asked = named_to_open(&mut session, &workspace, &["../shared".to_string()]);
        open_named(&mut session, &mut workspace, &mut trust, &asked);
        assert!(
            workspace.added_directories().contains(&canonical),
            "the directory was not open to begin with"
        );

        let _ = clear_session(&mut session, &mut workspace, handed_from);

        assert!(
            workspace.added_directories().is_empty(),
            "a directory the cleared session opened stayed open ({handed_from:?}): {:?}",
            workspace.added_directories()
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// A handoff is a clear that links back to the session it came from, and ends the same grants.
#[test]
fn a_handoff_ends_the_reach_and_the_host_answers_too() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-handoff-reach");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, _stored, _conversation, _answers) = a_session_to_branch(&root);
    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/in"),
        true,
        "writes in".to_string(),
    );
    workspace.grant_hosts(&["yes.example".to_string()]);

    let (_conversation, _fresh) =
        clear_session(&mut session, &mut workspace, Some("an-earlier-id"));

    assert!(
        workspace.path_reach().is_empty(),
        "the handoff kept a path programs were let reach"
    );
    assert_eq!(
        workspace.unanswered_hosts(&["yes.example".to_string()]),
        ["yes.example".to_string()],
        "the handoff kept a host answer"
    );
}

/// SANDBOX-24, SANDBOX-28: `/resume` hands on a workspace that holds no path programs were let
/// reach and no host answered. The workspace handed on is a clone that shares them with the
/// one left, so the answer is read from the clone the next session starts with.
#[test]
fn resuming_hands_on_a_workspace_without_the_reach_or_the_host_answers() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-resume-reach");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, stored, _conversation, _answers) = a_session_to_branch(&root);
    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/in"),
        false,
        "reads in".to_string(),
    );
    workspace.grant_hosts(&["yes.example".to_string()]);
    workspace.decline_hosts(&["no.example".to_string()]);
    let record = Box::new(
        bravebot_session::sessions::load(&root, stored.id()).expect("the record to resume"),
    );
    let mut servers = crate::state::Servers::default();

    let switch = switch_session(&mut session, &mut workspace, &mut servers, record);

    assert!(
        switch.workspace.path_reach().is_empty(),
        "the resumed session was handed a path programs were let reach"
    );
    assert_eq!(
        switch
            .workspace
            .unanswered_hosts(&["yes.example".to_string(), "no.example".to_string()]),
        ["yes.example".to_string(), "no.example".to_string()],
        "the resumed session was handed a host answer"
    );
    assert!(
        workspace.path_reach().is_empty(),
        "the session left kept a path programs were let reach"
    );
}

/// PERM-17: the session the terminal client opens on reads `permissions.bypassUnreachable` from
/// the settings it was started with, so the rung is off the ladder the key walks and the flag
/// that opened the session in bypass is put back to asking. The named tests call
/// `open_in_the_mode_asked_for` themselves, so a startup that handed it other settings would
/// leave every one passing. The control opens the same session under settings that say nothing
/// and reaches bypass on the ladder, so the walk cannot pass by never reaching it. Rejects:
/// `new_session` reading something other than the settings it was given for the key.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_session_the_startup_builds_keeps_bypass_off_the_ladder_where_the_settings_say_it_is_unreachable()
 {
    use bravebot_agent::PermissionMode;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-opening-bypass-unreachable");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    // The machine's own managed file is not the layer under test.
    let _machine = bravebot_config::managed_file_on_this_thread(std::path::Path::new(
        "no-managed-file-is-here.json",
    ));

    let opened_under = |settings: &bravebot_config::Settings| {
        new_session(
            &mut a_config_needing_no_sign_in(),
            settings,
            &workspace,
            &AskedFor::default(),
            crate::state::Servers::default(),
            SessionBasis {
                confinement: String::new(),
                sockets_with_a_path_reachable: false,
                sandbox: crate::sandbox_command::Sandboxed::at_start(),
                skip_permissions: true,
                asked_for_auto: false,
                from_link: false,
                prompts: bravebot_agent::turn::SystemPrompts::default(),
            },
        )
    };
    let modes_the_key_walks = |session: &mut Session| {
        (0..12)
            .map(|_| {
                session.cycle_permission_mode();
                session.permission_mode()
            })
            .collect::<Vec<_>>()
    };

    let mut unconstrained = opened_under(&bravebot_config::Settings::default());
    assert_eq!(
        unconstrained.permission_mode(),
        PermissionMode::Bypass,
        "the flag did not open the control session in bypass"
    );
    assert!(
        modes_the_key_walks(&mut unconstrained).contains(&PermissionMode::Bypass),
        "bypass is not on the ladder of a session whose settings say nothing"
    );

    let mut refused = opened_under(&bravebot_config::Settings::parse(
        r#"{"permissions": {"bypassUnreachable": true}}"#,
    ));
    assert_eq!(
        refused.permission_mode(),
        PermissionMode::Ask,
        "the flag left the session in bypass where the settings made it unreachable"
    );
    let walked = modes_the_key_walks(&mut refused);
    assert!(
        !walked.contains(&PermissionMode::Bypass),
        "the key reached bypass where the settings made it unreachable: {walked:?}"
    );
}

/// PERM-17: the file an administrator pinned makes bypass unreachable for a session that opens
/// with settings saying nothing, since the startup takes the strictest of the settings and that
/// file. `Settings::under` does not carry the key, so a read of the settings alone leaves the
/// rung on the ladder. The control opens the same session with no managed file and one press
/// of the key reaches bypass from auto, where the managed file puts it back to asking.
/// Rejects: a startup that reads the settings layers for the key and not the managed file.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_managed_file_that_makes_bypass_unreachable_keeps_it_off_the_ladder_of_a_new_session() {
    use bravebot_agent::PermissionMode;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-opening-managed-bypass");
    std::fs::create_dir_all(&root).expect("create");
    let managed_file = root.join("managed.json");
    std::fs::write(
        &managed_file,
        r#"{"permissions": {"bypassUnreachable": true}}"#,
    )
    .expect("write the managed file");
    let settings = bravebot_config::Settings::default();
    let after_one_press_from_auto = || {
        let mut session = open_in_the_mode_asked_for(
            Session::new("none"),
            false,
            true,
            false,
            &bravebot_config::Settings::default(),
        );
        assert_eq!(
            session.permission_mode(),
            PermissionMode::Auto,
            "the line did not open the session in auto"
        );
        session.cycle_permission_mode();
        session.permission_mode()
    };

    {
        let _machine = bravebot_config::managed_file_on_this_thread(std::path::Path::new(
            "no-managed-file-is-here.json",
        ));
        assert!(
            !bypass_is_unreachable(&settings),
            "the control has a layer that makes bypass unreachable"
        );
        assert_eq!(
            after_one_press_from_auto(),
            PermissionMode::Bypass,
            "bypass is not the rung after auto where nothing makes it unreachable"
        );
    }

    let _machine = bravebot_config::managed_file_on_this_thread(&managed_file);
    assert!(
        bypass_is_unreachable(&settings),
        "the managed file's bypassUnreachable was not read"
    );
    assert_eq!(
        after_one_press_from_auto(),
        PermissionMode::Ask,
        "a session opened under the managed file reached bypass by the key"
    );
}

/// A checkout at `root` whose own settings file says `body`.
fn checkout_with_settings(root: &std::path::Path, body: &str) {
    std::fs::create_dir_all(root.join(".bravebot")).expect("scratch");
    std::fs::write(root.join(".bravebot").join("settings.json"), body)
        .expect("write the project layer");
}

/// PERM-19: the rules read again after `/cd` carry the administrator's `deny`, not only the
/// checkout's layers. A reload that left the managed file out would let the move lift it.
#[test]
fn a_reload_after_moving_carries_the_managed_deny_rules() {
    let root = crate::testutil::scratch_dir("bravebot-cd-managed-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    let file = root.join("managed.json");
    std::fs::write(&file, r#"{"permissions": {"deny": ["Bash(ls)"]}}"#).expect("managed file");
    let sources = RuleSources {
        state: Some(root.join("state")),
        named: None,
        managed: bravebot_config::Managed::at(&file),
    };
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("scratch");
    assert_eq!(
        sources.settings(&elsewhere).permissions().deny,
        ["Bash(ls)"]
    );
}

/// PERM-15 and TRUST-13: a rule granted for one checkout's file answers nothing in the checkout
/// `/cd` moves to. Kept, it runs that checkout's `scripts/check.sh` with no prompt, on an answer
/// the person gave about another file (#843).
#[test]
fn a_rule_granted_in_one_checkout_is_not_in_force_after_moving_to_another() {
    use bravebot_core::permissions::{Decision, Ruling};
    let root = crate::testutil::scratch_dir("bravebot-cd-granted-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b) = (root.join("a"), root.join("b"));
    checkout_with_settings(&a, CHECK);
    std::fs::create_dir_all(&b).expect("scratch");

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["bash", "scripts/check.sh"]),
        Decision::Ruled(Ruling::Allow),
        "the rule was not granted where it was proposed"
    );

    let mut asked = 0;
    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            |_, _| {
                asked += 1;
                Some(true)
            },
        ),
        Changed::Moved
    );

    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["bash", "scripts/check.sh"]),
        Decision::Unmatched,
        "the grant for the checkout left behind still answers the run prompt"
    );
    assert_eq!(
        asked, 0,
        "a question was put about a checkout that proposes nothing"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-15: the checkout moved to has its own settings read, so a `deny` rule it writes is in
/// force from the first turn there, as it is for a session started in it.
#[test]
fn moving_to_a_checkout_puts_its_own_deny_rules_in_force() {
    use bravebot_core::permissions::{Decision, Ruling};
    let root = crate::testutil::scratch_dir("bravebot-cd-deny-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&a).expect("scratch");
    checkout_with_settings(&b, r#"{"permissions": {"deny": ["Bash(rm -rf build)"]}}"#);

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["rm", "-rf", "build"]),
        Decision::Unmatched
    );

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["rm", "-rf", "build"]),
        Decision::Ruled(Ruling::Deny),
        "the deny rule of the checkout moved to is not in force"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-15: what the checkout moved to proposes is put to the person, naming that checkout's
/// file, and accepting it is recorded for that checkout. The same text granted for the one left
/// behind was a grant for a different file, so it does not stand in for this answer.
#[test]
fn a_rule_the_checkout_moved_to_proposes_is_asked_about_and_granted_there() {
    use bravebot_core::permissions::{Decision, Ruling};
    let root = crate::testutil::scratch_dir("bravebot-cd-proposed-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b, state) = (root.join("a"), root.join("b"), root.join("state"));
    checkout_with_settings(&a, CHECK);
    checkout_with_settings(&b, CHECK);

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &state, workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);

    let mut offered = Vec::new();
    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            |asking, _| {
                offered = asking.to_vec();
                Some(true)
            },
        ),
        Changed::Moved
    );

    let file = workspace.root().join(".bravebot").join("settings.json");
    assert_eq!(
        offered,
        [bravebot_agent::granted::Proposed::new(
            &file,
            "Bash(bash scripts/check.sh)"
        )],
        "the question did not name the rule the checkout moved to proposed"
    );
    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["bash", "scripts/check.sh"]),
        Decision::Ruled(Ruling::Allow),
        "the rule accepted after the move is not in force"
    );
    assert_eq!(
        bravebot_agent::granted::Store::new(&state, workspace.root())
            .granted(&offered)
            .len(),
        1,
        "the grant was not recorded for the checkout it was given in"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-15: leaving at the rules question after a move leaves the session, as leaving at it
/// when the session opened does.
#[test]
fn leaving_at_the_rules_question_after_moving_leaves_the_session() {
    let root = crate::testutil::scratch_dir("bravebot-cd-leave-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&a).expect("scratch");
    checkout_with_settings(&b, CHECK);

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            |_, _| None,
        ),
        Changed::Left
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-15: the mode that answers every question answers the one a move would put, on the terms
/// it answers at startup: in force for the session, asked of nobody, recorded nowhere.
#[test]
fn bypassing_grants_what_the_checkout_moved_to_proposes_without_asking() {
    use bravebot_core::permissions::{Decision, Ruling};
    let root = crate::testutil::scratch_dir("bravebot-cd-bypass-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b, state) = (root.join("a"), root.join("b"), root.join("state"));
    std::fs::create_dir_all(&a).expect("scratch");
    checkout_with_settings(&b, CHECK);

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none").starting_in_bypass();
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &state, workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["bash", "scripts/check.sh"]),
        Decision::Ruled(Ruling::Allow),
        "the mode did not grant what the checkout moved to proposed"
    );
    let file = workspace.root().join(".bravebot").join("settings.json");
    assert!(
        bravebot_agent::granted::Store::new(&state, workspace.root())
            .granted(&[bravebot_agent::granted::Proposed::new(
                &file,
                "Bash(bash scripts/check.sh)"
            )])
            .is_empty(),
        "a grant nobody was asked for was written down"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-14: a file `--settings` named is the person's own wherever it sits, so one inside the
/// directory the session opened in grants its rule there and after `/cd` leaves it where the
/// platform holds the file back from a stage, and nothing is put to the person about it.
/// Elsewhere it is read as a checkout's file and grants nothing.
#[test]
fn a_named_file_inside_the_directory_left_behind_still_grants_after_moving() {
    use bravebot_core::permissions::{Decision, Ruling};
    let root = crate::testutil::scratch_dir("bravebot-cd-named-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&a).expect("scratch");
    std::fs::create_dir_all(&b).expect("scratch");
    let tooling = a.join("tooling.json");
    std::fs::write(&tooling, CHECK).expect("the named file");

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules_naming(
        &mut session,
        &root.join("state"),
        workspace.root(),
        Some(&tooling),
    );
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    let first = answers
        .rules
        .permissions
        .for_command(&["bash", "scripts/check.sh"]);
    if bravebot_sandbox::holds_one_file_back() {
        assert_eq!(
            first,
            Decision::Ruled(Ruling::Allow),
            "a named file inside the working directory was withheld"
        );
    } else {
        // Linux and Windows cannot hold the file back from a stage that may write `a`, so it
        // is read as a checkout's file there and its allow rule is only proposed (#2599). The
        // helper answers yes to a proposal, so the decision is `Allow` here too and the
        // withholding is read from the settings.
        assert!(
            answers
                .rules
                .sources
                .settings(workspace.root())
                .named_withheld()
                .is_some(),
            "a named file inside the working directory was not withheld where it cannot be held back"
        );
    }
    let mut offered = Vec::new();
    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            |asking, _| {
                offered = asking.to_vec();
                Some(false)
            },
        ),
        Changed::Moved
    );

    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["bash", "scripts/check.sh"]),
        Decision::Ruled(Ruling::Allow),
        "moving out of the directory withheld the named file's rule"
    );
    assert!(
        offered.is_empty(),
        "the named file was put to the person as a checkout's"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PERM-15: reading the rules again where the session is, as `/clear` does, asks nothing about
/// a rule the person granted there and puts in force what the file says now.
#[test]
fn reading_the_rules_again_in_the_same_checkout_asks_nothing_granted_and_reads_the_file_anew() {
    use bravebot_core::permissions::{Decision, Ruling};
    let root = crate::testutil::scratch_dir("bravebot-reread-test");
    let _ = std::fs::remove_dir_all(&root);
    let a = root.join("a");
    checkout_with_settings(&a, CHECK);

    let workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    let mut rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    checkout_with_settings(
        &a,
        r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"], "deny": ["Bash(rm -rf build)"]}}"#,
    );

    assert!(rules.read_for(
        &mut session,
        workspace.root(),
        Whence::Asked,
        "the-next-session",
        never_asked,
    ));
    assert_eq!(
        rules.permissions.for_command(&["bash", "scripts/check.sh"]),
        Decision::Ruled(Ruling::Allow),
        "the rule granted here is no longer in force"
    );
    assert_eq!(
        rules.permissions.for_command(&["rm", "-rf", "build"]),
        Decision::Ruled(Ruling::Deny),
        "the file was not read again"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// PROMPT-1: the lines said about an allow rule a checkout proposed name the rule and the file
/// as they are spelled, both when it is not granted and when it was granted before, so a rule
/// that holds a zero-width character, and a file under a directory that ends in a space, are not
/// read as the ones without them. The record a granted-before line names is spelled too.
#[test]
fn the_notes_about_a_checkouts_allow_rule_spell_the_rule_and_the_files() {
    let root = crate::testutil::scratch_dir("bravebot-allow-notes-spelled");
    let _ = std::fs::remove_dir_all(&root);
    let checkout = root.join("proj ");
    checkout_with_settings(
        &checkout,
        r#"{"permissions": {"allow": ["Bash(echo\u200bhi)"]}}"#,
    );
    let state = root.join("state ");
    let sources = RuleSources {
        state: Some(state.clone()),
        named: None,
        managed: bravebot_config::Managed::default(),
    };
    let settings = sources.settings(&checkout);
    let rules_with =
        |ask: fn(&[bravebot_agent::granted::Proposed], &mut String) -> Option<bool>| {
            let mut session = Session::new("none");
            rules_from(
                &mut session,
                &settings,
                &checkout,
                sources.grants(&checkout).as_ref(),
                Whence::Asked,
                "a-session",
                ask,
            )
            .expect("nobody left");
            session
        };
    let spelled_rule = "\"Bash(echo\\u{200b}hi)\"";
    let spelled_file = format!(
        "\"{}/proj\u{2423}/.bravebot/settings.json\"",
        root.display()
    );

    let ignored = rules_with(|_, _| Some(false));
    let note = notes_of(&ignored).join("\n");
    assert!(
        note.contains(&format!(
            "not granting the allow rule {spelled_rule} from {spelled_file}"
        )),
        "{note}"
    );
    assert!(!note.contains('\u{200b}'), "{note}");

    rules_with(|_, _| Some(true));
    let before = rules_with(never_asked);
    let note = notes_of(&before).join("\n");
    let spelled_record = format!("\"{}/state\u{2423}/", root.display());
    assert!(
        note.contains(&format!("granting the allow rule {spelled_rule} from {spelled_file}, which you allowed this project before")),
        "{note}"
    );
    assert!(note.contains(&spelled_record), "{note}");
    assert!(
        !note.contains('\u{200b}') && !note.contains("proj /"),
        "{note}"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// #2599: a session opened with a `--settings` file that grants nothing, because the platform
/// cannot hold it back from a directory a stage may write, says so and names the file. Where
/// the platform can hold it back the session says nothing about it. Rejects an opening that
/// leaves the withholding unsaid, or says it for a file that grants.
#[test]
fn the_opening_note_says_why_a_named_file_in_a_writable_directory_grants_nothing() {
    let root = crate::testutil::scratch_dir("bravebot-named-withheld-note");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    let root = std::fs::canonicalize(&root).expect("canonical scratch");
    let file = root.join("override.json");
    std::fs::write(&file, r#"{"sandbox": {"mode": "off"}}"#).expect("named file");
    let opened = |can_hold_one_file_back: bool| {
        let settings = bravebot_config::Settings::layered_reaching(
            None,
            Some(&root),
            Some(&file),
            std::slice::from_ref(&root),
            can_hold_one_file_back,
        );
        let mut session = Session::new("none");
        rules_from(
            &mut session,
            &settings,
            &root,
            None,
            Whence::Asked,
            "a-session",
            |_, _| Some(false),
        )
        .expect("nobody left");
        notes_of(&session).join("\n")
    };
    let withheld = opened(false);
    let held_back = opened(true);
    std::fs::remove_dir_all(&root).ok();

    assert!(withheld.contains("override.json"), "{withheld}");
    assert!(withheld.contains("--settings"), "{withheld}");
    assert!(
        withheld.contains("read as a checkout's file is"),
        "{withheld}"
    );
    assert!(!held_back.contains("override.json"), "{held_back}");
}

/// An answer to show a file that holds a credential is kept under the name the file has from
/// the root. After a move that name is another file, so the answer must not cover it.
#[test]
fn an_answer_to_show_a_file_does_not_cover_the_file_by_that_name_after_moving() {
    let root = crate::testutil::scratch_dir("bravebot-cd-exposed-test");
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(project.join("src")).expect("scratch");
    std::fs::create_dir_all(&other).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);

    answers.exposed.allow(".env");
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
        !answers.exposed.holds(".env"),
        "a yes for the root's .env covered src/.env after moving into src"
    );

    answers.exposed.allow(".env");
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
        !answers.exposed.holds(".env"),
        "a yes for one checkout's .env covered another checkout's"
    );

    std::fs::remove_dir_all(&root).ok();
}
