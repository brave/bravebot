//! WATCH: the watching view over delegates and command outputs, and the keys that open it, move in it and close it.

use super::*;
use crate::state::{WatchScope, Watched};
use bravebot_agent::report::{DelegateId, Delegation};

/// A delegate beginning, numbered the way the driver numbers them.
fn spawn(session: &mut Session, kind: &'static str, task: &str) -> DelegateId {
    let id = DelegateId::nth(session.delegates().len() as u32 + 1);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id,
        kind: kind.to_string(),
        task: task.to_string(),
    });
    session.reporting_for(Some(id));
    id
}

/// A turn in flight is the whole of when there is a delegate to watch, so the key has to
/// answer then above all.
#[test]
fn ctrl_l_watches_the_delegate_that_is_working() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    spawn(&mut session, "checker", "run the build");

    handle_key_while_working(&mut session, ctrl('l'));

    assert!(
        session.watching_a_delegate(),
        "the key did not open the view"
    );
}

/// A key that does nothing is better than a mode that opens on an empty screen, which
/// puts a person somewhere with nothing to read and something to get out of.
#[test]
fn ctrl_l_does_nothing_where_no_delegate_has_run() {
    let mut session = Session::new("kernel-enforced");

    let action = handle_key(&mut session, ctrl('l'));

    assert!(
        matches!(action, Action::None),
        "the key answered with a mode"
    );
    assert!(session.watching().is_none());
}

fn ran_a_command(session: &mut Session, command: &str) {
    session.command_printed(bravebot_agent::report::Printed {
        command: command.to_string(),
        lines: vec!["first".to_string()],
        total: 1,
        read_by_the_planner: true,
        outcome: bravebot_agent::report::Outcome::Succeeded,
        job: None,
    });
}

/// WATCH-7. A session that ran a command and a delegate: the watch key opens the agents,
/// and the one row it holds is the delegate. A key that listed the command beside it would
/// put a build's output in the list of the agents that did the work.
#[test]
fn ctrl_l_lists_the_delegate_and_not_the_command() {
    let mut session = Session::new("kernel-enforced");
    ran_a_command(&mut session, "cargo test");
    spawn(&mut session, "reader", "find the parser");
    session.reporting_for(None);

    handle_key(&mut session, ctrl('l'));

    assert!(session.watching_a_delegate(), "the key did not open");
    assert!(
        session.watched_delegate().is_some(),
        "the key did not land on the delegate"
    );
    assert!(
        !session.listing_delegates(),
        "a list of one row was shown as a list"
    );
    assert!(
        session
            .watchable()
            .iter()
            .all(|row| !matches!(row, Watched::Output(_))),
        "the watch key's list held a command"
    );
}

/// WATCH-7. A session that ran only commands has nothing for the watch key to open, so it
/// answers with nothing, as it does where nothing has run at all.
#[test]
fn ctrl_l_does_nothing_where_only_commands_have_run() {
    let mut session = Session::new("kernel-enforced");
    ran_a_command(&mut session, "cargo test");

    let action = handle_key(&mut session, ctrl('l'));

    assert!(matches!(action, Action::None));
    assert!(
        session.watching().is_none(),
        "the watch key opened on a command"
    );
}

/// WATCH-7. What commands printed is opened by `/outputs`, on the row where there is one
/// and on the list where there are several, and the command says so where there is none.
#[test]
fn the_outputs_command_opens_what_commands_printed() {
    let mut session = Session::new("kernel-enforced");
    assert_eq!(
        dispatch_command(&mut session, commanded(OUTPUTS_COMMAND)),
        Action::Redraw
    );
    assert!(session.watching().is_none(), "it opened on nothing");

    ran_a_command(&mut session, "cargo test");
    spawn(&mut session, "reader", "find the parser");
    session.reporting_for(None);
    dispatch_command(&mut session, commanded(OUTPUTS_COMMAND));

    assert_eq!(
        session
            .watched_output()
            .map(|output| output.command.as_str()),
        Some("cargo test")
    );
    assert_eq!(session.watch_scope(), WatchScope::Outputs);
}

fn alt_l() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('l'), KeyModifiers::ALT)
}

/// WATCH-7. A session that ran two commands and a delegate: the outputs chord opens the
/// list of the commands and holds nothing else. An implementation that opened the agents
/// list, or put the delegate beside the commands, lands on the wrong scope or on a row
/// that is not a command.
#[test]
fn alt_l_opens_the_outputs_and_not_the_agents() {
    let mut session = Session::new("kernel-enforced");
    ran_a_command(&mut session, "cargo test");
    ran_a_command(&mut session, "cargo build");
    spawn(&mut session, "reader", "find the parser");
    session.reporting_for(None);

    let action = handle_key(&mut session, alt_l());

    assert_eq!(action, Action::Redraw);
    assert_eq!(session.watch_scope(), WatchScope::Outputs);
    assert!(session.watching().is_some(), "the chord did not open");
    assert!(session.listing_delegates(), "two commands were not a list");
    let rows = session.watchable();
    assert_eq!(rows.len(), 2, "the outputs list held another row");
    assert!(
        rows.iter().all(|row| matches!(row, Watched::Output(_))),
        "the outputs list held something that is not a command"
    );
}

