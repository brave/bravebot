//! Shell mode and the command line a session runs.

use super::*;

/// What arms the mode is a keystroke, and a paste is not one. The text arrived from a file, a
/// web page or somebody suggesting a command, and any of those can begin with `!`, so arming
/// from one would leave a line nobody chose to run one Enter from a shell. It goes to the
/// planner as the words it is, `!` and all.
#[test]
fn a_pasted_line_does_not_arm_shell_mode() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "!rm -rf build");

    assert!(!session.shell, "a paste armed shell mode");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("!rm -rf build".to_string()),
        "the pasted line was taken as a command"
    );
}

/// A second `!` right after the first is the private form: the line that runs is what follows
/// both, and Enter asks for the run whose output is not recorded.
///
/// Rejects: `!!` read as a plain command line, or the second `!` typed into the command.
#[test]
fn a_second_bang_makes_the_command_private() {
    let mut session = Session::new("none");
    type_line(&mut session, "!!cat .env");

    assert!(session.shell_private(), "the second marker did not take");
    assert_eq!(session.input(), "cat .env");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::RunPrivately("cat .env".to_string())
    );
    assert!(!session.shell_private(), "the mode outlived the command");
}

/// The default is unchanged: one `!` runs the line and records it. A `!` after the first
/// character is history expansion for the shell.
///
/// Rejects: privacy leaking from an earlier `!!` into the next plain command, or a `!` inside a
/// command being taken for the marker.
#[test]
fn a_plain_command_after_a_private_one_is_recorded_again() {
    let mut session = Session::new("none");
    type_line(&mut session, "!!ls");
    handle_key(&mut session, key(KeyCode::Enter));
    type_line(&mut session, "!ls !x");

    assert!(!session.shell_private());
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("ls !x".to_string())
    );
}

/// SHELL-4: only keystrokes arm anything. A paste that begins with `!` into an armed line is
/// text in the line, never the second marker.
///
/// Rejects: a paste reaching the marker check and making a line private (or, by the same path,
/// arming the mode).
#[test]
fn a_pasted_bang_does_not_make_the_command_private() {
    let mut session = Session::new("none");
    type_line(&mut session, "!");
    handle_paste(&mut session, "!ls");

    assert!(!session.shell_private(), "a paste made the line private");
    assert_eq!(session.input(), "!ls");
}

/// Backspace takes the markers off one at a time, so a person who typed `!!` by mistake is back
/// at an ordinary shell line before they are back at the prompt.
///
/// Rejects: backspace leaving shell mode outright and dropping a plain command's recording.
#[test]
fn backspace_leaves_the_private_form_before_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "!!");
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(session.shell && !session.shell_private());
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(!session.shell);
}

/// The private form leaves the conversation as it was and still has output to draw; the plain
/// form puts the line and its output in the conversation.
///
/// Rejects: the private flag being ignored, so the output of `!!cat .env` reaches the next
/// request, and the opposite fault of a plain command no longer being recorded.
#[test]
fn a_private_command_leaves_the_conversation_unchanged() {
    let directory = std::env::temp_dir();
    let ran = bravebot_agent::shell::run("echo only-for-me", &directory, &Cancel::new())
        .expect("it runs");

    let mut kept = Conversation::new();
    let shown =
        shell_result("echo only-for-me", &ran, &mut kept, &mut Trail::new(), true).expect("shown");
    assert!(shown.text.contains("only-for-me"), "nothing to draw");
    assert!(kept.messages().is_empty(), "a private command was recorded");

    let mut recorded = Conversation::new();
    shell_result(
        "echo only-for-me",
        &ran,
        &mut recorded,
        &mut Trail::new(),
        false,
    )
    .expect("recorded");
    assert_eq!(recorded.messages().len(), 1);
}

