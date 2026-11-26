//! Clicks on controls other than the transcript: the panel's rows, pickers, the permission mode, the prompt box, the files state and waiting prompts.

use super::*;

/// The pointer is reported with no button held, in every terminal, since that is what shows the
/// target under it (MOUSE-51). The last tracking mode requested is the one a terminal that keeps
/// a single tracking state ends in, so it has to be the one that reports every movement, or such
/// a terminal reports none of them.
#[test]
fn every_terminal_is_asked_to_report_the_pointer_and_ends_in_the_mode_that_reports_all_of_it() {
    let asked = asked_for(false);
    let on = modes_left_on(&asked);
    for mode in ["1000", "1002", "1003", "1006"] {
        assert!(on.contains(mode), "mode {mode} is not on: {on:?}");
    }
    let last_tracking = asked
        .split("\x1b[?")
        .skip(1)
        .filter(|request| {
            ["1000h", "1002h", "1003h"]
                .iter()
                .any(|mode| request.starts_with(mode))
        })
        .last()
        .expect("a tracking mode was asked for");
    assert!(
        last_tracking.starts_with("1003h"),
        "the last tracking mode asked for is not all motion: {last_tracking:?}"
    );
}

/// MOUSE-28: a click on the last prompt drawn of a queue too long to draw takes it back with every
/// prompt after it, the ones not drawn included, and a click on the row saying how many more wait
/// takes nothing. The regression it rejects is a click that leaves the undrawn prompts waiting, or
/// one that takes the queue back from the row that counts them.
#[test]
fn a_click_on_the_last_prompt_drawn_takes_the_ones_not_drawn_back_with_it() {
    let (mut session, _) = two_panes();
    session.type_char('a');
    session.submit();
    for at in 0..12 {
        for c in format!("item {at:02}").chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
    }
    let buffer = drawn_at(&mut session, 120, 30);
    let (more, column) = find_cell(&buffer, "more waiting");
    click(&mut session, more, column);
    assert_eq!(session.queued.len(), 12, "the count row took prompts back");
    assert_eq!(session.input(), "");

    let (last, column) = find_cell(&buffer, "item 02");
    click(&mut session, last, column);
    assert_eq!(waiting(&session), ["item 00", "item 01"]);
    let expected: Vec<String> = (2..12).map(|at| format!("item {at:02}")).collect();
    assert_eq!(session.input(), expected.join("\n"));
}

/// MOUSE-28: `remove` on a drawn prompt takes out that one and the next prompt not drawn is drawn
/// in its place, and `clear all` removes every line waiting, the ones not drawn included. The
/// regression it rejects is a clear that stops at the prompts drawn, or a remove that takes the
/// count row's prompts with it.
#[test]
fn remove_and_clear_all_reach_the_prompts_not_drawn() {
    let (mut session, _) = two_panes();
    session.type_char('a');
    session.submit();
    for at in 0..12 {
        for c in format!("item {at:02}").chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
    }
    let buffer = drawn_at(&mut session, 120, 30);
    let (row, column) = find_cell(&buffer, "remove");
    click(&mut session, row, column);
    assert_eq!(session.queued.len(), 11);
    assert_eq!(session.queued[0].prompt, "item 01");

    let buffer = drawn_at(&mut session, 120, 30);
    let (more, _) = find_cell(&buffer, "more waiting");
    let (last, _) = find_cell(&buffer, "item 03");
    assert!(last < more, "the prompt after the removed one is not drawn");
    let (row, column) = find_cell(&buffer, "clear all");
    click(&mut session, row, column);
    assert!(session.queued.is_empty(), "{:?}", waiting(&session));
}

/// MOUSE-28: a click on a waiting prompt's words or on its mark takes that prompt and the ones
/// typed after it back into the box, one to a line in the order they were typed, and leaves the
/// ones before it waiting. The rows are two to a prompt, one apart from the next prompt's.
#[test]
fn a_click_on_a_waiting_prompt_takes_it_and_the_ones_after_it_back_into_the_box() {
    let (mut session, buffer) = working_with_three_waiting();
    let (third, column) = find_cell(&buffer, "third question");
    let reaching = session.interjections();

    // The mark under it is the same prompt's, and the words of the one above are not.
    assert_eq!(click(&mut session, third + 1, column + 2), Action::Redraw);
    assert_eq!(session.input(), "third question\nfourth question");
    assert_eq!(waiting(&session), ["second question"]);
    assert!(session.selection.is_none());
    assert_eq!(reaching.take().as_deref(), Some("second question"));
    assert_eq!(
        reaching.take(),
        None,
        "a prompt taken back was still on its way"
    );
}

/// MOUSE-28: the command a click on the panel's model row queued behind a waiting prompt is no
/// prompt anybody typed, so taking the prompts back, with the key or with a click, leaves it
/// waiting and does not put it in the box, where Enter would send `/model` to the planner as
/// words. The regression it rejects is the line coming back with the prompts as one more row of
/// the box.
#[test]
fn a_click_queued_command_stays_waiting_when_the_prompts_come_back() {
    for by_click in [false, true] {
        let (mut session, _) = panel_of_command_rows(true);
        for c in "second question".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        let buffer = drawn_at(&mut session, 120, 30);
        let (row, column) = find_cell(&buffer, "claude-sonnet");
        click(&mut session, row, column + 1);
        assert_eq!(waiting(&session), ["second question", "/model"]);
        let buffer = drawn_at(&mut session, 120, 30);

        match by_click {
            true => {
                let (row, column) = find_cell(&buffer, "second question");
                assert_eq!(click(&mut session, row, column), Action::Redraw);
            }
            false => {
                handle_key_while_working(&mut session, key(KeyCode::Up));
            }
        }

        assert_eq!(session.input(), "second question", "by click: {by_click}");
        assert_eq!(waiting(&session), ["/model"], "by click: {by_click}");
    }
}

/// MOUSE-28: a click on the oldest waiting prompt is what the key is: the box, the queue and what
/// the turn can still reach are what Up leaves.
#[test]
fn a_click_on_the_oldest_waiting_prompt_leaves_what_the_key_leaves() {
    let (mut clicked, buffer) = working_with_three_waiting();
    let (second, column) = find_cell(&buffer, "second question");
    let reaching = clicked.interjections();
    assert_eq!(click(&mut clicked, second, column), Action::Redraw);

    let (mut keyed, _) = working_with_three_waiting();
    assert_eq!(
        handle_key_while_working(&mut keyed, key(KeyCode::Up)),
        Action::Redraw
    );

    assert_eq!(clicked.input(), keyed.input());
    assert_eq!(
        clicked.input(),
        "second question\nthird question\nfourth question"
    );
    assert_eq!(waiting(&clicked), waiting(&keyed));
    assert!(clicked.queued.is_empty());
    assert_eq!(reaching.take(), None);
}

/// MOUSE-28: a click on the last prompt takes that one alone, and what is still being typed stays
/// below what comes back, as it does when the key takes the queue.
#[test]
fn a_click_on_the_newest_waiting_prompt_takes_that_one_and_keeps_the_line_in_the_box() {
    let (mut session, _) = working_with_three_waiting();
    for c in "half typed".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    let buffer = drawn_at(&mut session, 120, 30);
    let (fourth, column) = find_cell(&buffer, "fourth question");

    assert_eq!(click(&mut session, fourth, column), Action::Redraw);
    assert_eq!(session.input(), "fourth question\nhalf typed");
    assert_eq!(waiting(&session), ["second question", "third question"]);
}

/// MOUSE-28: a click on the row of a command a click queued removes it from the queue. It has no
/// words to put in the box, so the click that used to leave it waiting with nothing said, under a
/// hover that promised to take it back, is the only way to cancel it.
#[test]
fn a_click_on_a_command_a_click_queued_removes_it() {
    let (mut session, _) = panel_of_command_rows(true);
    for c in "second question".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    let buffer = drawn_at(&mut session, 120, 30);
    let (row, column) = find_cell(&buffer, "claude-sonnet");
    click(&mut session, row, column + 1);
    assert_eq!(waiting(&session), ["second question", "/model"]);

    // The row above its mark, since the note the click made also names the command.
    let buffer = drawn_at(&mut session, 120, 30);
    let (mark, column) = find_cell(&buffer, "after the prompt before it");
    assert_eq!(click(&mut session, mark - 1, column), Action::Redraw);

    assert_eq!(waiting(&session), ["second question"]);
    assert!(session.input().is_empty(), "{:?}", session.input());
}

/// The column of the first `word` on `row` of the frame.
fn word_on_row(buffer: &ratatui::buffer::Buffer, row: u16, word: &str) -> u16 {
    let text: String = (0..buffer.area.width)
        .map(|x| buffer[(x, row)].symbol().to_string())
        .collect();
    let at = text
        .find(word)
        .unwrap_or_else(|| panic!("{word:?} not on row {row}: {text:?}"));
    u16::try_from(text[..at].chars().count()).expect("a column")
}

/// MOUSE-28: the word `remove` on a waiting prompt's row removes that prompt and no other, puts
/// nothing in the box, and takes the prompt out of the turn's reach, so the planner is not given
/// a prompt the person removed. Rejects a removal that takes the prompt back into the box, one
/// that leaves its copy where the turn reads it, and one that removes the neighbour.
#[test]
fn the_word_remove_takes_that_prompt_out_of_the_queue_and_out_of_the_turns_reach() {
    let (mut session, buffer) = working_with_three_waiting();
    let reaching = session.interjections();
    let (third, _) = find_cell(&buffer, "third question");
    let column = word_on_row(&buffer, third, "remove");

    assert_eq!(click(&mut session, third, column), Action::Redraw);

    assert_eq!(waiting(&session), ["second question", "fourth question"]);
    assert!(session.input().is_empty(), "{:?}", session.input());
    assert_eq!(reaching.take().as_deref(), Some("second question"));
    assert_eq!(reaching.take().as_deref(), Some("fourth question"));
    assert_eq!(
        reaching.take(),
        None,
        "the removed prompt is still on its way"
    );
}