/// WATCH-7. The chord and `/outputs` share one path, so where no command has run both
/// leave the same note and open nothing, even in a session that has a delegate. A silent
/// chord, or one with wording of its own, differs from the command here.
#[test]
fn alt_l_says_what_the_outputs_command_says_where_no_command_has_run() {
    let mut typed = Session::new("kernel-enforced");
    spawn(&mut typed, "reader", "find the parser");
    typed.reporting_for(None);
    dispatch_command(&mut typed, commanded(OUTPUTS_COMMAND));
    let said = last_note(&typed).to_string();
    assert_eq!(said, t!(outputs_none).to_string());

    let mut pressed = Session::new("kernel-enforced");
    spawn(&mut pressed, "reader", "find the parser");
    pressed.reporting_for(None);
    let before = pressed.transcript.len();

    let action = handle_key(&mut pressed, alt_l());

    assert_eq!(action, Action::Redraw);
    assert!(pressed.watching().is_none(), "the chord opened on nothing");
    assert_eq!(pressed.transcript.len(), before + 1, "no note was left");
    assert_eq!(last_note(&pressed), said);
}

/// WATCH-7. Ctrl-L stays the key for the agents. Where only commands have run it does
/// nothing and leaves no note, so it did not fall back to the outputs path, which says
/// something where nothing opens.
#[test]
fn ctrl_l_where_only_commands_ran_does_not_take_the_outputs_path() {
    let mut session = Session::new("kernel-enforced");
    ran_a_command(&mut session, "cargo test");
    let before = session.transcript.len();

    let action = handle_key(&mut session, ctrl('l'));

    assert_eq!(action, Action::None);
    assert!(session.watching().is_none());
    assert_eq!(session.transcript.len(), before, "Ctrl-L left a note");
}

/// WATCH-7. Inside the view the chord is the view's: over the outputs it closes, as the
/// watch chord does, and over the agents it switches to the outputs, whether the view is
/// on a list or inside a delegate. The arm that refuses every other chord swallowing it
/// leaves the view as it was.
#[test]
fn alt_l_inside_the_view_closes_the_outputs_and_switches_from_the_agents() {
    let handlers: [fn(&mut Session, KeyEvent) -> Action; 2] =
        [handle_key, handle_key_while_working];
    for handler in handlers {
        let ready = || {
            let mut session = Session::new("kernel-enforced");
            session.status = Status::Working;
            ran_a_command(&mut session, "cargo test");
            spawn(&mut session, "reader", "find the parser");
            spawn(&mut session, "checker", "run the build");
            session.reporting_for(None);
            session
        };

        let mut session = ready();
        assert!(session.watch_outputs());
        handler(&mut session, alt_l());
        assert!(session.watching().is_none(), "the chord did not close");

        let mut session = ready();
        assert!(session.watch());
        assert_eq!(session.watch_scope(), WatchScope::Agents);
        handler(&mut session, alt_l());
        assert_eq!(
            session.watch_scope(),
            WatchScope::Outputs,
            "the chord did not switch from the list of agents"
        );
        assert_eq!(
            session
                .watched_output()
                .map(|output| output.command.as_str()),
            Some("cargo test")
        );

        let mut session = ready();
        assert!(session.watch());
        session.open_watched();
        assert!(session.watching_a_delegate());
        handler(&mut session, alt_l());
        assert_eq!(
            session.watch_scope(),
            WatchScope::Outputs,
            "the chord did not switch from inside a delegate"
        );
    }
}

/// WATCH-7. Over the agents with no command run, the chord says so as `/outputs` does and
/// the view stays where it was.
#[test]
fn alt_l_over_the_agents_with_no_command_says_so_and_stays() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    session.reporting_for(None);
    assert!(session.watch());
    let before = session.watching();

    handle_key_while_working(&mut session, alt_l());

    assert_eq!(session.watching(), before, "the view moved");
    assert_eq!(last_note(&session), t!(outputs_none).to_string());
}

