//! INPUT-47: Ctrl-Z stops the session and hands the shell the terminal, and `fg` brings it back as
//! it was. What a unit test can reach is the key's own handling and the stop itself, which is
//! observed here by a second process that sends it to its group and is watched by this one.
use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

fn typed(session: &mut Session, line: &str) {
    for c in line.chars() {
        handle_key(session, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

fn a_picture() -> crate::clipboard::Image {
    crate::clipboard::Image {
        media_type: "image/png",
        bytes: b"pixels".to_vec(),
    }
}

/// A session with a line typed, a picture attached to it and the caret moved off the end, which is
/// every part of the box a stop must leave alone.
fn a_box_with_something_in_it() -> Session {
    let mut session = Session::new("none");
    typed(&mut session, "look at ");
    session.attach(a_picture());
    typed(&mut session, " and tell me why");
    handle_key(&mut session, ctrl('a'));
    handle_key(&mut session, ctrl('f'));
    session
}

/// How many times the session has said the key was not a press.
fn refusals_said(session: &Session) -> usize {
    session
        .transcript
        .iter()
        .filter(|line| line.text.contains("press it again to suspend"))
        .count()
}

/// What a stop must leave in the box: the line, the caret and what is attached.
fn what_the_box_holds(session: &Session) -> (String, usize, Vec<String>) {
    (
        session.input().to_string(),
        session.caret(),
        session
            .attached_to_the_line()
            .map(|attached| format!("{attached:?}"))
            .collect(),
    )
}

/// The key that stops the session is the one a person reaches for, and pressing it hands the loop
/// the stop rather than typing a `z`, clearing the line or being read as an unclaimed control key.
/// What was in the box is where it was, so `fg` returns to the sentence that was being written.
#[test]
fn ctrl_z_suspends_the_session_and_leaves_the_box_as_it_was() {
    let mut session = a_box_with_something_in_it();
    let before = what_the_box_holds(&session);
    assert!(
        before.0.contains("[Image #1]"),
        "the fixture has no picture in the line: {before:?}"
    );

    assert_eq!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert_eq!(what_the_box_holds(&session), before);
}

/// The box is the same box whether a turn is running, so the key works in both and a person is not
/// left to find out which. The turn is not stopped by the key: it stops with the process.
#[test]
fn ctrl_z_suspends_while_a_turn_runs_and_leaves_the_box_as_it_was() {
    let mut session = a_box_with_something_in_it();
    session.status = Status::Working;
    let before = what_the_box_holds(&session);

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('z')),
        Action::Suspend
    );
    assert_eq!(what_the_box_holds(&session), before);
    assert_eq!(session.status, Status::Working, "the key ended the turn");
}

/// Both editing styles and shell mode read it, since each of them takes a key a chord has not
/// claimed and a letter held with Ctrl is one that vi's NORMAL mode and shell mode each handle.
#[test]
fn ctrl_z_suspends_in_either_editing_style_and_in_shell_mode() {
    for editing in crate::vim::Editing::ALL {
        let mut session = Session::new("none");
        session.choose_editing(editing);
        typed(&mut session, "word");
        assert_eq!(
            handle_key(&mut session, ctrl('z')),
            Action::Suspend,
            "{editing:?} did not suspend"
        );
        assert_eq!(session.input(), "word");
        // NORMAL mode as well, which is where vi's letters are instructions. In the ordinary box
        // Escape clears the line, so only vi's is pressed.
        if editing == crate::vim::Editing::Vi {
            handle_key(
                &mut session,
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            );
            assert_eq!(
                handle_key(&mut session, ctrl('z')),
                Action::Suspend,
                "vi's NORMAL mode did not suspend"
            );
            assert_eq!(session.input(), "word");
        }
    }

    let mut session = Session::new("none");
    typed(&mut session, "!");
    assert!(session.shell, "the fixture is not in shell mode");
    typed(&mut session, "ls");
    assert_eq!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert!(session.shell);
    assert_eq!(session.input(), "ls");
}

/// A Ctrl-Z that arrived in the same read as other keys is not a press (INPUT-34): a program that
/// writes the byte at the terminal would otherwise take the session from the person at it. It is
/// said, as the rung that leaves says it, and the line stays.
#[test]
fn a_ctrl_z_that_arrived_with_other_keys_does_not_suspend() {
    let mut session = a_box_with_something_in_it();
    let before = what_the_box_holds(&session);
    session.key_arrived_alone = false;

    assert_eq!(handle_key(&mut session, ctrl('z')), Action::Redraw);
    assert_eq!(what_the_box_holds(&session), before);
    assert_eq!(refusals_said(&session), 1, "the refusal was not said once");

    let mut session = a_box_with_something_in_it();
    session.status = Status::Working;
    session.key_arrived_alone = false;
    assert_eq!(
        handle_key_while_working(&mut session, ctrl('z')),
        Action::Redraw
    );
    assert_eq!(refusals_said(&session), 1, "the refusal was not said once");

    // Pressed on its own the next time, it is a press.
    session.key_arrived_alone = true;
    assert_eq!(
        handle_key_while_working(&mut session, ctrl('z')),
        Action::Suspend
    );
}

/// The offer to leave stands only for the press that follows it, because a person who went and did
/// something else has moved on. The shell is something else, so after `fg` one Ctrl-C offers again
/// and does not leave on the strength of the one pressed before the stop.
#[test]
fn the_offer_to_leave_does_not_survive_a_stop() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(session.offered_to_leave, "the first press did not offer");

    assert_eq!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert!(!session.offered_to_leave, "the offer survived the stop");

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(session.offered_to_leave, "the first press after fg left");
}

