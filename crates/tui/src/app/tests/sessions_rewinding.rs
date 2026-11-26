//! Rewinding, branching, handing off and undoing a turn.

use super::*;

/// The goal is the person's, goes to the request beside the conversation and is never sent as a
/// prompt. A word that merely starts with the command's is still a prompt.
#[test]
fn the_handoff_command_carries_its_goal_and_is_not_sent_as_a_prompt() {
    let mut session = Session::new("none");
    for c in "/handoff  port the parser, then  the tests".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Handoff("port the parser, then  the tests".to_string())
    );
    assert!(
        session.transcript.is_empty(),
        "the goal was sent as a prompt"
    );

    for c in "/handoffs the parser".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/handoffs the parser".to_string())
    );
}

/// Enter on the brief starts a new session from what is in the box now, edits included, and
/// does not send it into this one.
#[test]
fn enter_on_a_proposed_brief_starts_a_session_from_the_edited_text() {
    let mut session = Session::new("none");
    session.propose_handoff("the brief");
    for c in " and an edit".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::HandOver("the brief and an edit".to_string())
    );
    assert!(session.input().is_empty(), "the brief stayed in the box");
    assert!(
        session.transcript.is_empty(),
        "the brief joined this session"
    );
}

/// The box emptied and typed into again is a new line. A flag that outlived the brief would make
/// the next prompt somebody wrote start a session they did not ask for.
#[test]
fn a_prompt_typed_after_the_brief_was_wiped_out_is_sent_as_a_prompt() {
    let mut session = Session::new("none");
    // Wiped by backspacing, which edits the line in place, and by Escape, which replaces it.
    session.propose_handoff("the brief");
    for _ in 0.."the brief".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    assert!(session.input().is_empty());
    for c in "something else".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("something else".to_string())
    );

    let mut session = Session::new("none");
    session.propose_handoff("the brief");
    handle_key(&mut session, key(KeyCode::Esc));
    for c in "and another".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("and another".to_string())
    );
}

/// A paste into a box the brief was wiped out of is a new line, as a typed character is. A flag
/// that outlived the brief would make the pasted prompt start a session nobody asked for. The
/// handoff is ended before a paste as it is before a key.
#[test]
fn a_prompt_pasted_after_the_brief_was_wiped_out_is_sent_as_a_prompt() {
    let mut session = Session::new("none");
    session.propose_handoff("the brief");
    for _ in 0.."the brief".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    assert!(session.input().is_empty());

    handle_paste(&mut session, "something else");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("something else".to_string())
    );
}

/// SESSION-34: the first prompt of the session a handoff starts is sent as a line the person
/// typed, because they read and approved it. The same words typed and sent at rest are the
/// comparison: the action, the transcript's line, the turn and the history Up recalls from must
/// each be what the typed line leaves. Rejects a start that begins the turn without going
/// through the path a typed line takes, which sends the brief but leaves it out of the history
/// the person recalls from, and one that draws it as another speaker's.
#[test]
fn the_brief_is_sent_as_a_line_the_person_typed_and_is_recalled_as_one() {
    let mut typed = Session::new("none");
    type_line(&mut typed, "the brief");
    let typed_sends = handle_key(&mut typed, key(KeyCode::Enter));

    let mut handed = Session::new("none");
    handed.hand_over("the brief".to_string());
    let handed_sends = queued_next(&mut handed);

    assert_eq!(typed_sends, Action::Submit("the brief".to_string()));
    assert_eq!(
        handed_sends,
        Some(typed_sends),
        "the brief is not sent as the action a typed line is"
    );
    assert_eq!(handed.status, typed.status, "the turn did not begin alike");
    assert_eq!(handed.turns, typed.turns);
    assert_eq!(handed.transcript.len(), typed.transcript.len());
    let drawn = |session: &Session| {
        session
            .transcript
            .last()
            .map(|entry| (entry.speaker, entry.text.clone()))
    };
    assert_eq!(
        drawn(&handed),
        Some((crate::state::Speaker::User, "the brief".to_string())),
        "the brief is not in the transcript as a line the person typed"
    );
    assert_eq!(drawn(&handed), drawn(&typed));

    // Recalled with Up once the turn is over, as a line they sent is.
    for session in [&mut typed, &mut handed] {
        session.complete("done", Vec::new(), 0);
        assert!(session.input().is_empty());
        handle_key(session, key(KeyCode::Up));
    }
    assert_eq!(typed.input(), "the brief");
    assert_eq!(
        handed.input(),
        "the brief",
        "the brief is not among the lines the person can recall"
    );
}

/// Each way `/handoff` is refused says so and asks for nothing. Dropping the goal check, the
/// turn check or the record check would send a request for a brief that has nothing to link
/// back to, or nothing to be written for.
#[test]
fn a_handoff_with_no_goal_or_no_ended_turn_is_refused_with_a_line_saying_why() {
    assert_eq!(
        handoff_refusal("", 3, true),
        Some(t!(handoff_needs_a_goal)),
        "no goal was not refused as such"
    );
    assert_eq!(
        handoff_refusal("port the parser", 0, true),
        Some(t!(handoff_nothing_to_hand_off)),
        "a session whose first turn has not ended was not refused"
    );
    assert_eq!(
        handoff_refusal("port the parser", 3, false),
        Some(t!(handoff_nothing_to_hand_off)),
        "a session with no record to link back to was not refused"
    );
    assert_eq!(
        handoff_refusal("", 0, false),
        Some(t!(handoff_needs_a_goal)),
        "the missing goal is the first thing said"
    );
    assert_eq!(
        handoff_refusal("port the parser", 3, true),
        None,
        "a goal, an ended turn and a record were refused"
    );
}