/// SHELL-7: a frame drawn while a command runs carries what it has printed so far. The command
/// here cannot end until a frame has carried its first line, so a loop that takes the output only
/// once the command has ended never lets it end, and the bound turns that into a failure.
///
/// Rejects: output taken onto the session only when the command is over, which is what drew a
/// slow build as a spinner until it finished.
#[test]
fn a_frame_drawn_while_a_command_runs_shows_what_it_has_printed() {
    /// Removes the scratch directory when the test ends, however it ends.
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch =
        Scratch(std::env::temp_dir().join(format!("bravebot-live-frame-{}", std::process::id())));
    let directory = scratch.0.clone();
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch directory");

    let (live, printing) = mpsc::channel();
    let (worker_directory, worker_cancel) = (directory.clone(), Cancel::new());
    let worker = thread::spawn(move || {
        bravebot_agent::shell::run_live(
            "sh -c 'echo early; n=0; while [ ! -e release ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n+1)); done; echo late'",
            &worker_directory,
            &worker_cancel,
            &live,
        )
    });

    let mut session = Session::new("none");
    session.begin_command();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut shown_while_running = None;
    while_the_command_runs(&mut session, &worker, &printing, |session| {
        if let Some((text, _)) = session.command_so_far() {
            shown_while_running.get_or_insert_with(|| text.to_string());
            std::fs::write(directory.join("release"), "").expect("release the command");
        }
        assert!(
            Instant::now() < deadline,
            "no frame carried what the command had printed"
        );
        Ok(())
    })
    .expect("the frames are drawn");

    let ran = worker.join().expect("the worker ends").expect("it runs");
    assert_eq!(shown_while_running.as_deref(), Some("early\n"));
    assert_eq!(ran.stdout, "early\nlate\n");
}

/// SHELL-7: a command that was stopped or ran past the limit released nothing, so what it had
/// printed stays on the screen, above the note saying why it ended, and none of it reaches the
/// conversation. A person stops a build from the error it printed, and the error is what they
/// would be left without if the rows went with the command.
///
/// Rejects: the rows going with the command, and what a stopped command printed being recorded
/// as though it were a result.
#[test]
fn a_stopped_command_keeps_what_it_printed_on_the_screen_and_out_of_the_conversation() {
    use crate::state::Speaker;
    use bravebot_agent::shell::ShellError;
    for ended in [
        ShellError::Cancelled,
        ShellError::TookTooLong {
            after: Duration::from_secs(600),
        },
    ] {
        let mut session = Session::new("none");
        session.begin_command();
        session.command_printing("error: the build broke\n\n");
        let mut conversation = Conversation::new();
        let said = format!("{ended}");

        let events = settle_command(&mut session, "make", Err(ended), &mut conversation, false);

        assert!(session.command_so_far().is_none(), "the rows stayed up");
        let speakers: Vec<(Speaker, &str)> = session
            .transcript
            .iter()
            .map(|entry| (entry.speaker, entry.text.as_str()))
            .collect();
        let not_sent = t!(shell_stopped_not_sent).to_string();
        assert_eq!(
            speakers,
            [
                (Speaker::Output, "error: the build broke"),
                (Speaker::System, said.as_str()),
                (Speaker::System, not_sent.as_str())
            ]
        );
        assert!(conversation.messages().is_empty(), "{said}");
        assert!(events.is_empty());
    }
}

/// A command stopped before it printed anything has nothing to keep and nothing to say was not
/// sent, so only the note saying it stopped is left.
///
/// Rejects: the second note being said whether or not anything was kept.
#[test]
fn a_command_stopped_before_it_printed_says_only_that_it_stopped() {
    let mut session = Session::new("none");
    session.begin_command();
    let mut conversation = Conversation::new();

    settle_command(
        &mut session,
        "sleep 30",
        Err(bravebot_agent::shell::ShellError::Cancelled),
        &mut conversation,
        false,
    );

    let notes: Vec<&str> = session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(notes, ["stopped"]);
}

/// SHELL-7: what a command printed after the last frame was drawn is taken when it ends, so a
/// stopped command keeps its last lines. The piece is already queued when the command is found
/// to have ended, which is the case a loop that stops looking when the command ends loses.
///
/// Rejects: the pieces still queued when the command ends being dropped with the channel.
#[test]
fn what_a_command_printed_after_the_last_frame_is_taken_when_it_ends() {
    let (live, printing) = mpsc::channel();
    live.send("the last line\n".to_string()).expect("queued");
    let worker = thread::spawn(|| ());
    while !worker.is_finished() {
        thread::yield_now();
    }
    let mut session = Session::new("none");
    session.begin_command();

    while_the_command_runs(&mut session, &worker, &printing, |_| Ok(())).expect("ends");

    assert_eq!(
        session.command_so_far().map(|(text, _)| text),
        Some("the last line\n")
    );
}