/// The shell has the terminal only for as long as the process is stopped: it is given back before
/// the stop and taken again after it. Swapped, the shell draws under a screen of its own with mouse
/// reporting on, and the session comes back to a terminal it no longer holds.
#[test]
fn the_terminal_is_given_back_before_the_stop_and_taken_again_after_it() {
    let log = std::cell::RefCell::new(Vec::new());
    terminal::around_a_stop(
        &mut (),
        |_| {
            log.borrow_mut().push("given back");
            Ok(())
        },
        || log.borrow_mut().push("stopped"),
        |_| {
            log.borrow_mut().push("taken again");
            Ok(())
        },
    )
    .expect("the sequence");

    assert_eq!(*log.borrow(), ["given back", "stopped", "taken again"]);
}

/// A session continued with `bg` is stopped by the terminal at the first mode it sets, and the call
/// that was stopped reports itself interrupted when `fg` continues it. Made again it works, so the
/// session carries on and does not end with an interface error.
#[test]
fn a_call_a_stop_interrupted_is_made_again() {
    let mut calls = 0;
    let settled = terminal::until_not_interrupted(|| {
        calls += 1;
        match calls {
            1 | 2 => Err(std::io::Error::from(std::io::ErrorKind::Interrupted)),
            _ => Ok("raw mode"),
        }
    });

    assert_eq!(settled.expect("the third call settles"), "raw mode");
    assert_eq!(calls, 3);
}