/// A brief the planner could not have held (`kept` is `None`) is never put in the box, however
/// readable it is, because the person reading it is not a gate. Removing the `kept` guard would
/// offer text the exchange's untrusted content may have shaped.
#[test]
fn a_brief_from_an_exchange_that_met_something_untrusted_is_not_offered() {
    let mut session = Session::new("none");
    session.begin_aside();
    handoff_answered(
        &mut session,
        "port the parser",
        Ok(bravebot_agent::aside::Answered {
            shown: "a brief an attacker may have shaped".to_string(),
            kept: None,
            usage: Default::default(),
        }),
    );

    assert!(
        session.input().is_empty(),
        "the brief was put in the box: {:?}",
        session.input()
    );
    assert!(!session.handoff_pending(), "Enter would start a session");
    assert_eq!(last_note(&session), t!(handoff_untrusted));
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| !entry.text.contains("an attacker may have shaped")),
        "the withheld brief was written to the transcript"
    );
}

/// The same request over an exchange that held nothing untrusted does put the brief in the box,
/// so the refusal above is the labels' doing and not a brief that is never offered. A failed
/// request offers nothing and says it failed.
#[test]
fn a_brief_the_planner_could_have_held_is_offered_and_a_failed_request_offers_nothing() {
    let mut session = Session::new("none");
    session.begin_aside();
    handoff_answered(
        &mut session,
        "port the parser",
        Ok(bravebot_agent::aside::Answered {
            shown: "where things stand".to_string(),
            kept: Some("where things stand".to_string()),
            usage: Default::default(),
        }),
    );
    assert!(session.handoff_pending(), "the brief was not offered");
    assert!(session.input().contains("where things stand"));
    assert_eq!(last_note(&session), t!(handoff_ready));

    let mut session = Session::new("none");
    session.begin_aside();
    handoff_answered(
        &mut session,
        "port the parser",
        Err(Unanswered::Failed("offline".to_string())),
    );
    assert!(!session.handoff_pending());
    assert!(session.input().is_empty());
    assert_eq!(last_note(&session), t!(handoff_failed, problem = "offline"));
}

/// Renaming gives up every point a rewind could go back to (SESSION-19), and mid-turn that
/// includes the one the running turn opened: each holds the old name, and a rewind to it would
/// put that name back.
#[test]
fn a_session_renamed_mid_turn_gives_up_the_running_turns_rewind_point() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_running_on("first");
    session.rewind.open(a_point_before(0), "first".to_string());

    nothing_beside(|beside| {
        typed_during_a_turn(
            &mut session,
            "/rename deploy watch",
            key(KeyCode::Enter),
            beside,
        )
    });

    assert!(
        session.rewind.points().is_empty(),
        "a point holding the old name outlived the rename"
    );
}

/// A rewind point read off the live session lands inside the turn it undoes: by the time the
/// loop has a prompt in hand it is on the transcript and counted as a turn, so rewinding to
/// such a point would leave the prompt in the scrollback and number the next turn one too high.
#[test]
fn a_rewind_point_excludes_the_turn_it_undoes() {
    let root = crate::testutil::scratch_dir("bravebot-app-rewind-point");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create");

    let mut session = Session::new("none");
    let conversation = Conversation::new();
    let trust = TrustStore::new("/work");
    let programs = TrustedPrograms::new();
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    type_line(&mut session, "delete the tests");
    session.submit().expect("the prompt is sent");

    let first = rewind_point(&session, &conversation, &trust, &programs, &stored);
    assert_eq!(
        first.turns, 0,
        "the first turn rewinds to a session with no turns in it"
    );
    assert_eq!(
        first.transcript_len, 0,
        "the prompt is the turn's, and goes back with it"
    );
    assert!(
        !first.was_wrote,
        "nothing was written before the first turn"
    );

    session.complete("nothing was deleted", Vec::new(), 10);
    type_line(&mut session, "delete the docs");
    session.submit().expect("the prompt is sent");

    let second = rewind_point(&session, &conversation, &trust, &programs, &stored);
    assert_eq!(second.turns, 1, "the turn that finished stays");
    assert_eq!(
        second.transcript_len, 2,
        "the finished turn's prompt and reply stay"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// SESSION-31: the session moves onto a marked copy of its record and says where the original
/// is. The goal and the loop, which no record holds, end with the original; the rewind points,
/// which a fork inherits none of, go too. The original's record is left as it was.
#[test]
fn branching_a_session_moves_onto_the_copy_and_leaves_the_original() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    let prompt = "write a line saying hello into notes.txt";

    let original = stored.id().to_string();
    let original_path = sessions::project_directory(&root)
        .expect("the store")
        .join(format!("{original}.json"));
    let before = std::fs::read(&original_path).expect("the original's record");

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert_ne!(stored.id(), original, "the session stayed on the original");
    assert_eq!(
        stored.title(),
        format!("{} (fork)", sessions::title_from(prompt))
    );
    assert!(session.goal().is_none(), "a goal carried over to the copy");
    assert!(
        session.looping().is_none(),
        "a loop carried over to the copy"
    );
    assert!(
        session.rewind.points().is_empty(),
        "the copy kept the original's rewind points"
    );
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(
        said.contains(&format!("bravebot --resume {original}")),
        "the original's id is not in {said:?}"
    );
    let copy = sessions::load(&root, stored.id()).expect("the copy's record");
    assert_eq!(copy.turns, session.turns);
    assert_eq!(
        std::fs::read(&original_path).expect("the original's record"),
        before,
        "the original's record changed"
    );
}

/// SESSION-31: the copy holds what `--resume` of it would. The trust map and the programs
/// vouched for are in the record, so they stay; the language servers, the run prompts drawn,
/// the files agreed to be shown, the paths programs were let reach, the hosts answered and the
/// answers to the planner's questions are in none, so they are asked about again.
#[test]
fn a_branch_keeps_what_the_record_holds_and_forgets_the_rest() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-answers");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    give_answers(&mut answers, &root);
    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/in"),
        false,
        "reads in".to_string(),
    );
    workspace.grant_hosts(&["yes.example".to_string()]);
    workspace.decline_hosts(&["no.example".to_string()]);
    session.answers.remember(
        "pick one: Cache: Which?".into(),
        bravebot_core::ask::Answer::Chosen(vec![0]),
    );
    let programs = answers.programs.clone();

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert_eq!(
        answers.programs, programs,
        "the copy forgot a program vouched for"
    );
    assert!(
        answers.servers.is_none(),
        "the copy kept the servers approved"
    );
    assert_eq!(
        answers.asked_about,
        AskedAbout::new(),
        "the copy kept the run prompts drawn"
    );
    assert!(
        !answers.exposed.holds(".env"),
        "the copy kept a file agreed to be shown"
    );
    assert!(
        workspace.path_reach().is_empty(),
        "the copy kept a path programs were let reach"
    );
    assert_eq!(
        workspace.unanswered_hosts(&["yes.example".to_string(), "no.example".to_string()]),
        ["yes.example".to_string(), "no.example".to_string()],
        "the copy kept a host answer"
    );
    assert_eq!(
        session.answers.recall("pick one: Cache: Which?"),
        None,
        "the copy kept an answer to the planner's question"
    );
}