/// MOUSE-28: the words `clear all` are on the oldest prompt's row once a second waits, and remove
/// every waiting prompt, typed or queued by a click, leaving the box as it was.
#[test]
fn the_words_clear_all_remove_every_waiting_line_and_leave_the_box() {
    let (mut session, _) = panel_of_command_rows(true);
    for line in ["second question", "third question"] {
        for c in line.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
    }
    let buffer = drawn_at(&mut session, 120, 30);
    let (row, column) = find_cell(&buffer, "claude-sonnet");
    click(&mut session, row, column + 1);
    for c in "half typed".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        waiting(&session),
        ["second question", "third question", "/model"]
    );
    let reaching = session.interjections();
    let buffer = drawn_at(&mut session, 120, 30);
    let (second, _) = find_cell(&buffer, "second question");
    let column = word_on_row(&buffer, second, "clear all");

    assert_eq!(click(&mut session, second, column), Action::Redraw);

    assert!(waiting(&session).is_empty(), "{:?}", waiting(&session));
    assert_eq!(session.input(), "half typed");
    assert_eq!(
        reaching.take(),
        None,
        "a removed prompt is still on its way"
    );
}

/// MOUSE-28: `clear all` is drawn on the oldest prompt's row and only once a second waits, a long
/// prompt is cut to leave the words on its row, and a row too narrow to leave a legible few words
/// of the prompt draws no word. Rejects the word on every row, which a click on the wrong one
/// would answer for the whole queue, and a word that pushes the prompt or the row off a narrow
/// screen.
#[test]
fn clear_all_is_drawn_once_two_wait_and_a_narrow_row_draws_no_words() {
    let rows_of = |buffer: &ratatui::buffer::Buffer| -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    };
    let (mut session, _) = two_panes();
    session.type_char('a');
    session.submit();
    for line in ["second question", "third question"] {
        for c in line.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        let rows = rows_of(&drawn_at(&mut session, 120, 30));
        let with = |word: &str| rows.iter().filter(|row| row.contains(word)).count();
        assert_eq!(with("remove"), session.queued.len(), "{rows:?}");
        assert_eq!(
            with("clear all"),
            usize::from(session.queued.len() > 1),
            "{rows:?}"
        );
    }

    let long = format!("fourth {}", "word ".repeat(30));
    for c in long.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    let rows = rows_of(&drawn_at(&mut session, 60, 40));
    let fourth = rows
        .iter()
        .find(|row| row.contains("fourth"))
        .unwrap_or_else(|| panic!("the long prompt is not drawn: {rows:?}"));
    assert!(fourth.contains('…'), "{fourth:?}");
    assert!(fourth.trim_end().ends_with("remove"), "{fourth:?}");
    assert_eq!(fourth.chars().count(), 60, "{fourth:?}");

    let rows = rows_of(&drawn_at(&mut session, 20, 40));
    assert!(
        rows.iter().any(|row| row.contains("second question")),
        "{rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("remove")),
        "a word was drawn where the prompt has no room: {rows:?}"
    );
}

/// MOUSE-28: removing a prompt counts from the newest, so a prompt the turn has taken but the
/// session has not yet heard about does not make the removal drop the wrong one, and a prompt the
/// turn has taken is not removed at all. Rejects counting from the oldest.
#[test]
fn removing_a_prompt_the_turn_took_changes_nothing_and_one_after_it_is_the_one_removed() {
    let (mut session, buffer) = working_with_three_waiting();
    let reaching = session.interjections();
    assert_eq!(reaching.take().as_deref(), Some("second question"));
    let (third, _) = find_cell(&buffer, "third question");
    let column = word_on_row(&buffer, third, "remove");
    assert_eq!(click(&mut session, third, column), Action::Redraw);
    assert_eq!(waiting(&session), ["second question", "fourth question"]);
    assert_eq!(
        reaching.take().as_deref(),
        Some("fourth question"),
        "the wrong prompt was removed from the turn's reach"
    );

    let (mut session, buffer) = working_with_three_waiting();
    let reaching = session.interjections();
    while reaching.take().is_some() {}
    let (second, _) = find_cell(&buffer, "second question");
    let column = word_on_row(&buffer, second, "remove");
    assert_eq!(click(&mut session, second, column), Action::Copy);
    assert_eq!(
        session.queued.len(),
        3,
        "a prompt the turn took was removed"
    );
}

/// MOUSE-28: the rows beside the queue, a drag from a waiting prompt, a double click on it, a press
/// that began elsewhere, the info panel on its row, and a click where nothing waits take nothing
/// back, each against a fixture where the plain click does.
#[test]
fn a_click_off_a_waiting_prompt_or_a_drag_over_one_takes_nothing_back() {
    let (mut session, buffer) = working_with_three_waiting();
    let (second, column) = find_cell(&buffer, "second question");
    let last_mark = second + 5;
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1
        .x;

    // The row above the first prompt is the box's, the row below the last mark is the hint's.
    click(&mut session, second - 1, column);
    click(&mut session, last_mark + 1, column);
    click(&mut session, second, panel + 1);
    assert_eq!(session.queued.len(), 3);
    assert!(session.input().is_empty());

    swept(&mut session, (second, column), (second, column + 3));
    let up = drag(MouseEventKind::Up(MouseButton::Left), second, column + 3);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert_eq!(session.queued.len(), 3);

    session.last_click = None;
    press(&mut session, second, column, KeyModifiers::NONE);
    press(&mut session, second, column, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), second, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert_eq!(session.queued.len(), 3);

    session.clear_selection();
    session.last_click = None;
    press(&mut session, 0, 0, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), second, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert_eq!(session.queued.len(), 3);

    session.clear_selection();
    session.last_click = None;
    assert_eq!(click(&mut session, second, column), Action::Redraw);
    assert!(
        session.queued.is_empty(),
        "the fixture's click took nothing"
    );

    // The frame that showed the rows is the last one drawn, but nothing waits now.
    session.last_click = None;
    let input = session.input().to_string();
    click(&mut session, second, column);
    assert_eq!(session.input(), input);
}

/// MOUSE-28: a line put away sits between the box and the queue, so the rows the waiting prompts
/// are drawn on move down with it, and a click on a prompt takes that prompt and no other.
#[test]
fn a_line_put_away_above_the_queue_does_not_move_what_a_click_takes() {
    let (mut session, _) = working_with_three_waiting();
    for c in "put away".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, ctrl('s'));
    assert!(session.stashed().is_some(), "nothing was put away");
    let buffer = drawn_at(&mut session, 120, 30);
    let (stashed, _) = find_cell(&buffer, "put away");
    let (third, column) = find_cell(&buffer, "third question");
    assert!(third > stashed, "the queue is under the line put away");

    // The mark under the words, which is where a row counted one short would be the next prompt's.
    assert_eq!(click(&mut session, third + 1, column), Action::Redraw);

    assert_eq!(session.input(), "third question\nfourth question");
    assert_eq!(waiting(&session), ["second question"]);
}

/// MOUSE-28: a file attached to the line in the box is drawn between the box and the queue, so
/// the rows the waiting prompts are drawn on move down with it, and a click on a prompt takes
/// that prompt and no other.
#[test]
fn a_file_attached_above_the_queue_does_not_move_what_a_click_takes() {
    let directory = crate::testutil::scratch_dir("bravebot-app-queue-attached");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    std::fs::write(directory.join("shot.png"), [0x89u8, 0x50]).expect("write");
    let mut session = Session::new("kernel-enforced").in_workspace(&directory);
    session.type_char('a');
    session.submit();
    for line in ["second question", "third question", "fourth question"] {
        for c in line.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
    }
    let path = directory.join("shot.png").to_string_lossy().to_string();
    assert!(session.drop_files(&path), "not recognised as a drop");
    assert_eq!(session.attached_to_the_line().count(), 1);
    let buffer = drawn_at(&mut session, 120, 30);
    let (attached, _) = find_cell(&buffer, "shot.png");
    let (third, column) = find_cell(&buffer, "third question");
    assert!(third > attached, "the queue is under the attached file");

    // The mark under the words, which is where a row counted one short would be the next prompt's.
    assert_eq!(click(&mut session, third + 1, column), Action::Redraw);

    assert!(
        session
            .input()
            .starts_with("third question\nfourth question"),
        "{:?}",
        session.input()
    );
    assert_eq!(waiting(&session), ["second question"]);
    let _ = std::fs::remove_dir_all(&directory);
}

/// MOUSE-28: a prompt the turn has already been given is not given back, and the click leaves the
/// box as it was.
#[test]
fn a_click_on_a_prompt_the_turn_has_taken_leaves_the_box_alone() {
    let (mut session, buffer) = working_with_three_waiting();
    let reaching = session.interjections();
    let (second, column) = find_cell(&buffer, "second question");
    while reaching.take().is_some() {}

    click(&mut session, second, column);

    assert!(session.input().is_empty(), "{:?}", session.input());
    assert_eq!(session.queued.len(), 3);
}

/// MOUSE-28: a picker or a history search over the screen takes the click's attention, so a click
/// on a waiting prompt under it takes nothing back.
#[test]
fn a_click_on_a_waiting_prompt_takes_nothing_back_where_another_mode_is_in_use() {
    type Setup = fn(&mut Session);
    let modes: [(&str, Setup); 2] = [
        ("a history search", |session| {
            session.history =
                crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                    "old one", None,
                )]);
            assert!(session.open_history_search(), "no search opened");
        }),
        ("a picker", |session| {
            session.open_picking(crate::picking::Picking::Effort(
                crate::effort_prompt::Picker::new(None),
            ));
        }),
    ];
    for (name, setup) in modes {
        let (mut session, buffer) = working_with_three_waiting();
        let (second, column) = find_cell(&buffer, "second question");
        setup(&mut session);
        click(&mut session, second, column);
        assert_eq!(
            session.queued.len(),
            3,
            "{name}: a click took a prompt back"
        );
    }
}