/// The gate refusing what a command printed, or the bytes not being readable, leaves none of
/// what was drawn while it ran: the bytes were refused, and an entry made of them would put on
/// the screen what the gate kept off it.
///
/// Rejects: every command that ends without a result keeping its rows, which keeps a refusal's.
#[test]
fn a_command_whose_output_was_refused_leaves_none_of_it_on_the_screen() {
    use crate::state::Speaker;
    let mut session = Session::new("none");
    session.begin_command();
    session.command_printing("what the gate refused\n");
    let mut conversation = Conversation::new();

    settle_command(
        &mut session,
        "cat notes",
        Err(bravebot_agent::shell::ShellError::Io("refused".to_string())),
        &mut conversation,
        false,
    );

    assert!(session.command_so_far().is_none(), "the rows stayed up");
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker != Speaker::Output),
        "what it printed was kept as an entry"
    );
    assert!(conversation.messages().is_empty());
}

/// SHELL-6 and SHELL-7: a private command is drawn while it runs like any other, and when it ends
/// the entry is what the gate released with the line saying it was not sent, and nothing is
/// recorded.
///
/// Rejects: the private form keeping its rows, and a private command's output reaching the
/// conversation through the new path.
#[test]
fn a_private_command_that_ended_is_drawn_as_released_and_says_it_was_not_sent() {
    use crate::state::Speaker;
    let mut session = Session::new("none");
    session.begin_command();
    session.command_printing("SECRET=1\n");
    let ran = bravebot_agent::shell::Ran {
        stdout: "SECRET=1\nSECOND=2\n".to_string(),
        stderr: String::new(),
        code: Some(0),
    };
    let mut conversation = Conversation::new();

    settle_command(&mut session, "cat .env", Ok(ran), &mut conversation, true);

    assert!(session.command_so_far().is_none(), "the rows stayed up");
    let drawn: Vec<(Speaker, &str)> = session
        .transcript
        .iter()
        .map(|entry| (entry.speaker, entry.text.as_str()))
        .collect();
    assert_eq!(drawn.len(), 2, "{drawn:?}");
    assert_eq!(drawn[0], (Speaker::Output, "SECRET=1\nSECOND=2"));
    assert!(drawn[1].1.contains("not sent"), "{drawn:?}");
    assert!(
        conversation.messages().is_empty(),
        "a private command was recorded"
    );
}

/// SHELL-7: when a command ends, the entry is the text the gate released and the rows drawn
/// while it ran are gone, so nothing is drawn twice. The fixture releases a line the rows never
/// held, which tells an entry built from the released text from one kept from the pieces.
///
/// Rejects: the rows outliving the command, and the entry being built from what was drawn while
/// it ran instead of from what the gate released.
#[test]
fn a_command_that_ended_is_drawn_as_the_text_the_gate_released() {
    use crate::state::Speaker;
    let mut session = Session::new("none");
    session.begin_command();
    session.command_printing("a\n");
    let ran = bravebot_agent::shell::Ran {
        stdout: "a\nb\n".to_string(),
        stderr: String::new(),
        code: Some(0),
    };
    let mut conversation = Conversation::new();

    settle_command(
        &mut session,
        "echo a; echo b",
        Ok(ran),
        &mut conversation,
        false,
    );

    assert!(session.command_so_far().is_none(), "the rows stayed up");
    let outputs: Vec<&str> = session
        .transcript
        .iter()
        .filter(|entry| entry.speaker == Speaker::Output)
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(outputs, ["a\nb"]);
    assert_eq!(conversation.messages().len(), 1);
}

/// A running turn or command is what the title marks as working, and anything else is the box
/// waiting, whatever else the session holds.
#[test]
fn a_running_turn_or_command_is_working_in_the_title() {
    use crate::title::State;
    assert_eq!(run_state(Status::Working), State::Working);
    assert_eq!(run_state(Status::Running), State::Working);
    assert_eq!(run_state(Status::Idle), State::Ready);
    assert_eq!(run_state(Status::Quitting), State::Ready);
}

/// The mode belongs to the idle prompt. Entering it mid-turn arms a line the user cannot act on
/// until the turn ends, which is the wrong moment to find out what Enter now does.
///
/// The character is still typed, because words typed during a turn are kept: what is refused is
/// the mode, not the keystroke.
#[test]
fn shell_mode_cannot_be_entered_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "anything");
    session.submit().expect("the prompt is sent");

    handle_key_while_working(&mut session, key(KeyCode::Char('!')));

    assert!(!session.shell, "the mode turned on mid-turn");
    assert_eq!(session.input(), "!", "the keystroke was dropped");
}