/// The chord that opened the view leaves it, wherever it has been moved to. Bound onto `j`
/// on purpose: `j` walks the list and every chord but the view's own is refused, so a chord
/// read after either would walk the list or do nothing instead of leaving.
#[test]
fn a_moved_chord_leaves_the_view_it_opened() {
    let mut session = Session::new("kernel-enforced");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("watch".to_string(), "alt-j".to_string());
    session.adopt_keybindings(&moved);
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");

    let alt_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT);
    handle_key(&mut session, alt_j);
    assert!(session.watching().is_some(), "the chord did not open it");

    session.open_watched();
    handle_key(&mut session, alt_j);
    assert!(
        session.watching().is_none(),
        "the chord walked the list instead of leaving"
    );

    // And the chord it was moved off of does neither.
    handle_key(&mut session, ctrl('l'));
    assert!(
        session.watching().is_none(),
        "the old chord still opened it"
    );
}

/// A turn with two delegates and the actions on `moved`, standing at each of the view's
/// three places: the list on a delegate, the list on the session, and one delegate's lines.
fn at_every_place_in_the_view(moved: &[(&str, &str)]) -> Vec<(&'static str, Session)> {
    let bindings: std::collections::BTreeMap<String, String> = moved
        .iter()
        .map(|(action, chord)| (action.to_string(), chord.to_string()))
        .collect();
    let opened = || {
        let mut session = Session::new("kernel-enforced");
        session.status = Status::Working;
        session.adopt_keybindings(&bindings);
        spawn(&mut session, "reader", "find the parser");
        spawn(&mut session, "checker", "run the build");
        assert!(session.watch(), "there was nothing to watch");
        session
    };

    let on_a_delegate = opened();
    assert!(on_a_delegate.listing_delegates() && !on_a_delegate.listing_on_the_session());

    let mut on_the_session = opened();
    on_the_session.watch_previous();
    on_the_session.watch_previous();
    assert!(on_the_session.listing_on_the_session());

    let mut inside_one = opened();
    inside_one.open_watched();
    assert!(inside_one.watching_a_delegate());

    vec![
        ("on the list", on_a_delegate),
        ("on the session row", on_the_session),
        ("inside a delegate", inside_one),
    ]
}

/// Somebody who moved Watch off Ctrl-L presses it from habit, and the view walks the list
/// with bare letters: read as the `l` it carries, the chord opened the delegate the list
/// was on, or closed the view from the session row. Every default is vacated, so a letter
/// the view comes to read is held to this too. Ctrl-B is the one not pressed: the view
/// pages back on it whoever holds it, the way the scroller does.
#[test]
fn the_chord_an_action_was_moved_off_does_nothing_inside_the_view() {
    let moved = [
        ("background", "alt-b"),
        ("editor", "alt-e"),
        ("history", "alt-r"),
        ("paste", "alt-u"),
        ("scroller", "alt-o"),
        ("stash", "alt-s"),
        ("trail", "alt-t"),
        ("watch", "alt-w"),
    ];
    let handlers: [fn(&mut Session, KeyEvent) -> Action; 2] =
        [handle_key, handle_key_while_working];
    for handler in handlers {
        for (place, mut session) in at_every_place_in_the_view(&moved) {
            for vacated in ['g', 'l', 'o', 'r', 's', 't', 'v'].map(ctrl) {
                assert!(
                    !session.bindings().claims(&vacated),
                    "{vacated:?} was not vacated"
                );
                let before = (session.watching(), session.scroll);

                let action = handler(&mut session, vacated);

                assert_eq!(action, Action::None, "{vacated:?} answered {place}");
                assert_eq!(
                    (session.watching(), session.scroll),
                    before,
                    "{vacated:?} moved the view {place}"
                );
            }
        }
    }
}

/// The view asks the bindings about Watch alone, so the other eight chords are nobody's in
/// here. Moved onto the keys the view walks with, one read as its key would open, close or
/// move the view on a chord the person gave to something else. The second set is the
/// view's own chords with Alt added, and keys that are not letters.
#[test]
fn a_chord_moved_onto_a_key_the_view_reads_is_not_that_key() {
    let onto_letters = [
        ("background", "alt-b"),
        ("editor", "alt-y"),
        ("history", "alt-q"),
        ("paste", "alt-j"),
        ("scroller", "alt-k"),
        ("stash", "alt-n"),
        ("trail", "alt-p"),
        ("watch", "alt-w"),
    ];
    let onto_the_rest = [
        ("background", "ctrl-alt-b"),
        ("editor", "ctrl-alt-u"),
        ("history", "ctrl-alt-d"),
        ("paste", "ctrl-alt-c"),
        ("scroller", "alt-b"),
        ("stash", "ctrl-down"),
        ("trail", "alt-esc"),
        ("watch", "alt-w"),
    ];
    for moved in [onto_letters, onto_the_rest] {
        for (place, mut session) in at_every_place_in_the_view(&moved) {
            for (action, spelling) in moved.iter().filter(|(action, _)| *action != "watch") {
                let chord = crate::keybindings::KeyChord::parse(spelling)
                    .expect("the spelling should parse");
                let chord = KeyEvent::new(chord.code, chord.modifiers);
                assert!(
                    session.bindings().claims(&chord),
                    "{action} did not take {spelling}"
                );
                let before = (session.watching(), session.scroll);

                let answer = handle_key_while_working(&mut session, chord);

                assert_eq!(answer, Action::None, "{spelling} answered {place}");
                assert_eq!(
                    (session.watching(), session.scroll),
                    before,
                    "{spelling} was read as the key it carries {place}"
                );
            }
        }
    }
}