/// SESSION-34: the record a handoff starts holds the id of the session it came from, under an id
/// of its own, and a plain clear holds no link. The regressions it rejects are a handoff that
/// drops the link, one that reuses the source's id (overwriting the record the clause says is
/// still resumable), and a clear that links to something.
#[test]
fn a_handoff_links_the_new_record_to_the_session_it_came_from_and_a_clear_does_not() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-handoff-link");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, source, _conversation, _answers) = a_session_to_branch(&root);
    assert_eq!(source.handed_off_from(), None, "the source started linked");

    let (conversation, handed) = clear_session(&mut session, &mut workspace, Some(source.id()));

    assert_eq!(
        handed.handed_off_from(),
        Some(source.id()),
        "the new record does not hold the session it came from"
    );
    assert_ne!(
        handed.id(),
        source.id(),
        "the handoff reused the source's id"
    );
    assert!(
        conversation.recounted().is_empty(),
        "the handoff carried the conversation"
    );
    assert_eq!(
        last_note(&session),
        t!(handoff_started, id = source.id()),
        "the handoff did not say which session it came from"
    );

    let (_conversation, cleared) = clear_session(&mut session, &mut workspace, None);

    assert_eq!(
        cleared.handed_off_from(),
        None,
        "a plain clear linked the new record to a session"
    );
    assert_eq!(last_note(&session), t!(session_cleared));
}

/// SESSION-34: the record a handoff came from is not touched by the new session and can still
/// be resumed. The new session's first save is written, and the source is read back from disk
/// and compared with what it held before the handoff, field for field; the new record must
/// then be there under an id of its own, holding the link and none of the source's turns.
/// Rejects a handoff whose new record is begun under the source's id, which the first save
/// then writes over the record the clause says stays resumable.
#[test]
fn the_record_a_handoff_came_from_is_untouched_by_the_new_one_and_still_resumes() {
    use bravebot_session::sessions::{self, Standing};

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-handoff-source");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, source, _conversation, answers) = a_session_to_branch(&root);
    let before = sessions::load(&root, source.id()).expect("the source's record");
    assert_eq!(before.turns, 1, "the source did not start with its turn");
    assert_eq!(
        source.to_resume().map(|one| one.id),
        Some(source.id().to_string()),
        "the source was not resumable to begin with"
    );

    let (conversation, mut handed) = clear_session(&mut session, &mut workspace, Some(source.id()));
    handed.save(
        "the brief",
        Standing {
            history: Some(session.turn_history()),
            conversation: &conversation.snapshot(),
            turns: session.turns,
            tokens: session.tokens,
            split: Default::default(),
            spend: session.spend_by_turn(),
            timing: session.timing_by_turn(),
            model: None,
            todos: &session.todos_by_turn(),
            asides: &[],
            trust: &answers.trust,
            programs: &answers.programs,
            directories: &[],
            manifest: None,
            rewind: session.rewind.points(),
            checkouts: &[],
            touched: None,
            answered_by_mode: false,
        },
    );

    let after = sessions::load(&root, source.id()).expect("the source's record is gone");
    assert_eq!(
        serde_json::to_string(&after).expect("serialise"),
        serde_json::to_string(&before).expect("serialise"),
        "the handoff changed the record it came from"
    );
    assert_eq!(after.handed_off_from, None, "the source was linked");
    assert_eq!(
        source.to_resume().map(|one| one.id),
        Some(source.id().to_string()),
        "the source is no longer resumable"
    );
    let written = sessions::load(&root, handed.id()).expect("the new record was not written");
    assert_eq!(written.handed_off_from.as_deref(), Some(source.id()));
    assert_eq!(
        written.turns, 0,
        "the new record carries the source's turns"
    );
    assert_ne!(
        handed.id(),
        source.id(),
        "the handoff reused the source's id"
    );
}