/// Only an interruption is made again. A terminal that cannot be set is reported as it failed, since
/// asking it again would not change the answer and a loop on it would never end.
#[test]
fn a_call_that_failed_for_another_reason_is_not_made_again() {
    let mut calls = 0;
    let settled: std::io::Result<()> = terminal::until_not_interrupted(|| {
        calls += 1;
        match calls {
            1 => Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            _ => Ok(()),
        }
    });

    assert_eq!(
        settled.expect_err("the failure is returned").kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(calls, 1, "the call was made again");
}

/// A terminal that could not be given back is not stopped over: the shell would be handed raw mode
/// and a screen it did not ask for, and nothing would take them back.
#[test]
fn a_terminal_that_cannot_be_given_back_is_not_stopped_over() {
    let log = std::cell::RefCell::new(Vec::new());
    let result = terminal::around_a_stop(
        &mut (),
        |_| {
            Err(std::io::Error::other(
                "the terminal would not be given back",
            ))
        },
        || log.borrow_mut().push("stopped"),
        |_| {
            log.borrow_mut().push("taken again");
            Ok(())
        },
    );

    assert!(
        log.borrow().is_empty(),
        "went on after a failure: {:?}",
        log.borrow()
    );
    assert!(result.is_err(), "the failure was swallowed");
}

/// The scroller, the search over prompts and a picker read their own keys, so a stop is not asked
/// of them: it would leave a mode open over a screen that was handed back. The mode is still open
/// afterwards and nothing was typed into the box.
#[test]
fn a_mode_over_the_box_reads_ctrl_z_itself_and_does_not_suspend() {
    let mut session = a_box_with_something_in_it();
    let before = what_the_box_holds(&session);
    session.open_scroller();
    assert!(session.scrolling());
    assert_ne!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert!(session.scrolling(), "the key closed the scroller");
    assert_eq!(what_the_box_holds(&session), before);

    let mut session = a_box_with_something_in_it();
    let before = what_the_box_holds(&session);
    session.history.push("an earlier prompt", None);
    assert!(session.open_history_search(), "there was nothing to search");
    assert_ne!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert!(session.searching_history(), "the key closed the search");
    assert_eq!(what_the_box_holds(&session), before);

    let mut session = a_box_with_something_in_it();
    let before = what_the_box_holds(&session);
    open_effort_picker(&mut session);
    assert!(session.picking().is_some(), "the fixture opened no picker");
    assert_ne!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert!(session.picking().is_some(), "the key closed the picker");
    assert_eq!(what_the_box_holds(&session), before);
}

/// The loops that read the keys while work runs are handed the answer and cannot give the terminal
/// up where they stand, so each hands the stop back to the pump that holds it. A key that asks for
/// nothing hands nothing back, and a release is not a press.
#[test]
fn a_working_loop_hands_the_stop_back_to_the_pump_that_holds_the_terminal() {
    let nothing = || crate::clipboard::Pasted::Nothing;
    let mut session = Session::new("none");
    assert_eq!(
        act_while_working(&mut session, Action::Redraw, nothing),
        Action::None
    );
    assert_eq!(
        act_while_working(&mut session, Action::Suspend, nothing),
        Action::Suspend
    );

    let cancel = Cancel::new();
    session.status = Status::Working;
    assert_eq!(
        one_request_key(&mut session, ctrl('z'), &cancel),
        Action::Suspend
    );
    assert_eq!(
        goal_check_key(&mut session, ctrl('z'), &cancel),
        Action::Suspend
    );
    assert_eq!(
        one_request_key(&mut session, ctrl('t'), &cancel),
        Action::None,
        "another chord asked for a stop"
    );
    let released = KeyEvent::new_with_kind(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
        KeyEventKind::Release,
    );
    assert_eq!(
        one_request_key(&mut session, released, &cancel),
        Action::None
    );
    assert_eq!(
        goal_check_key(&mut session, released, &cancel),
        Action::None
    );
}

/// A settings file moves the chord like any other: the key it names stops the session, and the one
/// it left no longer does.
#[test]
fn the_suspend_chord_can_be_moved() {
    let mut session = Session::new("none");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("suspend".to_string(), "alt-z".to_string());
    session.adopt_keybindings(&moved);
    typed(&mut session, "word");

    assert_eq!(handle_key(&mut session, alt('z')), Action::Suspend);
    assert_ne!(handle_key(&mut session, ctrl('z')), Action::Suspend);
    assert_eq!(session.input(), "word", "the vacated chord typed something");
}

/// A chord a settings file may name on a key that other arms answer whatever is held with it:
/// Backspace, Tab, the arrows and Home and End each do their own work with a modifier held. The
/// chord is answered first wherever it was put, at rest and while a turn runs, so naming one of
/// these does not leave the action dead.
#[test]
fn the_suspend_chord_answers_on_keys_the_ladder_reads_whatever_is_held_with_them() {
    for spelling in [
        "ctrl-up",
        "ctrl-down",
        "alt-tab",
        "ctrl-end",
        "ctrl-home",
        "alt-pgdn",
        "alt-backspace",
        "ctrl-left",
    ] {
        let chord = crate::keybindings::KeyChord::parse(spelling).expect("a chord");
        let pressed = KeyEvent::new(chord.code, chord.modifiers);
        let mut moved = std::collections::BTreeMap::new();
        moved.insert("suspend".to_string(), spelling.to_string());

        let mut session = Session::new("none");
        session.adopt_keybindings(&moved);
        typed(&mut session, "word");
        assert_eq!(
            handle_key(&mut session, pressed),
            Action::Suspend,
            "{spelling} did not suspend at rest"
        );
        session.status = Status::Working;
        assert_eq!(
            handle_key_while_working(&mut session, pressed),
            Action::Suspend,
            "{spelling} did not suspend while a turn runs"
        );
        assert_eq!(session.input(), "word", "{spelling} edited the line");
    }
}

/// A chord moved onto one of the readline editing keys is the action's and not the line's
/// (INPUT-32), so moving the stop onto Ctrl-U stops the session and does not delete the line.
#[test]
fn a_moved_suspend_chord_takes_precedence_over_line_editing() {
    let mut session = Session::new("none");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("suspend".to_string(), "ctrl-u".to_string());
    session.adopt_keybindings(&moved);
    typed(&mut session, "word");

    assert_eq!(handle_key(&mut session, ctrl('u')), Action::Suspend);
    assert_eq!(session.input(), "word", "the editing key took the line");
}

/// How long to wait for something another process has to do. A broken stop fails when it runs out
/// rather than hanging the suite.
const BOUND: std::time::Duration = std::time::Duration::from_secs(20);

/// The environment variable that makes a re-run of this test binary stop itself.
const STOPS_ITSELF: &str = "BRAVEBOT_SUSPEND_TEST_STOPS_ITSELF";

/// Run by [`stopping_reaches_the_group_and_returns_only_once_continued`] in a process of its own.
/// It stops the way the session does and then says it was continued, by what it writes.
#[test]
#[ignore = "run as a child process by the test that watches it stop"]
fn a_child_that_stops_itself_for_its_parent_to_watch() {
    if std::env::var_os(STOPS_ITSELF).is_none() {
        return;
    }
    // The parent puts a sibling in this group before the stop is sent, and says so on stdin.
    let mut go = [0u8; 1];
    std::io::stdin().read_exact(&mut go).expect("go-ahead");

    terminal::stop_until_continued().expect("the stop was not sent");

    // Reached only once something sent `SIGCONT`.
    println!("continued");
}

/// What the kernel says about a child of this process.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Running,
    /// Stopped, by this signal.
    Stopped(i32),
    /// Ended, or not a child to be seen.
    Gone,
}