/// A terminal speaking the keyboard protocol this client asks for reports Super, Hyper and
/// Meta as well. No action can be moved onto them, and a letter held with one is still not
/// the letter: Super-L opened the delegate the list was on.
#[test]
fn a_letter_held_with_any_other_modifier_is_not_that_letter() {
    for held in [KeyModifiers::SUPER, KeyModifiers::HYPER, KeyModifiers::META] {
        for (place, mut session) in at_every_place_in_the_view(&[]) {
            for letter in ['l', 'q', 'j', 'k', 'n', 'p', 'b'] {
                let chord = KeyEvent::new(KeyCode::Char(letter), held);
                let before = (session.watching(), session.scroll);

                let answer = handle_key_while_working(&mut session, chord);

                assert_eq!(answer, Action::None, "{chord:?} answered {place}");
                assert_eq!(
                    (session.watching(), session.scroll),
                    before,
                    "{chord:?} was read as {letter:?} {place}"
                );
            }
        }
    }
}

/// Refusing chords stops at the ones the view answers itself: Ctrl-U and Ctrl-D move a
/// delegate's lines half a screen and Ctrl-B a whole one, as they move the scroller's, and
/// a terminal reports Shift-Tab as BackTab with Shift held.
#[test]
fn the_keys_the_view_reads_with_a_modifier_held_still_answer() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    assert!(session.watch(), "there was nothing to watch");
    session.open_watched();
    session.note_layout(crate::state::Laid {
        width: 80,
        columns: 80,
        lines: 0,
        height: 10,
        rows: 100,
        prompts: Vec::new(),
        matches: Vec::new(),
        continued: Vec::new(),
        panes: Vec::new(),
        ..crate::state::Laid::default()
    });

    handle_key_while_working(&mut session, ctrl('u'));
    assert_eq!(session.scroll, 5, "ctrl-u did not move half a screen back");
    handle_key_while_working(&mut session, ctrl('d'));
    assert_eq!(session.scroll, 0, "ctrl-d did not move half a screen on");
    handle_key_while_working(&mut session, ctrl('b'));
    assert_eq!(
        session.scroll, 10,
        "ctrl-b did not move a whole screen back"
    );

    let shift_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
    handle_key_while_working(&mut session, shift_tab);
    assert_eq!(
        session
            .watched_delegate()
            .map(|delegate| delegate.kind.as_str()),
        Some("reader"),
        "shift-tab did not step back a delegate"
    );
}

/// The way out is read against the nearest level. Somebody who opened a delegate from the
/// list is going back to the list, and only then out.
#[test]
fn q_goes_back_to_the_list_before_it_closes() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    handle_key(&mut session, ctrl('l'));
    session.open_watched();

    handle_key(&mut session, key(KeyCode::Char('q')));
    assert!(
        session.listing_delegates(),
        "q left the mode from a delegate"
    );

    handle_key(&mut session, key(KeyCode::Char('q')));
    assert!(session.watching().is_none(), "q did not close the list");
}

/// With one delegate there is no list behind it, and a key that went back to an empty
/// index would be a worse answer than closing.
#[test]
fn q_closes_outright_where_there_is_no_list_to_go_back_to() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    handle_key(&mut session, ctrl('l'));

    handle_key(&mut session, key(KeyCode::Char('q')));

    assert!(
        session.watching().is_none(),
        "the view had nowhere to go and stayed"
    );
}

