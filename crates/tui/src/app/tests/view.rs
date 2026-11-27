//! What is drawn in the transcript and the files state for a command's result.

use super::*;

/// VIEW-46, CMD-35. Tab pressed with the effort picker open reaches the picker and switches
/// its scope, at rest and under a turn, and edits nothing else: the line stays empty, the row
/// under the cursor stays, and no suggestion or completion takes the key first. Shift-Tab
/// switches it back. The row taken afterwards carries the scope Tab left the picker in. It
/// rejects a Tab taken by the box, by a suggestion or by the turn's own loop, and a pick that
/// carries the scope the picker was opened in instead of the one in force.
#[test]
fn tab_in_an_open_effort_picker_switches_the_scope_at_rest_and_under_a_turn() {
    use crate::model_prompt::Scope;
    use crate::picking::Picked;
    use bravebot_aichat::protocol::Effort;
    let row = crate::effort_prompt::Row(Some(Effort::High));
    let cursor = |session: &Session| {
        let Some(crate::picking::Picking::Effort(picker)) = session.picking() else {
            panic!("no effort picker is open");
        };
        picker.chosen().copied()
    };

    let mut session = Session::new("none");
    session.choose_effort(Some(Effort::High));
    open_effort_picker(&mut session, Scope::EverySession);
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(
        the_scope_of_the_open_effort_picker(&session),
        Scope::ThisSession
    );
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(
        the_scope_of_the_open_effort_picker(&session),
        Scope::EverySession
    );
    handle_key(&mut session, key(KeyCode::BackTab));
    assert_eq!(
        the_scope_of_the_open_effort_picker(&session),
        Scope::ThisSession
    );
    assert!(session.input().is_empty(), "Tab edited the line");
    assert_eq!(cursor(&session), Some(row), "Tab moved the cursor");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(
        session.take_pick(),
        Some(Picked::Effort(row, Scope::ThisSession)),
        "the pick did not carry the scope Tab left"
    );

    let mut working = a_turn_running_on("first");
    working.choose_effort(Some(Effort::High));
    open_effort_picker(&mut working, Scope::EverySession);
    handle_key_while_working(&mut working, key(KeyCode::Tab));
    assert_eq!(
        the_scope_of_the_open_effort_picker(&working),
        Scope::ThisSession
    );
    assert!(working.input().is_empty(), "Tab edited the line");
    assert_eq!(cursor(&working), Some(row), "Tab moved the cursor");
    handle_key_while_working(&mut working, key(KeyCode::Enter));
    assert_eq!(
        working.take_pick(),
        Some(Picked::Effort(row, Scope::ThisSession)),
        "the pick under a turn did not carry the scope Tab left"
    );
}

/// VIEW-46, CMD-35. The picker `/effort` opens at rest reads its own keys, and the scope in force
/// when Enter takes a row decides whether the level is recorded: the scope it was opened in,
/// switched by each Tab. The faults it rejects are a loop that starts in the scope it was
/// asked for and records whichever it ends in, one that records by the scope it was opened in,
/// and one that ignores Tab.
#[test]
fn the_effort_picker_opened_at_rest_records_by_the_scope_tab_left_it_in() {
    use crate::model_prompt::Scope;
    use bravebot_aichat::protocol::Effort;
    use ratatui::backend::TestBackend;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for (start, tabs, recorded) in [
        (Scope::EverySession, 0, true),
        (Scope::EverySession, 1, false),
        (Scope::EverySession, 2, true),
        (Scope::ThisSession, 0, false),
        (Scope::ThisSession, 1, true),
    ] {
        bravebot_session::store::save_effort(Some(Effort::Low));
        let mut session = Session::new("none").with_stored_history();
        session.choose_effort(Some(Effort::Low));
        let mut keys = vec![KeyCode::Tab; tabs];
        keys.extend([KeyCode::Down, KeyCode::Enter]);
        let _held =
            crate::input::script::hold(keys.into_iter().map(|code| TermEvent::Key(key(code))));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");

        choose_effort(&mut terminal, &mut session, start);

        let case = format!("opened in {start:?}, {tabs} Tab");
        assert_eq!(session.effort(), Some(Effort::Medium), "{case}");
        assert_eq!(
            session.effort_is_for_this_session(),
            !recorded,
            "{case}: the mark"
        );
        assert_eq!(
            bravebot_session::store::load_effort(),
            Some(if recorded {
                Effort::Medium
            } else {
                Effort::Low
            }),
            "{case}: the record"
        );
        assert_eq!(crate::input::script::unread(), 0, "{case}");
    }
}