/// What `Held::clear` is given to hand a session off, with the answers the session had been
/// given and the untrusted content it had met: a distrusted directory, a program vouched for, a
/// file agreed to be shown, a path reached, and a conversation that observed untrusted bytes.
fn a_session_with_answers_to_hand_off(
    root: &std::path::Path,
) -> (
    Workspace,
    Session,
    bravebot_session::sessions::Handle,
    Conversation,
    Answers,
) {
    let (workspace, session, stored, mut conversation, mut answers) = a_session_to_branch(root);
    answers.trust.distrust("vendor");
    give_answers(&mut answers, root);
    workspace.grant_path_reach(
        std::path::PathBuf::from("/data/in"),
        false,
        "reads in".to_string(),
    );
    conversation.observed(bravebot_core::label::Label::untrusted_public());
    assert_eq!(
        conversation.context(),
        bravebot_core::label::Integrity::Untrusted
    );
    (workspace, session, stored, conversation, answers)
}

/// SESSION-34: the session a handoff begins carries only the brief. The trust map, the programs
/// vouched for, the files agreed to be shown, the paths reached and the quarantine (the
/// untrusted label of the conversation) do not cross, and the next action is the brief sent as
/// a prompt. Goes through `Held::clear`, the entry point `/handoff` takes.
///
/// Rejects: a `clear` that keeps the trust map, the programs or the old untrusted label, and
/// one that drops the first prompt.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_handoff_through_held_clear_carries_only_the_brief() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-handoff-carries");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_with_answers_to_hand_off(&root);
    assert!(
        answers.trust.rules().any(|(path, _)| path == "vendor"),
        "the fixture's distrust was not recorded"
    );
    bypassing(&mut session);
    // The fixture's loop sent its first tick, so the session is working. A handoff is made at
    // rest, where the next pass of the loop sends the brief.
    session.status = crate::state::Status::Idle;
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    let mut terminal = a_fixed_viewport_terminal();

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
    .clear(&mut terminal, Some("carry on with the notes".to_string()));

    assert!(ended.is_none(), "the handoff ended the session: {ended:?}");
    assert!(
        session.looping().is_none(),
        "the loop of the source session crossed"
    );
    assert!(
        !answers.trust.rules().any(|(path, _)| path == "vendor"),
        "the trust map of the source session crossed"
    );
    assert_eq!(
        answers.programs,
        TrustedPrograms::new(),
        "a program vouched for crossed"
    );
    assert!(
        !answers.exposed.holds(".env"),
        "a file agreed to be shown crossed"
    );
    assert!(workspace.path_reach().is_empty(), "a path reached crossed");
    assert_eq!(
        conversation.context(),
        bravebot_core::label::Integrity::Trusted,
        "the quarantine of the source crossed"
    );
    assert!(
        conversation.recounted().is_empty(),
        "the conversation crossed"
    );
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Submit("carry on with the notes".to_string())),
        "the brief was not the next prompt"
    );
}

/// SESSION-34: the standing permissions of the session a handoff leaves do not cross. A rule
/// that allows a command and one that denies another, neither written in any settings layer
/// the new session reads (as a rule a person's yes put in force is not), are in force before
/// `Held::clear` and decide nothing after it, where the new session holds the rules read again
/// for its root. The rules are read from `Held::clear`'s own answers, the entry point
/// `/handoff` takes.
///
/// Rejects: a `clear` that leaves the `Rules` of the source session in place (the new session
/// would run a command the old one was told yes to, with no question), and one that keeps
/// only the deny rules or only the allow rules.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_handoff_through_held_clear_carries_no_standing_rule() {
    use bravebot_core::permissions::{Anchors, Decision, Permissions, Ruling};

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-handoff-rules");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_with_answers_to_hand_off(&root);
    let (standing, rejected) = Permissions::parse(
        &["Bash(rm -rf build)".to_string()],
        &[],
        &["Bash(make check)".to_string()],
        &Anchors::none(),
    );
    assert!(rejected.is_empty(), "{rejected:?}");
    answers.rules.permissions = standing;
    assert_eq!(
        answers.rules.permissions.for_command(&["make", "check"]),
        Decision::Ruled(Ruling::Allow),
        "the fixture's allow rule was not in force"
    );
    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["rm", "-rf", "build"]),
        Decision::Ruled(Ruling::Deny),
        "the fixture's deny rule was not in force"
    );
    bypassing(&mut session);
    session.status = crate::state::Status::Idle;
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    let mut terminal = a_fixed_viewport_terminal();

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
    .clear(&mut terminal, Some("carry on with the notes".to_string()));

    assert!(ended.is_none(), "the handoff ended the session: {ended:?}");
    assert_eq!(
        answers.rules.permissions.for_command(&["make", "check"]),
        Decision::Unmatched,
        "an allow rule of the source session crossed"
    );
    assert_eq!(
        answers
            .rules
            .permissions
            .for_command(&["rm", "-rf", "build"]),
        Decision::Unmatched,
        "a deny rule of the source session crossed"
    );
}

/// SESSION-34: the record a handoff came from is written as it was, through the entry point
/// `/handoff` takes. The source is read back after `Held::clear` and compared field for field
/// with what it held before, and the new record holds the link under an id of its own.
///
/// Rejects: a retitle or touch of the source inside `Held::clear`, and a new record begun
/// under the source's id.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_handoff_through_held_clear_leaves_the_source_record_as_it_was() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-handoff-source-clear");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_to_branch(&root);
    let source = stored.id().to_string();
    let before = serde_json::to_string(&sessions::load(&root, &source).expect("the source"))
        .expect("serialise");
    bypassing(&mut session);
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    let mut terminal = a_fixed_viewport_terminal();

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
    .clear(&mut terminal, Some("carry on with the notes".to_string()));

    assert!(ended.is_none(), "{ended:?}");
    assert_ne!(stored.id(), source, "the new session kept the source's id");
    assert_eq!(stored.handed_off_from(), Some(source.as_str()));
    let after = serde_json::to_string(&sessions::load(&root, &source).expect("the source"))
        .expect("serialise");
    assert_eq!(after, before, "the handoff changed the record it came from");
}

