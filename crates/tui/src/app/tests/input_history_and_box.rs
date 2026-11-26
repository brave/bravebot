//! The prompt box's history, its size and layout, and how the box behaves around a turn.

use super::*;

/// A paragraph can be written in the box rather than only pasted into it. Shift-Enter is the
/// one Enter that does not send.
#[test]
fn shift_enter_starts_a_line_instead_of_sending() {
    let mut session = typed_into("first");
    assert_eq!(
        handle_key(&mut session, shift(KeyCode::Enter)),
        Action::Redraw
    );
    for c in "second".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(session.input(), "first\nsecond");
    assert_eq!(session.status, Status::Idle, "the newline started a turn");
    assert!(session.transcript.is_empty(), "the newline sent something");
}

/// The bug this exists for: a terminal that cannot report the modifier on Enter is configured
/// to send `\n` for Shift-Enter instead, which arrives in raw mode as Ctrl-J. Unbound, the
/// keystroke did nothing at all, which is what iTerm and Terminal.app both showed.
#[test]
fn ctrl_j_starts_a_line_too() {
    let mut session = typed_into("first");
    assert_eq!(handle_key(&mut session, ctrl('j')), Action::Redraw);
    for c in "second".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(session.input(), "first\nsecond");
    assert_eq!(session.status, Status::Idle, "the newline started a turn");
}

/// Whichever spelling arrives, sending is still Enter's alone.
#[test]
fn ctrl_j_is_not_swallowed_while_a_turn_runs() {
    let mut session = typed_into("go");
    handle_key(&mut session, key(KeyCode::Enter));
    handle_key_while_working(&mut session, key(KeyCode::Char('a')));

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('j')),
        Action::Redraw,
        "the control guard swallowed the newline"
    );
    assert_eq!(session.input(), "a\n");
}

/// Ctrl-M is Enter itself, so binding it would take sending away.
#[test]
fn only_ctrl_j_starts_a_line() {
    assert!(starts_a_line(ctrl('j')));
    assert!(starts_a_line(shift(KeyCode::Enter)));
    assert!(!starts_a_line(ctrl('m')));
    assert!(!starts_a_line(key(KeyCode::Enter)));
    assert!(!starts_a_line(key(KeyCode::Char('j'))));
}

/// The newline lands at the caret like any other keystroke, not at the end of the line.
#[test]
fn a_newline_lands_at_the_caret() {
    let mut session = typed_into("ab");
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, shift(KeyCode::Enter));

    assert_eq!(session.input(), "a\nb");
    assert_eq!(session.caret(), 2);
}

/// And plain Enter still sends the paragraph, once: the two are not the same key.
#[test]
fn enter_still_sends_a_paragraph_written_with_shift_enter() {
    let mut session = typed_into("first");
    handle_key(&mut session, shift(KeyCode::Enter));
    for c in "second".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("first\nsecond".to_string())
    );
}

/// A `!` after a newline is punctuation in a sentence, not a request for a shell: the mode is
/// only ever armed by the first character of an empty line.
#[test]
fn a_newline_does_not_arm_shell_mode() {
    let mut session = typed_into("wait");
    handle_key(&mut session, shift(KeyCode::Enter));
    handle_key(&mut session, key(KeyCode::Char('!')));

    assert!(!session.shell, "a newline left the box looking empty");
    assert_eq!(session.input(), "wait\n!");
}

/// A multi-line command is a `for` loop somebody typed, so the mode gets the key too.
#[test]
fn shift_enter_works_in_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "!for f in *; do");
    handle_key(&mut session, shift(KeyCode::Enter));
    type_line(&mut session, "  echo $f");

    assert!(session.shell, "the mode was left by a newline");
    assert_eq!(session.input(), "for f in *; do\n  echo $f");
}

/// A turn in flight refuses Enter but not this one: what can be typed mid-turn can be written
/// as a paragraph mid-turn.
#[test]
fn shift_enter_works_while_a_turn_runs() {
    let mut session = typed_into("go");
    handle_key(&mut session, key(KeyCode::Enter));
    handle_key_while_working(&mut session, key(KeyCode::Char('a')));

    assert_eq!(
        handle_key_while_working(&mut session, shift(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "a\n");
}

#[test]
fn enter_submits_the_prompt() {
    let mut session = Session::new("none");
    for c in "hello".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("hello".to_string())
    );
}

#[test]
fn enter_on_empty_input_does_nothing() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
    assert_eq!(session.status, Status::Idle);
}

/// Handing the terminal to an editor mid-turn would take the screen away from the turn that
/// is drawing on it, and the line the editor returned would be waiting for a box that has
/// moved on. The keys the user can still use while a turn runs do not include this one.
#[test]
fn the_editor_key_does_nothing_while_a_turn_runs() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('g')),
        Action::None
    );
}

/// One key both ways, read against the line rather than remembered: a line to put away is put
/// away, and an empty box is where the one put away earlier is wanted. A control combination
/// falling through to the catch-all would type a stray 's' into the prompt instead.
#[test]
fn ctrl_s_puts_the_line_away_and_brings_it_back() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, ctrl('s')), Action::Redraw);
    assert_eq!(session.input(), "", "the line stayed in the box");

    assert_eq!(handle_key(&mut session, ctrl('s')), Action::Redraw);
    assert_eq!(session.input(), "half a thought");
}

/// The key writes a line and sends nothing, and sending is the whole of what a running turn
/// refuses. Mid-turn is also when it is most wanted: a person watching a turn go wrong has
/// somewhere to put the half-written thought a better one just replaced.
#[test]
fn the_stash_key_works_while_a_turn_runs() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "the next thing".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, ctrl('s')),
        Action::Redraw
    );
    assert_eq!(session.input(), "");

    handle_key_while_working(&mut session, ctrl('s'));
    assert_eq!(session.input(), "the next thing");
}