fn seen(pid: rustix::process::Pid) -> Seen {
    use rustix::process::{WaitOptions, waitpid};
    match waitpid(Some(pid), WaitOptions::UNTRACED | WaitOptions::NOHANG) {
        Ok(None) => Seen::Running,
        Ok(Some((_, status))) => match status.stopping_signal() {
            Some(signal) if status.stopped() => Seen::Stopped(signal),
            _ => Seen::Gone,
        },
        Err(_) => Seen::Gone,
    }
}

/// Wait until `pid` is stopped or gone, or the bound runs out.
fn wait_to_be_stopped(pid: rustix::process::Pid) -> Seen {
    let started = std::time::Instant::now();
    while started.elapsed() < BOUND {
        match seen(pid) {
            Seen::Running => std::thread::sleep(std::time::Duration::from_millis(10)),
            settled => return settled,
        }
    }
    Seen::Running
}

/// Ends what a test started even when it panics, so a stopped process is never left behind.
struct Reaped(Vec<Child>);

impl Drop for Reaped {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The stop is the one a terminal sends, to the whole group and not only to the process, and it
/// returns when the session is continued rather than at once. A stop that went to the process
/// alone leaves a member of a pipeline running, which a shell then waits on without ever saying
/// the job stopped; one that returned at once would take the terminal back while the shell still
/// held it. Both are observed on a real process, led by itself so the signal cannot reach this one.
#[test]
fn stopping_reaches_the_group_and_returns_only_once_continued() {
    // The program started is this test binary, the one that is already running and is being tested,
    // so nothing here is decided by a path that anything else could have put in its place.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let exe = std::env::current_exe().expect("this test binary");
    let mut stopper = Command::new(exe)
        .args([
            "--ignored",
            "--nocapture",
            "--test-threads=1",
            "a_child_that_stops_itself_for_its_parent_to_watch",
        ])
        .env(STOPS_ITSELF, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("the child");
    let group = i32::try_from(stopper.id()).expect("a pid");
    let stopper_pid = rustix::process::Pid::from_child(&stopper);
    let mut stdin = stopper.stdin.take().expect("its stdin");
    let mut stdout = stopper.stdout.take().expect("its stdout");

    // A member of the group that is not the one that stops. Only a signal sent to the group reaches it.
    let sibling = Command::new("sleep")
        .arg("120")
        .process_group(group)
        .spawn()
        .expect("a sibling in the group");
    let sibling_pid = rustix::process::Pid::from_child(&sibling);
    let mut reaped = Reaped(vec![sibling, stopper]);

    stdin.write_all(b"g").expect("go-ahead");

    let tstp = rustix::process::Signal::TSTP.as_raw();
    assert_eq!(
        wait_to_be_stopped(stopper_pid),
        Seen::Stopped(tstp),
        "the process was not stopped by SIGTSTP"
    );
    assert_eq!(
        wait_to_be_stopped(sibling_pid),
        Seen::Stopped(tstp),
        "a member of the group was left running"
    );

    rustix::process::kill_process_group(
        rustix::process::Pid::from_raw(group).expect("a group"),
        rustix::process::Signal::CONT,
    )
    .expect("SIGCONT");

    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = reaped.0[1].try_wait().expect("try_wait") {
            break status;
        }
        assert!(
            started.elapsed() < BOUND,
            "the stop did not return after SIGCONT"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(status.success(), "the child failed: {status:?}");

    let mut said = String::new();
    stdout.read_to_string(&mut said).expect("what it said");
    assert!(
        said.contains("continued"),
        "the stop did not return after SIGCONT: {said:?}"
    );
}