/// SESSION-19: undoing a turn takes back the program vouched for in it and stops the servers.
/// It keeps a file agreed to be shown or to hold a secret, which CRED-13 and CRED-15 give the
/// session's lifetime, and the run prompts drawn, which grant nothing.
#[test]
fn undoing_a_turn_takes_back_its_programs_and_keeps_the_files_agreed_to() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-answers");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), &root);
    let mut answers = Answers::opening(TrustStore::new(&root), TrustedPrograms::new(), rules);
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut conversation = Conversation::new();

    let prompt = "run the checks";
    let start = conversation.recounted().len();
    type_line(&mut session, prompt);
    session.submit().expect("the prompt is sent");
    let point = rewind_point(
        &session,
        &conversation,
        &answers.trust,
        &answers.programs,
        &stored,
    );
    session.rewind.open(point, prompt.to_string());
    session.prompt_recorded(conversation.recounted().len());
    conversation.push(Message::user(prompt));
    give_answers(&mut answers, &root);
    let key = root.join("deploy/key.pem");
    answers.exposed.allow_creating(&key);
    conversation.push(Message::assistant("done"));
    session.complete("done", vec![], 10);
    session.record_turn(start, &conversation);
    let asked_about = answers.asked_about.clone();

    let (trust, programs, servers) = answers.what_a_rewind_takes_back();
    rewind(
        &mut session,
        &mut conversation,
        trust,
        programs,
        &mut stored,
        &workspace,
        servers,
        &mut bravebot_agent::confirm::Unattended,
        1,
    );

    assert_eq!(session.turns, 0, "the turn was not undone");
    assert_eq!(
        answers.programs,
        TrustedPrograms::new(),
        "the rewind kept a program vouched for in the turn it undid"
    );
    assert!(
        answers.servers.is_none(),
        "the rewind kept the servers running"
    );
    assert!(
        answers.exposed.holds(".env"),
        "the rewind forgot a file agreed to be shown"
    );
    assert!(
        answers.exposed.allows_creating(&key),
        "the rewind forgot a file agreed to hold a secret"
    );
    assert_eq!(
        answers.asked_about, asked_about,
        "the rewind forgot the run prompts drawn"
    );
}

/// SESSION-31, CHECKOUT-16: a fork carries no checkout, so a branch while the session keeps one
/// is refused, naming it, and nothing is copied, ended or forgotten.
#[test]
fn branching_is_refused_while_the_session_keeps_a_checkout() {
    use bravebot_session::sessions;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-checkout");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    give_answers(&mut answers, &root);
    let original = stored.id().to_string();
    let (asked_about, exposed) = (answers.asked_about.clone(), answers.exposed.clone());

    assert!(!branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[kept_checkout(true)],
        "",
    ));

    assert_eq!(stored.id(), original, "the session moved to a copy");
    assert_eq!(sessions::list(&root).len(), 1, "a copy was written");
    assert_eq!(
        session.transcript.last().map(|entry| entry.text.clone()),
        Some(t!(session_branch_keeps_checkouts, count = 1, ids = "c2"))
    );
    assert!(session.looping().is_some(), "a refusal ended the loop");
    assert!(answers.servers.is_some(), "a refusal dropped the servers");
    assert_eq!(answers.asked_about, asked_about);
    assert_eq!(answers.exposed, exposed);
}

/// SESSION-31, GOAL-12: a goal is not written down, so the copy has none. The setup starts a
/// loop, which replaces a goal, so the loop is stopped before the goal is set.
#[test]
fn branching_a_session_with_a_goal_takes_the_goal_off() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-goal");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    session.stop_loops();
    session.start_goal("cargo test exits 0".to_string());
    assert!(session.goal().is_some(), "the goal was not set");

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert!(
        session.goal().is_none(),
        "the goal carried over to the copy"
    );
}

/// SESSION-31: a watch is not written down, so the copy has none, and the person who armed it
/// is told it ended. The setup's loop is stopped first, as a loop and a watch do not stand
/// together.
#[test]
fn branching_a_session_with_a_watch_ends_it_and_says_so() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-watch");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    session.stop_loops();
    session.arm_watch(
        "notes.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );
    assert_eq!(session.watches().len(), 1, "the watch was not armed");

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert!(
        session.watches().is_empty(),
        "a watch carried over to the copy"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(watches_stopped, count = 1)),
        "the watch ended in silence"
    );
}

/// SESSION-31: a session with no turn has no record, and says so rather than copying nothing.
#[test]
fn branching_a_session_with_no_turn_says_so_and_stays_put() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-empty");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(TrustStore::new(&root), TrustedPrograms::new(), rules);
    let mut stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let id = stored.id().to_string();

    assert!(!branch_session(
        &mut session,
        &mut stored,
        &Conversation::new(),
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert_eq!(stored.id(), id);
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(session_branch_nothing_written)
    );
    assert!(bravebot_session::sessions::list(&root).is_empty());
}