/// Inside the prompt search every character narrows the list, so a moved chord has to be read
/// before the arm that types: bound onto a letter it would otherwise open the search from the
/// box and then be typed into it.
#[test]
fn a_moved_chord_is_read_inside_the_search_it_opened() {
    let alt = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
    let mut session = Session::new("none").in_workspace("/work/here");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("history".to_string(), "alt-r".to_string());
    moved.insert("stash".to_string(), "alt-s".to_string());
    session.adopt_keybindings(&moved);

    type_line(&mut session, "run the tests");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    handle_key(&mut session, alt('r'));
    assert!(session.searching_history(), "the chord did not open it");

    handle_key(&mut session, alt('s'));
    let search = session.history_search().expect("the search closed");
    assert_eq!(
        search.scope(),
        crate::history::Scope::Project,
        "the chord did not narrow the scope"
    );
    assert_eq!(search.needle(), "", "the chord was typed into the needle");

    handle_key(&mut session, alt('s'));
    let search = session.history_search().expect("the search closed");
    assert_eq!(
        search.scope(),
        crate::history::Scope::Session,
        "one more press did not move on to this session"
    );

    handle_key(&mut session, alt('s'));
    let search = session.history_search().expect("the search closed");
    assert_eq!(
        search.scope(),
        crate::history::Scope::All,
        "one more press did not come back to everywhere"
    );

    // The chords they were moved off of do nothing in here, rather than narrowing the list to
    // prompts holding an `s` or an `r`.
    for old in [ctrl('s'), ctrl('r')] {
        handle_key(&mut session, old);
    }
    let search = session
        .history_search()
        .expect("an old chord closed the search");
    assert_eq!(
        search.needle(),
        "",
        "an old chord was typed into the needle"
    );

    handle_key(&mut session, alt('r'));
    assert!(!session.searching_history(), "the chord did not close it");
}

/// The same for `?`: it puts the list of keys up rather than typing a character, so nothing is
/// left in the box to delete and pressing it again takes the list down.
#[test]
fn a_question_mark_on_an_empty_line_toggles_the_list_without_being_typed() {
    let mut session = Session::new("none");

    type_line(&mut session, "?");
    assert!(session.shortcuts, "the list did not come up");
    assert!(session.input().is_empty(), "the marker was typed");

    type_line(&mut session, "?");
    assert!(!session.shortcuts, "the list did not go down again");
    assert!(session.input().is_empty(), "the marker was typed");
}

/// Punctuation everywhere but the head of the line. Somebody writing "what is this?" is asking
/// the model a question, not asking for the keys.
#[test]
fn a_question_mark_inside_a_sentence_is_punctuation() {
    let mut session = Session::new("none");
    type_line(&mut session, "what is this?");

    assert!(!session.shortcuts, "the list came up mid-sentence");
    assert_eq!(session.input(), "what is this?");
}

/// Typing again means the reading is over, so the list goes rather than sitting under a line it
/// says nothing about.
#[test]
fn typing_takes_the_list_down() {
    let mut session = Session::new("none");
    type_line(&mut session, "?");
    type_line(&mut session, "a");

    assert!(!session.shortcuts, "the list stayed up");
    assert_eq!(session.input(), "a", "the character was not typed");
}

/// Escape takes down whatever is up, and with an empty box the list is the only thing there is.
#[test]
fn escape_takes_the_list_down() {
    let mut session = Session::new("none");
    type_line(&mut session, "?");

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
    assert!(!session.shortcuts, "the list stayed up");
}

/// In vi's style Escape is the key that makes the letters instructions, and `?` there is vi's
/// own key, so nothing but Escape can take the list down from the box the style opens in.
#[test]
fn escape_takes_the_list_down_in_vis_editing_style() {
    let mut session = Session::new("none");
    session.choose_editing(crate::vim::Editing::Vi);
    type_line(&mut session, "?");
    assert!(session.shortcuts, "the list did not come up");

    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.shortcuts, "the list stayed up after Escape");
}

/// A line that arrives under the list takes it down, whichever path put it there: a paste, a
/// dropped path and Shift-Enter all write the line without typing a character.
#[test]
fn a_paste_or_a_newline_takes_the_list_down() {
    let mut pasted = Session::new("none");
    type_line(&mut pasted, "?");
    pasted.paste_text("hello");
    assert!(!pasted.shortcuts, "the list stands over a pasted line");
    assert_eq!(pasted.input(), "hello");

    let mut broken = Session::new("none");
    type_line(&mut broken, "?");
    broken.type_newline();
    assert!(!broken.shortcuts, "the list stands over a new line");
}

/// In shell mode a `?` is a glob for the shell to expand, so it is typed like any other
/// character rather than putting the list up.
#[test]
fn a_question_mark_in_shell_mode_is_a_glob() {
    let mut session = Session::new("none");
    type_line(&mut session, "!");
    type_line(&mut session, "?");

    assert!(!session.shortcuts, "the list came up over a command");
    assert_eq!(session.input(), "?");
}

#[test]
fn enter_in_shell_mode_runs_the_line_rather_than_prompting() {
    let mut session = Session::new("none");
    type_line(&mut session, "!ls -la");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("ls -la".to_string())
    );
}

/// The summariser read the exchange that stopped being sent and wrote what replaced it, and
/// the room won back is the difference. What it read is not the figure: the summary it wrote
/// is still in the conversation and still occupies room. Nor is what the whole call cost,
/// which adds the two together and reports a compaction as having freed more than the
/// conversation ever held.
#[test]
fn the_room_a_compaction_won_back_is_what_it_read_less_what_it_wrote() {
    let usage = bravebot_aichat::protocol::Usage {
        prompt_tokens: 40_000,
        completion_tokens: 4_000,
        ..Default::default()
    };

    assert_eq!(room_won_back(&usage), 36_000);
}

/// The two counts are of different things (what a server said it read, and what it said or
/// the interface estimated it wrote), so nothing guarantees which is larger. Subtracting an
/// unsigned count that turns out to be the larger one wraps, and a session that shortened
/// nothing would report having freed sixteen million million million tokens.
#[test]
fn a_count_of_what_was_written_past_what_was_read_does_not_wrap_the_figure() {
    let usage = bravebot_aichat::protocol::Usage {
        prompt_tokens: 3_000,
        completion_tokens: 9_000,
        ..Default::default()
    };

    assert_eq!(room_won_back(&usage), 0);
}

/// Nothing measured, nothing to divide: the command says so rather than drawing zeros or a
/// blank, which INPUT-22 forbids for the reading itself.
#[test]
fn the_context_command_says_when_nothing_has_been_measured() {
    let mut session = Session::new("none");
    session.set_last_request(a_request_of_known_parts());
    enter_line(&mut session, "/context");
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(context_not_measured)))
    );
    assert!(context_rows(&session).is_empty());

    // A count with no budget to divide it by is the same footing as nothing measured.
    let mut session = Session::new("none");
    session.set_last_request(a_request_of_known_parts());
    session.measured(500, 0, true);
    enter_line(&mut session, "/context");
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(context_not_measured)))
    );
}