/// VIEW-46, MOUSE-15. A second press on a row of the picker `/effort` opens at rest takes the
/// row in the scope the picker is in when it is pressed: the scope it was opened in, switched by
/// each Tab. That picker reads its own events, and the mouse arm of its loop answers for the
/// scope as the key arm does. The faults it rejects are a mouse arm that hands the scope back
/// as none, so a level asked to be kept for this session only is recorded for every session,
/// and one that hands back the scope the picker was opened in and not the one Tab left it in.
#[test]
fn the_effort_picker_opened_at_rest_takes_a_second_press_in_the_scope_tab_left_it_in() {
    use crate::model_prompt::Scope;
    use bravebot_aichat::protocol::Effort;
    use ratatui::backend::TestBackend;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for (start, tabs, recorded) in [
        (Scope::EverySession, 0, true),
        (Scope::EverySession, 1, false),
        (Scope::EverySession, 2, true),
        (Scope::ThisSession, 0, false),
        (Scope::ThisSession, 1, true),
    ] {
        bravebot_session::store::save_effort(Some(Effort::Low));
        let mut session = Session::new("none").with_stored_history();
        session.choose_effort(Some(Effort::Low));
        // The rows are the unset one, then low, medium, high, xhigh and max.
        let in_scope = (0..tabs).fold(start, |scope, _| scope.other());
        let shown = crate::effort_prompt::placed(
            ratatui::layout::Rect::new(0, 0, 80, 24),
            &crate::effort_prompt::Picker::new(Some(Effort::Low)).starting_in(in_scope),
        );
        let (row, column) = (shown.list.y + 3, shown.list.x + 3);
        let down = MouseEventKind::Down(MouseButton::Left);
        let mut events = vec![TermEvent::Key(key(KeyCode::Tab)); tabs];
        events.extend([
            TermEvent::Mouse(drag(down, row, column)),
            TermEvent::Mouse(drag(MouseEventKind::Up(MouseButton::Left), row, column)),
            TermEvent::Mouse(drag(down, row, column)),
        ]);
        let _held = crate::input::script::hold(events);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");

        choose_effort(&mut terminal, &mut session, start);

        let case = format!("opened in {start:?}, {tabs} Tab");
        assert_eq!(session.effort(), Some(Effort::High), "{case}");
        assert_eq!(
            session.effort_is_for_this_session(),
            !recorded,
            "{case}: the mark"
        );
        assert_eq!(
            bravebot_session::store::load_effort(),
            Some(if recorded { Effort::High } else { Effort::Low }),
            "{case}: the record"
        );
        assert_eq!(
            crate::input::script::unread(),
            0,
            "{case}: an event was left unread, so the picker ended on the first press"
        );
    }
}

/// VIEW-8: `/checkouts remove` draws a checkout it removed as finished, and a checkout kept
/// because the person declined, one it does not keep, and one it could not remove as failed,
/// apart from the ink every note shares.
#[test]
fn checkouts_remove_draws_what_it_removed_apart_from_what_it_refused() {
    use crate::testutil::inks_on_row_containing;
    use bravebot_agent::workspace::Unremoved;

    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();

    let mut session = Session::new("none");
    remove_checkout(
        &mut session,
        vec![kept_checkout(false)],
        "c2",
        |_| panic!("asked about a checkout nothing was done in"),
        |id| {
            assert_eq!(id, "c2");
            Ok(())
        },
    );
    let removed = inks_on_row_containing(&session, "is removed");

    let mut session = Session::new("none");
    remove_checkout(
        &mut session,
        vec![kept_checkout(true)],
        "c2",
        |_| crate::confirm::Answer::Reject,
        |_| panic!("removed without a yes"),
    );
    let kept = inks_on_row_containing(&session, "is kept");

    let mut session = Session::new("none");
    remove_checkout(
        &mut session,
        vec![kept_checkout(false)],
        "c3",
        |_| panic!("asked about a checkout the session does not keep"),
        |_| panic!("removed a checkout the session does not keep"),
    );
    let no_such = inks_on_row_containing(&session, "keeps no checkout");

    let mut session = Session::new("none");
    remove_checkout(
        &mut session,
        vec![kept_checkout(false)],
        "c2",
        |_| crate::confirm::Answer::Approve,
        |_| Err(Unremoved::Stuck),
    );
    let not_removed = inks_on_row_containing(&session, "could not be removed");

    assert!(
        removed.iter().all(|ink| *ink == crate::theme::ok()),
        "a checkout removed is not drawn as finished: {removed:?}"
    );
    assert!(
        !removed.contains(&crate::theme::note()),
        "the removal is drawn in the ink every note shares"
    );
    for inks in [&kept, &no_such, &not_removed] {
        assert!(
            inks.iter().all(|ink| *ink == crate::theme::fail()),
            "a refusal of /checkouts remove is not drawn as failed: {inks:?}"
        );
    }
    for inks in [&removed, &kept, &no_such, &not_removed] {
        assert!(
            !inks.contains(&crate::theme::running()),
            "a note is drawn in the ink that marks untrusted content: {inks:?}"
        );
    }
}

