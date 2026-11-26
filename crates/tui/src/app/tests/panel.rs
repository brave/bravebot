//! The info panel and the links, names and figures it shows.

use super::*;

/// A tab finds the session waiting for an answer by its marker, so every kind of question has
/// to turn the title to waiting, and a message that asks nothing must not: it would mark a run
/// that is only streaming as stuck on the person.
#[test]
fn a_question_turns_the_title_to_waiting_and_other_messages_do_not() {
    use crate::remote_confirm::ToMain;
    use crate::title::State;

    assert_eq!(
        title_state_during(&write_question("notes.md")),
        State::Waiting
    );
    assert_eq!(title_state_during(&fetch_question()), State::Waiting);
    assert_eq!(
        title_state_during(&ToMain::Ask(bravebot_core::ask::Asking::default())),
        State::Waiting
    );
    assert_eq!(
        title_state_during(&ToMain::Streaming("text".into())),
        State::Working
    );
}

/// A manifest run's steps put writes, fetches, vouches, credential exposures, a server's list and
/// a server's calls to the person, and each of those has to turn the title to waiting. Questions the mode refuses undrawn and
/// messages that ask nothing must not.
#[test]
fn every_approval_a_manifest_step_draws_turns_the_title_to_waiting() {
    use crate::remote_confirm::ToMain;
    assert!(manifest_step_asks(&write_question("notes.md")));
    assert!(manifest_step_asks(&fetch_question()));
    assert!(manifest_step_asks(&ToMain::Vouch(
        bravebot_agent::confirm::VouchRequest {
            path: "notes.md".into(),
            rule: "notes.md".into(),
            preview: String::new(),
            truncated: false,
            hidden: 0,
            verdict: bravebot_core::vetting::Verdict::Safe,
            reason: None,
            label: String::new(),
            reason_label: None,
        }
    )));
    assert!(manifest_step_asks(&ToMain::Exposure(
        bravebot_agent::confirm::ExposureRequest {
            path: "notes.md".into(),
            credentials: Vec::new(),
        }
    )));
    assert!(manifest_step_asks(&ToMain::ToolList(
        bravebot_agent::confirm::ToolListRequest {
            alias: "weather".into(),
            tools: Vec::new(),
            refused: 0,
            withheld: 0,
            verdict: bravebot_core::vetting::Verdict::Safe,
            reason: None,
        }
    )));
    assert!(manifest_step_asks(&ToMain::McpCall(
        bravebot_agent::confirm::McpCallRequest {
            alias: "weather".into(),
            tool: "get_forecast".into(),
            arguments: Vec::new(),
            also: Vec::new(),
            description: None,
            may_stand: false,
            asked_every_call: false,
            asked_for_private_arguments: false,
        }
    )));
    assert!(!manifest_step_asks(&ToMain::Streaming("text".into())));
    assert!(!manifest_step_asks(&ToMain::Ask(
        bravebot_core::ask::Asking::default()
    )));
}

/// The title a manifest run's step leaves is what the person finds a tab by, so the transition
/// the loop makes around each message is the one pinned here: waiting while an approval is open,
/// working once it is answered, and nothing written for a message that asks nothing.
#[test]
fn a_manifest_step_marks_the_title_waiting_while_it_asks_and_working_after() {
    use crate::remote_confirm::ToMain;
    use crate::title::State;

    let marked_around = |message: &ToMain| {
        let mut marked = Vec::new();
        let asks = begin_manifest_step(message, |state| marked.push(state));
        end_manifest_step(asks, |state| marked.push(state));
        marked
    };

    assert_eq!(
        marked_around(&write_question("notes.md")),
        [State::Waiting, State::Working]
    );
    assert_eq!(
        marked_around(&fetch_question()),
        [State::Waiting, State::Working]
    );
    assert!(marked_around(&ToMain::Streaming("text".into())).is_empty());

    let mut before = Vec::new();
    let asks = begin_manifest_step(&write_question("notes.md"), |state| before.push(state));
    assert_eq!(before, [State::Waiting], "waiting before the answer");
    let mut after = Vec::new();
    end_manifest_step(asks, |state| after.push(state));
    assert_eq!(after, [State::Working], "working once it is answered");
}