/// A line typed `!!` mid-turn is held in the box with a note, for both bang forms and for a
/// leading space, pressed twice as the person did. The wrong implementation is the one that
/// leaves the line to the generic queueing arm: that copies it into the planner's buffer, and
/// the turn then records it as a user message once per press (SHELL-8).
#[test]
fn a_bang_line_pressed_mid_turn_is_held_and_never_interjected() {
    for line in ["!!pwd", "!pwd", "  !!pwd"] {
        let mut session = a_turn_running_on("list the files");
        type_while_working_without_enter(&mut session, line);

        for press in 0..2 {
            assert_eq!(
                handle_key_while_working(&mut session, key(KeyCode::Enter)),
                Action::Redraw,
                "{line:?} press {press}"
            );
            assert_eq!(session.input(), line, "{line:?} left the box");
        }

        assert!(!session.shell, "{line:?} armed the mode");
        assert_eq!(
            session.queued_ahead(usize::MAX).count(),
            0,
            "{line:?} was queued"
        );
        assert!(
            session.interjections().drain().is_empty(),
            "{line:?} reached the planner's buffer"
        );
        session.interjected();
        assert!(
            session
                .transcript
                .iter()
                .all(|entry| entry.speaker != crate::Speaker::User || entry.text != line.trim()),
            "{line:?} was recorded as something said to the model"
        );
        let note = t!(shell_mode_idle_only).to_string();
        assert_eq!(
            session
                .transcript
                .iter()
                .filter(|entry| entry.text == note)
                .count(),
            2,
            "{line:?} did not say each press was refused"
        );
    }
}

/// Ctrl-Enter reaches the same arm and hurries nothing: the wrong implementation checks the
/// bang only for plain Enter, so the chord queues the line and then stops the turn for it.
#[test]
fn ctrl_enter_on_a_bang_line_mid_turn_neither_queues_it_nor_stops_the_turn() {
    let mut session = a_turn_running_on("list the files");
    type_while_working_without_enter(&mut session, "!!pwd");

    let action = handle_key_while_working(&mut session, ctrl_enter());

    assert_eq!(action, Action::Redraw);
    assert_eq!(session.input(), "!!pwd");
    assert_eq!(session.queued_ahead(usize::MAX).count(), 0);
    assert!(session.interjections().drain().is_empty());
}

/// The hold is for a first character of `!` only. A bang later in the line, a slash word and a
/// plain sentence are queued and interjected as before.
#[test]
fn a_line_with_a_bang_after_its_first_character_is_still_queued() {
    for line in ["wait! stop", "stop!!"] {
        let mut session = a_turn_running_on("list the files");
        type_while_working_without_enter(&mut session, line);

        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert_eq!(session.queued_ahead(usize::MAX).count(), 1, "{line:?}");
        assert_eq!(session.interjections().drain(), [line], "{line:?}");
    }
}

/// Enter on a bare marker would run an empty line, which a shell accepts and which would put a
/// pointless entry in the transcript.
#[test]
fn enter_on_an_empty_shell_line_does_nothing() {
    let mut session = Session::new("none");
    type_line(&mut session, "!");

    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
    assert!(session.transcript.is_empty());
}

/// CMDLINE-4: a `~` somebody types and a `~` the planner writes in a command line stand for
/// the same directory.
///
/// Two layers resolving it apart is the symptom a person meets: `~/notes.txt` typed at the
/// prompt reaches one file and the same path in a line the planner sent reaches another, with
/// nothing on screen to say why. Both answers come from one accessor so that a change to what
/// a `~` means cannot move one of them alone.
#[test]
fn a_typed_tilde_and_a_compiled_one_stand_for_the_same_directory() {
    let profile = bravebot_agent::home::profile().expect("a home directory");
    let root = std::path::Path::new("/tmp/named-directory-tilde-test-root");

    let resolved = named_directory(root, "~/notes.txt", Some(&profile)).expect("a tilde resolves");
    assert_eq!(resolved, format!("{}/notes.txt", profile.display()));
    assert!(
        !resolved.contains(".bravebot"),
        "a typed `~` named the state directory: {resolved}"
    );
}