/// Up walks a prompt at a time, which is no way to reach the hundredth. Ctrl-R is the chord
/// every shell answers with this question, so it is the one to answer it with here.
#[test]
fn ctrl_r_searches_the_prompts_already_sent() {
    let mut session = having_sent(&["first question", "second question"]);
    handle_key(&mut session, ctrl('r'));
    assert!(session.searching_history());
    assert_eq!(
        session.history_match().expect("a prompt").prompt,
        "second question"
    );
}

/// Mid-turn is when the prompt somebody wants back is most likely to be one they can no longer
/// see, and nothing about searching sends anything, which is the whole of what a turn refuses.
#[test]
fn the_prompts_can_be_searched_while_a_turn_is_running() {
    let mut session = having_sent(&["first question"]);
    type_line(&mut session, "second question");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    handle_key_while_working(&mut session, ctrl('r'));
    assert!(session.searching_history());
}

/// The loops answer the stop keys before any ladder does, so the guard that lets the search
/// have them first is the whole of what keeps the keys that close it from ending the turn a
/// person opened it during. Searching sends nothing, so there was never anything in it for the
/// turn to refuse.
#[test]
fn the_search_answers_the_stop_keys_before_the_turn_does() {
    let mut session = having_sent(&["first question"]);
    type_line(&mut session, "second question");
    handle_key(&mut session, key(KeyCode::Enter));

    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        assert!(
            stops_the_turn(&session, stopping),
            "{stopping:?} did not reach the turn with nothing in the way"
        );
    }

    handle_key_while_working(&mut session, ctrl('r'));
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        assert!(
            !stops_the_turn(&session, stopping),
            "{stopping:?} stopped the turn from inside the search"
        );
    }

    handle_key_while_working(&mut session, key(KeyCode::Esc));
    assert!(!session.searching_history(), "the search did not close");
    assert_eq!(session.status, Status::Working, "the turn was stopped");
}

/// A summary and an aside answer the stop keys themselves, so the search opened while one of
/// them was out has to be asked before the request is: answered by the request instead, either
/// key stopped it and the search a person was trying to leave was still open behind.
#[test]
fn the_search_answers_the_stop_keys_before_a_single_request_does() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = having_sent(&["first question"]);
        session.begin_aside();
        let cancel = Cancel::new();

        one_request_key(&mut session, ctrl('r'), &cancel);
        assert!(session.searching_history(), "the search did not open");

        one_request_key(&mut session, stopping, &cancel);

        assert!(
            !session.searching_history(),
            "{stopping:?} left the search open"
        );
        assert!(!session.is_quitting(), "{stopping:?} ended the session");
        assert!(
            !cancel.is_cancelled(),
            "{stopping:?} stopped the request from inside the search"
        );
    }
}

/// INPUT-22: a compaction that came back puts the session in the compacted reading, with the room
/// it won back, over the measured one it held. A compaction that had nothing to do, failed or was
/// stopped shortened nothing, so the measured reading stays. The regressions it rejects are a
/// finished compaction that leaves the old percentage on screen, and one that does not keep it
/// when the conversation was not shortened.
#[test]
fn a_finished_compaction_reads_what_it_won_back_and_an_unfinished_one_keeps_the_measured_reading() {
    let measured = crate::state::Occupancy::Measured {
        used: 90_000,
        budget: 100_000,
        guessed: false,
    };
    let usage = bravebot_aichat::protocol::Usage {
        prompt_tokens: 40_000,
        completion_tokens: 4_000,
        ..Default::default()
    };
    let summary = bravebot_agent::compact::Compacted {
        summarised: 4,
        kept: 2,
        model: "a-model".to_string(),
        usage,
    };
    let mut session = having_sent(&["first question"]);
    session.measured(90_000, 100_000, false);
    assert_eq!(session.occupancy(), measured);
    session.begin_aside();

    compact_ended(&mut session, 100_000, &Ok(Some(summary)), false);

    assert_eq!(
        session.occupancy(),
        crate::state::Occupancy::Compacted {
            won_back: room_won_back(&usage),
            budget: 100_000,
        },
        "a finished compaction left the reading it had"
    );
    assert_eq!(room_won_back(&usage), 36_000);

    for (what, done, stopped) in [
        ("nothing to do", Ok(None), false),
        ("a stopped request", Err("offline".to_string()), true),
        ("a failed request", Err("offline".to_string()), false),
    ] {
        let mut session = having_sent(&["first question"]);
        session.measured(90_000, 100_000, false);
        session.begin_aside();

        compact_ended(&mut session, 100_000, &done, stopped);

        assert_eq!(
            session.occupancy(),
            measured,
            "{what} replaced the measured reading"
        );
    }
}

/// With nothing sent there is nothing to search, and a panel saying so is a mode a person then
/// has to get out of.
#[test]
fn ctrl_r_with_nothing_sent_yet_opens_nothing() {
    let mut session = Session::new("none");
    handle_key(&mut session, ctrl('r'));
    assert!(!session.searching_history());
}

/// Walking back at all says the wanted prompt is an old one, and the workspace somebody is in
/// is the narrower question about it. The list it opens on is the whole point: seeded with the
/// prompt in the box it would answer with that prompt alone, and narrowed to somewhere else it
/// would answer with nothing. A line put away earlier is not touched, since the slot holds one
/// line and this press is not putting anything into it.
#[test]
fn ctrl_s_searches_this_workspace_while_an_older_prompt_is_shown() {
    let mut session = Session::new("none");
    for (workspace, prompt) in [
        ("/work/elsewhere", "why is the picker slow"),
        ("/work/here", "commit that"),
        ("/work/here", "run the tests"),
    ] {
        session.now_in_workspace(workspace);
        type_line(&mut session, prompt);
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);
    }

    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('s'));
    assert_eq!(
        session.stashed(),
        Some("half a thought"),
        "the line was not put away"
    );

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        "run the tests",
        "the history was not walked"
    );

    assert_eq!(handle_key(&mut session, ctrl('s')), Action::Redraw);
    let search = session.history_search().expect("the search did not open");
    assert_eq!(
        search.scope(),
        crate::history::Scope::Project,
        "the search opened over every workspace"
    );
    assert_eq!(
        search.needle(),
        "",
        "the search opened looking for the prompt in the box"
    );
    assert_eq!(
        session
            .history_matches()
            .iter()
            .map(|entry| entry.prompt.as_str())
            .collect::<Vec<_>>(),
        ["commit that", "run the tests"],
        "not this workspace's prompts"
    );
    assert_eq!(
        session.stashed(),
        Some("half a thought"),
        "the line put away earlier is gone"
    );
}