/// MOUSE-11: a click on the panel's model row asks for the model picker and one on its effort
/// row for the effort picker, and neither leaves a selection behind.
#[test]
fn a_click_on_the_panels_model_or_effort_row_opens_that_picker() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    assert_eq!(
        click(&mut session, row, column),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    assert!(session.selection.is_none());

    let (row, column) = find_on(&buffer, "effort");
    assert_eq!(
        click(&mut session, row, column),
        Action::ChooseEffort(crate::model_prompt::Scope::EverySession)
    );
    assert!(session.selection.is_none());
}

/// MOUSE-11, MOUSE-29. A click on the panel's effort row asks for the picker that records
/// the pick for every session, at rest and while a turn runs, also when the level in force is
/// one chosen for this session only: the click stands for the bare `/effort`, which has no
/// scope to carry over. The faults it rejects are a click that opens the picker in the scope of
/// the level in force, so a person who clicks to change a recorded level finds the pick will
/// record nothing, and a click that opens the picker in the scope that records nothing.
#[test]
fn a_click_on_the_panels_effort_row_opens_the_every_session_picker_whatever_is_in_force() {
    use crate::model_prompt::Scope;
    use bravebot_aichat::protocol::Effort;
    let (mut session, _) = panel_with_a_model();
    session.choose_effort_for_this_session(Some(Effort::Max));
    let buffer = drawn(&mut session);
    let (row, column) = find_on(&buffer, "effort");
    let action = click(&mut session, row, column);
    assert_eq!(action, Action::ChooseEffort(Scope::EverySession));
    let Action::ChooseEffort(scope) = action else {
        unreachable!()
    };
    open_effort_picker(&mut session, scope);
    assert_eq!(
        the_scope_of_the_open_effort_picker(&session),
        Scope::EverySession
    );
    assert!(
        session.effort_is_for_this_session(),
        "opening the picker changed the level in force"
    );

    let (mut working, _) = panel_of_command_rows(true);
    working.choose_effort_for_this_session(Some(Effort::Max));
    let buffer = drawn(&mut working);
    let (row, column) = find_cell(&buffer, &t!(panel_effort, level = "max"));
    beside_a_turn(|beside| clicked_during_a_turn(&mut working, row, column + 1, beside));
    assert_eq!(
        the_scope_of_the_open_effort_picker(&working),
        Scope::EverySession
    );
    assert!(working.queued.is_empty(), "the click waited");
}

/// MOUSE-11. A click on the panel's sandbox row opens the same list a bare `/sandbox` opens,
/// on the mode in force, at rest and while a turn runs, and nothing is queued or said.
///
/// Rejects: a click that is a copy, a click that waits for the turn, a click that notes the
/// mode in force instead of opening the list, and a list opened on a mode other than the one
/// held.
#[test]
fn a_click_on_the_panels_sandbox_row_opens_the_sandbox_picker_at_rest_and_mid_turn() {
    use crate::picking::Picking;
    let held = bravebot_sandbox::SandboxMode::ReadOnly;
    // `None` when the click opened no sandbox list, which the comparison below reports.
    let opened_on = |session: &Session| match session.picking() {
        Some(Picking::Sandbox(picker)) => picker.chosen().map(|row| row.0),
        _ => None,
    };

    let (mut session, _) = panel_with_a_model();
    session.set_sandbox_mode(held);
    let buffer = drawn(&mut session);
    let (row, column) = find_on(&buffer, held.name());
    let noted = session.transcript.len();
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert_eq!(opened_on(&session), Some(held));
    assert_eq!(session.transcript.len(), noted, "the click said something");
    assert!(session.selection.is_none());

    let (mut working, _) = panel_of_command_rows(true);
    working.set_sandbox_mode(held);
    let buffer = drawn(&mut working);
    let (row, column) = find_on(&buffer, held.name());
    beside_a_turn(|beside| clicked_during_a_turn(&mut working, row, column + 1, beside));
    assert_eq!(opened_on(&working), Some(held));
    assert!(working.queued.is_empty(), "the click waited");
}

/// A click on any other row of the panel, or on the transcript, is the copy it always was, and
/// a click on the border column is the resize handle.
#[test]
fn a_click_off_the_models_rows_asks_for_no_picker() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "/work/dir");
    assert_eq!(click(&mut session, row, column), Action::Copy);
    let (row, column) = find_on(&buffer, "TRANSCRIPTMARK");
    assert_eq!(click(&mut session, row, column), Action::Copy);

    let (row, column) = find_on(&buffer, "claude-sonnet");
    let border = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1
        .x;
    press(&mut session, row, border, KeyModifiers::NONE);
    assert!(session.dragging_panel(), "the border press began no resize");
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, border);
    assert_eq!(handle_mouse(&mut session, up), Action::Redraw);
    assert!(!session.dragging_panel());
    assert_eq!(session.panel_target_at(row, border), None);
    assert_eq!(
        session.panel_target_at(row, column),
        Some(crate::panel::Target::Model)
    );
}

/// A drag that began on the model row and a drag that ended on it are selections, so the picker
/// is not opened and the copy is. A double click on it, as a terminal reports one, asks for the
/// picker once with its first click, and its second press begins no selection and copies nothing.
#[test]
fn a_drag_over_the_model_row_opens_no_picker_and_a_double_click_opens_it_once() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    let (other, other_column) = find_on(&buffer, "PANELMARK");
    swept(&mut session, (other, other_column), (row, column));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.last_click = None;
    assert_eq!(
        click(&mut session, row, column),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    assert_eq!(click(&mut session, row, column), Action::None);
    assert!(session.selection.is_none(), "the second click selected");
}

/// The commands are typed at the plain prompt, so a click over the scroller asks for no picker.
#[test]
fn a_click_on_the_model_row_over_the_scroller_opens_no_picker() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    session.open_scroller();
    assert_ne!(
        click(&mut session, row, column),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
}

/// A press that began in the transcript and was let go over the model row is not a click on it.
#[test]
fn a_press_elsewhere_let_go_on_the_model_row_opens_no_picker() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    press(&mut session, 0, 0, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
}

/// MOUSE-12: a click on the panel's context reading says what `/context` says, and leaves no
/// selection behind.
#[test]
fn a_click_on_the_panels_context_reading_reports_the_context() {
    let (mut session, buffer) = panel_with_a_context();
    let (row, column) = find_on(&buffer, &render::context_reading(&session));
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(session.selection.is_none());
    assert_eq!(context_rows(&session).first().map(|(_, n)| *n), Some(777));
}

/// MOUSE-12: a drag over the reading, or a click over the scroller, reports nothing.
#[test]
fn a_drag_over_the_context_reading_or_a_click_in_the_scroller_reports_nothing() {
    let (mut session, buffer) = panel_with_a_context();
    let (row, column) = find_on(&buffer, &render::context_reading(&session));
    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.selection.is_some(), "the drag was a selection");
    assert!(context_rows(&session).is_empty());

    session.open_scroller();
    click(&mut session, row, column);
    assert!(context_rows(&session).is_empty());
}

/// MOUSE-13: a click on a started MCP server's row in the panel asks what a bare `/mcp` asks.
#[test]
fn a_click_on_the_panels_mcp_server_row_lists_the_servers() {
    let (mut session, _) = panel_with_a_context();
    session.servers.started = vec!["notes-server".to_string()];
    let buffer = drawn(&mut session);
    let (row, column) = find_on(&buffer, "notes-server");
    assert_eq!(click(&mut session, row, column), Action::Mcp(String::new()));
    assert!(session.selection.is_none());
}

/// MOUSE-13: a drag over the row, a click beside it and a click over the scroller ask for
/// nothing.
#[test]
fn a_drag_over_an_mcp_server_row_or_a_click_in_the_scroller_lists_nothing() {
    let (mut session, _) = panel_with_a_context();
    session.servers.started = vec!["notes-server".to_string()];
    let buffer = drawn(&mut session);
    let (row, column) = find_on(&buffer, "notes-server");
    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    let (other, other_column) = find_on(&buffer, "PANELMARK");
    assert_ne!(
        click(&mut session, other, other_column),
        Action::Mcp(String::new())
    );

    session.open_scroller();
    assert_ne!(click(&mut session, row, column), Action::Mcp(String::new()));
}

/// MOUSE-21: a click on the panel's hide row closes the panel, as the key does.
#[test]
fn a_click_on_the_panels_hide_row_hides_the_panel() {
    let (mut session, buffer) = panel_with_a_context();
    assert!(session.panel_open());
    let (row, column) = on_the_hide_row(&buffer);
    click(&mut session, row, column);
    assert!(!session.panel_open());
    assert!(session.selection.is_none());
}

/// MOUSE-21: a drag over the row and a click over the scroller leave the panel open.
#[test]
fn a_drag_over_the_hide_row_or_a_click_in_the_scroller_leaves_the_panel_open() {
    let (mut session, buffer) = panel_with_a_context();
    let (row, column) = on_the_hide_row(&buffer);
    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.panel_open());

    session.open_scroller();
    click(&mut session, row, column);
    assert!(session.panel_open());
}

/// A click on `row` and `column` taken the way the turn's own loop takes it: the press and the
/// release go through the mouse handler, and what the release asks for that is not a copy goes
/// to the loop.
fn clicked_during_a_turn(session: &mut Session, row: u16, column: u16, beside: &mut Beside<'_>) {
    let cancel = Cancel::new();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let action = handle_mouse(session, drag(kind, row, column));
        if action != Action::Copy {
            carry_out_while_working(session, action, &cancel, beside);
        }
    }
}