/// An answer opens the view by itself on its own row (WATCH-19), and a recap of a session
/// the person came back to is one. They never saw the list, so the way back leaves the
/// mode for the conversation they were reading, however many rows the list has. Going to
/// the list first puts them two steps from where they were.
#[test]
fn the_way_back_from_an_answer_the_view_opened_by_itself_closes_it() {
    for leaving in [key(KeyCode::Esc), key(KeyCode::Char('q'))] {
        let mut session = Session::new("kernel-enforced");
        spawn(&mut session, "reader", "find the parser");
        session.reporting_for(None);
        session.begin_aside();
        // Read back during the wait, which answers keys.
        session.scroll = 7;
        aside_answered(
            &mut session,
            crate::command_table::RECAP_LABEL.to_string(),
            bravebot_agent::aside::Answered {
                shown: "the parser was being rewritten".to_string(),
                kept: None,
                usage: Default::default(),
            },
        );
        assert!(
            session.watched_aside().is_some(),
            "the answer did not open the view"
        );
        assert_eq!(session.watchable().len(), 2, "there is no list to skip");

        handle_key(&mut session, leaving);

        assert!(
            session.watching().is_none(),
            "{leaving:?} did not close the view: listing {}",
            session.listing_delegates()
        );
        assert_eq!(
            session.scroll, 7,
            "{leaving:?} lost the conversation's place"
        );
    }
}

/// An answer that lands while a row opened from the list is on the screen replaces that
/// row with one the list did not open. The way back is the answer's, which closes, and not
/// the replaced row's.
#[test]
fn an_answer_over_a_row_opened_from_the_list_closes_on_the_way_back() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    session.reporting_for(None);
    handle_key(&mut session, ctrl('l'));
    session.open_watched();
    session.begin_aside();
    aside_answered(
        &mut session,
        crate::command_table::RECAP_LABEL.to_string(),
        bravebot_agent::aside::Answered {
            shown: "the parser was being rewritten".to_string(),
            kept: None,
            usage: Default::default(),
        },
    );
    assert!(session.watched_aside().is_some(), "the answer did not open");

    handle_key(&mut session, key(KeyCode::Esc));

    assert!(
        session.watching().is_none(),
        "escape went back to the list the replaced row came from"
    );
}

/// A person stops the nearest thing. Somebody who went to look at what a delegate was
/// doing is not asking for the turn to end when they come back out.
#[test]
fn the_view_answers_the_stop_keys_before_the_turn_does() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    spawn(&mut session, "checker", "run the build");

    // The loops answer the stop keys before any ladder does, so the guard that lets the
    // view have them first is the whole of what keeps the key that closes it from ending
    // the turn the person opened it to watch.
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        assert!(
            stops_the_turn(&session, stopping),
            "{stopping:?} did not reach the turn with nothing in the way"
        );
    }

    handle_key_while_working(&mut session, ctrl('l'));
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        assert!(
            !stops_the_turn(&session, stopping),
            "{stopping:?} stopped the turn from inside the view"
        );
    }

    let action = handle_key_while_working(&mut session, ctrl('c'));

    assert!(
        !matches!(action, Action::Cancel),
        "closing the view cancelled the turn behind it"
    );
    assert!(session.watching().is_none(), "the view did not close");
    assert_eq!(session.status, Status::Working, "the turn was stopped");
}

/// A summary and an aside read the stop keys themselves rather than through the turn's
/// loop, and the view is open across them: it is opened mid-turn and nothing closes it when
/// the turn ends. Answered by the request instead, either key stopped the request from
/// inside the view.
#[test]
fn the_view_answers_the_stop_keys_before_a_single_request_does() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = Session::new("kernel-enforced");
        spawn(&mut session, "checker", "run the build");
        session.begin_aside();
        let cancel = Cancel::new();

        one_request_key(&mut session, ctrl('l'), &cancel);
        assert!(session.watching().is_some(), "the view did not open");

        one_request_key(&mut session, stopping, &cancel);

        assert!(
            session.watching().is_none(),
            "{stopping:?} left the view open"
        );
        assert!(!session.is_quitting(), "{stopping:?} ended the session");
        assert!(
            !cancel.is_cancelled(),
            "{stopping:?} stopped the request from inside the view"
        );
    }
}

/// The goal check goes out as a turn ends, which is exactly when a view opened during that
/// turn is still standing over the session, so it is the loop most likely to be asked one
/// of these keys with the view open. The goal and the check are not what the press is about
/// there.
#[test]
fn the_view_answers_the_stop_keys_before_the_goal_check_does() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = Session::new("kernel-enforced");
        spawn(&mut session, "checker", "run the build");
        session.begin_aside();
        session.start_goal("cargo test exits 0".to_string());
        let cancel = Cancel::new();

        goal_check_key(&mut session, ctrl('l'), &cancel);
        assert!(session.watching().is_some(), "the view did not open");
        let said = session.transcript.len();

        goal_check_key(&mut session, stopping, &cancel);

        assert!(
            session.watching().is_none(),
            "{stopping:?} left the view open"
        );
        assert!(
            session.goal().is_some(),
            "{stopping:?} cleared the goal from inside the view"
        );
        assert!(
            !cancel.is_cancelled(),
            "{stopping:?} stopped the check from inside the view"
        );
        assert!(!session.is_quitting(), "{stopping:?} ended the session");
        assert_eq!(
            session.transcript.len(),
            said,
            "{stopping:?} reported the goal from inside the view"
        );
    }
}