/// Somebody who typed half a prompt and then reached for the history has already said what
/// they are looking for.
#[test]
fn the_search_starts_from_what_was_already_typed() {
    let mut session = having_sent(&["run the tests", "commit that"]);
    type_line(&mut session, "tests");
    handle_key(&mut session, ctrl('r'));

    assert_eq!(
        session.history_match().expect("a prompt").prompt,
        "run the tests"
    );
}

/// A prompt walked back to is not something anybody typed to look for it. Seeded with it, the
/// search answers with the one prompt already in the box, which is the walk it exists to take
/// somebody off: the hint offering the search is drawn in that very state.
#[test]
fn the_search_does_not_start_from_a_prompt_walked_back_to() {
    let mut session = having_sent(&["run the tests", "commit that"]);
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "commit that", "the history was not walked");

    handle_key(&mut session, ctrl('r'));
    assert_eq!(
        session.history_search().expect("a search").needle(),
        "",
        "the search opened looking for the prompt in the box"
    );
    assert_eq!(
        session.history_matches().len(),
        2,
        "prompts are missing from the list"
    );
}

/// While the search is open a letter narrows the list. Typed into the box instead it would go
/// somewhere nobody can see, and the list would never narrow.
#[test]
fn a_letter_narrows_the_search_rather_than_reaching_the_box() {
    let mut session = having_sent(&["run the tests", "commit that"]);
    handle_key(&mut session, ctrl('r'));
    type_line(&mut session, "run");

    assert!(session.input().is_empty(), "the box took the letters");
    assert_eq!(
        session.history_match().expect("a prompt").prompt,
        "run the tests"
    );
}

/// Into the box rather than into a request: a history file can be edited, on a shared machine
/// by somebody else, so the keystroke that sends a stored line has to be the person's own.
#[test]
fn enter_puts_the_chosen_prompt_in_the_box_without_sending_it() {
    let mut session = having_sent(&["run the tests", "commit that"]);
    handle_key(&mut session, ctrl('r'));
    type_line(&mut session, "run");
    let action = handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(action, Action::Redraw, "the prompt was sent");
    assert!(!session.searching_history());
    assert_eq!(session.input(), "run the tests");
    assert_eq!(
        session.status,
        Status::Idle,
        "a turn was started from the list"
    );
}

/// Escape means "stop what is happening", and what is happening is the search. The line in the
/// box was never taken away, so there is nothing to put back.
#[test]
fn escape_closes_the_search_and_leaves_the_box_as_it_was() {
    let mut session = having_sent(&["run the tests"]);
    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('r'));
    handle_key(&mut session, key(KeyCode::Esc));

    assert!(!session.searching_history());
    assert_eq!(session.input(), "half a thought");
}

/// A paste and a drop reach this process as events of their own rather than as keys, so the
/// guard that holds the box still for a letter does not see them. The panel is drawn in place
/// of the box, so whatever lands in the box is invisible until the search closes, and the
/// person comes back to a different prompt with a file staged that nobody asked for. An empty
/// paste is how a picture on the clipboard arrives, and answering it reads the clipboard into
/// that same box. The regressions it rejects are each of those, and spending the hint an empty
/// paste says once a session, at rest and mid-turn.
#[test]
fn a_paste_and_a_drop_do_not_reach_the_line_while_the_search_is_open() {
    let directory = crate::testutil::scratch_dir("bravebot-app-drop-while-searching");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    let file = directory.join("shot.png");
    std::fs::write(&file, [0x89u8, 0x50]).expect("write");
    let dropped = file.to_string_lossy().into_owned();

    for mid_turn in [false, true] {
        let place = if mid_turn { "mid-turn" } else { "at rest" };
        let press = |session: &mut Session, pressed: KeyEvent| match mid_turn {
            true => handle_key_while_working(session, pressed),
            false => handle_key(session, pressed),
        };

        let mut session = having_sent(&["half a thought sent before"]).in_workspace(&directory);
        if mid_turn {
            type_line(&mut session, "second question");
            handle_key(&mut session, key(KeyCode::Enter));
            assert!(session.a_turn_is_running(), "{place}: no turn was running");
        }
        for c in "half a thought".chars() {
            press(&mut session, key(KeyCode::Char(c)));
        }
        press(&mut session, key(KeyCode::Left));
        press(&mut session, ctrl('r'));
        assert!(
            session.searching_history(),
            "{place}: the search did not open"
        );

        let said = session.transcript.len();
        for pasted in ["pasted", "", dropped.as_str()] {
            assert_eq!(
                handle_paste(&mut session, pasted),
                Action::None,
                "{place}: a paste of {pasted:?} was answered from inside the search"
            );
        }
        assert_eq!(
            session.transcript.len(),
            said,
            "{place}: the empty paste spent the hint from inside the search"
        );
        assert_eq!(
            session.history_search().expect("a search").needle(),
            "half a thought",
            "{place}: a paste changed what was searched for"
        );
        press(&mut session, key(KeyCode::Esc));
        assert!(
            !session.searching_history(),
            "{place}: the search stayed open"
        );
        assert_eq!(
            session.input(),
            "half a thought",
            "{place}: a paste reached the box under the search"
        );
        assert!(
            session.attached().is_empty(),
            "{place}: a drop staged an attachment from inside the search"
        );

        // The caret is where it was left, which is the half of the line's state that is not
        // in the text.
        press(&mut session, key(KeyCode::Char('!')));
        assert_eq!(
            session.input(),
            "half a though!t",
            "{place}: the caret moved"
        );
    }

    let _ = std::fs::remove_dir_all(&directory);
}