/// `/rename` gives up every rewind point and then writes the record, so the session it wrote
/// has nothing to rewind to and neither has the one resumed from it. Left in the record, the
/// point came back to a session whose own `/undo` had just said there was nothing left to undo,
/// and taking it put the tree back and the old name with it: the record of a session renamed
/// before its first turn was rewound out of existence.
#[test]
fn a_resumed_session_whose_record_was_renamed_has_nothing_to_undo() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions::{self, Standing};

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-rename");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("none");
    let mut conversation = Conversation::new();

    // One finished turn, which is the point a rewind could reach.
    let prompt = "write a line saying hello into notes.txt";
    let start = conversation.recounted().len();
    type_line(&mut session, prompt);
    session.submit().expect("the prompt is sent");
    let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
    session.rewind.open(point, prompt.to_string());
    session.prompt_recorded(conversation.recounted().len());
    conversation.push(Message::user(prompt));
    conversation.push(Message::assistant("written"));
    session.complete("written", vec![], 10);
    session.record_turn(start, &conversation);
    stored.save(
        prompt,
        Standing {
            history: Some(session.turn_history()),
            conversation: &conversation.snapshot(),
            turns: session.turns,
            tokens: session.tokens,
            split: Default::default(),
            spend: session.spend_by_turn(),
            timing: session.timing_by_turn(),
            model: None,
            todos: &session.todos_by_turn(),
            asides: &[],
            trust: &trust,
            programs: &programs,
            directories: &[],
            manifest: None,
            rewind: session.rewind.points(),
            checkouts: &workspace.session_checkouts(),
            touched: None,
            answered_by_mode: false,
        },
    );
    assert_eq!(
        sessions::load(&root, stored.id())
            .expect("the record")
            .rewind_points(&root)
            .len(),
        1,
        "the turn left no point in the record, so the rename below gives up nothing"
    );

    rename_session(&mut session, &mut stored, "the parser bug");
    assert_eq!(stored.title(), "the parser bug");

    // The resume, as `run` does it.
    let record = sessions::load(&root, stored.id()).expect("the record survived the rename");
    let conversation = Conversation::restored(record.conversation.clone());
    let mut resumed = Session::new("none");
    resumed.replay(
        &conversation,
        &record.title,
        &sessions::recall(&root, &record),
    );
    resumed.restore_spend(record.tokens, record.spend.clone());
    resumed.restore_rewind_points(record.rewind_points(&root), &conversation);
    let mut conversation = conversation;

    assert!(
        resumed.rewind.points().is_empty(),
        "the resumed session was handed a point the rename gave up"
    );

    rewind(
        &mut resumed,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        1,
    );

    assert_eq!(
        resumed.turns, 1,
        "the resumed session rewound a turn it had given up the point for"
    );
    let after = sessions::load(&root, stored.id())
        .expect("the rewind discarded the renamed session's record");
    assert_eq!(
        after.title, "the parser bug",
        "the rewind put back the name the session had before it was renamed"
    );
    assert_eq!(after.turns, 1, "the turn left the record with the rewind");
}

/// SESSION-19: the prompt a rewind undid comes back into the box. It has to be read off the
/// point, since the rewind removes the rewound turns' display prompts, and from the earliest
/// of the turns gone back past, which is the request the session now stands before.
#[test]
fn undoing_a_turn_puts_its_prompt_back_in_the_box() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-prompt");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("none");
    let mut conversation = Conversation::new();

    // Two turns, with distinct prompts: going back past both must return the earlier one.
    for prompt in ["rename the parser module", "add a test for the parser"] {
        let start = conversation.recounted().len();
        type_line(&mut session, prompt);
        session.submit().expect("the prompt is sent");
        let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
        session.rewind.open(point, prompt.to_string());
        session.prompt_recorded(conversation.recounted().len());
        conversation.push(Message::user(prompt));
        conversation.push(Message::assistant("done"));
        session.complete("done", vec![], 10);
        session.record_turn(start, &conversation);
    }
    assert_eq!(session.rewind.points().len(), 2);
    assert!(session.input().is_empty(), "the box did not start empty");

    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        2,
    );

    assert_eq!(
        session.input(),
        "rename the parser module",
        "the prompt the session now stands before did not come back"
    );
    assert_eq!(
        session.turns, 0,
        "the session did not go back past both turns"
    );
}

/// `/rewind` goes back the number of turns typed after it. Anything that is not a whole number
/// above zero goes nowhere and says what the command wants, and the bare word lists the points
/// without moving.
#[test]
fn the_rewind_command_goes_back_the_number_it_was_given_and_nowhere_for_anything_else() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-command");
    std::fs::create_dir_all(&root).expect("create");
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

    for prompt in ["first", "second", "third"] {
        let start = conversation.recounted().len();
        type_line(&mut session, prompt);
        session.submit().expect("the prompt is sent");
        let point = rewind_point(
            &session,
            &conversation,
            &answers.trust,
            &answers.programs,
            &stored,
        );
        session.rewind.open(point, prompt.to_string());
        session.prompt_recorded(conversation.recounted().len());
        conversation.push(Message::user(prompt));
        conversation.push(Message::assistant("done"));
        session.complete("done", vec![], 10);
        session.record_turn(start, &conversation);
    }
    assert_eq!(session.turns, 3, "the fixture did not have three turns");

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

    held.rewind_command(&mut bravebot_agent::confirm::Unattended, "");
    assert_eq!(held.session.turns, 3, "the bare word went back");
    assert!(
        held.session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(session_rewind_points)),
        "the bare word did not list the points"
    );

    for refused in ["0", "two", "-1", "1.5"] {
        held.rewind_command(&mut bravebot_agent::confirm::Unattended, refused);
        assert_eq!(held.session.turns, 3, "`/rewind {refused}` went back");
        assert_eq!(
            held.session.transcript.last().expect("a note").text,
            t!(session_rewind_needs_a_number),
            "`/rewind {refused}` did not say what the command wants"
        );
    }

    // Past what the session remembers it rewinds nothing and says how far back it does go,
    // not as far as it can, which would leave a tree younger than the person believes.
    held.rewind_command(&mut bravebot_agent::confirm::Unattended, "4");
    assert_eq!(
        held.session.turns, 3,
        "`/rewind 4` went back as far as the session could"
    );
    assert_eq!(
        held.session.transcript.last().expect("a note").text,
        t!(session_rewind_goes_no_further, kept = 3),
        "`/rewind 4` did not say how far back the session goes"
    );

    // A number too large to fit a `usize` is still a number past what the session
    // remembers, not a non-number: it takes the same path `/rewind 4` just did.
    held.rewind_command(
        &mut bravebot_agent::confirm::Unattended,
        "99999999999999999999",
    );
    assert_eq!(
        held.session.turns, 3,
        "`/rewind 99999999999999999999` went back as far as the session could"
    );
    assert_eq!(
        held.session.transcript.last().expect("a note").text,
        t!(session_rewind_goes_no_further, kept = 3),
        "`/rewind 99999999999999999999` did not say how far back the session goes"
    );

    held.rewind_command(&mut bravebot_agent::confirm::Unattended, "2");
    assert_eq!(
        held.session.turns, 1,
        "`/rewind 2` did not go back two turns"
    );
}