/// Reading the mode first is what makes a release worth telling from a press here: the press
/// closes the view, so the release of that same chord arrives with nothing open and reaches
/// the request behind it. Handled as a press it stops the request, or takes the goal off,
/// on the way up from the press that did neither.
#[test]
fn the_release_of_the_press_that_closed_the_view_is_not_a_second_press() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "checker", "run the build");
    session.begin_aside();
    let cancel = Cancel::new();

    one_request_key(&mut session, ctrl('l'), &cancel);
    one_request_key(&mut session, ctrl('c'), &cancel);
    assert!(session.watching().is_none(), "the view did not close");

    one_request_key(&mut session, released(ctrl('c')), &cancel);
    assert!(
        !cancel.is_cancelled(),
        "the release stopped the request the press never reached"
    );
    assert!(!session.is_quitting(), "the release ended the session");

    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "checker", "run the build");
    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());
    let cancel = Cancel::new();

    goal_check_key(&mut session, ctrl('l'), &cancel);
    goal_check_key(&mut session, ctrl('c'), &cancel);
    assert!(session.watching().is_none(), "the view did not close");

    goal_check_key(&mut session, released(ctrl('c')), &cancel);
    assert!(session.goal().is_some(), "the release took the goal off");
    assert!(
        !cancel.is_cancelled(),
        "the release stopped the check the press never reached"
    );
    assert!(!session.is_quitting(), "the release ended the session");
}

/// A command line queued behind a turn begins as the turn ends, and nothing closes the view
/// then, so a person reading a delegate when the turn finished is still reading it while
/// the command runs. The view is on the second of two delegates, so there is a list behind
/// it.
fn watched_under_a_running_command() -> (Session, Cancel) {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    let reader = spawn(&mut session, "reader", "find the parser");
    let checker = spawn(&mut session, "checker", "run the build");
    handle_key_while_working(&mut session, ctrl('l'));
    session.open_watched();
    session.delegate_finished(reader, "answered".to_string(), false, None);
    session.delegate_finished(checker, "built".to_string(), false, None);
    session.complete("an answer", Vec::new(), 0);
    session.begin_command();
    assert_eq!(
        session.watched_delegate().map(|d| d.kind.as_str()),
        Some("checker"),
        "the view was not on a delegate when the command began"
    );
    (session, Cancel::new())
}

/// SHELL-7: the rows of a running command belong to the transcript the command is in. A
/// delegate's own view is another sequence, so a command printing under it is not drawn
/// there, and is drawn again once the view is closed.
///
/// Rejects: the rows being drawn at the end of whatever view is open.
#[test]
fn the_view_left_open_under_a_running_command_does_not_draw_what_it_prints() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let drawn = |session: &Session| -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| {
                render::draw(frame, session);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..30)
            .map(|y| {
                (0..100)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let (mut session, _cancel) = watched_under_a_running_command();
    session.command_printing("rows of the running command\n");
    assert!(
        !drawn(&session).contains("rows of the running command"),
        "the command's rows were drawn in a delegate's view"
    );

    session.stop_watching();
    assert!(
        drawn(&session).contains("rows of the running command"),
        "the rows were not drawn once the view closed"
    );
}

/// The two keys that close the view from either level close it from a delegate with a
/// list behind it, and the command they did not reach goes on.
#[test]
fn ctrl_c_and_ctrl_l_close_the_view_left_open_under_a_running_command() {
    for closing in [ctrl('c'), ctrl('l')] {
        let (mut session, cancel) = watched_under_a_running_command();

        command_key(&mut session, closing, &cancel);

        assert!(
            session.watching().is_none(),
            "{closing:?} left the view open"
        );
        assert!(
            !cancel.is_cancelled(),
            "{closing:?} stopped the command as well as closing the view"
        );
    }
}

/// Escape and `q` are read against the nearest level while a command runs as they are
/// mid-turn: back to the list from a delegate, and out from the list.
#[test]
fn escape_and_q_under_a_running_command_go_back_to_the_list_before_they_close() {
    for leaving in [key(KeyCode::Esc), key(KeyCode::Char('q'))] {
        let (mut session, cancel) = watched_under_a_running_command();

        command_key(&mut session, leaving, &cancel);
        assert!(
            session.listing_delegates(),
            "{leaving:?} did not go back to the list from a delegate"
        );

        command_key(&mut session, leaving, &cancel);
        assert!(
            session.watching().is_none(),
            "{leaving:?} did not close the list"
        );
        assert!(
            !cancel.is_cancelled(),
            "{leaving:?} stopped the command from inside the view"
        );
    }
}

/// The press that closes the view is spent doing so, and its release is not a press, so
/// the next press is the one that stops the command.
#[test]
fn the_press_after_the_view_closes_stops_the_command() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let (mut session, cancel) = watched_under_a_running_command();

        command_key(&mut session, ctrl('c'), &cancel);
        assert!(session.watching().is_none(), "the view did not close");
        assert!(
            !cancel.is_cancelled(),
            "the press that closed the view stopped the command"
        );
        command_key(&mut session, released(ctrl('c')), &cancel);
        assert!(
            !cancel.is_cancelled(),
            "the release of the press that closed the view stopped the command"
        );

        command_key(&mut session, stopping, &cancel);
        assert!(cancel.is_cancelled(), "{stopping:?} did not reach it");
    }
}