/// Backspacing past the start leaves, which is what the key means once there is nothing left of
/// the thing it deletes. The same ladder the scroller's search walks.
#[test]
fn backspacing_past_the_start_of_the_search_closes_it() {
    let mut session = having_sent(&["run the tests"]);
    handle_key(&mut session, ctrl('r'));
    type_line(&mut session, "ru");

    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(session.searching_history());
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(session.searching_history());
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(!session.searching_history(), "the search would not close");
}

/// The arrows walk the matches while the search is open, rather than the history behind it or
/// the transcript behind that.
#[test]
fn the_arrows_walk_the_matches_while_the_search_is_open() {
    let mut session = having_sent(&["first question", "second question"]);
    handle_key(&mut session, ctrl('r'));
    handle_key(&mut session, key(KeyCode::Up));

    assert_eq!(
        session.history_match().expect("a prompt").prompt,
        "first question"
    );
    assert_eq!(session.scroll, 0, "the transcript scrolled instead");
    assert!(!session.history.is_browsing(), "the box walked the history");
}

/// With nothing typed the prompt has nowhere to go, so the keys are the transcript's, which is
/// what they were before there was a caret to move.
#[test]
fn page_keys_scroll_further() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.scroll, 10);
    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(session.scroll, 0);
}

/// The first press takes the start of the line and the second the line before it, so someone
/// who wanted this line's start gets it without losing their place in the paragraph.
#[test]
fn page_up_takes_the_start_of_the_line_then_the_line_before() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "first line\nsecond line\nthird line");

    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.caret(), "first line\nsecond line\n".len());
    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.caret(), "first line\n".len());
    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.caret(), 0);
}

/// And the same downwards, by line ends.
#[test]
fn page_down_takes_the_end_of_the_line_then_the_line_after() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "first line\nsecond line\nthird line");
    // Up to the very start, which is where paging down has somewhere to go from.
    for _ in 0..3 {
        handle_key(&mut session, key(KeyCode::PageUp));
    }
    assert_eq!(session.caret(), 0);

    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(session.caret(), "first line".len());
    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(session.caret(), "first line\nsecond line".len());
}

/// Off the ends of the prompt the keys go back to the transcript, so a paragraph does not trap
/// them and scrolling stays reachable.
#[test]
fn the_page_keys_reach_the_transcript_from_the_ends_of_the_prompt() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "one\ntwo");

    // The caret starts at the end, so the start of "two" and then the start of "one".
    handle_key(&mut session, key(KeyCode::PageUp));
    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.caret(), 0);

    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.scroll, 10, "the transcript was unreachable");

    // And back down: the end of "one", then the end of "two".
    handle_key(&mut session, key(KeyCode::PageDown));
    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(session.caret(), session.input().len());

    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(session.scroll, 0);
}

/// A single-line prompt has ends too, so the keys reach them before the transcript.
#[test]
fn the_page_keys_work_on_one_line() {
    let mut session = typed_into("a sentence");
    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(session.caret(), 0);
    assert_eq!(session.scroll, 0, "the transcript scrolled instead");
    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(session.caret(), "a sentence".len());
}

/// Under Ctrl, because the bare keys belong to the line being typed.
#[test]
fn home_and_end_jump_to_the_extremes() {
    let mut session = Session::new("none");
    handle_key(&mut session, ctrl_key(KeyCode::Home));
    assert_eq!(session.scroll, u16::MAX);
    handle_key(&mut session, ctrl_key(KeyCode::End));
    assert_eq!(session.scroll, 0);
}

/// The keys that move around a line of text, which is what makes the box editable rather than
/// only appendable.
#[test]
fn the_arrows_move_the_caret_through_the_line() {
    let mut session = typed_into("abc");
    assert_eq!(session.caret(), 3);

    assert_eq!(handle_key(&mut session, key(KeyCode::Left)), Action::Redraw);
    assert_eq!(session.caret(), 2);
    handle_key(&mut session, key(KeyCode::Right));
    assert_eq!(session.caret(), 3);

    // And they stop at the ends rather than wrapping or panicking.
    handle_key(&mut session, key(KeyCode::Right));
    assert_eq!(session.caret(), 3);
    for _ in 0..5 {
        handle_key(&mut session, key(KeyCode::Left));
    }
    assert_eq!(session.caret(), 0);
}

/// The bug this exists for: typing used to only ever append, so a correction meant deleting
/// back to it and retyping the rest.
#[test]
fn typing_lands_where_the_caret_is() {
    let mut session = typed_into("ac");
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, key(KeyCode::Char('b')));
    assert_eq!(session.input(), "abc");
    assert_eq!(
        session.caret(),
        2,
        "the caret did not follow what was typed"
    );
}

/// Home and End reach the ends of the line, which is what they do in every other text field.
#[test]
fn home_and_end_reach_the_ends_of_the_line() {
    let mut session = typed_into("a sentence");
    handle_key(&mut session, key(KeyCode::Home));
    assert_eq!(session.caret(), 0);
    assert_eq!(session.scroll, 0, "the transcript scrolled instead");
    handle_key(&mut session, key(KeyCode::End));
    assert_eq!(session.caret(), "a sentence".len());
}

/// Backspace deletes before the caret rather than at the end of the line.
#[test]
fn backspace_deletes_at_the_caret() {
    let mut session = typed_into("abc");
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert_eq!(session.input(), "ac");
    assert_eq!(session.caret(), 1);
}

/// Delete takes the character after the caret, which is the half Backspace cannot reach.
#[test]
fn delete_takes_the_character_after_the_caret() {
    let mut session = typed_into("abc");
    handle_key(&mut session, key(KeyCode::Home));
    handle_key(&mut session, key(KeyCode::Delete));
    assert_eq!(session.input(), "bc");
    assert_eq!(session.caret(), 0);
}

/// A word at a time, since a path or a flag is one thing to cross rather than a dozen.
#[test]
fn the_word_keys_cross_a_word_at_a_time() {
    let mut session = typed_into("read src/main.rs now");
    handle_key(&mut session, ctrl_key(KeyCode::Left));
    assert_eq!(session.caret(), "read src/main.rs ".len());
    handle_key(&mut session, ctrl_key(KeyCode::Left));
    assert_eq!(session.caret(), "read ".len());
    handle_key(&mut session, ctrl_key(KeyCode::Right));
    assert_eq!(session.caret(), "read src/main.rs".len());
}

