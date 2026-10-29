//! ADDRESS-2 and MEMORY-14 in the terminal: an `@` offers the definitions a message may be
//! addressed to ahead of the files, a pick leaves a marker in the line and addresses the message the
//! way `/agent` does, and taking the marker out takes the pick away.

use super::*;

fn rows() -> Vec<bravebot_mentions::Entry> {
    vec![
        bravebot_mentions::definition_entry("lantern", "Keeps the lights."),
        bravebot_mentions::definition_entry("reviewer", "Reviews a diff."),
    ]
}

/// A session over a directory holding one file, with the definitions held as the loop holds them
/// once a reference is being typed, and `typed` in the box.
fn typing(name: &str, typed: &str) -> (Session, std::path::PathBuf) {
    let root = crate::testutil::scratch_dir(&format!("bravebot-summoning-{name}"));
    std::fs::create_dir_all(&root).expect("scratch");
    std::fs::write(root.join("lantern-notes.md"), "text").expect("a file");
    let mut session = Session::new("none").in_workspace(&root);
    type_line(&mut session, typed);
    session.settle_definitions(rows);
    (session, root)
}

/// The screen the session draws, one string per row.
fn screen(session: &mut Session) -> Vec<String> {
    let buffer = drawn(session);
    let area = buffer.area;
    (area.top()..area.bottom())
        .map(|y| {
            (area.left()..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// ADDRESS-2. Rejects: the definitions left out of the list, listed after the files, or drawn
/// without what each is for.
#[test]
fn the_definitions_are_listed_ahead_of_the_files_with_what_each_is_for() {
    let (mut session, root) = typing("the_definitions_are_listed_ahead_of_the_", "ask @l");
    let rows = screen(&mut session);
    let at = |needle: &str| rows.iter().position(|row| row.contains(needle));
    let definition = at("Keeps the lights.").expect("the definition is not drawn");
    assert!(rows[definition].contains("lantern"), "{rows:?}");
    let file = at("lantern-notes.md")
        .unwrap_or_else(|| panic!("the file is not drawn: {rows:#?} {:?}", session.offered()));
    assert!(definition < file, "the file is listed first: {rows:?}");
    std::fs::remove_dir_all(&root).ok();
}

/// ADDRESS-2. Rejects: a word that holds no definition's name offering one.
#[test]
fn a_word_holding_no_name_offers_no_definition() {
    let (session, root) = typing("a_word_holding_no_name_offers_no_definit", "ask @zzz");
    assert!(matches!(session.offered(), crate::state::Offered::Nothing));
    std::fs::remove_dir_all(&root).ok();
}

/// MEMORY-14. Rejects: a pick naming the definition in the line as a file, or leaving the word
/// half typed, or sending on the first Enter.
#[test]
fn picking_a_definition_writes_its_marker_and_names_no_file() {
    let (mut session, root) = typing("picking_a_definition_writes_its_marker_a", "@l");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw,
        "Enter on a row picks it and sends nothing"
    );
    assert_eq!(session.input(), "[@lantern] ");
    assert!(session.pick_stands());
    assert!(
        bravebot_mentions::referenced(session.input()).is_empty(),
        "the marker names a file"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// ADDRESS-2: the line takes the road `/agent <name> <task>` takes, with the marker taken out of
/// the task. Rejects: the marker reaching the task, or the box keeping the line.
#[test]
fn a_picked_definition_addresses_the_line_as_the_agent_command_does() {
    let (mut session, root) = typing("a_picked_definition_addresses_the_line_a", "@l");
    handle_key(&mut session, key(KeyCode::Enter));
    type_line(&mut session, "what changed?");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Address("lantern what changed?".to_string(), Vec::new(), Vec::new())
    );
    assert!(session.input().is_empty());
    assert!(!session.pick_stands(), "the pick outlived the line");
    std::fs::remove_dir_all(&root).ok();
}

/// A marker typed or pasted after the line it belonged to was sent is words, as a dropped file's
/// is (DROP-6). Rejects: a pick that outlives its line addressing the next one.
#[test]
fn a_marker_typed_after_the_line_was_sent_addresses_nothing() {
    let (mut session, root) = typing("a_marker_typed_after_the_line_was_sent_a", "@l");
    handle_key(&mut session, key(KeyCode::Enter));
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    type_line(&mut session, "[@lantern] again");
    assert!(!session.pick_stands());
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("[@lantern] again".to_string())
    );
    std::fs::remove_dir_all(&root).ok();
}

/// MEMORY-14. Rejects: a line whose marker was deleted still addressing the definition: the marker
/// is what says the pick stands.
#[test]
fn taking_the_marker_out_sends_an_ordinary_prompt() {
    let (mut session, root) = typing("taking_the_marker_out_sends_an_ordinary_", "@l");
    handle_key(&mut session, key(KeyCode::Enter));
    type_line(&mut session, "what changed?");
    handle_key(&mut session, key(KeyCode::Home));
    for _ in 0.."[@lantern] ".len() {
        handle_key(&mut session, key(KeyCode::Delete));
    }
    assert_eq!(session.input(), "what changed?");

    assert!(!session.pick_stands());
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what changed?".to_string())
    );
    std::fs::remove_dir_all(&root).ok();
}

/// A pick staged at rest and sent while a turn runs waits as the `/agent` line it is, and
/// addresses the definition when the turn ends, not the turn running. Rejects: the marker queued as
/// a prompt, or the running turn addressed.
#[test]
fn a_picked_line_waits_as_an_agent_command_and_addresses_the_turn_after() {
    let (mut session, root) = typing("a_picked_line_waits_as_an_agent_command_", "@l");
    handle_key(&mut session, key(KeyCode::Enter));
    type_line(&mut session, "check it");
    session.status = crate::state::Status::Working;

    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(session.input().is_empty(), "the line was not taken");
    assert!(
        session.take_addressing().is_none(),
        "the turn already running was addressed"
    );

    session.complete("answered", Vec::new(), 0);
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Address(
            "lantern check it".to_string(),
            Vec::new(),
            Vec::new()
        ))
    );
    std::fs::remove_dir_all(&root).ok();
}

/// A line holds one pick, so taking a second replaces the first. Rejects: the first marker staying
/// in the line, or Enter sending the first name.
#[test]
fn a_second_definition_picked_replaces_the_first_and_only_it_is_sent() {
    let (mut session, root) = typing(
        "a_second_definition_picked_replaces_the_",
        "review the diff @l",
    );
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.input(), "review the diff [@lantern] ");

    type_line(&mut session, "@rev");
    session.settle_definitions(rows);
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw,
        "Enter on the second row sent the line the first pick stood for"
    );
    assert_eq!(session.input(), "review the diff [@reviewer] ");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Address(
            "reviewer review the diff".to_string(),
            Vec::new(),
            Vec::new()
        )
    );
    std::fs::remove_dir_all(&root).ok();
}
