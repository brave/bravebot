//! What a session asks of the terminal for its length and what it gives back, and how input that arrives together is read.

use super::*;

/// A session draws on a screen of its own, so what was in the terminal before it started is
/// still there afterwards. Nothing else in this file can be checked against a terminal, and a
/// takeover that stopped happening would look like a working program.
#[test]
fn a_session_draws_on_a_screen_of_its_own() {
    // 1049 is the alternate screen. Named as a number because that is what is sent.
    assert!(modes_left_on(&asked_for(false)).contains("1049"));
    assert!(!modes_left_on(&given_back(false)).contains("1049"));
}

/// Every mode is given back, whether the session ended by being left or by failing. A mode
/// kept past the last frame is a terminal the person has to repair by hand: mouse reporting
/// left on turns every click into unreadable bytes, and bracketed paste left on prints its
/// markers into whatever they type next.
#[test]
fn every_mode_a_session_asks_for_is_given_back() {
    for enhanced in [false, true] {
        let asked = modes_left_on(&asked_for(enhanced));
        let handed_back = modes_left_on(&given_back(enhanced));
        assert!(!asked.is_empty(), "nothing was asked for");
        for mode in &asked {
            assert!(
                !handed_back.contains(mode),
                "mode {mode} is left on: asked {asked:?}, handed back {handed_back:?}"
            );
        }
    }
}

/// Disambiguated keys are pushed onto a stack the terminal keeps, so the one thing owed is a
/// pop: a push left on the stack outlives the process and changes what every later program
/// reads from the keyboard.
#[test]
fn a_pushed_keyboard_mode_is_popped_and_an_unpushed_one_is_not() {
    assert!(asked_for(true).contains("\x1b[>"));
    assert!(given_back(true).contains("\x1b[<"));

    assert!(!asked_for(false).contains("\x1b[>"));
    assert!(!given_back(false).contains("\x1b[<"));
}

/// INPUT-33: the bytes written to ask for the modes and to give them back. The tracking modes
/// (1000, 1002, 1003), the urxvt form of coordinates (1015) and SGR (1006) are asked for in one
/// run, with 1003 the last tracking mode so that a terminal that keeps one tracking state ends in
/// all motion, and SGR last so that a terminal that knows both forms of coordinates uses the one
/// that names a column or row past 223. They are given back in the reverse order. Rejects: SGR
/// asked for apart from the tracking modes or not at all, which reads a click at a column past
/// 223 as the last cell the older form can name; tracking modes in an order that ends in another
/// mode; and a mode asked for that nothing gives back.
#[test]
fn the_terminal_is_asked_for_sgr_with_the_tracking_modes_and_given_all_of_it_back_in_order() {
    let mouse = "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h";
    let mouse_off = "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
    let screen = "\x1b[?1049h";
    let paste_and_focus = "\x1b[?2004h\x1b[?1004h";
    let leave = "\x1b[?2004l\x1b[?1004l";

    assert_eq!(
        asked_for(false),
        format!("{screen}{mouse}{paste_and_focus}")
    );
    assert_eq!(
        asked_for(true),
        format!("{screen}{mouse}{paste_and_focus}\x1b[>1u")
    );
    assert_eq!(given_back(false), format!("{leave}{mouse_off}\x1b[?1049l"));
    assert_eq!(
        given_back(true),
        format!("\x1b[<1u{leave}{mouse_off}\x1b[?1049l")
    );

    let mut without_the_mouse = Vec::new();
    ask_for_modes(&mut without_the_mouse, false, false).expect("ask");
    assert_eq!(
        String::from_utf8(without_the_mouse).expect("utf8"),
        format!("{screen}{paste_and_focus}"),
        "capture off still asked for a mouse mode"
    );
}

/// INPUT-33: every way the interface is left or handed over (the end of a session, a failure,
/// the editor, the viewer and the stop that gives the shell the terminal) goes through
/// `hand_back_terminal`, which writes what `give_back_screen` writes before raw mode is left.
/// Rejects: a way out that writes less than `give_back_modes`, such as one that leaves out the
/// mouse modes, which stay on in the terminal for the next program.
#[test]
fn what_every_way_out_of_the_interface_writes_includes_every_mode_asked_for() {
    for enhanced in [false, true] {
        let mut out = Vec::new();
        give_back_screen(&mut out, enhanced).expect("give back");
        let out = String::from_utf8(out).expect("utf8");
        assert!(
            out.contains(&given_back(enhanced)),
            "the way out left a mode on: {out:?}"
        );
        let left_on = modes_left_on(&format!("{}{out}", asked_for(enhanced)));
        assert!(left_on.is_empty(), "left on after the way out: {left_on:?}");
    }
}