/// A turn is lent the conversation and every answer the session holds, and what it returns
/// replaces them. A field left out of the return leaves the next turn with an empty trust map,
/// no vouched programs or no servers, and moving the values into the turn gives the compiler
/// nothing to say about it.
#[test]
fn a_turn_is_lent_everything_the_session_holds_and_what_it_returns_replaces_it() {
    use bravebot_aichat::protocol::Message;
    use bravebot_core::programs::Command;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-lend");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_to_branch(&root);
    give_answers(&mut answers, &root);
    answers.trust.trust("notes");
    let held_trust = answers.trust.clone();
    let held_programs = answers.programs.clone();
    let held_asked_about = answers.asked_about.clone();
    let held_exposed = answers.exposed.clone();
    let held_messages = conversation.recounted().len();
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

    let lent = held.lend();
    assert_eq!(
        lent.conversation.recounted().len(),
        held_messages,
        "the turn was lent a conversation the session did not hold"
    );
    assert_eq!(lent.trust, held_trust, "the turn was lent another map");
    assert_eq!(
        lent.programs, held_programs,
        "the turn was lent other programs"
    );
    assert!(lent.servers.is_some(), "the turn was lent no servers");
    assert_eq!(
        lent.asked_about, held_asked_about,
        "the turn was lent other prompts drawn"
    );
    assert_eq!(
        lent.exposed, held_exposed,
        "the turn was lent other files agreed to"
    );

    let git = Command::new("/usr/bin/git", vec!["status".into()], &root);
    let mut next = lent.conversation;
    next.push(Message::user("one more"));
    let mut trust = lent.trust;
    trust.trust("returned");
    let mut programs = lent.programs;
    programs.trust(git.clone());
    let mut asked_about = lent.asked_about;
    asked_about.record(git);
    let mut exposed = lent.exposed;
    exposed.allow(".other");
    let events = held.take_back(Continued {
        conversation: next,
        trust: trust.clone(),
        programs: programs.clone(),
        servers: lent.servers,
        asked_about: asked_about.clone(),
        exposed: exposed.clone(),
        events: Vec::new(),
    });

    assert!(events.is_empty());
    assert_eq!(
        held.conversation.recounted().len(),
        held_messages + 1,
        "the conversation the turn returned was not put back"
    );
    assert_eq!(held.answers.trust, trust, "the map was not put back");
    assert_ne!(held.answers.trust, held_trust);
    assert_eq!(
        held.answers.programs, programs,
        "the programs were not put back"
    );
    assert!(
        held.answers.servers.is_some(),
        "the servers were not put back"
    );
    assert_eq!(
        held.answers.asked_about, asked_about,
        "the prompts drawn were not put back"
    );
    assert_eq!(
        held.answers.exposed, exposed,
        "the files agreed to were not put back"
    );
}

/// The bare word is the list, which is the surface the command exists for: seeing what a
/// rewind would put back before running it.
#[test]
fn the_bare_rewind_command_asks_for_the_list() {
    let mut session = Session::new("none");
    for c in REWIND_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Rewind(String::new())
    );
}

/// A number after the word is how many turns to go back.
#[test]
fn the_rewind_command_carries_how_far_back_to_go() {
    let mut session = Session::new("none");
    for c in "/rewind 3".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Rewind("3".to_string())
    );
}

/// The bare word is the list of hunks, and a number after it names one, so the argument is
/// carried as it was typed and read where the transcript is.
#[test]
fn the_hunks_command_carries_the_number_it_was_given() {
    for (typed, carried) in [("/hunks", ""), ("/hunks 2", "2"), ("/hunks two", "two")] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Hunks(carried.to_string()),
            "{typed}"
        );
    }
}

/// `/undo` takes no argument, so a sentence that begins with it is still a prompt. The list
/// and the number went to a word of their own to keep that true.
#[test]
fn undo_with_something_after_it_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "/undo the last thing I asked for".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/undo the last thing I asked for".to_string())
    );
}

/// Deciding whether to go back is deciding about the files a turn wrote, so the list names
/// them, and names what each turn was asked so the rows can be told apart.
#[test]
fn the_list_names_what_each_point_would_put_back() {
    let mut session = Session::new("none");
    session
        .rewind
        .open(a_point_before(0), "add a line to notes.md".into());
    session
        .rewind
        .keep_backups(vec![bravebot_agent::workspace::Backup {
            captured_trust: bravebot_core::label::Integrity::Trusted,
            path: std::path::PathBuf::from("/work/notes.md"),
            was: bravebot_agent::workspace::Before::Nothing,
            kept: false,
        }]);
    session
        .rewind
        .open(a_point_before(1), "read the notes back".into());

    list_rewind_points(&mut session);

    let said = session
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("/work/notes.md"),
        "the list did not say which file would go back: {said}"
    );
    assert!(
        said.contains("add a line to notes.md"),
        "the list did not say what the turn was asked: {said}"
    );
    let first = said.find("read the notes back").expect("the newest point");
    let second = said
        .find("add a line to notes.md")
        .expect("the older point");
    assert!(
        first < second,
        "the list did not begin at the turn that just ended: {said}"
    );
}