/// MOUSE-29: a click on a row of the panel that stands for a command does what the command
/// typed does at the plain prompt: the same action for the loop, the same words in the
/// transcript, nothing queued and the box as it was. The regression it rejects is a click that
/// calls the command's own function and so says something the line would not, or leaves out what
/// the line does first.
#[test]
fn a_click_on_a_panel_row_does_what_the_command_typed_does_at_rest() {
    let (probe, _) = panel_of_command_rows(false);
    for (needle, line) in command_rows(&probe) {
        let (mut clicked, buffer) = panel_of_command_rows(false);
        let (row, column) = find_cell(&buffer, &needle);
        let (mut typed, _) = panel_of_command_rows(false);

        let by_click = click(&mut clicked, row, column + 1);
        type_line(&mut typed, line);
        let by_typing = handle_key(&mut typed, key(KeyCode::Enter));

        assert_eq!(by_click, by_typing, "{line}: what the loop is asked for");
        assert_eq!(
            said_in_the_transcript(&clicked),
            said_in_the_transcript(&typed),
            "{line}: what is said"
        );
        assert!(clicked.queued.is_empty(), "{line} was queued at rest");
        assert_eq!(clicked.input(), typed.input(), "{line}: the box");
    }
}

/// MOUSE-29: while a turn runs the click is carried out as the command typed is: the model and
/// the effort ask for their picker as a mode of the session, the context is said under the turn,
/// and `/mcp`, which waits for the turn, waits with the note a typed one gets. With a prompt
/// waiting ahead of it, a command that could be changed by it waits behind it and says so, as
/// typed. The regression it rejects is the click dropped, which is what the loop did with
/// everything but a copy, and a click that says its words outside the turn's own record of what
/// the commands said.
#[test]
fn a_click_on_a_panel_row_does_what_the_command_typed_does_while_a_turn_runs() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (probe, _) = panel_of_command_rows(true);
    for waiting in [None, Some("second")] {
        for (needle, line) in command_rows(&probe) {
            let (mut clicked, buffer) = panel_of_command_rows(true);
            let (row, column) = find_cell(&buffer, &needle);
            let (mut typed, _) = panel_of_command_rows(true);
            if let Some(waiting) = waiting {
                wait_as(&mut clicked, waiting);
                wait_as(&mut typed, waiting);
            }

            beside_a_turn(|beside| {
                clicked_during_a_turn(&mut clicked, row, column + 1, beside);
            });
            beside_a_turn(|beside| {
                typed_during_a_turn(&mut typed, line, key(KeyCode::Enter), beside);
            });

            let behind = format!("{line} behind {waiting:?}");
            assert_eq!(
                said_under_the_turn(&clicked),
                said_under_the_turn(&typed),
                "{behind}: what was said under the turn"
            );
            assert_eq!(
                said_in_the_transcript(&clicked),
                said_in_the_transcript(&typed),
                "{behind}: the transcript"
            );
            assert_eq!(
                waiting_prompts(&clicked),
                waiting_prompts(&typed),
                "{behind}: the queue"
            );
            assert_eq!(
                clicked.picking().is_some(),
                typed.picking().is_some(),
                "{behind}: the picker"
            );
            assert_eq!(clicked.input(), typed.input(), "{behind}: the box");
        }
    }

    // What each of them comes to, so that two clicks that both did nothing do not pass for the
    // same as two typed lines that both did something. The model's picker opens as a mode of the
    // session with its rows still to come, which a unit test cannot wait for.
    let (mut session, buffer) = panel_of_command_rows(true);
    let rows = command_rows(&session);
    let at = |needle: &str| find_cell(&buffer, needle);
    let (row, column) = at("claude-sonnet");
    let (mut model, _) = panel_of_command_rows(true);
    beside_a_turn(|beside| clicked_during_a_turn(&mut model, row, column + 1, beside));
    assert!(model.picking().is_some(), "the model picker did not open");
    let (row, column) = at(&rows[1].0);
    let (mut effort, _) = panel_of_command_rows(true);
    beside_a_turn(|beside| clicked_during_a_turn(&mut effort, row, column + 1, beside));
    assert!(effort.picking().is_some(), "the effort picker did not open");
    let (row, column) = at(&rows[2].0);
    beside_a_turn(|beside| clicked_during_a_turn(&mut session, row, column + 1, beside));
    assert!(
        !said_under_the_turn(&session).is_empty(),
        "the context was said outside what the turn keeps apart"
    );
    let (mut session, _) = panel_of_command_rows(true);
    let (row, column) = at("rust-analyzer");
    beside_a_turn(|beside| clicked_during_a_turn(&mut session, row, column + 1, beside));
    assert_eq!(
        said_under_the_turn(&session),
        vec![
            t!(lsp_started, count = 1).to_string(),
            t!(lsp_server_running, program = "rust-analyzer").to_string()
        ],
        "the language servers were not said under the turn"
    );
    assert!(session.queued.is_empty(), "/lsp waited");
    let (mut session, _) = panel_of_command_rows(true);
    let (row, column) = at("notes-server");
    let remembered = session.history.len();
    beside_a_turn(|beside| clicked_during_a_turn(&mut session, row, column + 1, beside));
    assert!(session.queued.is_empty(), "/mcp waited");
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(mcp_command_none).to_string()],
        "the servers were not listed under the turn"
    );
    assert_eq!(
        session.history.len(),
        remembered,
        "a click was remembered as a line somebody typed"
    );
    // Behind a waiting `/mcp` that starts a server the click waits too, as the typed line does.
    let (mut behind, _) = panel_of_command_rows(true);
    wait_as(&mut behind, "/mcp reconnect weather");
    beside_a_turn(|beside| clicked_during_a_turn(&mut behind, row, column + 1, beside));
    assert_eq!(
        waiting_prompts(&behind),
        vec!["/mcp reconnect weather", "/mcp"]
    );
    let (mut files, _) = panel_of_command_rows(true);
    let (row, column) = at(&rows[5].0);
    beside_a_turn(|beside| clicked_during_a_turn(&mut files, row, column + 1, beside));
    assert!(files.files_open(), "the files state did not open");
    assert!(
        waiting_prompts(&files).is_empty(),
        "a command that runs mid-turn was queued"
    );
}

/// MOUSE-29: where the session is working and no turn is running, as it is under a summary, an
/// aside or a command, a command typed waits in the queue and so does the click, every one of
/// them, with what a typed line gets. The regression it rejects is a click that carries a
/// command out in a loop that holds nothing to carry it out with.
#[test]
fn a_click_on_a_panel_row_queues_the_command_where_no_turn_is_running() {
    let (probe, _) = panel_of_command_rows(false);
    for (needle, line) in command_rows(&probe) {
        let (mut clicked, buffer) = panel_of_command_rows(false);
        let (row, column) = find_cell(&buffer, &needle);
        let (mut typed, _) = panel_of_command_rows(false);
        clicked.status = Status::Working;
        typed.status = Status::Working;
        assert!(!clicked.a_turn_is_running());

        let by_click = click(&mut clicked, row, column + 1);
        let by_typing = type_while_working(&mut typed, line);

        assert_eq!(by_click, by_typing, "{line}");
        assert_eq!(
            waiting_prompts(&clicked),
            vec![line],
            "{line} was not queued"
        );
        assert_eq!(waiting_prompts(&clicked), waiting_prompts(&typed));
        assert_eq!(
            said_in_the_transcript(&clicked),
            said_in_the_transcript(&typed),
            "{line}: what is said"
        );
        assert_eq!(clicked.picking().is_some(), typed.picking().is_some());
    }
}

/// A press and a release on one cell answered as the loop that runs a command the person typed
/// answers them, with what each leaves for the loop to carry out.
fn clicked_while_a_command_runs(session: &mut Session, row: u16, column: u16) -> Vec<Action> {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    [MouseEventKind::Down, MouseEventKind::Up]
        .into_iter()
        .map(|kind| {
            answer_mouse(
                &mut terminal,
                session,
                drag(kind(MouseButton::Left), row, column),
                |_| true,
            )
            .expect("the report was answered")
        })
        .collect()
}

/// MOUSE-29: while a command the person typed runs, nothing can carry out what a click on a panel
/// row stands for and nothing reads a line typed, so the click waits in the queue as it does under
/// a summary, with a note that says what it waits for, and is carried out when the command has
/// ended. The regressions it rejects are a click that asks for its picker or says its answer into
/// the transcript while the command is still printing, and one that the loop that runs the command
/// drops without a word.
#[test]
fn a_click_on_a_panel_row_waits_for_the_command_that_is_running() {
    let (probe, _) = panel_of_command_rows(false);
    for (needle, line) in command_rows(&probe) {
        let (mut session, buffer) = panel_of_command_rows(false);
        let (row, column) = find_cell(&buffer, &needle);
        session.begin_command();
        let said = said_in_the_transcript(&session);

        let left = clicked_while_a_command_runs(&mut session, row, column + 1);

        assert_eq!(left, vec![Action::Redraw, Action::Redraw], "{line}");
        assert_eq!(waiting_prompts(&session), vec![line], "{line}: not queued");
        assert!(session.picking().is_none(), "{line}: a picker opened");
        let spoken = said_in_the_transcript(&session);
        assert_eq!(
            spoken[said.len()..],
            [t!(command_waits_for_the_command, command = line).to_string()],
            "{line}: what was said while the command ran"
        );
        assert!(session.input().is_empty(), "{line}: the box was written in");

        session.finish_command();
        let carried_out = queued_next(&mut session).expect("the line waited for its turn");
        let typed = {
            let (mut typed, _) = panel_of_command_rows(false);
            type_line(&mut typed, line);
            handle_key(&mut typed, key(KeyCode::Enter))
        };
        assert_eq!(carried_out, typed, "{line}: carried out as typed at rest");
        assert!(session.queued.is_empty(), "{line}: carried out twice");
    }
}

/// MOUSE-29: a click on a row that stands for a command leaves a line the person has begun in the
/// box as it is, at rest, while a turn runs, under a summary and while a command runs, whether the
/// click carries the command out or queues it. The regression it rejects is a click that clears
/// the box to make room for the command it stands for, which the fixtures of the other tests never
/// show since they click over an empty box.
#[test]
fn a_click_on_a_panel_row_leaves_a_line_being_typed_in_the_box() {
    let (probe, _) = panel_of_command_rows(false);
    for (needle, line) in command_rows(&probe) {
        for state in ["at rest", "a turn", "a summary", "a command"] {
            let (mut session, buffer) = panel_of_command_rows(state == "a turn");
            type_line(&mut session, "half a sentence");
            match state {
                "a summary" => session.status = Status::Working,
                "a command" => session.begin_command(),
                _ => {}
            }
            let (row, column) = find_cell(&buffer, &needle);

            clicked_while_a_command_runs(&mut session, row, column + 1);

            assert_eq!(
                session.input(),
                "half a sentence",
                "{line} clicked at {state} changed the box"
            );
        }
    }
}