/// The view is moved through with the keys it answers mid-turn, since somebody reading a
/// delegate when the command began is still reading.
#[test]
fn the_view_left_open_under_a_running_command_still_moves_between_delegates() {
    let (mut session, cancel) = watched_under_a_running_command();

    command_key(&mut session, key(KeyCode::Char('p')), &cancel);
    assert_eq!(
        session.watched_delegate().map(|d| d.kind.as_str()),
        Some("reader"),
        "p did not move to the previous delegate"
    );

    command_key(&mut session, key(KeyCode::Char('n')), &cancel);
    assert_eq!(
        session.watched_delegate().map(|d| d.kind.as_str()),
        Some("checker"),
        "n did not move to the next delegate"
    );
    assert!(!cancel.is_cancelled());
}

/// Both keys close, and a person who reached for the one they close every other panel
/// with is not asking for the turn behind it to end.
#[test]
fn escape_leaves_the_view_the_way_q_does() {
    for leaving in [key(KeyCode::Esc), key(KeyCode::Char('q'))] {
        let mut session = Session::new("kernel-enforced");
        session.status = Status::Working;
        spawn(&mut session, "reader", "find the parser");
        spawn(&mut session, "checker", "run the build");
        handle_key_while_working(&mut session, ctrl('l'));
        session.open_watched();

        let action = handle_key_while_working(&mut session, leaving);
        assert!(
            !matches!(action, Action::Cancel),
            "{leaving:?} cancelled the turn behind the view"
        );
        assert!(
            session.listing_delegates(),
            "{leaving:?} did not go back to the list"
        );

        handle_key_while_working(&mut session, leaving);
        assert!(session.watching().is_none(), "{leaving:?} did not close");
        assert_eq!(session.status, Status::Working, "the turn was stopped");
    }
}

/// Nothing falls through to a box the person cannot see. Typed there, the words would be
/// waiting in a line nobody knows they are writing.
#[test]
fn a_typed_character_does_not_reach_the_box_while_a_delegate_is_watched() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    handle_key(&mut session, ctrl('l'));

    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Char('z')));

    assert!(session.input().is_empty(), "what was typed reached the box");
}