/// INPUT-33. Rejects: a way out that leaves raw mode before it has written the mouse modes off,
/// which leaves a window in which a report of the pointer arrives with nothing reading it as one:
/// the shell echoes it as text. The test leaves raw mode through a function that says whether the
/// mouse had been given back by then, and again with a write that fails, where raw mode is left
/// all the same and the failure is what comes back.
#[test]
fn the_modes_are_given_back_before_raw_mode_is_left_and_it_is_left_whatever_they_do() {
    use std::cell::Cell;
    let mouse_off = "\x1b[?1000l";
    let written = Cell::new(Vec::<u8>::new());
    struct Recording<'a>(&'a Cell<Vec<u8>>);
    impl std::io::Write for Recording<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let mut held = self.0.take();
            held.extend_from_slice(bytes);
            self.0.set(held);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let had_the_mouse_off = Cell::new(None);
    hand_back(&mut Recording(&written), false, || {
        let so_far = written.take();
        had_the_mouse_off.set(Some(String::from_utf8_lossy(&so_far).contains(mouse_off)));
        Ok(())
    })
    .expect("handed back");
    assert_eq!(
        had_the_mouse_off.get(),
        Some(true),
        "raw mode was left with the mouse still reporting"
    );

    struct Failing;
    impl std::io::Write for Failing {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("the terminal went away"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let left = Cell::new(false);
    let failure = hand_back(&mut Failing, false, || {
        left.set(true);
        Ok(())
    })
    .expect_err("the failure was swallowed");
    assert!(left.get(), "raw mode was kept because a write failed");
    assert_eq!(failure.to_string(), "the terminal went away");
}

/// INPUT-33: disambiguated keys are asked for only where the terminal said it understands them.
/// A terminal that answered nothing in time is taken not to, because the request is left on its
/// screen as text where it is not understood. Rejects: an answer of `None` read as yes (the
/// request sent blind to a terminal that gave no reply), and an answer of no read as yes.
#[test]
fn disambiguated_keys_are_asked_for_only_where_the_terminal_says_it_understands_them() {
    assert!(!enhanced(None), "no reply was taken for a yes");
    assert!(!enhanced(Some(false)), "a no was taken for a yes");
    assert!(enhanced(Some(true)), "a yes was not taken");
}

/// INPUT-33: the terminal is put in raw mode before anything is written to it, and a terminal that
/// will not enter raw mode is not taken at all. The colour question comes after raw mode, so its
/// reply arrives as bytes rather than as a line, and before the alternate screen, so the query is
/// not painted into the session; the keyboard is asked about after both. Rejects: a takeover that
/// never enters raw mode, one that enters it after the modes are written (a reply to the question,
/// and every key until then, is a line the terminal echoes), one that senses the terminal first,
/// and one that goes on to draw when raw mode could not be entered.
#[test]
fn raw_mode_is_entered_before_anything_is_written_and_the_takeover_ends_where_it_cannot_be() {
    use std::cell::RefCell;
    struct Recording<'a>(&'a RefCell<Vec<u8>>);
    impl std::io::Write for Recording<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let written = RefCell::new(Vec::new());
    let steps: RefCell<Vec<(&str, usize)>> = RefCell::new(Vec::new());
    take_over(
        &mut Recording(&written),
        true,
        || {
            steps
                .borrow_mut()
                .push(("raw mode", written.borrow().len()));
            Ok(())
        },
        |out| {
            steps.borrow_mut().push(("colours", written.borrow().len()));
            out.write_all(b"COLOUR-QUERY")
                .expect("the query is written");
        },
        || {
            steps
                .borrow_mut()
                .push(("keyboard", written.borrow().len()));
            true
        },
    )
    .expect("taken over");
    assert_eq!(
        *steps.borrow(),
        [("raw mode", 0), ("colours", 0), ("keyboard", 12)],
        "raw mode, then the colour question, then the keyboard question, each before the modes"
    );
    assert_eq!(
        String::from_utf8(written.take()).expect("utf8"),
        format!("COLOUR-QUERY{}", asked_for(true)),
        "the query comes before the alternate screen and the modes follow it"
    );

    steps.borrow_mut().clear();
    let failure = take_over(
        &mut Recording(&written),
        true,
        || Err(std::io::Error::other("no raw mode here")),
        |_| steps.borrow_mut().push(("colours", 0)),
        || {
            steps.borrow_mut().push(("keyboard", 0));
            true
        },
    )
    .expect_err("a terminal that would not enter raw mode was taken");
    assert_eq!(failure.to_string(), "no raw mode here");
    assert!(
        steps.borrow().is_empty(),
        "the takeover went on after raw mode failed: {:?}",
        steps.borrow()
    );
    assert!(
        written.borrow().is_empty(),
        "something was written to a terminal that is not in raw mode"
    );
}