/// MOUSE-29: a click on a panel row over a picker that is open is on the picker, which is drawn
/// over the frame and takes every key, so it asks for nothing and the picker stays. The
/// regression it rejects is the click reaching the row under the popup.
#[test]
fn a_click_on_a_panel_row_under_an_open_picker_asks_for_nothing() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (mut session, buffer) = panel_of_command_rows(true);
    open_effort_picker(&mut session, crate::model_prompt::Scope::EverySession);
    let said = said_in_the_transcript(&session);
    for (needle, _) in command_rows(&session) {
        let (row, column) = find_cell(&buffer, &needle);
        beside_a_turn(|beside| clicked_during_a_turn(&mut session, row, column + 1, beside));
    }
    assert!(session.picking().is_some(), "the picker was closed");
    assert_eq!(said_in_the_transcript(&session), said);
    assert!(session.queued.is_empty(), "a command was queued");
}

/// MOUSE-30: a click on any row of the session's name in the panel puts `/rename` and the whole
/// name in the box with the caret after it, and Enter then renames. The name is the one the
/// session has and not the rows the panel drew: an address in it is drawn as its shorthand and
/// a long name is wrapped. The regression it rejects is a box holding what was on the screen.
#[test]
fn a_click_on_the_session_name_puts_rename_and_the_name_in_the_box() {
    let name = "tidy the parser, see https://github.com/brave/bravebot/pull/12 for the rest";
    for needle in ["tidy", "rest"] {
        let (mut session, _) = two_panes();
        session.identify(name, "/work/dir".to_string(), None);
        let buffer = drawn(&mut session);
        let drawn_rows: Vec<String> = crate::panel::lines(&session, crate::panel::WIDTH, 24)
            .iter()
            .map(|line| line.to_string())
            .collect();
        assert!(
            drawn_rows
                .iter()
                .any(|row| row.contains("brave/bravebot#12"))
        );
        assert!(!drawn_rows.iter().any(|row| row.contains("https://")));
        let (row, column) = find_cell(&buffer, needle);

        assert_eq!(click(&mut session, row, column), Action::Redraw);

        assert_eq!(session.input(), format!("{RENAME_COMMAND} {name}"));
        assert_eq!(session.caret(), session.input().len());
        assert!(session.selection.is_none());
        type_line(&mut session, " v2");
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Rename(format!("{name} v2")),
            "Enter did not confirm the edited name"
        );
    }
}

/// MOUSE-30: while a turn runs the click puts the line in the box as typing it would, and Enter
/// carries `/rename` out as typed, which the turn does not hold anything of.
#[test]
fn a_click_on_the_session_name_while_a_turn_runs_puts_rename_in_the_box() {
    let (mut session, buffer) = panel_of_command_rows(true);
    let (row, column) = find_cell(&buffer, "PANELMARK");
    click(&mut session, row, column);
    assert_eq!(session.input(), "/rename PANELMARK");
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Rename("PANELMARK".to_string())
    );
    assert!(session.queued.is_empty(), "/rename waited");
}

/// MOUSE-30: the note a click on the name leaves while a turn runs, that the line is in use, is
/// what a command says while a turn works and not something the turn did, so stopping the turn
/// with nothing else recorded still gives the prompt back to the box. The regression it rejects
/// is the note written outside the record of what commands said, which counts it as the turn's
/// work and leaves the stopped prompt sent.
#[test]
fn the_note_of_a_click_on_the_name_while_a_turn_runs_is_not_something_the_turn_did() {
    for armed in ["shell mode", "a half-typed line"] {
        let (mut session, _) = panel_of_command_rows(true);
        match armed {
            "shell mode" => session.shell = true,
            _ => type_line(&mut session, "half a sentence"),
        }
        let buffer = drawn(&mut session);
        let (row, column) = find_cell(&buffer, "PANELMARK");

        click(&mut session, row, column);

        let said = said_in_the_transcript(&session);
        assert_eq!(
            said.last(),
            Some(&t!(panel_rename_line_in_use).to_string()),
            "{armed}: the click said nothing"
        );
        assert_eq!(
            session.said_while_working().len(),
            1,
            "{armed}: the note was not recorded as said while the turn ran"
        );
        if armed == "a half-typed line" {
            // The box is taken by the line, which keeps it, so the stop cannot give the prompt
            // back; shell mode is left by a stop and the box is free.
            continue;
        }
        assert!(
            session.restore("first"),
            "{armed}: the stop left the prompt sent for a note a command made"
        );
    }
}

/// MOUSE-30: control characters in the name are left out of the line, since no key types one.
#[test]
fn a_name_with_control_characters_is_put_in_the_box_without_them() {
    let (mut session, _) = two_panes();
    session.identify("a\u{1b}[31mred\u{7}", "/work/dir".to_string(), None);
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "red");
    click(&mut session, row, column);
    assert_eq!(session.input(), "/rename a[31mred");
}

/// MOUSE-30: a tab in the name is the space the panel draws it as, so the box holds the words the
/// person read. The regression it rejects is a tab left out with the other control characters,
/// which joins the two words the panel shows apart.
#[test]
fn a_tab_in_the_name_is_put_in_the_box_as_the_space_the_panel_draws() {
    let (mut session, _) = two_panes();
    session.identify("fix\tthe bug", "/work/dir".to_string(), None);
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "fix the bug");
    click(&mut session, row, column);
    assert_eq!(session.input(), "/rename fix the bug");
}

/// MOUSE-30: over a line somebody has begun, in shell mode and in the vi style of box, the click
/// leaves the box as it was and says it did, and a click off the name, a drag over it and a click
/// over the scroller put nothing in the box. The fixtures are the gestures that would fill the box
/// if their guards were missing.
#[test]
fn a_click_that_cannot_put_rename_in_the_box_leaves_it_alone() {
    let line_in_use = t!(panel_rename_line_in_use).to_string();

    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "PANELMARK");
    type_line(&mut session, "half a sentence");
    click(&mut session, row, column);
    assert_eq!(session.input(), "half a sentence", "a draft was replaced");
    assert_eq!(said_in_the_transcript(&session).last(), Some(&line_in_use));

    let (mut session, _) = two_panes();
    session.shell = true;
    click(&mut session, row, column);
    assert_eq!(session.input(), "", "shell mode took the line");
    assert_eq!(said_in_the_transcript(&session).last(), Some(&line_in_use));

    let (mut session, _) = two_panes();
    session.choose_editing(crate::vim::Editing::Vi);
    click(&mut session, row, column);
    assert_eq!(session.input(), "", "the vi box took the line");
    assert_eq!(said_in_the_transcript(&session).last(), Some(&line_in_use));

    let (mut session, _) = two_panes();
    let said = said_in_the_transcript(&session);
    let (directory, directory_column) = find_cell(&buffer, "/work/dir");
    click(&mut session, directory, directory_column);
    click(&mut session, row - 1, column);
    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    session.open_scroller();
    click(&mut session, row, column);
    assert_eq!(session.input(), "", "something put a line in the box");
    assert_eq!(said_in_the_transcript(&session), said);
}

/// MOUSE-30 and MOUSE-50: a double click on the name, as a terminal reports one, puts the line in
/// the box with its first click, and the second press begins nothing: it selects no word of the
/// panel, copies none, and does not find the box taken by the line the first click wrote and say
/// so. The regression it rejects is the second click acting as a click of its own.
#[test]
fn a_double_click_on_the_session_name_puts_rename_in_the_box_once() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "PANELMARK");
    let said = said_in_the_transcript(&session);

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert_eq!(session.input(), "/rename PANELMARK");
    assert_eq!(click(&mut session, row, column), Action::None);

    assert_eq!(session.input(), "/rename PANELMARK");
    assert!(
        session.selection.is_none(),
        "the second press selected a word"
    );
    assert_eq!(
        said_in_the_transcript(&session),
        said,
        "the second click said something"
    );
}

/// A string nothing in the interface says, which the tools of the fixture below returned.
const FROM_A_TOOL: &str = "TOOLSAID-7f3c1e";

/// What a tool's result holds that the Pull requests and Issues sections would list if they read it.
const A_REFERENCE_FROM_A_TOOL: &str =
    "https://github.com/evil/trap/issues/9 https://github.com/evil/trap/pull/10";

/// A session with every row of the panel a click acts on, whose tools returned
/// [`FROM_A_TOOL`] in a result, in what a call showed and in what it was told, and whose reply
/// repeats it, with a turn running on it or not.
fn a_session_whose_tools_returned_something(turn_running: bool) -> Session {
    use bravebot_agent::report::{Activity, Landing, Reach, Returned, Shown};
    use bravebot_core::todo::{Item, List, Status};
    let (mut session, _) = panel_of_command_rows(false);
    session.opens_an_address = records_the_address;
    type_line(
        &mut session,
        "do the work on https://github.com/brave/bravebot/issues/7 and https://github.com/brave/bravebot/pull/8",
    );
    handle_key(&mut session, key(KeyCode::Enter));
    session.set_todos(bravebot_core::todo::rows(&List::new(vec![
        Item::new(
            "Read the tokenizer and list every place a token is consumed twice, then write down what each does",
            Status::Done,
        ),
        Item::new("Fix it", Status::Active),
    ])));
    session.finish_activity(
        Activity::running("Run", "make")
            .done(format!("{FROM_A_TOOL} finished {A_REFERENCE_FROM_A_TOOL}")),
    );
    session.landed(Landing::Context);
    let result = format!("{FROM_A_TOOL} {A_REFERENCE_FROM_A_TOOL}");
    session.returned(Returned {
        lines: vec![result.clone()],
        total: 1,
        from_the_end: false,
        whole: vec![result.clone()],
    });
    session.show(Shown {
        as_the_planner_had_it: None,
        origin: FROM_A_TOOL.to_string(),
        reach: Reach::NotThePlanner,
        label: "untrusted".to_string(),
        preview: vec![result],
        lines: 1,
    });
    session.complete(
        format!("it says {FROM_A_TOOL} {A_REFERENCE_FROM_A_TOOL}"),
        Vec::new(),
        0,
    );
    if turn_running {
        type_line(&mut session, "next");
        handle_key(&mut session, key(KeyCode::Enter));
        assert!(session.a_turn_is_running());
    }
    session
}