/// Ctrl-K then Ctrl-Y at another place moves the text.
#[test]
fn ctrl_y_puts_back_what_ctrl_k_took_somewhere_else() {
    let mut session = typed_into("keep this drop that");
    for _ in 0..2 {
        handle_key(&mut session, ctrl_key(KeyCode::Left));
    }
    handle_key(&mut session, ctrl('k'));
    assert_eq!(session.input(), "keep this ");
    handle_key(&mut session, ctrl('a'));
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "drop thatkeep this ");
    assert_eq!(session.caret(), "drop that".len());
}

/// Ctrl-U and Ctrl-W keep what they took as well, and Alt-D takes the word after the caret.
#[test]
fn every_delete_key_keeps_what_it_took() {
    for (take, line, left, kept) in [
        (ctrl('u'), "one two", 0, "one two"),
        (ctrl('w'), "one two", 0, "two"),
        (alt('d'), "one two", 7, "one"),
    ] {
        let mut session = typed_into(line);
        for _ in 0..left {
            handle_key(&mut session, ctrl_key(KeyCode::Left));
        }
        if left > 0 {
            handle_key(&mut session, ctrl('a'));
        }
        handle_key(&mut session, take);
        handle_key(&mut session, ctrl('y'));
        assert!(
            session.input().contains(kept),
            "{take:?} kept nothing: {:?}",
            session.input()
        );
    }
}

/// The undo chord puts back what a delete took, pressed through the key handler rather than
/// called on the session directly, so the wiring from the chord to the step is covered too.
#[test]
fn the_undo_chord_puts_back_what_a_delete_took() {
    let mut session = typed_into("one two three");
    handle_key(&mut session, ctrl('w'));
    assert_eq!(session.input(), "one two ");
    handle_key(&mut session, ctrl('_'));
    assert_eq!(session.input(), "one two three");
}

/// A run of typing is one step back, not one for every character, the same way vi folds a
/// whole INSERT session into the one `u` after it reaches.
#[test]
fn the_undo_chord_takes_back_a_whole_typing_run() {
    let mut session = typed_into("hello");
    assert_eq!(session.input(), "hello");
    handle_key(&mut session, ctrl('_'));
    assert_eq!(session.input(), "");
}

/// A second run after an undo opens a step of its own, pressed through the key handler. The
/// flag that marks a run open must be closed by the undo that just put the first run's step
/// back, or the second run's keys would find it still open, share no step at all, and the
/// second press of the chord would have nothing to undo.
#[test]
fn the_undo_chord_leaves_the_next_run_able_to_open_its_own_step() {
    let mut session = typed_into("hello");
    handle_key(&mut session, ctrl('_'));
    assert_eq!(session.input(), "");

    for c in "abc".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(session.input(), "abc");
    handle_key(&mut session, ctrl('_'));
    assert_eq!(
        session.input(),
        "",
        "the second run opened no step of its own"
    );
}

/// Editing a line is one of the things a running turn still allows, undoing an edit among
/// them, so the chord reaches the same step through the mid-turn handler as it does at rest.
#[test]
fn the_undo_chord_works_while_a_turn_is_running() {
    let mut session = Session::new("kernel-enforced");
    for c in "one two".chars() {
        session.type_char(c);
    }
    session.status = Status::Working;

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('w')),
        Action::Redraw
    );
    assert_eq!(session.input(), "one ");

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('_')),
        Action::Redraw
    );
    assert_eq!(session.input(), "one two");
}

/// Kills in one direction join, and a change of direction starts a new buffer.
#[test]
fn consecutive_kills_join_in_the_direction_they_went() {
    let mut session = typed_into("one two three");
    handle_key(&mut session, ctrl('w'));
    handle_key(&mut session, ctrl('w'));
    assert_eq!(session.input(), "one ");
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "one two three");

    let mut session = typed_into("one two three");
    handle_key(&mut session, ctrl('a'));
    handle_key(&mut session, alt('d'));
    handle_key(&mut session, alt('d'));
    assert_eq!(session.input(), " three");
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "one two three");

    // A kill the other way starts a new buffer.
    let mut session = typed_into("ab cd ef");
    handle_key(&mut session, ctrl('a'));
    handle_key(&mut session, ctrl_key(KeyCode::Right));
    handle_key(&mut session, KeyEvent::from(KeyCode::Right));
    handle_key(&mut session, alt('d'));
    assert_eq!(session.input(), "ab  ef");
    handle_key(&mut session, ctrl('w'));
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "ab  ef");

    // A kill after typing replaces the buffer, and the ones after it join.
    let mut session = typed_into("one two three");
    handle_key(&mut session, ctrl('w'));
    handle_key(&mut session, KeyEvent::from(KeyCode::Char('x')));
    handle_key(&mut session, ctrl('w'));
    handle_key(&mut session, ctrl('w'));
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "one two x");
}

/// Ctrl-Y with nothing killed does nothing, and the line is not a change to undo.
#[test]
fn ctrl_y_with_nothing_killed_does_nothing() {
    let mut session = typed_into("abc");
    assert_eq!(handle_key(&mut session, ctrl('y')), Action::Redraw);
    assert_eq!(session.input(), "abc");
    assert_eq!(session.caret(), 3);
}

/// Alt-D at the end of the line deletes nothing and keeps nothing.
#[test]
fn alt_d_at_the_end_of_the_line_keeps_nothing() {
    let mut session = typed_into("abc");
    handle_key(&mut session, ctrl('w'));
    handle_key(&mut session, ctrl('y'));
    handle_key(&mut session, alt('d'));
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "abcabc");
}

/// A chord moved onto ctrl-y still wins over the yank.
#[test]
fn a_chord_on_ctrl_y_takes_precedence_over_the_yank() {
    let mut session = Session::new("none");
    let mut custom = std::collections::BTreeMap::new();
    custom.insert("stash".to_string(), "ctrl-y".to_string());
    session.adopt_keybindings(&custom);
    type_line(&mut session, "line to stash");
    handle_key(&mut session, ctrl('y'));
    assert_eq!(session.input(), "");
    assert_eq!(session.stashed(), Some("line to stash"));
}