/// The mode lasts one command. Leaving it on would send the next thing typed to a shell, which
/// is the sort of surprise that ends up running a sentence.
#[test]
fn running_a_command_leaves_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "!pwd");
    handle_key(&mut session, key(KeyCode::Enter));

    assert!(!session.shell, "the mode stayed on after the command ran");
    type_line(&mut session, "what did that print");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what did that print".to_string())
    );
}

/// The key and the command are two ways to one toggle, and a moved key leaves the old chord
/// doing nothing, or the default would still answer a key the person gave away.
#[test]
fn the_panel_key_and_the_panel_command_both_toggle_it() {
    let wide = crate::state::Laid {
        columns: 120,
        ..crate::state::Laid::default()
    };
    let mut session = Session::new("none");
    session.note_layout(wide.clone());
    handle_key(&mut session, ctrl('x'));
    assert!(session.panel_open(), "ctrl-x did not open the panel");
    assert_eq!(
        dispatch_command(&mut session, commanded(PANEL_COMMAND)),
        Action::Redraw
    );
    assert!(!session.panel_open(), "/panel did not close the panel");

    let mut moved = Session::new("none");
    let mut bindings = std::collections::BTreeMap::new();
    bindings.insert("panel".to_string(), "alt-i".to_string());
    moved.adopt_keybindings(&bindings);
    moved.note_layout(wide);
    handle_key(&mut moved, ctrl('x'));
    assert!(
        !moved.panel_open(),
        "the chord the panel was moved off still opened it"
    );
    handle_key(
        &mut moved,
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::ALT),
    );
    assert!(moved.panel_open(), "the moved chord did not open the panel");
}

/// PANEL-5. Whether the panel is open, the width it was dragged to and the sections turned off
/// are read from `~/.bravebot` after a record is restored, and the record holds none of them.
/// Resuming the same record twice, with the three files changed between, opens the panel as
/// each set of files says. The faults it rejects are a resume that never reads the files, one
/// that restores the choice from the record, and an adopt that reads the open choice but not
/// the width or the sections.
#[test]
fn a_resumed_session_opens_the_panel_as_the_files_say_and_not_as_the_record_does() {
    use bravebot_session::{sessions, store};

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-resumed-chrome");
    std::fs::create_dir_all(&root).expect("create");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let config = a_config_needing_no_sign_in();

    store::save_panel(true);
    store::save_panel_width(48);
    store::save_sidebar(&["goal", "plan"]);
    let session = Session::new("none").with_stored_history();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    let record = sessions::load(&root, stored.id()).expect("the record");
    let written = serde_json::to_string(&record).expect("a record serialises");
    for word in ["panel", "sidebar", "width"] {
        assert!(
            !written.contains(word),
            "the record names `{word}`: {written}"
        );
    }

    // The session that wrote the record has ended, so nothing holds it.
    drop(stored);
    let mut resume = || {
        let mut resumed = Session::new("none").with_stored_history();
        restore_resumed(&mut resumed, &config, &mut workspace, &record)
            .expect("the record is free");
        adopt_settings(&mut resumed, &bravebot_config::Settings::default(), None);
        resumed
    };

    let opened = resume();
    assert!(opened.panel_open(), "the open choice was not read back");
    assert_eq!(opened.panel_columns(200), 48, "the width was not read back");
    assert_eq!(
        opened.panel_sections(),
        crate::panel::Sections::from_ids(&["goal", "plan"]),
        "the sections turned off were not read back"
    );

    store::save_panel(false);
    store::save_panel_width(30);
    store::save_sidebar(&["model"]);
    let closed = resume();
    assert!(
        !closed.panel_open(),
        "the panel opened as the record was written, not as the file now says"
    );
    assert_eq!(closed.panel_columns(200), 30);
    assert_eq!(
        closed.panel_sections(),
        crate::panel::Sections::from_ids(&["model"])
    );
}