/// The text of the cells of `area` in a frame, one string.
fn text_in(buffer: &ratatui::buffer::Buffer, area: ratatui::layout::Rect) -> String {
    (area.top()..area.bottom())
        .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
        .map(|(x, y)| buffer[(x, y)].symbol().to_string())
        .collect()
}

/// MOUSE-33: nothing a tool returned reaches the panel, whichever of its targets is clicked, at
/// the plain prompt and while a turn runs, and nothing a click says holds it either. The
/// fixture holds the string in a result, in what a call showed and in the reply, and the panel
/// has every kind of target there is. The regression it rejects is a click that puts what a
/// tool returned where the panel draws it, such as a task's detail taken from the result of the
/// call that did it.
#[test]
fn a_click_on_any_target_of_the_panel_never_draws_what_a_tool_returned() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    use crate::panel::Target;
    for turn_running in [false, true] {
        let mut fixture = a_session_whose_tools_returned_something(turn_running);
        let frame = drawn_at(&mut fixture, 120, 36);
        assert!(
            render::as_text(&fixture).contains(FROM_A_TOOL),
            "the transcript does not hold what the tools returned"
        );
        let panel = fixture
            .laid
            .panes
            .iter()
            .find(|(pane, _)| *pane == crate::select::Pane::Panel)
            .expect("panel recorded")
            .1;
        assert!(
            !text_in(&frame, panel).contains(FROM_A_TOOL),
            "the panel drew it before any click"
        );

        let targets = the_panels_targets(&fixture);
        for wanted in [
            Target::Hide,
            Target::Name,
            Target::Model,
            Target::Effort,
            Target::Context,
            Target::Servers,
            Target::Languages,
            Target::Task(0),
            Target::Task(1),
            Target::Detail(0),
            Target::PullRequest(0),
            Target::Issue(0),
        ] {
            assert!(
                targets.iter().any(|(found, ..)| *found == wanted),
                "{wanted:?} is not a target of the fixture's panel: {targets:?}"
            );
        }

        for (target, row, column) in targets {
            let mut session = a_session_whose_tools_returned_something(turn_running);
            drawn_at(&mut session, 120, 36);
            let said = session.transcript.len();
            let _ = addresses_opened();
            // At rest the loop opens what the click asks for, so the action says it; during a
            // turn the click opens it at once, through the session's opener.
            let opened = if turn_running {
                beside_a_turn(|beside| {
                    clicked_during_a_turn(&mut session, row, column, beside);
                });
                addresses_opened()
            } else {
                match click(&mut session, row, column) {
                    Action::OpenUrl(address) => vec![address],
                    _ => Vec::new(),
                }
            };
            let expected: &[&str] = match target {
                Target::Issue(_) => &["https://github.com/brave/bravebot/issues/7"],
                Target::PullRequest(_) => &["https://github.com/brave/bravebot/pull/8"],
                _ => &[],
            };
            assert_eq!(
                opened, expected,
                "{target:?} opened the wrong addresses (turn running: {turn_running})"
            );

            let after = drawn_at(&mut session, 120, 36);
            if let Some((_, panel)) = session
                .laid
                .panes
                .iter()
                .find(|(pane, _)| *pane == crate::select::Pane::Panel)
            {
                assert!(
                    !text_in(&after, *panel).contains(FROM_A_TOOL),
                    "{target:?} drew what a tool returned (turn running: {turn_running})"
                );
                assert!(
                    !text_in(&after, *panel).contains("evil/trap"),
                    "{target:?} listed a reference a tool returned (turn running: {turn_running})"
                );
            }
            let spoken: String = session.transcript[said..]
                .iter()
                .map(|entry| entry.text.as_str())
                .collect();
            assert!(
                !spoken.contains(FROM_A_TOOL),
                "{target:?} said what a tool returned"
            );
            assert!(
                !session.input().contains(FROM_A_TOOL),
                "{target:?} put what a tool returned in the box"
            );
        }
    }
}

/// MOUSE-22: a drag over a language server's row, a click beside it and a click over the
/// scroller say nothing about the servers. The fixtures are each a gesture that would say it if
/// its guard were missing.
#[test]
fn a_drag_over_a_language_server_row_or_a_click_in_the_scroller_says_nothing() {
    let (mut session, buffer) = panel_of_command_rows(false);
    let (row, column) = find_cell(&buffer, "rust-analyzer");
    let said = said_in_the_transcript(&session);

    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    let (other, other_column) = find_cell(&buffer, "notes-server");
    click(&mut session, other + 1, other_column);
    click(&mut session, row - 1, column);
    assert_eq!(said_in_the_transcript(&session), said);

    session.open_scroller();
    click(&mut session, row, column);
    assert_eq!(said_in_the_transcript(&session), said);
}

/// MOUSE-22 and MOUSE-50: a double click on a language server's row, as a terminal reports one,
/// says which servers are running once, with its first click, and the second press begins no
/// selection and says nothing more. The regression it rejects is the second click saying it
/// again, or beginning a selection of the word under it.
#[test]
fn a_double_click_on_a_language_server_row_says_it_once() {
    let (mut session, buffer) = panel_of_command_rows(false);
    let (row, column) = find_cell(&buffer, "rust-analyzer");
    let said = said_in_the_transcript(&session);

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    let after_the_first = said_in_the_transcript(&session);
    assert!(
        after_the_first.len() > said.len(),
        "the first click said nothing"
    );

    assert_eq!(click(&mut session, row, column), Action::None);
    assert_eq!(said_in_the_transcript(&session), after_the_first);
    assert!(
        session.selection.is_none(),
        "the second press selected a word"
    );
}

/// MOUSE-14: a click on the permission mode under the box cycles it, as the key does.
#[test]
fn a_click_on_the_footers_permission_mode_cycles_it() {
    let mut session = Session::new("none");
    session.cycle_permission_mode();
    let first = session.permission_mode();
    let buffer = drawn(&mut session);
    let (row, column) = find_on(
        &buffer,
        crate::status::named_mode(first, false).expect("named"),
    );
    assert_eq!(click(&mut session, row, column + 1), Action::Redraw);
    assert_ne!(session.permission_mode(), first);
    assert!(session.selection.is_none());
}

/// MOUSE-14: a click on the permission mode moves to the mode the key moves to and says nothing
/// the key does not, at the plain prompt and while a turn runs, at each step as far as the row
/// draws a name to click on, with one difference: the key reaches bypass and the click never
/// does, because a stray click on the indicator would otherwise stop every later question
/// being asked. Auto is the same: the key reaches it, and a click passes it by because it hands
/// the run and fetch questions to a model. The regressions it rejects are a click that cycles by
/// another order than the key, one that reaches a mode the key does not, one that writes a note,
/// and one that moves into bypass or auto, which the key still does from the same state.
#[test]
fn a_click_on_the_permission_mode_moves_as_the_key_does_and_never_into_bypass() {
    use bravebot_agent::PermissionMode;
    for turn_running in [false, true] {
        let start = |turn_running: bool| {
            let mut session = match turn_running {
                true => a_turn_running_on("first"),
                false => Session::new("none"),
            };
            session.cycle_permission_mode();
            session
        };
        let press_the_key = |session: &mut Session| {
            let back_tab = key(KeyCode::BackTab);
            match turn_running {
                true => handle_key_while_working(session, back_tab),
                false => handle_key(session, back_tab),
            };
        };
        let (mut clicked, mut keyed) = (start(turn_running), start(turn_running));
        let (mut steps, mut key_reached_bypass, mut key_reached_auto) = (0, false, false);
        while let Some(name) = crate::status::named_mode(clicked.permission_mode(), false) {
            let buffer = drawn(&mut clicked);
            let (row, column) = find_cell(&buffer, name);
            // The name moves under the pointer, so a second click on the cell is not a double
            // one here.
            clicked.last_click = None;
            click(&mut clicked, row, column + 1);
            press_the_key(&mut keyed);
            assert!(
                !matches!(
                    clicked.permission_mode(),
                    PermissionMode::Bypass | PermissionMode::Auto
                ),
                "step {steps}, turn running: {turn_running}: the click moved into a mode that stops asking"
            );
            // The key goes on round from auto and bypass to where the click already is.
            while matches!(
                keyed.permission_mode(),
                PermissionMode::Bypass | PermissionMode::Auto
            ) {
                key_reached_bypass |= keyed.permission_mode() == PermissionMode::Bypass;
                key_reached_auto |= keyed.permission_mode() == PermissionMode::Auto;
                press_the_key(&mut keyed);
            }
            assert_eq!(
                clicked.permission_mode(),
                keyed.permission_mode(),
                "step {steps}, turn running: {turn_running}"
            );
            assert_eq!(
                said_in_the_transcript(&clicked),
                said_in_the_transcript(&keyed),
                "step {steps}, turn running: {turn_running}"
            );
            steps += 1;
            assert!(steps < 6, "the modes never came back round");
        }
        assert!(steps >= 2, "fewer modes than the key has were reached");
        assert!(
            key_reached_bypass,
            "the key no longer reaches bypass from the state the click left"
        );
        assert!(
            key_reached_auto,
            "the key no longer reaches auto from the state the click left"
        );
    }
}