/// A paste and a drop reach this process as events of their own rather than as keys, so
/// the guard that holds every key at the view does not see them. The box they would land
/// in is not drawn at either level, so the words would be waiting in a line nobody knows
/// they are writing, to be sent to the turn the person went to look behind. The empty
/// paste is the one worth saying separately: answered, it spends a hint said once a
/// session on a sentence drawn nowhere and asks the loop to read the clipboard into that
/// same invisible line.
#[test]
fn a_paste_and_a_drop_do_not_reach_the_line_while_the_view_is_open() {
    let directory = crate::testutil::scratch_dir("bravebot-app-drop-while-watching");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    let file = directory.join("shot.png");
    std::fs::write(&file, [0x89u8, 0x50]).expect("write");
    let dropped = file.to_string_lossy().into_owned();

    let mut session = Session::new("kernel-enforced").in_workspace(&directory);
    session.status = Status::Working;
    spawn(&mut session, "reader", "find the parser");
    for c in "half a thought".chars() {
        session.type_char(c);
    }
    handle_key_while_working(&mut session, ctrl('l'));
    assert!(session.watching().is_some(), "the view did not open");
    let said = session.transcript.len();

    // Mid-turn, because a delegate only runs inside a turn, so that is when a paste while
    // watching arrives.
    for pasted in ["pasted", "", dropped.as_str()] {
        assert_eq!(
            handle_paste(&mut session, pasted),
            Action::None,
            "a paste of {pasted:?} was answered from inside the view"
        );
    }

    assert_eq!(
        session.transcript.len(),
        said,
        "the empty paste spent the hint from inside the view"
    );
    assert!(
        session.attached().is_empty(),
        "a drop staged an attachment from inside the view"
    );

    handle_key_while_working(&mut session, key(KeyCode::Char('q')));
    assert!(session.watching().is_none(), "q did not close the view");
    assert_eq!(
        session.input(),
        "half a thought",
        "what was pasted reached the box"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// Comparing two runs is what having several is for, and going back through the list to
/// do it is three keys where one will do.
#[test]
fn n_and_p_move_between_delegates() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    handle_key(&mut session, ctrl('l'));
    session.open_watched();

    handle_key(&mut session, key(KeyCode::Char('p')));
    assert_eq!(
        session
            .watched_delegate()
            .map(|delegate| delegate.kind.as_str()),
        Some("reader")
    );

    handle_key(&mut session, key(KeyCode::Char('n')));
    assert_eq!(
        session
            .watched_delegate()
            .map(|delegate| delegate.kind.as_str()),
        Some("checker")
    );
}

/// A person who sees one delegate going the wrong way stops that one. The turn and the
/// delegate beside it are the work they did not want thrown away.
#[test]
fn x_stops_the_delegate_the_view_is_on_and_no_other() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    handle_key(&mut session, ctrl('l'));
    session.open_watched();
    handle_key(&mut session, key(KeyCode::Char('p')));
    assert_eq!(
        session.watched_delegate().map(|d| d.kind.as_str()),
        Some("reader")
    );

    handle_key(&mut session, key(KeyCode::Char('x')));

    let stopped: Vec<_> = session
        .delegates()
        .iter()
        .map(|delegate| (delegate.kind.clone(), delegate.stop.is_requested()))
        .collect();
    assert_eq!(
        stopped,
        vec![("reader".to_string(), true), ("checker".to_string(), false)]
    );
    assert!(
        session.watching_a_delegate(),
        "stopping a delegate closed the view"
    );
    assert_eq!(
        session.status,
        Status::Working,
        "stopping a delegate ended the turn"
    );
}

/// The key is the view's for a delegate that is still working. On the list, on a delegate
/// that has finished and on one already asked it is read as nothing.
#[test]
fn x_does_nothing_on_the_list_or_on_a_delegate_that_is_not_working() {
    let mut session = Session::new("kernel-enforced");
    let finished = spawn(&mut session, "reader", "find the parser");
    session.delegate_finished(finished, "answered".to_string(), false, None);
    spawn(&mut session, "checker", "run the build");

    handle_key(&mut session, ctrl('l'));
    assert!(session.listing_delegates());
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert!(
        session.delegates().iter().all(|d| !d.stop.is_requested()),
        "x on the list stopped a delegate"
    );

    handle_key(&mut session, key(KeyCode::Up));
    session.open_watched();
    assert_eq!(
        session.watched_delegate().map(|d| d.kind.as_str()),
        Some("reader")
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('x'))),
        Action::None
    );
    assert!(
        session.delegates().iter().all(|d| !d.stop.is_requested()),
        "x asked a delegate that had finished to stop"
    );
}

/// A chord is not the key: Ctrl-X and Alt-X are not the view's.
#[test]
fn a_chord_on_x_does_not_stop_a_delegate() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    handle_key(&mut session, ctrl('l'));
    session.open_watched();

    handle_key(&mut session, ctrl('x'));
    handle_key(
        &mut session,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
    );

    assert!(session.delegates().iter().all(|d| !d.stop.is_requested()));
}

/// The list is a list of one thing to do: open the row it is on.
#[test]
fn enter_opens_the_delegate_the_list_is_on() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    handle_key(&mut session, ctrl('l'));
    handle_key(&mut session, key(KeyCode::Up));

    handle_key(&mut session, key(KeyCode::Enter));

    assert!(
        session.watching_a_delegate(),
        "enter did not open a delegate"
    );
    assert_eq!(
        session
            .watched_delegate()
            .map(|delegate| delegate.kind.as_str()),
        Some("reader"),
        "enter opened a delegate other than the one the list was on"
    );
}

/// The session is a row in the list like the others, so the key that opens a row is what
/// takes somebody back to the conversation. Leaving by a key that is on no row is the
/// part of this that was hard to find.
#[test]
fn opening_the_session_row_goes_back_to_the_conversation() {
    let mut session = Session::new("kernel-enforced");
    spawn(&mut session, "reader", "find the parser");
    spawn(&mut session, "checker", "run the build");
    handle_key(&mut session, ctrl('l'));
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    assert!(
        session.listing_on_the_session(),
        "the highlight never reached the session row"
    );

    handle_key(&mut session, key(KeyCode::Enter));

    assert!(
        session.watching().is_none(),
        "enter on the session row did not go back to the conversation"
    );
}