/// The picker offers what was configured, under the tier names a person recognises rather than
/// the ARNs they were given as. An ARN is unreadable and identical-looking between tiers.
#[test]
fn the_bedrock_picker_offers_the_configured_tiers_by_name() {
    use bravebot_config::env_var;

    let models = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        (env_var::BEDROCK_SONNET_MODEL, "sonnet-arn"),
        (env_var::BEDROCK_HAIKU_MODEL, "haiku-arn"),
    ]));

    let shown: Vec<&str> = models.iter().map(|m| m.display_name.as_str()).collect();
    assert_eq!(shown, ["Opus", "Sonnet", "Haiku"]);
    // The key is what lands in the request's model field, and it has to be the ARN.
    assert_eq!(models[0].key, "opus-arn");
}

/// The half of #403 that reaches the box rather than the question. The activation arrives after
/// somebody has answered, so its characters are typed into the live box and its trailing return
/// sent them: a line nobody wrote reached the planner and spent a turn on it.
#[test]
fn a_return_that_arrived_with_other_keys_does_not_send() {
    let mut session = Session::new("none");
    type_line(&mut session, "source /tmp/x/env/bin/activate");

    session.key_arrived_alone = false;
    let action = handle_key(&mut session, key(KeyCode::Enter));

    assert!(
        !matches!(action, Action::Submit(_)),
        "a return out of a write sent the line: {action:?}"
    );
    assert_eq!(
        session.input(),
        "source /tmp/x/env/bin/activate",
        "the line went somewhere instead of staying to be read"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|line| line.text.contains("arrived with other keys")),
        "the refusal explained nothing"
    );
}

/// Every refusal, not the first one a session sees. `note_once` remembers a message for the life of
/// the session, so a second written return said nothing and the line sat in the box with no account
/// of why, which is the dead interface the rule exists to prevent.
#[test]
fn every_refused_return_is_said_and_not_just_the_first() {
    let mut session = Session::new("none");
    type_line(&mut session, "one");
    session.key_arrived_alone = false;
    handle_key(&mut session, key(KeyCode::Enter));
    let after_first = session
        .transcript
        .iter()
        .filter(|line| line.text.contains("arrived with other keys"))
        .count();
    assert_eq!(after_first, 1, "the first refusal said nothing");

    handle_key(&mut session, key(KeyCode::Enter));
    let after_second = session
        .transcript
        .iter()
        .filter(|line| line.text.contains("arrived with other keys"))
        .count();
    assert_eq!(after_second, 2, "the second refusal said nothing");
}

/// INPUT-34: the refusal holds while a turn runs, where the idle path's refusal is not reached.
/// A return another program wrote would otherwise join the queue and reach the planner when the
/// turn ended. The line stays in the box, nothing waits, and every refusal is said. A return the
/// person pressed in a read of its own queues the same line, which keeps the refusal from
/// holding back every return.
#[test]
fn a_return_that_arrived_with_other_keys_is_not_queued_while_a_turn_runs() {
    let line = "source /tmp/x/env/bin/activate";
    let refusals = |session: &Session| {
        session
            .transcript
            .iter()
            .filter(|said| said.text.contains("arrived with other keys"))
            .count()
    };
    let mut session = a_turn_running_on("first");
    type_while_working_without_enter(&mut session, line);

    session.key_arrived_alone = false;
    let action = handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(action, Action::Redraw);
    assert!(
        session.queued.is_empty(),
        "a return out of a write queued the line for the planner"
    );
    assert_eq!(session.input(), line, "the line did not stay in the box");
    assert_eq!(refusals(&session), 1, "the refusal was not said");
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(refusals(&session), 2, "the second refusal was not said");
    assert!(session.queued.is_empty());

    session.key_arrived_alone = true;
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    let waiting: Vec<&str> = session
        .queued
        .iter()
        .map(|queued| queued.prompt.as_str())
        .collect();
    assert_eq!(waiting, vec![line], "a return of its own did not queue");
    assert_eq!(session.input(), "");
}