/// MOUSE-14: a click on the permission mode while the session is in bypass moves out of it, to the
/// mode the key goes to from there, and a session where a settings layer took bypass off the
/// ladder cycles by click as it does by key. Rejects: a click that is refused in bypass, so
/// that the indicator that shows bypass is one the click cannot leave.
#[test]
fn a_click_on_the_permission_mode_in_bypass_moves_out_of_it() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    for _ in 0..4 {
        session.cycle_permission_mode();
    }
    assert_eq!(session.permission_mode(), PermissionMode::Bypass);
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(
        &buffer,
        crate::status::named_mode(PermissionMode::Bypass, false).expect("named"),
    );
    click(&mut session, row, column + 1);
    assert_eq!(session.permission_mode(), PermissionMode::Ask);
}

/// MOUSE-14: a drag over the mode, a click on the rest of the row, a mode that is not drawn
/// and a click over the scroller change nothing.
#[test]
fn a_drag_over_the_mode_or_a_click_beside_it_leaves_the_permission_mode_alone() {
    let mut session = Session::new("none");
    session.cycle_permission_mode();
    let mode = session.permission_mode();
    let buffer = drawn(&mut session);
    let (row, column) = find_on(
        &buffer,
        crate::status::named_mode(mode, false).expect("named"),
    );

    swept(&mut session, (row, column), (row, column + 3));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 3);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    click(&mut session, row, 0);
    click(&mut session, row, column + 40);
    assert_eq!(session.permission_mode(), mode);

    session.open_scroller();
    click(&mut session, row, column);
    assert_eq!(session.permission_mode(), mode);

    let mut asking = Session::new("none");
    let buffer = drawn(&mut asking);
    let (row, _) = find_on(&buffer, "?");
    click(&mut asking, row, 2);
    assert_eq!(
        asking.permission_mode(),
        bravebot_agent::PermissionMode::Ask
    );
}

/// MOUSE-16: a click on a character of the box puts the caret before it, on whichever row the
/// line was drawn over, and one past the end of a row or on the prompt character puts it at the
/// row's end or start.
#[test]
fn a_click_in_the_box_puts_the_caret_where_it_landed() {
    let (mut session, buffer) = typing("hello world\nsecond row");
    let (row, column) = find_cell(&buffer, "world");
    assert_eq!(click(&mut session, row, column + 2), Action::Redraw);
    assert_eq!(session.caret(), "hello wo".len());
    assert!(session.selection.is_none());

    let (row, column) = find_cell(&buffer, "second");
    click(&mut session, row, column);
    assert_eq!(session.caret(), "hello world\n".len());
    click(&mut session, row, column + 60);
    assert_eq!(session.caret(), session.input().len());
    click(&mut session, row, column - 2);
    assert_eq!(session.caret(), "hello world\n".len());
}

/// MOUSE-16: a report at the largest column or row a terminal can name, and at the first cells
/// past the screen, is a cell outside the box, so a click there leaves the caret where it was.
/// The click on the same row inside the box is the control.
#[test]
fn a_click_at_the_largest_column_or_row_lands_outside_the_box() {
    let (mut session, buffer) = typing("hello world\nsecond row");
    let (row, column) = find_cell(&buffer, "world");
    let end = session.input().len();
    assert_eq!(session.caret(), end);
    for (at_row, at_column) in [
        (row, u16::MAX),
        (row, u16::MAX - 1),
        (row, 120),
        (row, 224),
        (u16::MAX, column),
        (u16::MAX - 1, column),
        (u16::MAX, u16::MAX),
    ] {
        session.last_click = None;
        click(&mut session, at_row, at_column);
        assert_eq!(
            session.caret(),
            end,
            "a click at row {at_row}, column {at_column} moved the caret"
        );
        assert_eq!(session.input(), "hello world\nsecond row");
    }
    session.last_click = None;
    click(&mut session, row, column + 2);
    assert_eq!(session.caret(), "hello wo".len(), "the click in the box");
}

/// MOUSE-16: a character drawn two cells wide is taken from either of its cells, and a click
/// after it lands after it.
#[test]
fn a_click_in_the_box_counts_a_wide_character_as_one() {
    let (mut session, buffer) = typing("日本語 end");
    let (row, column) = find_cell(&buffer, "日");
    click(&mut session, row, column + 1);
    assert_eq!(session.caret(), 0);
    click(&mut session, row, column + 2);
    assert_eq!(session.caret(), "日".len());
    click(&mut session, row, column + 6);
    assert_eq!(session.caret(), "日本語".len());
}

/// MOUSE-16: a click on a row the box drew for a line that has since been shortened lands
/// nowhere. A key can change the line between a frame and the click that follows it, and a row
/// recorded for the longer line is then a range of bytes the line no longer has.
#[test]
fn a_click_on_a_row_of_a_line_that_has_changed_since_it_was_drawn_lands_nowhere() {
    let (mut session, buffer) = typing("a line of words to click in");
    let (row, column) = find_cell(&buffer, "words");
    for _ in 0.."a line of words to click in".len() - 2 {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    let end = session.caret();
    assert_eq!(session.input(), "a ");

    click(&mut session, row, column);
    assert_eq!(session.caret(), end);
    assert_eq!(session.input(), "a ");
}

/// MOUSE-16: a click on a folded paste puts the caret at the start of the marker, never inside it.
#[test]
fn a_click_on_a_marker_puts_the_caret_before_it() {
    let mut session = Session::new("none");
    session.paste("see ");
    session.paste_text("one\ntwo\nthree\nfour");
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "Pasted");
    click(&mut session, row, column + 3);
    assert_eq!(session.caret(), "see ".len());
}

/// MOUSE-16: a drag across the box, a double click, a click on the border and a click over the
/// scroller leave the caret where it was.
#[test]
fn a_drag_a_double_click_or_a_click_off_the_text_leaves_the_caret_alone() {
    let (mut session, buffer) = typing("hello world");
    let (row, column) = find_cell(&buffer, "hello");
    let end = session.caret();

    swept(&mut session, (row, column), (row, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert_eq!(session.caret(), end);

    session.clear_selection();
    session.last_click = None;
    click(&mut session, row, column + 1);
    assert_eq!(session.caret(), 1);
    session.move_to_line_end();
    click(&mut session, row, column + 1);
    click(&mut session, row, column + 1);
    assert_eq!(session.caret(), end);

    session.last_click = None;
    click(&mut session, row - 1, column + 1);
    click(&mut session, row, 0);
    assert_eq!(session.caret(), end);

    session.open_scroller();
    click(&mut session, row, column + 1);
    assert_eq!(session.caret(), end);
}

/// MOUSE-16: a click on the box's text leaves the caret alone while a history search, the files
/// state, a picker or the vi style of box is the one in use, each of which a guard of its own
/// refuses.
#[test]
fn a_click_in_the_box_does_nothing_where_another_mode_is_in_use() {
    type Setup = fn(&mut Session);
    let modes: [(&str, Setup); 4] = [
        ("a history search", |session| {
            session.history =
                crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                    "old one", None,
                )]);
            assert!(session.open_history_search(), "no search opened");
        }),
        ("the files state", |session| {
            let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
            session.transcript.push(crate::state::Entry::tool(
                bravebot_agent::report::Activity::running("Update", "a.rs")
                    .of_tool("edit_file")
                    .with_changes(diff.condensed(3))
                    .with_written(Some(bravebot_agent::report::Written {
                        landed: "a.rs".into(),
                        added: diff.added(),
                        removed: diff.removed(),
                        numbers: diff.condensed_numbers(3),
                    }))
                    .done("done"),
            ));
            session.laid.columns = 120;
            session.toggle_files();
            assert!(session.files_open(), "the files state did not open");
        }),
        ("a picker", |session| {
            session.open_picking(crate::picking::Picking::Effort(
                crate::effort_prompt::Picker::new(None),
            ));
        }),
        ("the vi box", |session| {
            session.choose_editing(crate::vim::Editing::Vi);
        }),
    ];
    for (name, setup) in modes {
        let (mut session, buffer) = typing("hello world");
        let (row, column) = find_cell(&buffer, "hello");
        let end = session.caret();
        click(&mut session, row, column + 1);
        assert_eq!(
            session.caret(),
            1,
            "{name}: the fixture's click moved nothing"
        );
        session.move_to_line_end();
        session.last_click = None;
        setup(&mut session);
        click(&mut session, row, column + 1);
        assert_eq!(session.caret(), end, "{name}: a click moved the caret");
    }
}

/// A mouse report names a row and a column that are as large as 65535 and a terminal can send
/// them, so arithmetic on either must not overflow, in the box, the panel, the scroller, the
/// transcript and the rest of the screen, for every kind of report and while a selection is held.
/// A test build panics where arithmetic overflows, so the run finishing is the assertion that
/// none did, and no report types, so what the box held when the run began is what it holds at
/// the end.
#[test]
fn a_mouse_report_at_the_last_coordinate_overflows_nothing() {
    let kinds = [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Moved,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
    ];
    let edges = [0, 1, u16::MAX - 1, u16::MAX];
    // And the rows of the box, so that a column at the last coordinate meets a row inside it.
    let rows = [0, 1, 20, 21, 22, 23, u16::MAX - 1, u16::MAX];
    type Fixture = fn() -> Session;
    let states: [(&str, Fixture); 3] = [
        ("the box and the panel", || {
            let (mut session, _) = two_panes();
            session.paste("a line of text in the box\nand a second row of it");
            session
        }),
        ("the scroller", || {
            let mut session = a_long_reply(40);
            drawn_at(&mut session, 120, 24);
            session.open_scroller();
            session
        }),
        ("the plain view", || {
            let mut session = a_long_reply(40);
            session.paste("text");
            session
        }),
    ];
    for (name, state) in states {
        // Each way a report reaches the session is its own run, since a press sent twice to
        // one cell is a double click and the click that follows it is not a single one.
        for through in ["the handler", "the loop at rest", "the loop of a turn"] {
            let mut session = state();
            let typed = session.input().to_string();
            let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24))
                .expect("terminal");
            // Each cell gets the whole run of reports, press first, so a release is the end of a
            // click on it.
            for row in rows {
                for column in edges {
                    for kind in kinds {
                        let event = drag(kind, row, column);
                        match through {
                            "the handler" => {
                                handle_mouse(&mut session, event);
                            }
                            "the loop at rest" => {
                                idle_action(&mut session, TermEvent::Mouse(event));
                            }
                            _ => {
                                answer_mouse(&mut terminal, &mut session, event, |_| true)
                                    .unwrap_or_else(|error| panic!("{name}: {error}"));
                            }
                        }
                        drawn_at(&mut session, 120, 24);
                        session.scroll_toward_the_pointer(Instant::now());
                    }
                }
            }
            assert_eq!(session.input(), typed, "{name}, through {through}");
        }
        // A selection begun inside the transcript and dragged out to each of the cells, which
        // is the one run in which a report far past the screen reaches a selection that is held.
        let mut session = state();
        for row in rows {
            for column in edges {
                press(&mut session, 1, 2, KeyModifiers::NONE);
                handle_mouse(
                    &mut session,
                    drag(MouseEventKind::Drag(MouseButton::Left), row, column),
                );
                drawn_at(&mut session, 120, 24);
                session.scroll_toward_the_pointer(Instant::now());
                handle_mouse(
                    &mut session,
                    drag(MouseEventKind::Up(MouseButton::Left), row, column),
                );
                session.last_click = None;
            }
        }
    }
}