/// `/thinking` is carried out as typed, mid-turn included, since the thinking is most worth
/// seeing while it arrives, and it turns what is drawn on and off and nothing else.
#[test]
fn the_thinking_command_toggles_what_is_drawn_and_runs_mid_turn() {
    let mut session = Session::new("none");
    assert_eq!(
        commands()
            .into_iter()
            .find(|command| command.name == THINKING_COMMAND)
            .map(|command| command.mid_turn),
        Some(MidTurn::Runs)
    );
    session.thinking("the user wants a file");
    assert_eq!(session.thinking_so_far(), "");
    assert_eq!(
        dispatch_command(&mut session, commanded(THINKING_COMMAND)),
        Action::Redraw
    );
    assert_eq!(session.thinking_so_far(), "the user wants a file");
    assert_eq!(
        dispatch_command(&mut session, commanded(THINKING_COMMAND)),
        Action::Redraw
    );
    assert_eq!(session.thinking_so_far(), "");
}

/// VIEW-8: `/export` draws writing the transcript as finished and a name already taken as
/// failed, apart from the ink every note shares.
#[test]
fn export_draws_what_it_wrote_apart_from_what_it_refused() {
    use crate::testutil::inks_on_row_containing;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();

    let (root, stored) = exporting_into("bravebot-app-export-inks-test");
    let mut session = Session::new("none");
    answered(&mut session, "first", "the answer");

    export_session(&mut session, &stored, &root, Some("notes/chat.md"));
    let written = inks_on_row_containing(&session, "exported transcript");

    export_session(&mut session, &stored, &root, Some("notes/chat.md"));
    let refused = inks_on_row_containing(&session, "could not export");

    assert!(
        written.iter().all(|ink| *ink == crate::theme::ok()),
        "the export is not drawn as finished: {written:?}"
    );
    assert!(
        !written.contains(&crate::theme::note()),
        "the export is drawn in the ink every note shares"
    );
    assert!(
        refused.iter().all(|ink| *ink == crate::theme::fail()),
        "the refused export is not drawn as failed: {refused:?}"
    );
    for inks in [&written, &refused] {
        assert!(
            !inks.contains(&crate::theme::running()),
            "a note is drawn in the ink that marks untrusted content: {inks:?}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: `/rename` draws a name it took as finished, and a name with nothing in it or with
/// nothing typed at all as failed, apart from the ink every note shares.
#[test]
fn rename_draws_the_name_it_took_apart_from_what_it_refused() {
    use crate::testutil::inks_on_row_containing;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();

    let (root, mut stored) = exporting_into("bravebot-app-rename-inks-test");
    let mut session = Session::new("none");
    rename_session(&mut session, &mut stored, "the parser bug");
    let renamed = inks_on_row_containing(&session, "renamed to");

    let mut session = Session::new("none");
    rename_session(&mut session, &mut stored, "");
    let needs_a_name = inks_on_row_containing(&session, "as in /rename the parser bug");

    let mut session = Session::new("none");
    rename_session(&mut session, &mut stored, "   ");
    let needs_something = inks_on_row_containing(&session, "with something in it");

    assert!(
        renamed.iter().all(|ink| *ink == crate::theme::ok()),
        "the rename is not drawn as finished: {renamed:?}"
    );
    assert!(
        !renamed.contains(&crate::theme::note()),
        "the rename is drawn in the ink every note shares"
    );
    for inks in [&needs_a_name, &needs_something] {
        assert!(
            inks.iter().all(|ink| *ink == crate::theme::fail()),
            "a refused rename is not drawn as failed: {inks:?}"
        );
    }
    for inks in [&renamed, &needs_a_name, &needs_something] {
        assert!(
            !inks.contains(&crate::theme::running()),
            "a note is drawn in the ink that marks untrusted content: {inks:?}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: `/cd` draws moving to a directory as finished, and a directory with no name typed at
/// all, or one that cannot be moved to, as failed, apart from the ink every note shares.
#[test]
fn cd_draws_the_directory_it_moved_to_apart_from_what_it_refused() {
    use crate::testutil::inks_on_row_containing;

    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();

    let root = crate::testutil::scratch_dir("bravebot-app-cd-inks-test");
    let _ = std::fs::remove_dir_all(&root);
    for directory in ["one", "two"] {
        std::fs::create_dir_all(root.join(directory)).expect("scratch");
    }

    let mut workspace = Workspace::new(root.join("one")).expect("workspace");
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
        &root.join("two").display().to_string(),
        |_, _| Some(true),
    );
    assert_eq!(moved, Changed::Moved);
    let changed = inks_on_row_containing(&session, "now working in");

    let mut session = Session::new("none");
    change_directory(
        &mut session,
        &mut workspace,
        &mut answers,
        "a-session",
        "",
        |_, _| panic!("asked about a /cd with no path"),
    );
    let needs_a_path = inks_on_row_containing(&session, "/cd needs a directory");

    let mut session = Session::new("none");
    change_directory(
        &mut session,
        &mut workspace,
        &mut answers,
        "a-session",
        &root.join("nowhere-here").display().to_string(),
        |_, _| Some(true),
    );
    let not_changed = inks_on_row_containing(&session, "could not move to");

    assert!(
        changed.iter().all(|ink| *ink == crate::theme::ok()),
        "the move is not drawn as finished: {changed:?}"
    );
    assert!(
        !changed.contains(&crate::theme::note()),
        "the move is drawn in the ink every note shares"
    );
    for inks in [&needs_a_path, &not_changed] {
        assert!(
            inks.iter().all(|ink| *ink == crate::theme::fail()),
            "a refusal of /cd is not drawn as failed: {inks:?}"
        );
    }
    for inks in [&changed, &needs_a_path, &not_changed] {
        assert!(
            !inks.contains(&crate::theme::running()),
            "a note is drawn in the ink that marks untrusted content: {inks:?}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: what `/add-dir` did and what it refused are drawn in different inks, so a directory
/// that was added is not read as one more error beside two refusals. The notes come from the
/// handler, so a call site left on the plain note fails here.
///
/// The failure this rejects is every note of the command drawn in the one ink of the plain
/// notes, which drew the success in the colour of the refusals before it.
#[test]
fn add_dir_draws_the_directory_it_added_apart_from_the_names_it_refused() {
    use crate::testutil::inks_on_row_containing;

    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();
    let root = crate::testutil::scratch_dir("bravebot-add-dir-inks-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");
    for named in ["../nowhere", "", &notes.display().to_string()] {
        add_directory(&mut session, &mut workspace, &mut trust, named);
    }

    let added = inks_on_row_containing(&session, "added ");
    let refused = inks_on_row_containing(&session, "could not add");
    let needs_a_path = inks_on_row_containing(&session, "/add-dir needs a directory");

    assert!(
        added.iter().all(|ink| *ink == crate::theme::ok()),
        "the directory added is not drawn as finished: {added:?}"
    );
    for refusal in [&refused, &needs_a_path] {
        assert!(
            refusal.iter().all(|ink| *ink == crate::theme::fail()),
            "a refusal is not drawn as failed: {refusal:?}"
        );
    }
    assert!(
        !added.contains(&crate::theme::note()),
        "the success is drawn in the ink every note shares"
    );
    for inks in [&added, &refused, &needs_a_path] {
        assert!(
            !inks.contains(&crate::theme::running()),
            "a note is drawn in the ink that marks untrusted content: {inks:?}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: `/add-dir close` draws a close that happened, with or without a rule withdrawn, in
/// the ink for finished, and a name it could not close and a close with no name in the ink for
/// failed. Each is drawn in a session of its own so that a note is found by the words it begins
/// with and not by a word the others share.
#[test]
fn add_dir_close_draws_what_it_closed_apart_from_what_it_refused() {
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();
    let root = crate::testutil::scratch_dir("bravebot-add-dir-close-inks-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let notes = notes.canonicalize().expect("canonical");
    let opened = || {
        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut trust = TrustStore::new(workspace.root());
        add_directory(
            &mut Session::new("none"),
            &mut workspace,
            &mut trust,
            &notes.display().to_string(),
        );
        (workspace, trust)
    };

    let closing = |named: &str, distrusted: bool| {
        let (mut workspace, mut trust) = opened();
        if distrusted {
            trust.distrust(&bravebot_agent::workspace::key_of(&notes));
        }
        let mut session = Session::new("none");
        close_directory(&mut session, &mut workspace, &mut trust, named);
        session
    };
    let notes = notes.display().to_string();
    for (session, needle, ink) in [
        (closing(&notes, false), "closed", crate::theme::ok()),
        (closing(&notes, true), "closed", crate::theme::ok()),
        (
            closing("../project", false),
            "could not close",
            crate::theme::fail(),
        ),
        (
            closing("", false),
            "/add-dir close needs a directory",
            crate::theme::fail(),
        ),
    ] {
        let said = session.transcript.last().expect("a note").text.clone();
        let inks = crate::testutil::inks_on_row_containing(&session, needle);
        assert!(
            inks.iter().all(|drawn| *drawn == ink),
            "{said:?} was not drawn in {ink:?}: {inks:?}"
        );
        assert!(
            !inks.contains(&crate::theme::running()),
            "{said:?} is drawn in the ink that marks untrusted content"
        );
    }
    let withdrawn = closing(&notes, false).transcript.remove(0).text;
    let kept = closing(&notes, true).transcript.remove(0).text;
    assert!(withdrawn.contains("no longer trusting it"), "{withdrawn}");
    assert!(!kept.contains("no longer trusting it"), "{kept}");

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: where the palette draws every ink as the terminal's own, which is what `NO_COLOR`
/// asks for, the notes of `/add-dir` are told apart by what they say, and nothing about how
/// they are drawn depends on a colour.
///
/// The failure this rejects is a tone drawn in a colour of its own and not through the
/// palette, which would paint a person's terminal after they asked for no colour.
#[test]
fn under_a_palette_without_colour_the_wording_tells_what_add_dir_did_from_what_it_refused() {
    use crate::testutil::inks_on_row_containing;
    use ratatui::style::Color;

    /// Puts `brave` back when the test ends, however it ends, so a failed assertion does not
    /// leave the palette without colour in force for the tests that read a colour.
    struct BraveWhenDropped;
    impl Drop for BraveWhenDropped {
        fn drop(&mut self) {
            crate::theme::apply_brave();
        }
    }

    let _held = crate::theme::exclusive();
    let _restored = BraveWhenDropped;
    crate::theme::apply(&crate::theme::Theme {
        name: "no-colour".to_string(),
        palette: crate::theme::plain_palette(),
        adapts: false,
        listed: false,
    });
    let root = crate::testutil::scratch_dir("bravebot-add-dir-no-colour-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");
    add_directory(&mut session, &mut workspace, &mut trust, "../nowhere");
    add_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &notes.display().to_string(),
    );

    let refused = inks_on_row_containing(&session, "could not add");
    let added = inks_on_row_containing(&session, "added ");

    assert!(refused.iter().chain(&added).all(|ink| *ink == Color::Reset));
    let said: Vec<&str> = session.transcript.iter().map(|n| n.text.as_str()).collect();
    assert!(said[0].starts_with("could not add "), "{said:?}");
    assert!(said[1].starts_with("added "), "{said:?}");

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: the directories a settings file names are told in the tones `/add-dir` tells them
/// in. A name that does not open is refused through `named_to_open`, in the message
/// `/add-dir` refuses with, and a name that does open is opened through `open_named`, which
/// goes through the `/add-dir` handler, so the one is a refused note and the other a done note.
///
/// The failures this rejects are the refusal left as a plain note, which would draw the same
/// message in another ink depending on which door the name came in by, and an opening that
/// does not go through the handler and writes its note some other way.
#[test]
fn the_directories_a_settings_file_names_are_told_in_the_tones_of_add_dir() {
    use crate::state::Tone;

    let root = crate::testutil::scratch_dir("bravebot-settings-names-tones-test");
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");

    let accepted = named_to_open(
        &mut session,
        &workspace,
        &["../nowhere".to_string(), "../notes".to_string()],
    );
    open_named(&mut session, &mut workspace, &mut trust, &accepted);

    let said: Vec<(Tone, &str)> = session
        .transcript
        .iter()
        .map(|entry| (entry.tone, entry.text.as_str()))
        .collect();
    assert_eq!(said.len(), 2, "{said:?}");
    assert_eq!(said[0].0, Tone::Refused, "{said:?}");
    assert!(
        said[0].1.starts_with("could not add ../nowhere: "),
        "{said:?}"
    );
    assert_eq!(said[1].0, Tone::Done, "{said:?}");
    assert!(said[1].1.starts_with("added "), "{said:?}");

    std::fs::remove_dir_all(&root).ok();
}

/// VIEW-8: a refused command is not the failure of a turn. A note of a refusal written during a
/// turn that then fails, and another written after it, leave the reason the status area
/// repeats, the reason the turn is recorded under and the one failure an export holds as they
/// were.
///
/// The failure this rejects is a refusal recorded as `Entry::failure`, which an export writes
/// under "Failed", the status area would repeat in place of the turn's reason, and a turn's
/// record would take as its reason when it came first.
#[test]
fn a_refused_command_is_not_taken_for_the_failure_of_the_turn_around_it() {
    use bravebot_aichat::{AccountFailure, ChatError};
    use bravebot_session::sessions::StoredOutcome;

    let mut session = Session::new("none");
    type_line(&mut session, "read a file");
    session.submit().expect("the prompt is sent");
    session.note_refused("could not add ../before: refused during the turn");
    let error = bravebot_agent::backend::BackendError::Attempted {
        usage: None,
        context_tokens: None,
        conceded: Default::default(),
        backend: None,
        retry_setting: bravebot_agent::RetrySetting::None,
        idle_gap: None,
        attempts: 2,
        cause: Box::new(bravebot_agent::backend::BackendError::from(
            ChatError::Account(AccountFailure::NotSignedIn),
        )),
    };
    fold_outcome(
        &mut session,
        Err(turn::TurnError::from(error)),
        Trail::new(),
        Carried {
            asked: Default::default(),
            exposed: Default::default(),
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
        },
        Occupied {
            budget: 1000,
            guessed: false,
            last_request_tokens: 0,
        },
        Asked {
            name: "chatgpt/gpt-test".into(),
            comparable: true,
            configured: "test-model".to_string(),
        },
        Line {
            text: "read a file",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );
    session.record_turn(0, &Conversation::new());
    session.note_refused("could not close ../after: refused after the turn");
    assert!(session.finished.expect("the turn ended").failed());

    let said = session.failure_said().expect("the turn's reason");
    assert!(said.contains("not signed in"), "{said}");
    let recorded = session.turn_history().last().expect("the turn is recorded");
    match &recorded.outcome {
        Some(StoredOutcome::Failed { reason }) => {
            assert!(reason.contains("not signed in"), "{reason}")
        }
        other => panic!("the turn was not recorded as failed: {other:?}"),
    }
    let exported = render::as_markdown(&session, "test");
    assert_eq!(exported.matches("## Failed").count(), 1, "{exported}");
}

/// FILES-10: `/rewind` removes the edits of the turns it undoes, and the state ends once none is
/// left, since the list is read from the transcript the rewind cut.
#[test]
fn undoing_a_turn_removes_its_edits_from_the_list() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-files");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("none");
    session.note_layout(crate::state::Laid {
        columns: 120,
        ..crate::state::Laid::default()
    });
    let mut conversation = Conversation::new();

    for (prompt, path) in [("first change", "a.rs"), ("second change", "b.rs")] {
        let start = conversation.recounted().len();
        type_line(&mut session, prompt);
        session.submit().expect("the prompt is sent");
        let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
        session.rewind.open(point, prompt.to_string());
        session.prompt_recorded(conversation.recounted().len());
        let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
        session.transcript.push(crate::state::Entry::tool(
            bravebot_agent::report::Activity::running("Update", path)
                .of_tool("edit_file")
                .with_changes(diff.condensed(3))
                .with_written(Some(bravebot_agent::report::Written {
                    landed: path.into(),
                    added: diff.added(),
                    removed: diff.removed(),
                    numbers: diff.condensed_numbers(3),
                }))
                .done("done"),
        ));
        conversation.push(Message::user(prompt));
        conversation.push(Message::assistant("done"));
        session.complete("done", vec![], 10);
        session.record_turn(start, &conversation);
    }
    session.toggle_files();
    let paths = |session: &Session| -> Vec<String> {
        session
            .written_files()
            .into_iter()
            .map(|file| file.path)
            .collect()
    };
    assert_eq!(paths(&session), ["a.rs", "b.rs"]);
    assert!(session.files_open());

    rewind(
        &mut session,
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
        paths(&session),
        ["a.rs"],
        "the undone turn's edit is still listed"
    );
    assert!(session.files_open(), "the state ended with an edit left");

    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        1,
    );
    assert!(paths(&session).is_empty());
    assert!(
        !session.files_open(),
        "the state stayed open with nothing to list"
    );
}

/// VIEW-32: a number in a reply links into the repository of the directory the session is in,
/// so a move to another checkout changes where it links and a move to one with no GitHub
/// origin ends the links. Left as it was, a number would open an issue of the repository the
/// session had left.
#[test]
fn moving_to_another_checkout_changes_the_repository_numbers_link_into() {
    let root = crate::testutil::scratch_dir("bravebot-cd-repository-test");
    let _ = std::fs::remove_dir_all(&root);
    let remotes = [
        ("a", "git@github.com:brave/first.git"),
        ("b", "https://github.com/netzenbot/second"),
        ("c", "https://gitlab.com/brave/third.git"),
    ];
    for (name, remote) in remotes {
        let git = root.join(name).join(".git");
        std::fs::create_dir_all(&git).expect("scratch");
        std::fs::write(
            git.join("config"),
            format!("[remote \"origin\"]\n\turl = {remote}\n"),
        )
        .expect("write config");
    }

    let mut workspace = Workspace::new(root.join("a")).expect("workspace");
    let mut session = Session::new("none");
    session.adopt_repository(crate::issue_link::Repository::of(workspace.root()));
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    let linking_to = |session: &Session| {
        session
            .repository()
            .and_then(|repository| repository.address("7"))
    };
    assert_eq!(
        linking_to(&session).as_deref(),
        Some("https://github.com/brave/first/issues/7")
    );

    for (name, wanted) in [
        ("b", Some("https://github.com/netzenbot/second/issues/7")),
        ("c", None),
    ] {
        let target = root.join(name);
        assert_eq!(
            change_directory(
                &mut session,
                &mut workspace,
                &mut answers,
                "a-session",
                target.to_str().expect("utf-8 path"),
                |_, _| Some(true),
            ),
            Changed::Moved
        );
        assert_eq!(
            linking_to(&session).as_deref(),
            wanted,
            "after moving to {name}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn failure_reporting_uses_safe_fields_in_the_transcript_and_status() {
    let mut session = Session::new("none");
    type_line(&mut session, "read a file");
    session.submit().unwrap();
    let error = bravebot_agent::backend::BackendError::Attempted {
        usage: None,
        context_tokens: None,
        conceded: Default::default(),
        backend: None,
        retry_setting: bravebot_agent::RetrySetting::None,
        idle_gap: None,
        attempts: 3,
        cause: Box::new(bravebot_agent::backend::BackendError::from(
            bravebot_aichat::ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://example.test/PRIVATE_PATH?key=PRIVATE_QUERY".into(),
                status: 503,
                retry_after: None,
                body: None,
            }),
        )),
    };
    fold_outcome(
        &mut session,
        Err(turn::TurnError::from(error)),
        Trail::new(),
        Carried {
            asked: Default::default(),
            exposed: Default::default(),
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
        },
        Occupied {
            budget: 1000,
            guessed: false,
            last_request_tokens: 0,
        },
        Asked {
            name: "test-model".into(),
            comparable: true,
            configured: "test-model".to_string(),
        },
        Line {
            text: "read a file",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );
    let reason = session.failure_said().expect("visible reason");
    assert!(reason.contains("503"), "{reason}");
    assert!(reason.contains("3 attempts"), "{reason}");
    let exported = render::as_markdown(&session, "test");
    for secret in ["PRIVATE_PATH", "PRIVATE_QUERY", "example.test"] {
        assert!(!reason.contains(secret), "{reason}");
        assert!(!exported.contains(secret), "{exported}");
    }
    assert_eq!(exported.matches("503").count(), 1);
    assert!(session.finished.unwrap().failed());
}

/// A session wide enough for the panel with `src/a.rs` written once.
fn session_that_wrote() -> Session {
    let mut session = Session::new("none");
    session.note_layout(crate::state::Laid {
        columns: 120,
        ..crate::state::Laid::default()
    });
    session
        .transcript
        .push(crate::state::Entry::user("edit it"));
    let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
    session.transcript.push(crate::state::Entry::tool(
        bravebot_agent::report::Activity::running("Update", "src/a.rs")
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(bravebot_agent::report::Written {
                landed: "src/a.rs".into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("added 1 line, removed 1 line"),
    ));
    session
}

/// FILES-3: `/diff` draws the panel in its files state, opening it where it was closed, and `q`
/// puts it back as it was: a panel that was open is open and ordinary again.
#[test]
fn diff_opens_the_panel_in_its_files_state_and_q_puts_it_back() {
    let mut session = session_that_wrote();
    session.toggle_panel();
    assert!(session.panel_open() && !session.files_open());
    assert_eq!(
        dispatch_command(&mut session, commanded(DIFF_COMMAND)),
        Action::Redraw
    );
    assert!(session.files_open(), "/diff did not open the state");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('q'))),
        Action::Redraw
    );
    assert!(!session.files_open(), "q did not end the state");
    assert!(session.panel_open(), "the panel that was open was closed");
}

/// FILES-3: a panel `/diff` opened is closed again when the state ends, whichever key ends it,
/// and `/diff` again ends it too.
#[test]
fn leaving_the_state_closes_a_panel_it_opened() {
    for ending in [KeyCode::Char('q'), KeyCode::Esc] {
        let mut session = session_that_wrote();
        dispatch_command(&mut session, commanded(DIFF_COMMAND));
        assert!(session.panel_open());
        handle_key(&mut session, key(ending));
        assert!(!session.panel_open(), "{ending:?} left the panel open");
    }
    let mut session = session_that_wrote();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    assert!(!session.files_open() && !session.panel_open());
}

/// FILES-3, FILES-9: typing `/diff` with the state open reaches nothing, so the state stays, and
/// the box keeps its line. The same line queued earlier and dispatched ends it.
#[test]
fn a_typed_diff_cannot_end_the_state_and_a_queued_one_does() {
    let mut session = session_that_wrote();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    assert!(session.files_open());
    for character in DIFF_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(character)));
    }
    let action = handle_key(&mut session, key(KeyCode::Enter));
    assert_ne!(action, Action::Submit(DIFF_COMMAND.to_string()));
    assert!(session.files_open(), "typing /diff ended the state");
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    assert!(
        !session.files_open(),
        "a queued /diff did not end the state"
    );
}

/// FILES-3: the panel chord hides the panel from the state and ends the state with it.
#[test]
fn the_panel_chord_ends_the_state_with_the_panel() {
    let mut session = session_that_wrote();
    session.toggle_panel();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    handle_key(&mut session, ctrl('x'));
    assert!(!session.files_open(), "the chord left the state open");
    assert!(!session.panel_open(), "the chord left the panel open");
}

/// FILES-3: with no file written `/diff` answers with a note and changes nothing, and on a
/// terminal too narrow it says how wide the terminal has to be.
#[test]
fn diff_with_nothing_written_answers_with_a_note() {
    let mut session = Session::new("none");
    session.note_layout(crate::state::Laid {
        columns: 120,
        ..crate::state::Laid::default()
    });
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    assert!(!session.files_open() && !session.panel_open());
    let said = session.transcript.last().map(|entry| entry.text.clone());
    assert_eq!(said, Some(t!(files_nothing_written).to_string()));

    let mut narrow = session_that_wrote();
    narrow.note_layout(crate::state::Laid {
        columns: 80,
        ..crate::state::Laid::default()
    });
    dispatch_command(&mut narrow, commanded(DIFF_COMMAND));
    assert!(!narrow.files_open());
    let said = narrow.transcript.last().map(|entry| entry.text.clone());
    assert_eq!(
        said,
        Some(t!(files_too_narrow, columns = crate::panel::NARROWEST).to_string())
    );
}

/// FILES-3, CMD-2: typed in full `/diff` opens the state, and no command claims `/files`, so
/// that line completes to nothing, is not carried out mid-turn and is sent as a prompt like any
/// other line that starts with a word nobody claims.
#[test]
fn diff_is_the_command_and_files_is_not_one() {
    assert_eq!(DIFF_COMMAND, "/diff");
    assert!(commands().iter().any(|command| command.name == "/diff"));
    assert!(!commands().iter().any(|command| command.name == "/files"));
    assert_eq!(command_typed("/diff"), Some("/diff"));
    assert_eq!(command_typed("/files"), None);
    assert!(completions("/files").is_empty());
    assert!(!runs_while_working("/files"));

    let mut session = session_that_wrote();
    type_line(&mut session, "/diff");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.files_open(), "typing /diff did not open the state");
    handle_key(&mut session, key(KeyCode::Char('q')));

    type_line(&mut session, "/files");
    let action = handle_key(&mut session, key(KeyCode::Enter));
    assert!(!session.files_open(), "typing /files opened the state");
    assert_eq!(action, Action::Submit("/files".to_string()));
}

/// FILES-9: a character does not reach the box, Enter sends nothing and a paste does not reach
/// the line.
#[test]
fn a_typed_character_does_not_reach_the_box_while_the_files_state_is_open() {
    let mut session = session_that_wrote();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    for c in ['x', 'a', 'z', ' '] {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(session.input(), "");
    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
    assert_eq!(handle_paste(&mut session, "pasted"), Action::None);
    assert_eq!(session.input(), "");
    assert!(session.files_open());
}

/// FILES-9: a key the state does not name does nothing, and leaves the state where it was.
#[test]
fn a_key_the_state_does_not_name_does_nothing() {
    let mut session = session_that_wrote();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    let before = session.files_state().cloned();
    for code in [
        KeyCode::Tab,
        KeyCode::Char('?'),
        KeyCode::Char('['),
        KeyCode::Char(']'),
        KeyCode::Backspace,
        KeyCode::F(5),
    ] {
        assert_eq!(
            handle_key(&mut session, key(code)),
            Action::None,
            "{code:?}"
        );
    }
    assert_eq!(handle_key(&mut session, ctrl('a')), Action::None);
    assert_eq!(session.files_state().cloned(), before);
    assert!(session.files_open());
}

/// FILES-9: the line keeps its text and caret while the state is open and has both when it ends.
#[test]
fn the_line_comes_back_untouched_when_the_state_ends() {
    let mut session = session_that_wrote();
    for c in "half a prompt".chars() {
        session.type_char(c);
    }
    handle_key(&mut session, key(KeyCode::Left));
    let caret = session.caret();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Char('q')));
    assert_eq!(session.input(), "half a prompt");
    assert_eq!(session.caret(), caret);
}

/// FILES-9: Ctrl-C ends the state and does nothing else, so a turn in flight goes on.
#[test]
fn ctrl_c_ends_the_state_and_leaves_the_turn_running() {
    let mut session = session_that_wrote();
    session.status = Status::Working;
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    assert!(
        !stops_the_turn(&session, ctrl('c')),
        "Ctrl-C reaches the turn while the state is open"
    );
    let action = handle_key_while_working(&mut session, ctrl('c'));
    assert_ne!(action, Action::Cancel, "Ctrl-C reached the turn");
    assert!(!session.files_open(), "Ctrl-C did not end the state");
    assert_eq!(session.status, Status::Working);
    // The next press is the one that reaches the turn.
    assert!(stops_the_turn(&session, ctrl('c')));
}

/// FILES-3: entering and leaving the state writes nothing to the file that keeps the panel's
/// choice, so a panel left closed stays closed for the next session.
#[test]
fn entering_and_leaving_the_state_writes_no_panel_choice() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = session_that_wrote().with_stored_history();
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    handle_key(&mut session, key(KeyCode::Char('q')));
    assert_eq!(bravebot_session::store::load_panel(), None);
    bravebot_session::store::save_panel(true);
    dispatch_command(&mut session, commanded(DIFF_COMMAND));
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(bravebot_session::store::load_panel(), Some(true));
}