/// The bare word says what is set, a link sets it, and `clear` removes it, for each command and
/// only its own link.
#[test]
fn the_issue_and_pr_commands_show_set_and_clear_their_own_link() {
    use bravebot_session::sessions::Link;
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none");
    let mut stored = bravebot_session::sessions::Handle::begin(
        std::path::Path::new("/work"),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    assert_eq!(
        linked(&mut session, &mut stored, ISSUE_COMMAND),
        t!(session_issue_none)
    );
    assert_eq!(
        linked(&mut session, &mut stored, &format!("/issue {ISSUE}")),
        t!(session_issue_set, url = ISSUE)
    );
    assert_eq!(
        linked(&mut session, &mut stored, &format!("/pr {PULL}")),
        t!(session_pull_request_set, url = PULL)
    );
    assert_eq!(stored.link(Link::Issue), Some(ISSUE));
    assert_eq!(stored.link(Link::PullRequest), Some(PULL));
    assert_eq!(
        linked(&mut session, &mut stored, PR_COMMAND),
        t!(session_pull_request_is, url = PULL)
    );

    assert_eq!(
        linked(&mut session, &mut stored, "/issue clear"),
        t!(session_issue_cleared)
    );
    assert_eq!(stored.link(Link::Issue), None);
    assert_eq!(
        stored.link(Link::PullRequest),
        Some(PULL),
        "clearing the issue cleared the pull request"
    );
    assert_eq!(
        linked(&mut session, &mut stored, "/issue clear"),
        t!(session_issue_none)
    );
}

/// A turn carries the repository of the session's own link, the pull request's before the
/// issue's, and a link that is not a GitHub address carries none, so a bare number in a command
/// the planner writes is read against the repository the person said the work is for (PANEL-16).
#[test]
fn a_turn_carries_the_repository_of_the_sessions_own_link() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none");
    let mut stored = bravebot_session::sessions::Handle::begin(
        std::path::Path::new("/work"),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let carried =
        |session: &Session| with_own_repository(Task::new("p"), session).default_repository;
    assert_eq!(carried(&session), None);

    linked(
        &mut session,
        &mut stored,
        "/issue https://github.com/brave/issue-repo/issues/1",
    );
    assert_eq!(
        carried(&session),
        Some(("brave".to_string(), "issue-repo".to_string()))
    );

    linked(
        &mut session,
        &mut stored,
        "/pr https://github.com/brave/pr-repo/pull/2",
    );
    assert_eq!(
        carried(&session),
        Some(("brave".to_string(), "pr-repo".to_string())),
        "the issue's repository was carried over the pull request's"
    );

    linked(&mut session, &mut stored, "/pr clear");
    linked(
        &mut session,
        &mut stored,
        "/issue https://example.com/tracker/issues/1",
    );
    assert_eq!(
        carried(&session),
        None,
        "a link that is not on GitHub named a repository"
    );
}

/// A value that is not one web address on one line sets nothing, leaves a link already set as
/// it was, and is not repeated back, since what made it no link may be an escape.
#[test]
fn a_link_with_a_newline_an_escape_or_another_scheme_sets_nothing() {
    use bravebot_session::sessions::{Link, Url};
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none");
    let mut stored = bravebot_session::sessions::Handle::begin(
        std::path::Path::new("/work"),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    stored.set_link(Link::PullRequest, Url::read(PULL));

    for value in [
        "https://example.com/a\nhttps://example.com/b",
        "https://example.com/\u{1b}]0;owned\u{7}",
        "ftp://example.com/pull/1",
        "javascript:alert(1)",
        "https://example.com/\u{202e}1/llup",
        "https:///pull/1",
    ] {
        for (command, kind, refused, kept) in [
            (ISSUE_COMMAND, Link::Issue, t!(session_issue_refused), None),
            (
                PR_COMMAND,
                Link::PullRequest,
                t!(session_pull_request_refused),
                Some(PULL),
            ),
        ] {
            assert_eq!(
                linked(&mut session, &mut stored, &format!("{command} {value}")),
                refused,
                "{command} {value:?} was not refused"
            );
            assert_eq!(stored.link(kind), kept, "{command} {value:?} set a link");
        }
    }
}

/// A link the command line gave replaces the one the record held, and a link it did not give is
/// left as the record had it rather than cleared.
#[test]
fn links_from_the_command_line_replace_what_it_names_and_leave_the_other() {
    use bravebot_session::sessions::{Link, Url};
    const OLD_ISSUE: &str = "https://github.com/brave/bravebot/issues/1";
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut stored = bravebot_session::sessions::Handle::begin(
        std::path::Path::new("/work"),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    stored.set_link(Link::Issue, Url::read(OLD_ISSUE));
    stored.set_link(Link::PullRequest, Url::read(PULL));

    Links::default().give_to(&mut stored);
    assert_eq!(stored.link(Link::Issue), Some(OLD_ISSUE));
    assert_eq!(stored.link(Link::PullRequest), Some(PULL));

    Links {
        issue: Url::read(ISSUE),
        pull_request: None,
    }
    .give_to(&mut stored);
    assert_eq!(stored.link(Link::Issue), Some(ISSUE));
    assert_eq!(
        stored.link(Link::PullRequest),
        Some(PULL),
        "an issue given alone cleared the pull request"
    );
}

/// R65 of #1267, PANEL-12, CLI-31, LINK-4: what the command line asked for is put on the session
/// once its record is open. `begin_session` makes one call to `apply_the_command_line` after
/// its questions, so that function is tested without a terminal. The faults this rejects: the
/// links of `--issue` and `--pr` never reaching the record, and a prompt from `-p` not held for
/// the loop's first pass to send.
#[test]
fn the_command_line_links_and_prompt_reach_the_session_once_its_record_is_open() {
    use bravebot_session::sessions::{Link, Url};
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let workspace = workspace_for_test();
    let mut stored = a_handle_for_test(&workspace);
    stored.set_link(Link::PullRequest, Url::read(PULL));
    let mut session = Session::new("none");

    apply_the_command_line(
        &mut session,
        &mut stored,
        workspace.root(),
        Links {
            issue: Url::read(ISSUE),
            pull_request: None,
        },
        Some("fix the build".to_string()),
        None,
        None,
    );

    assert_eq!(stored.link(Link::Issue), Some(ISSUE), "--issue was dropped");
    assert_eq!(
        stored.link(Link::PullRequest),
        Some(PULL),
        "--issue alone cleared the pull request"
    );
    assert_eq!(
        session.start_handed_over().as_deref(),
        Some("fix the build"),
        "the prompt was not held for the first pass"
    );
}

/// R3 of #4223, PANEL-12: the addresses `--issue` and `--pr` gave are the person's own words, so
/// they are in the list when the session opens, and a link the record already held is not read
/// again.
///
/// The fault it rejects: reading the line the person typed and not the two flags.
#[test]
fn the_addresses_the_command_line_gave_are_in_the_list_when_the_session_opens() {
    use bravebot_session::sessions::Url;
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let workspace = workspace_for_test();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");

    apply_the_command_line(
        &mut session,
        &mut stored,
        workspace.root(),
        Links {
            issue: Url::read(ISSUE),
            pull_request: Url::read(PULL),
        },
        None,
        None,
        None,
    );

    let met: Vec<String> = session
        .references()
        .iter()
        .map(|reference| reference.shorthand())
        .collect();
    assert_eq!(met, ["brave/bravebot#1267", "brave/bravebot#1270"]);
}

/// `/pr` comes after `/panel` in the list, so `/p` and Tab still give `/panel`, as they did
/// before `/pr` was added.
#[test]
fn slash_p_and_tab_still_give_the_panel() {
    let mut session = Session::new("none");
    for key_code in [KeyCode::Char('/'), KeyCode::Char('p'), KeyCode::Tab] {
        handle_key(&mut session, key(key_code));
    }
    assert_eq!(session.input(), PANEL_COMMAND);
    assert!(
        completions("/p")
            .iter()
            .any(|command| command.name == PR_COMMAND),
        "/pr is not among the commands /p matches"
    );
}

/// PANEL-12: a link gives up no rewind point, since no point holds a link. Setting, clearing,
/// showing and refusing a link, typed during a turn and at rest, all leave the point the turn
/// opened for `/undo`.
///
/// The fault it rejects: `link_session` closing the rewind as `rename_session` does, which
/// the session record's own test cannot see because the rewind lives on the session.
#[test]
fn a_link_set_cleared_or_refused_gives_up_no_rewind_point_during_a_turn_or_at_rest() {
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let lines = [
        format!("/pr {PULL}"),
        format!("/issue {ISSUE}"),
        "/pr".to_string(),
        "/issue not a link".to_string(),
        "/pr clear".to_string(),
        "/issue clear".to_string(),
    ];
    let root = std::path::Path::new("/work");
    let managed = bravebot_config::Managed::default();
    let settings = bravebot_config::Settings::default();
    let workspace = a_workspace();
    let mut config = a_config_needing_no_sign_in();
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    let mut running = a_turn_running_on("first");
    running.rewind.open(a_point_before(0), "first".to_string());
    let mut beside = Beside {
        managed: &managed,
        settings: &settings,
        stored: &mut stored,
        root,
        home: None,
        workspace: &workspace,
        scratch: None,
        config: &mut config,
    };
    for line in &lines {
        typed_during_a_turn(&mut running, line, key(KeyCode::Enter), &mut beside);
        assert_eq!(
            running.rewind.points().len(),
            1,
            "{line:?} typed during a turn gave up the running turn's point"
        );
    }

    let mut at_rest = Session::new("none");
    at_rest.rewind.open(a_point_before(0), "first".to_string());
    for line in &lines {
        linked(&mut at_rest, &mut stored, line);
        assert_eq!(
            at_rest.rewind.points().len(),
            1,
            "{line:?} typed at rest gave up the session's point"
        );
    }
}

/// PANEL-17, CMD-8. A section turned off in the checklist a turn is running under is kept for
/// the next start as it is turned off, and the turn is not touched: nothing is queued, handed
/// to the turn, said under it or cancelled, and the turn goes on.
///
/// Rejects: a toggle that is saved when the checklist closes or only at rest, one that is
/// queued for the turn's end, and one that is handed to the turn as a line to answer.
#[test]
fn a_section_turned_off_mid_turn_is_saved_at_once_and_the_turn_is_not_touched() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_persisting_turn_running_on("first");
    let cancel = nothing_beside(|beside| {
        let cancel =
            typed_during_a_turn(&mut session, SIDEBAR_COMMAND, key(KeyCode::Enter), beside);
        for code in [KeyCode::Char(' '), KeyCode::Char('j'), KeyCode::Enter] {
            turn_key(&mut session, key(code), &cancel, beside);
        }
        cancel
    });

    assert!(session.picking().is_some(), "the checklist closed");
    assert_eq!(
        bravebot_session::store::load_sidebar(),
        Some(vec!["session".to_string(), "model".to_string()])
    );
    assert!(session.queued.is_empty(), "a toggle waited");
    assert_eq!(session.interjections().take(), None);
    assert!(said_under_the_turn(&session).is_empty());
    assert_eq!(session.input(), "");
    assert!(!cancel.is_cancelled());
    assert!(session.a_turn_is_running());
}

/// PANEL-9. A name the record takes reaches an open panel on the next pass of the loop, which
/// draws a frame with no key. Rejects: a pass that gives the panel the new name and does not ask
/// for a draw, so the panel keeps showing the old name until a key redraws it; and a pass that
/// never gives the panel the name at all.
#[test]
fn a_new_name_reaches_an_open_panel_on_the_next_pass_with_no_key() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-panel-name-pass");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, answers) = a_session_to_branch(&root);
    drawn_at(&mut session, 120, 30);
    session.toggle_panel();
    assert!(session.panel_open());
    let named =
        |session: &Session, word: &str| panel_rows(session).iter().any(|row| row.contains(word));
    assert!(
        !named(&session, "hello"),
        "the panel named the session before any pass"
    );

    assert!(
        settled(
            &mut session,
            &workspace,
            &mut stored,
            &conversation,
            &answers
        ),
        "the first pass did not draw the name the record holds"
    );
    assert!(named(&session, "hello"), "{:#?}", panel_rows(&session));
    assert!(
        !settled(
            &mut session,
            &workspace,
            &mut stored,
            &conversation,
            &answers
        ),
        "a pass with nothing new asked for a draw"
    );

    assert!(stored.rename("lintel the parser"));
    assert!(
        settled(
            &mut session,
            &workspace,
            &mut stored,
            &conversation,
            &answers
        ),
        "the pass after a rename did not ask for a draw"
    );
    assert!(named(&session, "lintel"), "{:#?}", panel_rows(&session));
    assert!(
        !named(&session, "hello"),
        "the old name stayed on the panel"
    );
}

/// PANEL-9. The id on an open panel follows `/branch` and `/clear` through the loop's pass, with
/// no key between. Rejects: a pass that names the id once and then keeps the first one, so the
/// panel offers an id `--resume` would find for the session before the branch or the clear,
/// and a pass that changes the id without asking for a draw.
#[test]
fn the_id_on_an_open_panel_follows_a_branch_and_a_clear_on_the_next_pass() {
    use bravebot_aichat::protocol::Message;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-panel-id-pass");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    drawn_at(&mut session, 120, 30);
    session.toggle_panel();
    assert!(session.panel_open());
    let shows = |session: &Session, id: &str| {
        let (head, tail) = id.rsplit_once('-').expect("an id with hyphens");
        let head = format!("{head}-");
        panel_rows(session)
            .windows(2)
            .any(|pair| pair[0] == head && pair[1] == tail)
    };

    let first = stored.id().to_string();
    assert!(
        settled(
            &mut session,
            &workspace,
            &mut stored,
            &conversation,
            &answers
        ),
        "the first pass did not draw the id of the record"
    );
    assert!(shows(&session, &first), "{:#?}", panel_rows(&session));

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));
    let branched = stored.id().to_string();
    assert_ne!(branched, first);
    assert!(
        settled(
            &mut session,
            &workspace,
            &mut stored,
            &conversation,
            &answers
        ),
        "the pass after a branch did not ask for a draw"
    );
    assert!(shows(&session, &branched), "{:#?}", panel_rows(&session));
    assert!(
        !shows(&session, &first),
        "the id of the original stayed on the panel"
    );

    let (mut conversation, mut stored) = clear_session(&mut session, &mut workspace, None);
    let cleared = stored.id().to_string();
    assert_ne!(cleared, branched);
    settled(
        &mut session,
        &workspace,
        &mut stored,
        &conversation,
        &answers,
    );
    assert!(
        !shows(&session, &branched),
        "the id of the session before the clear stayed on the panel"
    );
    conversation.push(Message::user("hello"));
    with_standing(
        &session,
        &conversation,
        &answers.trust,
        &answers.programs,
        &workspace,
        |standing| stored.save("a session", standing),
    );
    assert!(
        settled(
            &mut session,
            &workspace,
            &mut stored,
            &conversation,
            &answers
        ),
        "the pass after the cleared session was written did not ask for a draw"
    );
    assert!(shows(&session, &cleared), "{:#?}", panel_rows(&session));
}