/// The keys that used to be typed as characters, and now must not be: Ctrl-A on an empty line
/// once inserted a literal 'a'.
#[test]
fn the_editing_keys_are_not_typed_as_characters() {
    let mut session = Session::new("none");
    for binding in ['a', 'e', 'b', 'f', 'w', 'u', 'k'] {
        handle_key(&mut session, ctrl(binding));
    }
    assert!(
        session.input().is_empty(),
        "a binding was typed: {}",
        session.input()
    );
}

/// A pasted paragraph has rows, and Up and Down move between them before they reach for the
/// history: a line the user can see is the one they meant to edit.
#[test]
fn up_and_down_move_within_a_pasted_paragraph() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "first line\nsecond row");
    assert_eq!(handle_key(&mut session, key(KeyCode::Up)), Action::Redraw);
    assert_eq!(session.caret(), "first line".len());
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.caret(), "first line\nsecond row".len());
}

/// The caret keeps its place along the line, and clamps to the end of a shorter one rather
/// than off it.
#[test]
fn moving_between_lines_keeps_the_place_along_them() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "a longer first line\nshort\nlast");
    handle_key(&mut session, key(KeyCode::Home));
    for _ in 0..3 {
        handle_key(&mut session, key(KeyCode::Right));
    }
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.caret(), "a longer first line\nsho".len());

    // "last" is shorter than where the caret is, so it clamps to the end of it.
    handle_key(&mut session, key(KeyCode::End));
    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.caret(), session.input().len());
}

/// Off the top of the line, Up is the history's again, so a one-line prompt is unaffected and
/// a paragraph does not trap the keys.
#[test]
fn up_reaches_history_from_the_top_of_the_line() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    handle_paste(&mut session, "one\ntwo");
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    assert!(session.history.is_browsing(), "history was unreachable");
}

/// The line can be typed mid-turn, so it has to be editable mid-turn: a box that takes words
/// and will not let them be fixed is worse than one that takes none.
#[test]
fn the_caret_moves_while_a_turn_runs() {
    let mut session = typed_into("ac");
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "ac".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    handle_key_while_working(&mut session, key(KeyCode::Left));
    handle_key_while_working(&mut session, key(KeyCode::Char('b')));
    assert_eq!(session.input(), "abc");
}

/// The wheel is what most people reach for, so it scrolls with no modifier.
#[test]
fn the_mouse_wheel_scrolls() {
    let mut session = Session::new("none");
    let wheel = |kind| MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };

    assert_eq!(
        handle_mouse(&mut session, wheel(MouseEventKind::ScrollUp)),
        Action::Redraw
    );
    assert_eq!(session.scroll, 3);
    handle_mouse(&mut session, wheel(MouseEventKind::ScrollDown));
    assert_eq!(session.scroll, 0);
}

/// Clicks and drags are not bound to anything, so they must be ignored rather than
/// misinterpreted as scrolling.
#[test]
fn other_mouse_events_are_ignored() {
    let mut session = Session::new("none");
    let moved = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(handle_mouse(&mut session, moved), Action::None);
    assert_eq!(session.scroll, 0);
}

/// Typing must not be captured by the scroll bindings.
#[test]
fn scroll_keys_do_not_disturb_the_input() {
    let mut session = Session::new("none");
    for c in "hello".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.input(), "hello", "scrolling altered the input");
}

#[test]
fn backspace_deletes() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(session.input().is_empty());
}

/// The ladder the key walks, from the box. Four rungs and bypass where the flag was not given,
/// and back to asking: a key that stopped cycling would leave somebody in a mode they could not
/// press their way out of.
#[test]
fn shift_tab_cycles_the_permission_mode() {
    use bravebot_agent::PermissionMode;
    let mut session = Session::new("none");
    assert_eq!(session.permission_mode(), PermissionMode::Ask);

    for expected in [
        PermissionMode::AcceptEdits,
        PermissionMode::Plan,
        PermissionMode::Auto,
        PermissionMode::Bypass,
        PermissionMode::Ask,
    ] {
        handle_key(&mut session, shift(KeyCode::Tab));
        assert_eq!(session.permission_mode(), expected);
    }
}

/// Which spelling arrives is the terminal's choice: one that has been asked to disambiguate sends
/// Tab with a modifier, one that has not sends `BackTab`. A binding that worked on one machine
/// and not the next would read as broken.
#[test]
fn either_spelling_of_shift_tab_cycles_the_mode() {
    use bravebot_agent::PermissionMode;
    for press in [shift(KeyCode::Tab), key(KeyCode::BackTab)] {
        let mut session = Session::new("none");
        handle_key(&mut session, press);
        assert_eq!(
            session.permission_mode(),
            PermissionMode::AcceptEdits,
            "{press:?} did not cycle the mode"
        );
    }
}

/// The mode key must not type anything, and must not be taken for the completion Tab: a press
/// that changed the mode *and* accepted a half-typed command would do two things at once.
#[test]
fn the_mode_key_leaves_the_line_alone() {
    let mut session = typed_into("/mod");
    handle_key(&mut session, shift(KeyCode::Tab));
    assert_eq!(
        session.input(),
        "/mod",
        "the mode key completed the line or typed into it"
    );
}

