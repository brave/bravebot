//! CMD-8: a command carried out is remembered in the prompt history as a prompt is, once, in the
//! form a person can recall and press Enter on again (CMD-1).

use super::*;

/// What the history holds, oldest first.
fn remembered(session: &Session) -> Vec<&str> {
    session
        .history
        .entries()
        .iter()
        .map(|entry| entry.prompt.as_str())
        .collect()
}

/// A command carried out at rest is recalled with Up and, pressed again, is a command again.
/// Rejects: a command left out of the history, and a recalled line that is sent as a prompt.
#[test]
fn a_command_carried_out_at_rest_is_recalled_and_runs_again() {
    let mut session = Session::new("none");
    type_line(&mut session, "/cost");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(remembered(&session), ["/cost"]);

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        "/cost",
        "Up did not bring the command back"
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.input().is_empty());
    assert!(
        !session.a_turn_is_running(),
        "the recalled command was sent as a prompt"
    );
}

/// A command carried out as it is typed while a turn runs is remembered like one at rest, after
/// the prompt that started the turn. Rejects: only the at-rest road remembering a command.
#[test]
fn a_command_carried_out_mid_turn_is_remembered() {
    let mut session = a_turn_running_on("first");
    assert_eq!(type_while_working(&mut session, "/cost"), Action::Redraw);
    assert!(session.queued.is_empty(), "/cost waited");

    assert_eq!(remembered(&session), ["first", "/cost"]);
}

/// A command that waited was remembered when it was typed, so releasing it adds nothing, even
/// after a line typed behind it. Rejects: the release recording the line a second time, which
/// shows as the command again after the prompt queued behind it.
#[test]
fn a_command_that_waited_is_remembered_once_and_not_when_it_is_released() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, COMPACT_COMMAND);
    type_while_working(&mut session, "later");
    assert_eq!(remembered(&session), ["first", COMPACT_COMMAND, "later"]);

    session.complete("answered", Vec::new(), 0);
    queued_next(&mut session);

    assert_eq!(remembered(&session), ["first", COMPACT_COMMAND, "later"]);
}

/// A folded paste in a command is remembered as its words, since the marker is a handle only this
/// session can undo. Rejects: the placeholder going into the history.
#[test]
fn a_command_with_a_paste_is_remembered_as_the_words_pasted() {
    let mut session = Session::new("none");
    type_line(&mut session, "/note ");
    session.paste_text("one\ntwo\nthree\n");
    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(remembered(&session), ["/note one\ntwo\nthree"]);
}

/// Carry out each of `lines` at rest, so the history holds them in order.
fn carried_out(session: &mut Session, lines: &[&str]) {
    for line in lines {
        type_line(session, line);
        handle_key(session, key(KeyCode::Enter));
        assert_eq!(
            remembered(session).last().copied(),
            Some(*line),
            "{line} was not carried out"
        );
    }
}

/// A recalled command is a lone slash word, so the list of commands is open under it, and Up and
/// Down still walk the history rather than the list: every command remembered is reachable, not
/// only the newest. Rejects: the arrows going to the list while the line is a recalled entry,
/// which stops the walk at the newest command.
#[test]
fn up_and_down_walk_past_a_recalled_command_while_the_list_is_open() {
    let mut session = Session::new("none");
    carried_out(&mut session, &["/status", "/cost"]);

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "/cost");
    assert!(session.is_completing(), "the list is not open on /cost");

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "/status", "a second Up did not walk on");

    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.input(), "/cost", "Down did not walk back");
    handle_key(&mut session, key(KeyCode::Down));
    assert!(session.input().is_empty(), "Down did not leave the history");
    assert!(!session.history.is_browsing());
}

/// The same walk while a turn runs, where the commands are offered as at rest. Rejects: the
/// running-turn path giving the arrows to the list.
#[test]
fn up_walks_past_a_recalled_command_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/status");
    type_while_working(&mut session, "/cost");

    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "/cost");
    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "/status", "a second Up did not walk on");
    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        "first",
        "a third Up did not reach the prompt"
    );
}

/// A recalled prompt that ends in a file reference has the list of files open under it, and the
/// arrows walk the history there too. Rejects: the file list taking the arrows from a recalled
/// entry.
#[test]
fn up_walks_past_a_recalled_prompt_while_the_file_list_is_open() {
    let root = crate::testutil::scratch_dir("bravebot-recalled-file-list-test");
    std::fs::create_dir_all(&root).expect("scratch directory");
    std::fs::write(root.join("Cargo.toml"), "").expect("scratch file");
    let mut session = Session::new("none").in_workspace(&root);
    session.adopt_workspace(&Workspace::new(&root).expect("workspace"));
    session.history.push("an older prompt", None);
    session.history.push("look at @Cargo", None);

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "look at @Cargo");
    assert!(
        matches!(session.offered(), crate::state::Offered::Files(_)),
        "no file list is open on the recalled line: {:?}",
        session.offered()
    );

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "an older prompt");
    std::fs::remove_dir_all(&root).ok();
}

/// Editing a recalled command leaves the history, and the arrows are the list's again. Rejects:
/// the arrows staying with the history after the line was changed, which would make the list
/// unreachable by arrow for the rest of the line.
#[test]
fn the_arrows_return_to_the_list_once_a_recalled_command_is_edited() {
    let mut session = Session::new("none");
    carried_out(&mut session, &["/status", "/cost"]);
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Backspace));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert_eq!(session.input(), "/co");
    assert!(!session.history.is_browsing(), "the edit kept the walk");
    let first = session.highlighted_completion();

    handle_key(&mut session, key(KeyCode::Down));
    assert_ne!(
        session.highlighted_completion(),
        first,
        "Down left the list"
    );
    assert_eq!(session.input(), "/co", "Down walked the history");
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.highlighted_completion(), first, "Up left the list");
    assert_eq!(session.input(), "/co");
}