/// R19, R52. Tab pressed with the picker open reaches the picker and switches its scope, at
/// rest and under a turn, and edits nothing else: the line stays empty and no suggestion or
/// completion takes the key first. Shift-Tab switches it back. It rejects a Tab taken by the
/// box, by a suggestion, or by the turn's own loop.
#[test]
fn tab_in_an_open_model_picker_switches_the_scope_at_rest_and_under_a_turn() {
    use crate::model_prompt::Scope;
    let config = a_config_with_a_named_roster();

    let mut session = Session::new("none");
    a_model_picker_open_in(&mut session, &config, Scope::EverySession);
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(the_scope_of_the_open_picker(&session), Scope::ThisSession);
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(the_scope_of_the_open_picker(&session), Scope::EverySession);
    handle_key(&mut session, key(KeyCode::BackTab));
    assert_eq!(the_scope_of_the_open_picker(&session), Scope::ThisSession);
    assert!(session.input().is_empty(), "Tab edited the line");

    let mut working = a_turn_running_on("first");
    a_model_picker_open_in(&mut working, &config, Scope::EverySession);
    handle_key_while_working(&mut working, key(KeyCode::Tab));
    assert_eq!(the_scope_of_the_open_picker(&working), Scope::ThisSession);
    assert!(working.input().is_empty(), "Tab edited the line");
}

/// MOUSE-52: `/copy text` is the keyboard route to the menu's `copy as text`. Typed, it puts on the
/// clipboard the reply as the screen shows it, with none of the marks of the markdown it was written
/// in, none of the marker and margin and no wrapping, and `/copy` still puts the markdown the
/// planner wrote. A number counts replies back as it does for `/copy`.
#[test]
fn a_typed_text_command_copies_a_reply_as_the_screen_shows_it_and_counts_back_as_copy_does() {
    let mut session = Session::new("none");
    answered(&mut session, "one", "an older **answer** with `code`");
    answered(
        &mut session,
        "two",
        "# A heading\n\nSome **bold** and `code` in a paragraph that the screen would wrap over \
             several rows in a narrow transcript, kept as one line.\n\n- one\n- two",
    );

    let text = copied_by(&mut session, "/copy text").expect("a reply as text");
    assert!(text.contains("A heading"), "{text:?}");
    assert!(
        text.contains("Some bold and code in a paragraph"),
        "{text:?}"
    );
    assert!(text.contains("kept as one line."), "{text:?}");
    for mark in ["# ", "**", "`", "\u{23fa}"] {
        assert!(!text.contains(mark), "{mark:?} is in {text:?}");
    }
    assert_eq!(copied_by(&mut session, "/copy text 1"), Ok(text.clone()));
    assert_eq!(
        copied_by(&mut session, "/copy text 2"),
        Ok("an older answer with code".to_string())
    );
    let markdown = copied_by(&mut session, "/copy").expect("the markdown");
    assert!(markdown.contains("**bold**"), "{markdown:?}");
    assert_ne!(markdown, text);
}

/// MOUSE-52: `/copy text` copies what the menu item copies, from the same function, so the two
/// cannot disagree about what a reply is as the screen shows it.
#[test]
fn a_typed_text_command_copies_what_the_menu_item_copies() {
    let words = "# A heading\n\nSome **bold** and a [link](https://example.test/a) and `code`.\n\n\
                     1. one\n2. two\n\n> quoted";
    let mut session = Session::new("none");
    answered(&mut session, "ask", words);
    assert_eq!(
        copied_by(&mut session, "/copy text"),
        Ok(copyable_as_text(words))
    );
    assert_ne!(copyable_as_text(words), copyable(words));
}

/// MOUSE-52: `/copy text` refuses as `/copy` does, with the notes `/copy` gives, and a longer word
/// is read as a number of replies back and refused as one.
#[test]
fn a_typed_text_command_refuses_what_it_cannot_take_and_copies_nothing() {
    let mut session = Session::new("none");
    assert_eq!(
        copied_by(&mut session, "/copy text"),
        Err(t!(session_copy_no_reply).to_string())
    );
    answered(&mut session, "one", "the only reply");
    for line in [
        "/copy text 0",
        "/copy text x",
        "/copy text -1",
        "/copy text 1 2",
    ] {
        assert_eq!(
            copied_by(&mut session, line),
            Err(t!(session_copy_needs_a_number).to_string()),
            "{line}"
        );
    }
    for line in ["/copy text 2", "/copy text 99999999999999999999999999"] {
        assert_eq!(
            copied_by(&mut session, line),
            Err(t!(session_copy_goes_no_further, replies = 1)),
            "{line}"
        );
    }
    assert_eq!(
        copied_by(&mut session, "/copy texts"),
        Err(t!(session_copy_needs_a_number).to_string())
    );
    assert_eq!(session.copied, None, "a refusal reported a copy");
    assert_eq!(
        copied_by(&mut session, "/copy text"),
        Ok("the only reply".to_string())
    );
}

/// MOUSE-52: a reply is wanted on the clipboard while a turn runs, so `/copy text` does not wait,
/// as `/copy` does not.
#[test]
fn a_typed_text_command_takes_the_reply_without_waiting_while_a_turn_runs() {
    let mut session = Session::new("none");
    answered(&mut session, "first", "an **older** answer");
    type_line(&mut session, "second");
    assert!(matches!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit(_)
    ));
    assert_eq!(
        type_while_working(&mut session, "/copy text"),
        Action::CopyReply("an older answer".to_string())
    );
    assert!(session.queued.is_empty(), "/copy text waited");
}

/// MOUSE-52: Tab finishes `text` after `/copy` as it finishes `block`, and leaves what it does not
/// know.
#[test]
fn tab_finishes_text_after_the_copy_command() {
    for (typed, finished) in [("/copy t", "/copy text"), ("/copy te", "/copy text")] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
        assert_eq!(session.input(), finished, "{typed:?}");
    }
    for typed in ["/copy text", "/copy text 2", "/copy tx"] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), typed, "{typed:?}");
    }
}

/// MOUSE-42 and MOUSE-43: a click on a path is carried out against the working directory and the
/// session's rules. A name that climbs out, leads out, names a directory or names nothing is
/// refused with the one sentence and without the name, by the preview and by the editor alike.
/// A file a `deny` rule covers is refused by the preview, which reads it, and is opened by the
/// editor, which is the person's own program.
#[test]
fn a_click_on_a_path_is_held_to_the_working_directory_and_only_the_preview_to_the_deny_rules() {
    use crate::text_links::PathRef;
    use bravebot_core::permissions::{Anchors, Permissions};
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let base = crate::test_profile::project("bravebot-app-click-on-a-path");
    let root = base.join("project");
    std::fs::create_dir_all(root.join("src")).expect("create");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("a file");
    std::fs::write(root.join(".env"), "TOKEN=1\n").expect("a file");
    std::fs::write(base.join("outside.txt"), "outside\n").expect("a file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(base.join("outside.txt"), root.join("leads-out.txt"))
        .expect("a link");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(TrustStore::new(&root), TrustedPrograms::new(), rules);
    let (denying, rejected) =
        Permissions::parse(&["Read(.env)".to_string()], &[], &[], &Anchors::none());
    assert!(rejected.is_empty());
    answers.rules.permissions = denying;
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
    let path = |name: &str| PathRef {
        path: name.to_string(),
        line: None,
        column: None,
    };
    let outside = base.join("outside.txt").display().to_string();
    let last_note = |held: &Held| held.session.transcript.last().expect("a note").text.clone();

    let refused = t!(session_path_not_a_file).to_string();
    for named in [
        "../outside.txt",
        "src/../../outside.txt",
        outside.as_str(),
        "leads-out.txt",
        "src",
        "missing.rs",
    ] {
        assert_eq!(held.file_to_edit(&path(named)), None, "{named:?}");
        assert_eq!(last_note(&held), refused, "{named:?}");
        held.preview_file(&path(named));
        assert_eq!(last_note(&held), refused, "{named:?}");
        assert!(!last_note(&held).contains(named), "{named:?}");
    }

    let denied = t!(session_preview_denied).to_string();
    held.preview_file(&path(".env"));
    assert_eq!(last_note(&held), denied);
    assert_eq!(
        held.file_to_edit(&path(".env")),
        Some(held.workspace.root().join(".env")),
        "the editor was refused a file a deny rule covers"
    );

    held.preview_file(&path("src/main.rs"));
    let shown = held
        .session
        .transcript
        .last()
        .and_then(|entry| entry.shown.as_ref())
        .expect("the file was shown");
    assert_eq!(shown.preview, ["1  fn main() {}"]);
    assert_eq!(
        held.file_to_edit(&path("src/main.rs")),
        Some(held.workspace.root().join("src/main.rs"))
    );
}