/// A session with nothing to rewind says so, rather than printing an empty heading.
#[test]
fn the_list_of_a_session_with_no_points_says_there_is_nothing() {
    let mut session = Session::new("none");

    list_rewind_points(&mut session);

    assert_eq!(session.transcript.len(), 1, "an empty list still had rows");
}

/// Rewind's actual save and audit truncation must agree before a turn number is reused.
#[test]
fn rewinding_reopened_history_removes_outcomes_plans_and_audit_before_reuse() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions::{self, Standing};
    fn save(
        stored: &mut sessions::Handle,
        session: &Session,
        conversation: &Conversation,
        root: &std::path::Path,
    ) {
        stored.save(
            "history",
            Standing {
                conversation: &conversation.snapshot(),
                history: Some(session.turn_history()),
                turns: session.turns,
                tokens: session.tokens,
                split: Default::default(),
                spend: session.spend_by_turn(),
                timing: session.timing_by_turn(),
                model: None,
                todos: &session.todos_by_turn(),
                asides: &[],
                trust: &TrustStore::new(root),
                programs: &TrustedPrograms::new(),
                directories: &[],
                manifest: None,
                rewind: session.rewind.points(),
                checkouts: &[],
                touched: None,
                answered_by_mode: false,
            },
        );
    }
    fn reopen(root: &std::path::Path, record: &sessions::Record) -> (Session, Conversation) {
        let conversation = Conversation::restored(record.conversation.clone());
        let mut session = Session::new("test");
        session.replay(
            &conversation,
            &record.title,
            &sessions::recall(root, record),
        );
        session.restore_spend(record.tokens, record.spend.clone());
        session.restore_timing(record.timing.clone());
        session.restore_rewind_points(record.rewind_points(root), &conversation);
        (session, conversation)
    }
    fn audit(stored: &sessions::Handle, turn: usize, detail: &str) {
        stored.append_audit(
            turn,
            &[bravebot_session::audit::Stamped {
                at: turn as u64,
                from: None,
                event: bravebot_core::event::Event::GatePassed {
                    gate: "file_read",
                    detail: detail.into(),
                },
            }],
        );
    }
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("reopened-history-rewind");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("test");
    let mut conversation = Conversation::new();
    for (index, prompt) in ["kept", "failed", "cancelled"].into_iter().enumerate() {
        let start = conversation.recounted().len();
        type_line(&mut session, prompt);
        session.submit().unwrap();
        let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
        session.rewind.open(point, prompt.into());
        session.prompt_recorded(conversation.recounted().len());
        conversation.push(Message::user(prompt));
        conversation.push(Message::assistant(format!("working {prompt}")));
        session.narrate(format!("working {prompt}"));
        session.set_todos(bravebot_core::todo::rows(&bravebot_core::todo::List::new(
            vec![bravebot_core::todo::Item::new(
                prompt,
                bravebot_core::todo::Status::Active,
            )],
        )));
        session.progressed(bravebot_agent::Spent {
            tokens: (index as u64 + 1) * 10,
            timing: bravebot_agent::timing::Timing {
                inference_ms: index as u64 + 1,
                ..Default::default()
            },
            ..Default::default()
        });
        match index {
            0 => session.complete("done", vec![], 10),
            1 => session.fail(
                "safe failure",
                bravebot_agent::Ending::Failed(bravebot_agent::Diagnosis::of(
                    bravebot_agent::Category::Transport,
                )),
            ),
            _ => {
                finish_cancelled_turn(&mut session, prompt, Some(1));
            }
        }
        session.record_turn(start, &conversation);
        save(&mut stored, &session, &conversation, &root);
        audit(&stored, session.turns, prompt);
    }
    let record = sessions::load(&root, stored.id()).unwrap();
    let (mut session, mut conversation) = reopen(&root, &record);
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        2,
    );
    let record = sessions::load(&root, stored.id()).unwrap();
    let (mut session, mut conversation) = reopen(&root, &record);
    assert_eq!(session.turns, 1);
    assert_eq!(session.tokens, 10);
    assert_eq!(
        session.todos_by_turn().keys().copied().collect::<Vec<_>>(),
        [1]
    );
    assert!(!session.transcript.iter().any(|e| matches!(
        e.speaker,
        crate::state::Speaker::Failure | crate::state::Speaker::Stopped
    )));
    let trail = sessions::audit_of(&root, stored.id());
    assert_eq!(trail.keys().copied().collect::<Vec<_>>(), [1]);
    let start = conversation.recounted().len();
    type_line(&mut session, "replacement");
    session.submit().unwrap();
    session.prompt_recorded(conversation.recounted().len());
    conversation.push(Message::user("replacement"));
    session.complete("new answer", vec![], 41);
    session.record_turn(start, &conversation);
    save(&mut stored, &session, &conversation, &root);
    audit(&stored, 2, "replacement");
    let record = sessions::load(&root, stored.id()).unwrap();
    let (session, _) = reopen(&root, &record);
    assert_eq!(session.tokens, 51);
    assert_eq!(session.spend_by_turn()[&2], 41);
    assert_eq!(session.timing_by_turn()[&2].inference_ms, 0);
    assert!(!session.todos_by_turn().contains_key(&2));
    let trail = sessions::audit_of(&root, stored.id());
    assert_eq!(trail[&2].len(), 1);
    assert!(trail[&2][0].text.contains("replacement"));
    assert!(!session.transcript.iter().any(|e| matches!(
        e.speaker,
        crate::state::Speaker::Failure | crate::state::Speaker::Stopped
    )));
}