/// And a person's own return still sends, which is the half that has to keep working.
#[test]
fn a_return_of_its_own_still_sends() {
    let mut session = Session::new("none");
    type_line(&mut session, "work");
    assert!(matches!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit(_)
    ));
}

/// The bypass a review found in the mark this replaces: a program wrote an arrow to make the line
/// look touched and a return to send it, in one four-byte write. Both arrive with each other, so
/// neither the arrow nor the return is a press, and the return no longer sends.
#[test]
fn a_program_cannot_send_its_own_line_with_an_arrow_and_a_return() {
    let mut session = Session::new("none");
    type_line(&mut session, "source /tmp/x/env/bin/activate");

    session.key_arrived_alone = false;
    handle_key(&mut session, key(KeyCode::Left));
    let action = handle_key(&mut session, key(KeyCode::Enter));

    assert!(
        !matches!(action, Action::Submit(_)),
        "an arrow and a return in one write sent the line"
    );
}

/// Nothing else draws while the box is idle, so a resize that maps to no action leaves the
/// frame the old size laid out, hint row and status row included, until a key is pressed.
/// Events that change nothing on screen stay quiet, so the answer is not "redraw on anything".
#[test]
fn a_resize_while_the_box_is_idle_asks_for_a_frame() {
    let mut session = Session::new("none");
    assert_eq!(
        idle_action(&mut session, TermEvent::Resize(70, 20)),
        Action::Redraw
    );
    assert_eq!(
        idle_action(&mut session, TermEvent::FocusLost),
        Action::None
    );
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert_eq!(
        idle_action(&mut session, TermEvent::Key(release)),
        Action::None
    );
}

/// INPUT-34: the fact the reader keeps about each event reaches the session through
/// `took_input`, and the rung that leaves is held to it. Two interrupts that were one read of
/// the terminal must not be two presses, however the session is driven, so this goes through
/// the reader and not through a field set by hand. Rejects: `took_input` that does not take the
/// reader's fact (so a run is read as presses), and one that takes it as always true or always
/// false (so two real presses cannot leave).
#[test]
fn what_the_reader_says_arrived_together_reaches_the_rung_that_leaves() {
    let mut session = Session::new("none");
    {
        let _held = crate::input::script::hold_together([
            TermEvent::Key(ctrl('c')),
            TermEvent::Key(ctrl('c')),
        ]);
        for press in 1..=2 {
            let taken = crate::input::read().expect("the script holds the event");
            took_input(&mut session, &taken);
            assert!(
                !session.key_arrived_alone,
                "press {press}: the reader said it arrived with others and the session did not hear it"
            );
            assert_eq!(
                handle_key(&mut session, ctrl('c')),
                Action::Redraw,
                "press {press}: an interrupt that arrived with others did more than say so"
            );
            assert!(
                !session.offered_to_leave,
                "press {press}: an interrupt that arrived with others offered the way out"
            );
        }
    }
    assert!(!session.is_quitting(), "a run of two interrupts left");

    let _held = crate::input::script::hold([TermEvent::Key(ctrl('c')), TermEvent::Key(ctrl('c'))]);
    let taken = crate::input::read().expect("the script holds the event");
    took_input(&mut session, &taken);
    assert!(
        session.key_arrived_alone,
        "an event on its own was heard as a run"
    );
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(
        session.offered_to_leave,
        "the first of two presses did not offer the way out"
    );
    let taken = crate::input::read().expect("the script holds the event");
    took_input(&mut session, &taken);
    assert_eq!(
        handle_key(&mut session, ctrl('c')),
        Action::Quit,
        "the second of two presses that each arrived alone did not leave"
    );
}

/// The turn's own key reader hands the stop back to the pump that holds the terminal, with the
/// turn still running and the box as it was, for a key that is not one of the two that stop it
/// (INPUT-47). The other working loops are read through [`act_while_working`], which the
/// tests of the suspension cover.
#[cfg(unix)]
#[test]
fn the_turns_key_reader_hands_ctrl_z_back_and_leaves_the_turn_running() {
    let mut session = Session::new("kernel-enforced");
    for c in "one two".chars() {
        session.type_char(c);
    }
    session.status = Status::Working;
    let cancel = Cancel::new();

    assert!(!stops_the_turn(&session, ctrl('z')));
    let answer = nothing_beside(|beside| turn_key(&mut session, ctrl('z'), &cancel, beside));

    assert_eq!(answer, Action::Suspend);
    assert_eq!(session.status, Status::Working);
    assert!(!cancel.is_cancelled(), "the key stopped the turn");
    assert_eq!(session.input(), "one two");
}