/// PANEL-9. The directory the loop's pass gives an open panel is written from `~`, as `/status`
/// writes it. Rejects: a pass that hands the panel the absolute path of the workspace.
#[test]
fn the_directory_on_an_open_panel_is_written_from_the_home_directory() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("panel-home");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, answers) = a_session_to_branch(&root);
    drawn_at(&mut session, 120, 30);
    session.toggle_panel();
    assert!(session.panel_open());

    settled(
        &mut session,
        &workspace,
        &mut stored,
        &conversation,
        &answers,
    );
    let rows = panel_rows(&session);
    assert!(
        rows.iter().any(|row| row == "~/projects/panel-home"),
        "{rows:#?}"
    );
}

/// A report as the agent sends it: every figure is the turn's running total, not the round's.
fn running_totals(
    prompt_tokens: u64,
    output_tokens: u64,
    read_tokens: u64,
    written_tokens: u64,
) -> bravebot_agent::Spent {
    bravebot_agent::Spent {
        tokens: prompt_tokens + output_tokens,
        output_tokens,
        context_tokens: prompt_tokens,
        cached: bravebot_aichat::protocol::Cached {
            read_tokens,
            written_tokens,
        },
        ..Default::default()
    }
}

fn panel_rows(session: &Session) -> Vec<String> {
    crate::panel::lines(session, crate::panel::WIDTH, 40)
        .iter()
        .map(|line| line.to_string().trim().to_string())
        .collect()
}