#[test]
fn a_failed_turn_measures_context_if_requests_were_sent() {
    let mut session = Session::new("none");
    let sink = Trail::new();
    let fallback = TrustStore::new("/work");
    let fallback_programs = TrustedPrograms::new();
    let asked = Asked {
        name: "test-model".to_string(),
        comparable: true,
        configured: "test-model".to_string(),
    };

    fold_outcome(
        &mut session,
        Err(turn::TurnError::Precommit("failed".to_string())),
        sink,
        Carried {
            trust: fallback,
            programs: fallback_programs,
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
        },
        Occupied {
            budget: 100_000,
            guessed: false,
            last_request_tokens: 45_000,
        },
        asked,
        Line {
            text: "",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );

    assert_eq!(
        session.occupancy(),
        crate::state::Occupancy::Measured {
            used: 45_000,
            budget: 100_000,
            guessed: false,
        }
    );
    assert_eq!(session.fullness(), Some(45));
}

/// INPUT-22: a session resumed from a record that holds a measured request reads how full the
/// context is from that figure, and one resumed from a record that measured nothing reads that
/// it has not measured. The regression it rejects is a resume that leaves the reading at "not
/// yet measured" until a turn runs.
#[test]
fn a_resumed_session_reads_the_context_from_the_request_its_record_measured() {
    let extra = crate::testutil::scratch_dir("bravebot-restore-resumed-reading");
    std::fs::create_dir_all(&extra).expect("scratch");
    let config = a_config_needing_no_sign_in();

    let mut workspace = workspace_for_test();
    let mut record = a_recorded_session_in(workspace.root(), &extra);
    let mut measured = Conversation::new();
    measured.measured(60_000);
    record.conversation = measured.snapshot();
    let mut session = Session::new("none");
    restore_resumed(&mut session, &config, &mut workspace, &record).expect("the record is free");

    assert_eq!(
        session.occupancy(),
        crate::state::Occupancy::Measured {
            used: 60_000,
            budget: config.context_budget,
            guessed: config.budget_is_guessed(),
        },
        "a resumed session did not read the context from its record"
    );

    let mut workspace = workspace_for_test();
    let mut record = a_recorded_session_in(workspace.root(), &extra);
    record.conversation = Conversation::new().snapshot();
    let mut unmeasured = Session::new("none");
    restore_resumed(&mut unmeasured, &config, &mut workspace, &record).expect("the record is free");

    assert_eq!(
        unmeasured.occupancy(),
        crate::state::Occupancy::Unmeasured,
        "a record that measured nothing gave the session a reading"
    );
    let _ = std::fs::remove_dir_all(&extra);
}

/// A level named on the command line outranks what the settings say and is the one in force,
/// and a limit in the settings is the one the next turn is held to.
#[test]
fn adopting_settings_puts_the_asked_level_and_the_limit_in_force() {
    use bravebot_aichat::protocol::Effort;
    let settings = bravebot_config::Settings::parse(r#"{"limit": "250k"}"#);
    let mut asked = Session::new("none");
    let mut unasked = Session::new("none");

    adopt_settings(&mut asked, &settings, Some(Effort::Low));
    adopt_settings(&mut unasked, &settings, None);

    assert_eq!(asked.effort(), Some(Effort::Low));
    assert_ne!(
        unasked.effort(),
        Some(Effort::Low),
        "the level was set without being asked for"
    );
    assert!(
        asked.spend_limit().limit().is_some(),
        "the limit in the settings was not adopted"
    );
}

/// The idle time the settings file names reaches the recap through `adopt_settings`, and is
/// not the built-in five minutes.
#[test]
fn adopting_settings_puts_the_recap_idle_time_in_force() {
    use std::time::{Duration, Instant};
    let settings = bravebot_config::Settings::parse(r#"{"awaySummaryIdleSeconds": 90}"#);
    let mut session = Session::new("none");
    adopt_settings(&mut session, &settings, None);

    let start = Instant::now();
    session.away.turn_done(start);
    session.away.focus(false);
    let turns = crate::away::FEWEST_TURNS;
    assert!(
        !session.away.due(start + Duration::from_secs(89), turns),
        "the recap came before the named time"
    );
    assert!(
        session.away.due(start + Duration::from_secs(90), turns),
        "the idle time in the settings was not adopted"
    );
}

fn round_that_came_to(context_tokens: u64) -> bravebot_agent::Spent {
    bravebot_agent::Spent {
        tokens: context_tokens + 40,
        output_tokens: 40,
        context_tokens,
        ..Default::default()
    }
}

/// A first turn that has completed one round knows what its request came to. Leaving the
/// reading to the end of the turn kept it at "not measured" for a turn that ran for minutes.
#[test]
fn a_turn_in_flight_reads_how_full_the_context_is_after_its_first_round() {
    let mut session = Session::new("none");
    assert_eq!(
        crate::render::context_reading(&session),
        "context not yet measured"
    );

    progress_of_a_turn(&mut session, round_that_came_to(56_000), 100_000, true);

    assert_eq!(
        session.occupancy(),
        crate::state::Occupancy::Measured {
            used: 56_000,
            budget: 100_000,
            guessed: true,
        }
    );
    assert_eq!(crate::render::context_reading(&session), "context ~56%");
}

#[test]
fn each_round_of_a_turn_replaces_the_reading_with_what_its_request_came_to() {
    let mut session = Session::new("none");
    progress_of_a_turn(&mut session, round_that_came_to(30_000), 100_000, false);
    progress_of_a_turn(&mut session, round_that_came_to(48_000), 100_000, false);

    assert_eq!(crate::render::context_reading(&session), "context 48%");
}

/// A report that carries no figure, as a compaction's does, must not replace a compaction's
/// account of the room it won back, or an earlier turn's reading.
#[test]
fn a_round_that_reports_no_request_leaves_the_reading_where_it_was() {
    let mut compacted = Session::new("none");
    compacted.compacted(40_000, 100_000);
    progress_of_a_turn(&mut compacted, round_that_came_to(0), 100_000, false);
    assert_eq!(
        compacted.occupancy(),
        crate::state::Occupancy::Compacted {
            won_back: 40_000,
            budget: 100_000,
        }
    );

    let mut measured = Session::new("none");
    measured.measured(20_000, 100_000, false);
    progress_of_a_turn(&mut measured, round_that_came_to(0), 100_000, false);
    assert_eq!(measured.fullness(), Some(20));

    let mut nothing = Session::new("none");
    progress_of_a_turn(&mut nothing, round_that_came_to(0), 100_000, false);
    assert_eq!(nothing.occupancy(), crate::state::Occupancy::Unmeasured);
}

#[test]
fn a_failed_turn_with_no_requests_sent_remains_unmeasured() {
    let mut session = Session::new("none");
    let sink = Trail::new();
    let fallback = TrustStore::new("/work");
    let fallback_programs = TrustedPrograms::new();
    let asked = Asked {
        name: "test-model".to_string(),
        comparable: true,
        configured: "test-model".to_string(),
    };

    fold_outcome(
        &mut session,
        Err(turn::TurnError::Precommit("failed".to_string())),
        sink,
        Carried {
            trust: fallback,
            programs: fallback_programs,
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
        },
        Occupied {
            budget: 100_000,
            guessed: false,
            last_request_tokens: 0,
        },
        asked,
        Line {
            text: "",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );

    assert_eq!(session.occupancy(), crate::state::Occupancy::Unmeasured);
    assert_eq!(session.fullness(), None);
}