fn cache_rows(read: u64, written: u64) -> [String; 2] {
    [
        t!(panel_cache_read, tokens = crate::status::tokens(read)).to_string(),
        t!(panel_cache_written, tokens = crate::status::tokens(written)).to_string(),
    ]
}

fn hit_rate(whole: u64, tenth: u64) -> String {
    t!(
        hint_cache_hit_rate,
        rate = format!("{whole}{}{tenth}", t!(number_decimal_separator))
    )
    .to_string()
}

/// A first turn has no earlier figure to show, so leaving them to the end of the turn drew
/// neither row nor the hit rate for as long as the turn ran.
#[test]
fn a_first_turn_in_flight_draws_the_cache_figures_after_its_first_round() {
    let mut session = Session::new("none");
    let before = panel_rows(&session);
    for row in cache_rows(12_000, 3_000) {
        assert!(!before.contains(&row), "{row:?} drawn before any report");
    }
    assert_eq!(crate::render::cache_hit_rate(&session), None);

    progress_of_a_turn(
        &mut session,
        running_totals(20_000, 40, 12_000, 3_000),
        100_000,
        false,
    );

    let after = panel_rows(&session);
    for row in cache_rows(12_000, 3_000) {
        assert!(after.contains(&row), "{row:?} missing: {after:#?}");
    }
    assert_eq!(
        crate::render::cache_hit_rate(&session),
        Some(hit_rate(60, 0))
    );
}

/// Each report carries the turn's totals so far, so the second replaces the first. Adding it
/// to the first would count round one twice, and keeping the first would stop at round one.
#[test]
fn each_round_of_a_turn_replaces_the_cache_figures_with_the_turns_running_totals() {
    let mut session = Session::new("none");
    progress_of_a_turn(
        &mut session,
        running_totals(20_000, 40, 12_000, 3_000),
        100_000,
        false,
    );
    progress_of_a_turn(
        &mut session,
        running_totals(50_000, 80, 39_000, 4_000),
        100_000,
        false,
    );

    let rows = panel_rows(&session);
    for row in cache_rows(39_000, 4_000) {
        assert!(rows.contains(&row), "{row:?} missing: {rows:#?}");
    }
    for row in cache_rows(12_000, 3_000) {
        assert!(!rows.contains(&row), "{row:?} outlived the second round");
    }
    assert_eq!(
        crate::render::cache_hit_rate(&session),
        Some(hit_rate(78, 0))
    );
}

/// A report with nothing spent behind it says nothing about the cache, so it must not zero
/// the figures of the turn before, nor draw rows in a session that has none.
#[test]
fn a_report_with_no_completed_request_leaves_the_cache_figures_where_they_were() {
    let mut session = Session::new("none");
    session.served_from_cache(
        bravebot_aichat::protocol::Cached {
            read_tokens: 12_000,
            written_tokens: 3_000,
        },
        20_000,
    );
    progress_of_a_turn(
        &mut session,
        bravebot_agent::Spent::default(),
        100_000,
        false,
    );

    let rows = panel_rows(&session);
    for row in cache_rows(12_000, 3_000) {
        assert!(rows.contains(&row), "{row:?} missing: {rows:#?}");
    }
    assert_eq!(session.cached_prompt_tokens(), Some(20_000));
    assert_eq!(
        crate::render::cache_hit_rate(&session),
        Some(hit_rate(60, 0))
    );

    let mut nothing = Session::new("none");
    progress_of_a_turn(
        &mut nothing,
        bravebot_agent::Spent::default(),
        100_000,
        false,
    );
    assert_eq!(nothing.cached(), None);
    assert_eq!(nothing.cached_prompt_tokens(), None);
    let none = panel_rows(&nothing);
    assert!(
        !none
            .iter()
            .any(|row| row.contains(&t!(panel_cache_read, tokens = "").to_string())),
        "a cache row was drawn with nothing reported: {none:#?}"
    );
}
