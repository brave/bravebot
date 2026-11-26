//! How a typed word is recognised as a command, its argument taken verbatim, and a command never sent as a prompt.

use super::*;

/// Rejects: offering a command for a word that is not the whole line, which is a prompt
/// (CMD-2), for a word nothing starts with, or files, which a line queued mid-turn does not
/// reach for.
#[test]
fn nothing_is_offered_while_a_turn_runs_for_a_prompt_an_unknown_word_or_a_file() {
    for typed in ["fix /ef", "/zzz", "@"] {
        let mut session = a_turn_running_on("anything");
        type_without_sending_while_working(&mut session, typed);
        assert_eq!(
            session.offered(),
            crate::state::Offered::Nothing,
            "{typed} was offered something while a turn ran"
        );
    }
}

/// Rejects: a line armed as a shell command line being offered command words while work runs,
/// since there `/ef` is a path to a program (CMD-3).
#[test]
fn nothing_is_offered_in_shell_mode_while_work_runs() {
    let mut session = Session::new("none");
    session.begin_aside();
    session.shell = true;

    type_without_sending_while_working(&mut session, "/ef");

    assert_eq!(session.offered(), crate::state::Offered::Nothing);
}

/// A slash command is a word this program knows, but in shell mode the line is a command line:
/// `/status` could be a program somebody has, and `!` is how they said which they meant.
#[test]
fn a_slash_command_in_shell_mode_is_a_command_line() {
    let mut session = Session::new("none");
    type_line(&mut session, "!/status");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("/status".to_string())
    );
}

/// Nothing is offered in shell mode: `/usr/bin/env` is a path and an argument with an `@` in it
/// is an argument, so completing either would rewrite the line under someone typing it.
#[test]
fn shell_mode_offers_no_completions() {
    let mut session = Session::new("none");
    type_line(&mut session, "!/st");

    assert!(!session.is_completing(), "a completion was offered");
    // Enter must therefore run it rather than accepting a highlighted row.
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("/st".to_string())
    );
}

/// The marker looks like a character, so deleting back past it has to leave the mode. Otherwise
/// the only way out is clearing the whole line, which nobody would guess.
#[test]
fn backspacing_past_the_marker_leaves_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "!ls");

    handle_key(&mut session, key(KeyCode::Backspace));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(session.shell, "the mode was left while text remained");

    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(!session.shell, "the mode outlived the marker");
}

/// The marker sits before the caret, not before the line, so Backspace with the caret moved to
/// the start is the press that deletes it. What follows was typed on purpose and stays: the
/// mode is what was deleted, and the words become an ordinary prompt.
#[test]
fn backspacing_at_the_start_leaves_the_mode_and_keeps_the_line() {
    let mut session = Session::new("none");
    type_line(&mut session, "!ls -la");

    handle_key(&mut session, key(KeyCode::Home));
    handle_key(&mut session, key(KeyCode::Backspace));

    assert!(!session.shell, "the mode outlived the marker");
    assert_eq!(session.input(), "ls -la", "the line went with the marker");
}

/// Escape abandons the line, and the mode is part of the line: leaving it armed would send the
/// next thing typed to a shell.
#[test]
fn escape_leaves_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "!rm -rf /");

    handle_key(&mut session, key(KeyCode::Esc));

    assert!(!session.shell, "the mode survived being cancelled");
    assert!(session.input().is_empty());
}

/// The mode is part of the line even when nothing was typed behind the marker, so Escape
/// leaves it rather than ending the session. Backspace at the same caret already does, and
/// having the two keys disagree is how a press meant to back out of a mode loses the session.
#[test]
fn escape_leaves_shell_mode_armed_on_an_empty_line() {
    let mut session = Session::new("none");
    type_line(&mut session, "!");
    assert!(session.shell, "the mode was never armed");

    let action = handle_key(&mut session, key(KeyCode::Esc));

    assert_eq!(action, Action::Redraw, "escape ended the session");
    assert!(!session.shell, "the mode survived being cancelled");
}

/// A `!` mid-sentence is punctuation. Treating it as the mode would make "no way!" a command.
#[test]
fn a_bang_inside_a_line_is_an_ordinary_character() {
    let mut session = Session::new("none");
    type_line(&mut session, "no way!");

    assert!(!session.shell);
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("no way!".to_string())
    );
}

/// Shell history expansion is the shell's business, and a `!` inside a command belongs to it.
#[test]
fn a_bang_inside_a_command_stays_in_the_command() {
    let mut session = Session::new("none");
    type_line(&mut session, "!echo hi!");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("echo hi!".to_string())
    );
}

/// A prompt that comes back after a cancelled turn is English, not a command line. The mode was
/// enterable mid-turn, so `!` then escape then enter ran the user's own sentence: "rm the old
/// builds" is a reasonable prompt, and a shell reads it as an instruction.
#[test]
fn a_prompt_restored_after_a_cancelled_turn_is_never_run_as_a_command() {
    let mut session = Session::new("none");
    type_line(&mut session, "rm the old builds");
    session.submit().expect("the prompt is sent");

    // Mid-turn, which used to be the moment the mode could be armed unseen.
    handle_key_while_working(&mut session, key(KeyCode::Char('!')));
    session.restore("rm the old builds".to_string());
    session.clear_input();

    assert!(
        !session.shell,
        "the mode survived a cancelled turn, so the prompt was armed as a command"
    );
    type_line(&mut session, "rm the old builds");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("rm the old builds".to_string()),
        "the user's sentence was about to be run by a shell"
    );
}

/// Whatever else a cancelled turn does with the line, it must not come back armed: the text is a
/// prompt, and the mode changes what Enter does to it.
#[test]
fn restoring_a_prompt_leaves_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "some prompt");
    session.submit().expect("the prompt is sent");
    session.shell = true;

    session.restore("some prompt".to_string());

    assert!(!session.shell);
}

/// The word is a command, not a prompt: it must end the session rather than reach the planner.
#[test]
fn typing_the_exit_command_quits() {
    let mut session = Session::new("none");
    for c in EXIT_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Quit);
    assert!(session.is_quitting());
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// Only the bare word, so a prompt that merely mentions it is still a prompt.
#[test]
fn a_prompt_containing_the_exit_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /exit do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /exit do".to_string())
    );
    assert!(!session.is_quitting());
}

/// Other terminal agents leave on a lone `exit` or `quit`, so a person who types one may expect
/// to leave. Here it is a prompt like any other: the first Enter sends it, in whatever case and
/// with whatever spaces, and the session neither ends nor waits for a second press (CMD-2).
#[test]
fn a_lone_leave_word_is_sent_at_the_first_enter() {
    for line in ["exit", "quit", "Exit", "QUIT", "  exit  ", "qUiT "] {
        let mut session = Session::new("none");
        type_line(&mut session, line);

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.trim().to_string()),
            "{line:?} was not sent at the first Enter"
        );
        assert!(session.a_turn_is_running(), "{line:?} began no turn");
        assert!(!session.is_quitting(), "{line:?} left");
        assert!(session.leave_word_sent, "{line:?} was sent with no mark");
    }
}

/// Only the whole line is the word, so asking about leaving, or any sentence that contains it,
/// is sent as the prompt it is and with no mark on it (CMD-2).
#[test]
fn a_line_that_holds_more_than_a_leave_word_is_sent_without_the_mark() {
    for line in [
        "exit now",
        "how do I exit",
        "exits",
        "quit?",
        "exit quit",
        "e xit",
    ] {
        let mut session = Session::new("none");
        type_line(&mut session, line);

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line:?} was not sent"
        );
        assert!(!session.leave_word_sent, "{line:?} was sent with a mark");
    }
}

/// The slash command still leaves on the first press and says nothing of a prompt, since a person
/// who typed the command meant it and nothing was sent to the model.
#[test]
fn the_exit_command_leaves_on_the_first_enter_and_marks_nothing() {
    let mut session = Session::new("none");
    type_line(&mut session, EXIT_COMMAND);

    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Quit);
    assert!(!session.leave_word_sent, "the command was marked");
}

/// In shell mode the line is a command line and nothing else (CMD-3), and `exit` is a word a
/// shell has. It runs at the first Enter and no turn begins, so there is nothing to say under
/// the box about a prompt.
#[test]
fn a_bare_leave_word_in_shell_mode_is_run_as_the_command_line_it_is() {
    let mut session = Session::new("none");
    type_line(&mut session, "!exit");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("exit".to_string())
    );
    assert!(!session.leave_word_sent, "a command line was marked");
}

/// A command the table gives no argument, typed with words after it, is a prompt (CMD-2). The
/// first Enter sends it, as it would any prompt, and the session marks the turn it begins so the
/// row under the box can say the line went to the model and was not the command.
#[test]
fn a_no_argument_command_given_words_is_sent_as_a_prompt_with_the_mark() {
    for line in [
        "/model gpt-5",
        "/model   gpt-5  ",
        "/undo the last change",
        "/exit now",
        "/lsp servers please",
        "/request again",
        "/init the project",
        "/recap in one line",
        "/context all of it",
        "/changelog since monday",
        "/todos done",
    ] {
        let mut session = Session::new("none");
        type_line(&mut session, line);

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.trim().to_string()),
            "{line:?} was not sent at the first Enter"
        );
        assert!(session.a_turn_is_running(), "{line:?} began no turn");
        assert!(!session.is_quitting(), "{line:?} left");
        assert!(
            session.command_sent_as_prompt,
            "{line:?} was sent with no mark"
        );
        assert!(
            !session.leave_word_sent,
            "{line:?} was marked as a leave word"
        );
    }
}

/// Only a line whose first word is a command with no argument and that has words after it is
/// marked (CMD-2). The bare command is the command, a command with an argument takes the words,
/// a longer word is another word, and a prompt that merely mentions a command is a prompt of
/// the person's own words, so none of them is told it was sent in place of a command.
#[test]
fn a_line_that_is_not_a_no_argument_command_given_words_is_sent_without_that_mark() {
    for line in [
        "/model",
        "/rename foo",
        "/theme nord",
        "/effort high",
        "/models",
        "/undone",
        "/models gpt-5",
        "why is /model slow",
        "please /undo that",
        "how do I exit",
        "exit",
        "list the files",
        "!/model x",
        "!/undo the last change",
    ] {
        let mut session = Session::new("none");
        type_line(&mut session, line);
        handle_key(&mut session, key(KeyCode::Enter));

        assert!(
            !session.command_sent_as_prompt,
            "{line:?} was marked as a command sent as a prompt"
        );
    }
}

/// A command typed with words in shell mode is a command line (CMD-3) and runs at the first
/// Enter, so nothing was sent to the model and no note is owed.
#[test]
fn a_no_argument_command_given_words_in_shell_mode_is_run_as_the_command_line_it_is() {
    let mut session = Session::new("none");
    type_line(&mut session, "!/model gpt-5");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("/model gpt-5".to_string())
    );
    assert!(!session.command_sent_as_prompt, "a command line was marked");
}

/// A command, not a prompt: asking for the picker must not also ask the planner about models.
#[test]
fn typing_the_model_command_opens_the_picker() {
    let mut session = Session::new("none");
    for c in MODEL_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
    assert_eq!(session.status, Status::Idle, "a turn began");
}

/// Only the bare word. "/model is slow today" is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_model_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "why is /model slow".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("why is /model slow".to_string())
    );
}

/// A command, not a prompt: asking for the picker must not also ask the planner about themes.
#[test]
fn typing_the_theme_command_opens_the_picker() {
    let mut session = Session::new("none");
    for c in THEME_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChooseTheme
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// A command, not a prompt: asking how hard to think must not also ask the planner about it.
#[test]
fn typing_the_effort_command_opens_the_picker() {
    let mut session = Session::new("none");
    for c in EFFORT_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChooseEffort(crate::model_prompt::Scope::EverySession)
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
    assert_eq!(session.status, Status::Idle, "a turn began");
}

/// A command, not a prompt: asking how the box edits must not also ask the planner about it.
#[test]
fn typing_the_config_command_opens_the_panel() {
    let mut session = Session::new("none");
    for c in CONFIG_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChooseEditing
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
    assert_eq!(session.status, Status::Idle, "a turn began");
}

/// Only the bare word. "what does /config change" is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_config_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    type_line(&mut session, "what does /config change");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /config change".to_string())
    );
}

/// The command is on the list `/` offers, since a panel nobody can find is a panel nobody has.
#[test]
fn the_config_command_is_offered_like_every_other() {
    assert!(
        commands()
            .iter()
            .any(|command| command.name == CONFIG_COMMAND),
        "the command is not on the list"
    );
    assert!(
        completions("/con")
            .iter()
            .any(|command| command.name == CONFIG_COMMAND),
        "the command does not complete"
    );
}

/// Only the bare word. "why is /effort high" is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_effort_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "why is /effort high".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("why is /effort high".to_string())
    );
}

/// `/efforts` is not `/effort`: the whole word must match.
#[test]
fn a_longer_word_starting_with_effort_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/efforts".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/efforts".to_string())
    );
}

/// CMD-2: `/redraw` is a command only as the whole line, so a question about it, a longer word
/// that starts with it, and the word followed by others are each a prompt.
#[test]
fn redraw_is_a_command_only_as_the_whole_line() {
    for line in ["what does /redraw do", "/redrawn", "/redraw now"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line}"
        );
        assert!(!session.retake_terminal, "{line} retook the terminal");
    }
}

/// CMD-2: `/caffeinate` is a command only as the whole line, so a question about it is a prompt.
#[test]
fn a_prompt_containing_the_caffeinate_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /caffeinate hold".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /caffeinate hold".to_string())
    );
    assert!(!session.caffeinate.is_on());
}

/// CMD-2: a longer word that starts with the command is not the command.
#[test]
fn a_longer_word_starting_with_caffeinate_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/caffeinated".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/caffeinated".to_string())
    );
}

/// CMD-2: a longer word starting with the command's name is a prompt.
#[test]
fn a_prompt_containing_the_commit_message_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["/commit-messages please", "/commit-message-style"] {
        let mut session = Session::new("none");
        assert_eq!(
            enter_line(&mut session, line),
            Action::Submit(line.to_string())
        );
    }
}

/// CMD-2: the word takes no argument, so words after it make a prompt and none of them reach
/// the one the driver wrote.
#[test]
fn init_with_words_after_it_is_a_prompt() {
    let root = crate::testutil::scratch_dir("init-command-prompt");
    let mut session = Session::new("none").in_workspace(&root);
    assert_eq!(
        enter_line(&mut session, "/init the project"),
        Action::Submit("/init the project".to_string())
    );
}

/// Naming a level on the line takes it without opening the picker.
#[test]
fn the_effort_command_carries_its_level() {
    let mut session = Session::new("none");
    for c in "/effort xhigh".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::SetEffort(
            "xhigh".to_string(),
            crate::model_prompt::Scope::EverySession
        )
    );
}

/// A level the user chose must be the level the turn asks for, or the choice is decoration.
#[test]
fn a_chosen_level_reaches_the_session() {
    let mut session = Session::new("none");
    assert_eq!(session.effort(), None);

    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );
    assert_eq!(
        session.effort(),
        Some(bravebot_aichat::protocol::Effort::Max)
    );
}

/// Asking for no level puts the session back to sending none, so a first pick is not
/// permanent.
#[test]
fn asking_for_no_level_puts_the_session_back_to_sending_none() {
    let mut session = Session::new("none");
    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );

    session.choose_effort(None);
    assert_eq!(session.effort(), None);
}

/// `/advisors` is not `/advisor`: the whole word must match.
#[test]
fn a_longer_word_starting_with_advisor_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/advisors are useful".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/advisors are useful".to_string())
    );
}

/// Only the command word. "ask the /advisor about it" is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_advisor_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "ask the /advisor about it".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("ask the /advisor about it".to_string()),
        "the line was not sent"
    );
}

/// A longer word that only starts with the command is a prompt, not the command.
#[test]
fn a_prompt_containing_the_theme_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /theme do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /theme do".to_string())
    );
}

/// `/themes` is not `/theme`: the whole word must match.
#[test]
fn a_longer_word_starting_with_theme_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/themes".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/themes".to_string())
    );
}

/// Naming a theme on the line applies it without opening the picker.
#[test]
fn the_theme_command_carries_its_name() {
    let mut session = Session::new("none");
    for c in "/theme nord".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::SetTheme("nord".to_string())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
}

/// The argument is the point of this one, so it must arrive with the action.
#[test]
fn the_add_dir_command_carries_its_directory() {
    let mut session = Session::new("none");
    for c in "/add-dir ~/notes".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::AddDirectory("~/notes".to_string())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// The argument is the point of this one too, so it must arrive with the action.
#[test]
fn the_cd_command_carries_its_directory() {
    let mut session = Session::new("none");
    for c in "/cd ~/projects/other".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChangeDirectory("~/projects/other".to_string())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// With no argument there is nowhere to move to, and the loop says so rather than moving to
/// the home directory the way a shell would.
#[test]
fn the_bare_cd_command_is_still_the_command() {
    let mut session = Session::new("none");
    for c in CD_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChangeDirectory(String::new())
    );
}

/// A longer word beginning with it is a prompt, or "/cdn caching is slow" would try to work in
/// a directory called "n caching is slow".
#[test]
fn a_longer_word_starting_with_cd_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/cdn caching is slow".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/cdn caching is slow".to_string())
    );
}

/// Asking about the command has to stay a question.
#[test]
fn a_prompt_containing_the_cd_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /cd do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /cd do".to_string())
    );
}

/// With no argument there is nothing to open, and the loop says so rather than doing nothing.
#[test]
fn the_bare_add_dir_command_is_still_the_command() {
    let mut session = Session::new("none");
    for c in ADD_DIR_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::AddDirectory(String::new())
    );
}

/// After `close` and a space, the rest of the line is the directory to close, spaces and all.
/// `close` alone is still the close command, which the loop answers by asking for a path, and a
/// name that only begins with the word is a directory to open.
///
/// The failures this rejects are a parse with no `close` arm, which would try to open a
/// directory called "close ~/my notes", and one that strips the word as a prefix, which would
/// read `/add-dir closet` as closing a directory called "t".
#[test]
fn the_add_dir_close_command_carries_the_directory_to_close() {
    for (typed, expected) in [
        (
            "/add-dir close ~/my notes",
            Action::CloseDirectory("~/my notes".to_string()),
        ),
        ("/add-dir close", Action::CloseDirectory(String::new())),
        (
            "/add-dir closet",
            Action::AddDirectory("closet".to_string()),
        ),
        (
            "/add-dir /work/close",
            Action::AddDirectory("/work/close".to_string()),
        ),
        (
            "/add-dir ../close",
            Action::AddDirectory("../close".to_string()),
        ),
    ] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            expected,
            "{typed}"
        );
        assert!(
            session.transcript.is_empty(),
            "{typed} was sent as a prompt"
        );
    }
}

/// A longer word beginning with the command is not the command, or "/add-dirs are useful"
/// would open a directory called "s are useful".
#[test]
fn a_longer_word_starting_with_the_command_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/add-dirs are confusing".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/add-dirs are confusing".to_string())
    );
}

/// And a sentence that merely mentions it is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_add_dir_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /add-dir do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /add-dir do".to_string())
    );
}

/// A command, not a prompt: it must not reach the planner as a request to clear something.
#[test]
fn typing_the_clear_command_starts_a_new_session() {
    let mut session = Session::new("none");
    for c in CLEAR_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Clear);
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// Typed before submitting, so the word never reaches the planner as a prompt: a session
/// asking to be shortened must not answer by talking about shortening itself.
#[test]
fn the_compact_command_asks_for_a_summary_rather_than_being_sent() {
    let mut session = Session::new("none");
    for c in COMPACT_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Compact(String::new())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// Only the bare word, so asking the planner about compacting something still asks it.
#[test]
fn a_prompt_containing_the_compact_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "how does /compact work".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("how does /compact work".to_string())
    );
}

/// CMD-1. Rejects: the working path reading the planner's first line as a command, which carried
/// out `/rename` at once and queued `/cd` to be dispatched when the turn ended.
#[test]
fn enter_on_a_brief_mid_turn_neither_runs_nor_queues_it_as_a_command() {
    for brief in [
        "/rename parser work",
        "/cd /srv/app Handoff: port the parser",
    ] {
        let mut session = a_turn_running_over_a_brief(brief);

        assert_eq!(
            handle_key_while_working(&mut session, key(KeyCode::Enter)),
            Action::None,
            "{brief}"
        );

        assert_eq!(session.input(), brief, "the brief left the box");
        assert!(session.queued.is_empty(), "{brief} was queued");
        assert_eq!(session.commands_waiting().count(), 0, "{brief} waits");
        assert!(session.handoff_pending(), "the brief was withdrawn");
    }
}

/// The focus is what the person wants the summary to keep, so the whole of it has to arrive,
/// spaces and punctuation included, and the line is never sent as a prompt (CMD-5).
#[test]
fn the_compact_command_carries_its_focus_verbatim() {
    let mut session = Session::new("none");
    for c in "/compact keep the lexer's benchmarks, and  the paths".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Compact("keep the lexer's benchmarks, and  the paths".to_string())
    );
    assert!(
        session.transcript.is_empty(),
        "the focus was sent as a prompt"
    );
}

/// A longer word is not the command, as for every other (CMD-2).
#[test]
fn a_word_longer_than_compact_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "/compacted the parser".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/compacted the parser".to_string())
    );
}

/// The question is what the command acts on, so the whole of what was typed after the word
/// has to arrive, spaces and punctuation included.
#[test]
fn the_btw_command_carries_its_question() {
    let mut session = Session::new("none");
    for c in "/btw why is the parser recursive?".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside(
            "why is the parser recursive?".to_string(),
            Vec::new(),
            Vec::new()
        )
    );
}

/// With no question there is nothing to ask, and the loop says so rather than spending a
/// request to find out.
#[test]
fn the_bare_btw_command_is_still_the_command() {
    let mut session = Session::new("none");
    for c in BTW_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside(String::new(), Vec::new(), Vec::new())
    );
}

/// A sentence mentioning it is a thing to say to the planner, and the word this one claims is
/// short enough that it will be said.
#[test]
fn a_prompt_containing_the_btw_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /btw do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /btw do".to_string())
    );
}

#[test]
fn a_longer_word_starting_with_btw_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/btwice is not a word".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/btwice is not a word".to_string())
    );
}

/// `commands()` is the one place the set is written down, so a command missing from it is a
/// command the hint line, the completion list and Tab all fail to know about.
#[test]
fn compacting_is_offered_while_a_command_is_being_typed() {
    assert_eq!(
        completions("/comp")
            .iter()
            .map(|command| command.name)
            .collect::<Vec<_>>(),
        vec![COMPACT_COMMAND]
    );
}

/// Only the bare word, so asking the planner about clearing something still asks it.
#[test]
fn a_prompt_containing_the_clear_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "does /clear delete anything".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("does /clear delete anything".to_string())
    );
}

/// The name is the point of this one, so it must arrive with the action, spaces and all.
#[test]
fn the_rename_command_carries_the_whole_name() {
    let mut session = Session::new("none");
    for c in "/rename the parser bug".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Rename("the parser bug".to_string())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

/// With no name there is nothing to rename to, and the loop says so.
#[test]
fn the_bare_rename_command_is_still_the_command() {
    let mut session = Session::new("none");
    for c in RENAME_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Rename(String::new())
    );
}

/// A sentence mentioning it is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_rename_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /rename do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /rename do".to_string())
    );
}

/// The command that does not send the line it was typed on. What goes to the planner is what
/// was left after the interval was read off, which is what every later tick sends too.
#[test]
fn the_loop_command_sends_what_is_left_after_the_interval() {
    let mut session = Session::new("none");
    for c in "/loop 5m check the deploy".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("check the deploy".to_string())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert_eq!(
        session.looping().map(|running| running.prompt()),
        Some("check the deploy")
    );
}

/// The bare word reads the loop rather than starting one, and sends nothing: between ticks the
/// transcript is a record of what has already happened, and a loop is the part that has not.
#[test]
fn the_bare_loop_command_says_what_is_repeating() {
    let mut session = Session::new("none");
    // A line no message in the catalog quotes, so the sentence that says what the command needs
    // cannot be mistaken for the report: that sentence gives `check the deploy` as its example.
    session.start_loop(
        crate::loops::request("5m tail the log"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    session.complete("done", Vec::new(), 0);
    session.loop_turn_ended(None);
    session.transcript.clear();
    for c in LOOP_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(
        session.looping().map(|running| running.prompt()),
        Some("tail the log"),
        "the report ended the loop"
    );
    assert_eq!(session.transcript.len(), 2, "the report is not two lines");
    assert!(
        session.transcript[0].text.contains("tail the log"),
        "{}",
        session.transcript[0].text
    );
    assert_eq!(session.transcript[1].text, t!(loop_ends_with));
}

/// CMD-5's rule, on the one command whose argument is a whole sentence: the condition a judge
/// is given has to be the characters the person endorsed.
#[test]
fn the_goal_command_carries_the_whole_condition() {
    let mut session = Session::new("none");
    for c in "/goal every test in bravebot-tui passes and clippy is clean".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(
        session.goal().map(crate::goals::Running::condition),
        Some("every test in bravebot-tui passes and clippy is clean")
    );
}

#[test]
fn the_goal_command_takes_the_goal_off_again() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());

    for c in "/goal clear".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.goal().is_none());
}

/// A sentence mentioning it is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_loop_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /loop do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /loop do".to_string())
    );
    assert!(session.looping().is_none());
}

#[test]
fn a_longer_word_starting_with_loop_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/looper is a good name".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/looper is a good name".to_string())
    );
}

/// One parser serves every command that takes an argument, so its rules are worth pinning
/// once: the bare word, the word with an argument, and a longer word that merely starts alike.
#[test]
fn an_argument_is_taken_only_after_the_whole_command_word() {
    assert_eq!(
        argument_to("/rename a name", RENAME_COMMAND),
        Some("a name")
    );
    assert_eq!(argument_to("/rename", RENAME_COMMAND), Some(""));
    assert_eq!(
        argument_to("  /rename  spaced  ", RENAME_COMMAND),
        Some("spaced")
    );
    assert_eq!(argument_to("/renamed thing", RENAME_COMMAND), None);
    assert_eq!(argument_to("please /rename it", RENAME_COMMAND), None);
}

/// A slash offers everything; a letter narrows it; a space settles it, since the argument
/// comes next and there is nothing left to complete.
#[test]
fn what_a_half_typed_line_could_become() {
    assert_eq!(completions("/").len(), commands().len());
    assert_eq!(
        completions("/cl")
            .iter()
            .map(|c| c.name)
            .collect::<Vec<_>>(),
        vec![CLEAR_COMMAND]
    );
    assert!(
        completions("/add-dir ~/notes").is_empty(),
        "a settled command"
    );
    assert!(completions("what does /model do").is_empty(), "a prompt");
    assert!(completions("").is_empty());
    assert!(completions("/zzz").is_empty(), "a word matching nothing");
}

/// Tab is what completes everywhere else, and a command taking an argument gets the space its
/// argument goes after.
#[test]
fn tab_takes_the_highlighted_command() {
    let mut session = Session::new("none");
    for c in "/mod".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), MODEL_COMMAND, "no argument, so no space");

    let mut session = Session::new("none");
    for c in "/add".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(
        session.input(),
        format!("{ADD_DIR_COMMAND} "),
        "argument follows"
    );
}

/// Enter on a half-typed command takes the highlighted row rather than sending "/mod" to the
/// planner, which is never what was meant.
#[test]
fn enter_on_a_half_typed_command_completes_it() {
    let mut session = Session::new("none");
    for c in "/mod".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), MODEL_COMMAND);
    assert!(session.transcript.is_empty(), "a fragment was sent");
}

/// A command typed out in full still runs on Enter: completing must not get in the way of the
/// thing it exists to help with.
#[test]
fn enter_on_a_whole_command_still_runs_it() {
    let mut session = Session::new("none");
    for c in CLEAR_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Clear);
}

/// The arrows belong to the list while it is open, and to history and scrolling once it is not.
#[test]
fn the_arrows_walk_the_offered_commands_while_one_is_being_typed() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('/')));

    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands()[0].name),
        "the list opens at the top"
    );
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands()[1].name)
    );
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands()[0].name)
    );

    // Up at the top stays, rather than wrapping to the end.
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands()[0].name)
    );
}

/// Down past the end stays on the last, so Tab always takes something.
#[test]
fn walking_past_the_end_stays_on_the_last_command() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('/')));
    for _ in 0..commands().len() + 3 {
        handle_key(&mut session, key(KeyCode::Down));
    }
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands()[commands().len() - 1].name)
    );
}

/// Typing returns the cursor to the top, so the highlighted row does not drift to a different
/// command as the list narrows under it.
#[test]
fn typing_another_letter_returns_to_the_top_of_the_list() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('/')));
    for _ in 0..4 {
        handle_key(&mut session, key(KeyCode::Down));
    }
    handle_key(&mut session, key(KeyCode::Char('c')));

    // The first command that still matches, whichever it is: the property is where the cursor
    // lands, and naming one command would make the test fail the next time one is added.
    let first = completions("/c").first().map(|command| command.name);
    assert!(first.is_some(), "nothing matched, so this proves nothing");
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        first,
        "the cursor did not return to the top of what now matches"
    );
}

/// A paste narrows the list the way typing does, and the cursor goes back to the top of it for
/// the same reason: a row it was left on belongs to the list that was on the screen before the
/// paste, and Tab takes a row as the command somebody picked.
#[test]
fn a_paste_returns_the_cursor_to_the_top_of_the_narrowed_command_list() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('/')));
    // To the last of them, counted rather than named, so adding a command does not make this
    // test assert about the wrong row.
    for _ in 0..commands().len() {
        handle_key(&mut session, key(KeyCode::Down));
    }
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands()[commands().len() - 1].name)
    );

    handle_paste(&mut session, "c");
    // More than one, or the top row and whatever the old cursor was clamped to are the same
    // row and the assertion below would hold with the cursor left where it was.
    let matching = completions("/c");
    assert!(
        matching.len() > 1,
        "one match cannot tell the top of the list from the end of it"
    );
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        matching.first().map(|command| command.name),
        "the cursor did not return to the top of what now matches"
    );
}

/// With nothing being offered, Tab must not insert anything and the arrows go back to what they
/// do the rest of the time.
#[test]
fn tab_does_nothing_when_no_command_is_being_typed() {
    let mut session = Session::new("none");
    for c in "an ordinary prompt".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::None);
    assert_eq!(session.input(), "an ordinary prompt");
}

#[test]
fn typing_the_status_command_reports_rather_than_prompting() {
    let mut session = Session::new("none");
    for c in STATUS_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Status
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session.transcript.is_empty(),
        "the command was sent as a prompt"
    );
}

#[test]
fn typing_the_cost_command_reports_rather_than_prompting() {
    let mut session = Session::new("none");
    for c in COST_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker == crate::state::Speaker::System),
        "the command was sent as a prompt"
    );
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(cost_nothing_spent))),
        "the command did not report what the session has spent"
    );
}

/// Asking the planner what a session has cost is a question, not a command.
#[test]
fn a_prompt_containing_the_cost_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /cost show".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /cost show".to_string())
    );
}

/// Asking the planner about status is a question, not a command.
#[test]
fn a_prompt_containing_the_status_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /status show".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /status show".to_string())
    );
}

/// What issue #1327 asks for: the reply as the planner wrote it, its paragraphs whole and none
/// of the marker or indent the screen draws beside it.
#[test]
fn typing_the_copy_command_copies_the_latest_reply_rather_than_prompting() {
    let reply = "The parser drops the last token because the loop stops one short of the end of \
                     the buffer, which only shows on input with no trailing newline.\n\n\
                     - check `lexer.rs`\n- add a test";
    let mut session = Session::new("none");
    answered(&mut session, "first", "an older answer");
    answered(&mut session, "why does it drop a token", reply);
    let transcript = session.transcript.len();

    assert_eq!(copied_by(&mut session, COPY_COMMAND), Ok(reply.to_string()));
    assert!(session.input().is_empty(), "the command stayed on the line");
    assert_eq!(
        session.transcript.len(),
        transcript,
        "the command was sent as a prompt or noted something"
    );
}

/// Asking the planner how to copy something is a question, not a command.
#[test]
fn a_prompt_containing_the_copy_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    answered(&mut session, "first", "an answer");
    type_line(&mut session, "what does /copy do");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /copy do".to_string())
    );
}

/// A command is not a prompt, so what waits for the turn is the word itself, to be carried out
/// when the turn ends. It used to wait as a prompt, which put `/clear` into the running turn's
/// reach: the planner was asked what to clear and the session was never cleared.
#[test]
fn a_command_typed_while_a_turn_runs_is_not_sent_as_a_prompt() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in CLEAR_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    assert!(session.input().is_empty(), "the line stayed in the box");
    assert_eq!(session.queued.len(), 1, "the command is not waiting");
    assert_eq!(session.queued[0].prompt, CLEAR_COMMAND);
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed the command"
    );
    let prompts: Vec<&str> = session
        .transcript
        .iter()
        .filter(|e| matches!(e.speaker, crate::state::Speaker::User))
        .map(|e| e.text.as_str())
        .collect();
    assert_eq!(
        prompts,
        ["first"],
        "the command joined the conversation while it waited"
    );
    let last = session
        .transcript
        .last()
        .expect("the notice is in the transcript");
    assert!(
        matches!(last.speaker, crate::state::Speaker::System)
            && last.text.starts_with(CLEAR_COMMAND)
            && last.text.contains("waits for the turn to end"),
        "what the command said about waiting is not the last entry: {:?}",
        last.text
    );
    let said = said_under_the_turn(&session);
    assert!(
        said.len() == 1 && said[0].contains("waits for the turn to end"),
        "the notice is not counted as said under the turn: {said:?}"
    );
}

/// In shell mode the line is a command line and `/status` is a path to a program, which is the
/// guard the idle ladder answers ahead of its command arm. Taking it for a command would run this
/// program's `/status` at the end of the turn instead of the one the person named.
#[test]
fn a_shell_line_is_not_taken_for_a_command_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    session.shell = true;

    for c in STATUS_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    session.complete("answered", Vec::new(), 0);

    assert_eq!(
        session.take_queued_command(),
        None,
        "a command line was queued as a command"
    );
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Run(STATUS_COMMAND.to_string())),
        "the program the person named was not the line that ran"
    );
}

/// The mode is how a person said the line was for a shell, and a turn beginning under them says
/// nothing about it: one begins with no press behind it, from a loop's tick or a watch firing, so
/// the mode is armed mid-turn over a line that was typed at rest to be run. Queued as a prompt,
/// `echo pwned` reached the planner as a sentence somebody had said, from the keystroke that at
/// rest runs it.
#[test]
fn a_command_line_is_not_sent_to_the_running_turn() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);
    session.shell = true;

    for c in "echo pwned".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    assert!(session.input().is_empty(), "the line stayed in the box");
    assert_eq!(session.queued.len(), 1, "the command line is not waiting");
    assert_eq!(session.queued[0].prompt, "echo pwned");
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed the command line"
    );
    assert_eq!(
        session
            .transcript
            .last()
            .expect("the first prompt is in the transcript")
            .text,
        "first",
        "the command line joined the conversation while it waited"
    );
}

/// What queueing a command line promises, and the whole of what the press deferred: the line is
/// run, it is run as a command line rather than as anything the planner is asked, and it is run
/// off the loop, since the press that asked for it has already been made. It joins the transcript
/// as it starts running, behind the marker the scrollback echoes a command line with.
#[test]
fn the_command_line_queued_while_a_turn_ran_is_run_when_the_turn_ends() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    session.shell = true;

    for c in "echo pwned".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(
        queued_next(&mut session),
        None,
        "it ran while the turn was still running"
    );

    session.complete("answered", Vec::new(), 0);
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Run("echo pwned".to_string()))
    );
    assert!(session.queued.is_empty(), "it is still waiting");
    assert_eq!(queued_next(&mut session), None, "it ran twice");

    let echoed = session
        .transcript
        .last()
        .expect("the command line is in the transcript");
    assert_eq!(echoed.text, "echo pwned");
    assert_eq!(
        echoed.speaker,
        crate::state::Speaker::Shell,
        "the line the person ran was recorded as something they said"
    );
}

/// A `!!` line queued behind a turn comes out of the queue as the private action, the one whose
/// output is drawn and never recorded.
///
/// Rejects: `queued_next` returning `Action::Run` for a queued private line, which would put the
/// output of a line the person asked to keep private into the conversation.
#[test]
fn the_private_command_line_queued_while_a_turn_ran_is_run_privately() {
    let mut session = Session::new("none");
    // Both markers are typed at rest, since `!` arms nothing mid-turn.
    type_line(&mut session, "!!");
    assert!(session.shell_private(), "the second marker did not take");
    session.status = Status::Working;

    for c in "cat .env".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(session.queued.len(), 1, "the command line is not waiting");
    assert_eq!(
        queued_next(&mut session),
        None,
        "it ran while the turn was still running"
    );

    session.status = Status::Idle;
    assert_eq!(
        queued_next(&mut session),
        Some(Action::RunPrivately("cat .env".to_string())),
        "a queued private line was not run privately"
    );
    assert!(session.queued.is_empty(), "it is still waiting");
    assert_eq!(queued_next(&mut session), None, "it ran twice");
}

/// The mode lasts one command line, whether that line ran at once or waited for a turn. Left
/// armed over an emptied box it would claim whatever was typed next, which is the reason `!`
/// itself is refused mid-turn: nobody armed a mode over a sentence they have not written yet.
#[test]
fn queueing_a_command_line_leaves_shell_mode() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    session.shell = true;

    for c in "echo pwned".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert!(!session.shell, "the mode is armed over the next line");
}

/// The turn takes the oldest prompt, and a command line queued ahead of one is not it. Taking the
/// head of the queue regardless would record the command line as the line the planner was given,
/// which is the whole of what the mode says it is not.
#[test]
fn a_queued_command_line_is_not_what_the_turn_took() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    session.shell = true;

    for c in "echo pwned".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    for c in "and tidy up".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(
        session.interjections().take().as_deref(),
        Some("and tidy up"),
        "the turn was offered something other than the prompt"
    );
    session.interjected();

    assert_eq!(
        session
            .transcript
            .last()
            .expect("the interjection is in the transcript")
            .text,
        "and tidy up",
        "the command line was recorded as the line the turn took"
    );
    assert_eq!(session.queued.len(), 1, "the command line stopped waiting");
    assert_eq!(session.queued[0].prompt, "echo pwned");
}

/// The order somebody typed things in is the order they happen in, and what sends a queued prompt
/// is a turn ending. The turn that ended stopped at the command line in front of it, so without
/// the loop taking that one the prompt behind it would wait for a press nobody is going to make.
#[test]
fn a_prompt_queued_behind_a_command_line_is_sent_once_it_has_run() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    session.shell = true;

    for c in "echo pwned".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    for c in "and tidy up".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    session.complete("answered", Vec::new(), 0);

    assert_eq!(
        queued_next(&mut session),
        Some(Action::Run("echo pwned".to_string())),
        "the prompt went before the command line in front of it"
    );
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Submit("and tidy up".to_string())),
        "the prompt behind the command line was left waiting"
    );
    assert_eq!(session.status, Status::Working);
}

/// A waiting command line comes back like anything else waiting, and it comes back whatever the
/// turn has reached: what stops a prompt coming back is the planner having been given it, and this
/// line was given to nobody. It comes back as words, the way a line put away does, so running it
/// takes arming the mode again.
#[test]
fn a_queued_command_line_comes_back_to_the_box() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    session.shell = true;

    for c in "echo pwned".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert!(
        session.unqueue(),
        "the command line could not be taken back"
    );
    assert_eq!(session.input(), "echo pwned");
    assert!(session.queued.is_empty(), "it is waiting still");
    assert!(!session.shell, "the line brought the mode back with it");
}

/// The narrowness CMD-2 asks for, on the path that had no command arm at all: a sentence about
/// a command is a sentence, and a command with its argument is still the command. An arm that
/// took either for the other would make the box mid-turn a different language from the box at
/// rest.
#[test]
fn a_prompt_mentioning_a_command_is_queued_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(session.status, Status::Working);
    for c in "what does /clear do".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(
        session.queued.len(),
        1,
        "the question was taken as a command"
    );
    assert_eq!(session.queued[0].prompt, "what does /clear do");
    assert_eq!(
        session.interjections().take().as_deref(),
        Some("what does /clear do"),
        "a question waits as a command"
    );
}

/// A file dropped onto a `/btw` typed beside a turn is refused, because reading it records a rule
/// in the map the turn is answering into. Rejects reading it from the aside's thread.
#[test]
fn btw_with_a_dropped_file_beside_a_turn_is_refused() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let directory = crate::testutil::scratch_dir("bravebot-app-btw-drop-mid-turn");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    std::fs::write(directory.join("shot.png"), [0x89u8, 0x50]).expect("write");
    let path = directory.join("shot.png").to_string_lossy().to_string();
    let mut session = a_turn_that_has_sent_a_request().in_workspace(&directory);
    for c in "/btw what is in ".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, &path);
    let transcript = session.transcript.len();

    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    turn_key(
        &mut session,
        key(KeyCode::Enter),
        &Cancel::new(),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert!(!session.aside_in_flight(), "the file went out");
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(btw_no_files_beside_a_turn)]
    );
    assert_eq!(session.transcript.len(), transcript + 1);
    let _ = std::fs::remove_dir_all(&directory);
}

/// The word after `/crash-reports` reaches the action whole, and the bare word asks for the
/// report. A word the command does not take is carried too, so the command can say what it
/// takes and change nothing (CMD-2).
#[test]
fn the_crash_reports_command_carries_its_word() {
    for (line, word) in [
        ("/crash-reports off", "off"),
        ("/crash-reports on", "on"),
        ("/crash-reports false", "false"),
        ("/crash-reports", ""),
    ] {
        let mut session = Session::new("none");
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::CrashReports(word.to_string()),
            "{line}"
        );
    }
}

/// CMD-2: `/crash-reports` is a command only as the first word, whole, so a longer word and a
/// question about it are prompts.
#[test]
fn a_prompt_containing_the_crash_reports_command_is_still_a_prompt() {
    for line in ["/crash-reportsx", "what does /crash-reports do"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line}"
        );
    }
}

/// `/mcp` carries what follows it whole to the loop, and a longer word or a sentence that
/// mentions it is not the command (CMD-2).
#[test]
fn the_mcp_command_carries_its_words_unparsed() {
    let mut session = Session::new("none");
    for (line, argument) in [
        ("/mcp", ""),
        ("/mcp list", "list"),
        ("/mcp reconnect weather", "reconnect weather"),
        ("/mcp disable weather", "disable weather"),
    ] {
        type_line(&mut session, line);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Mcp(argument.to_string()),
            "{line}"
        );
    }
    for line in ["/mcps", "does /mcp list work"] {
        type_line(&mut session, line);
        assert!(
            !matches!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Mcp(_)
            ),
            "{line}"
        );
    }
}

/// `/plugin` carries what follows it whole to the loop, and a longer word or a sentence that
/// mentions it is not the command (CMD-2, PLUGIN-1).
///
/// Rejects: `/plugin` read as a command inside a sentence or as the prefix of another word.
#[test]
fn the_plugin_command_carries_its_words_unparsed() {
    let mut session = Session::new("none");
    for (line, argument) in [
        ("/plugin", ""),
        ("/plugin list", "list"),
        ("/plugin add ./demo", "add ./demo"),
        ("/plugin remove demo", "remove demo"),
        ("/plugin update demo", "update demo"),
    ] {
        type_line(&mut session, line);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Plugin(argument.to_string()),
            "{line}"
        );
    }
    for line in ["/plugins", "does /plugin add work"] {
        type_line(&mut session, line);
        assert!(
            !matches!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Plugin(_)
            ),
            "{line}"
        );
    }
}

/// `/sandbox` carries its word whole to the loop, and a longer word or a
/// sentence that mentions it is not the command (CMD-2). The regression it rejects is the word
/// cut at the first space, which would read `/sandbox off please` as `/sandbox off`.
#[test]
fn the_sandbox_command_carries_its_word_unparsed() {
    let mut session = Session::new("none");
    for (line, word) in [
        ("/sandbox strict", "strict"),
        ("/sandbox off please", "off please"),
    ] {
        type_line(&mut session, line);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Sandbox(word.to_string()),
            "{line}"
        );
    }
    for line in ["/sandboxes off", "does /sandbox off ask first"] {
        type_line(&mut session, line);
        assert!(
            !matches!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Sandbox(_)
            ),
            "{line}"
        );
    }
}

/// The fallback in [`dispatch_command`] is unreachable, and this is what says so: a word the table
/// names that no arm answers would be recognised, taken off the box and then quietly dropped, which
/// is worse than a word nobody recognises. The bare word is enough for every one of them, since a
/// command that takes an argument takes an empty one.
#[test]
fn every_command_in_the_table_dispatches() {
    for command in commands() {
        let mut session = Session::new("none");
        assert_ne!(
            dispatch_command(&mut session, commanded(command.name)),
            Action::None,
            "{} is in the table and does nothing",
            command.name
        );
    }
}

/// CMD-2: `/branch` is a command only as the whole line or with a name after it, so a question
/// about it is a prompt, and so is a longer word that starts with it.
#[test]
fn a_prompt_containing_the_branch_command_is_still_a_prompt() {
    for line in ["what does /branch copy", "/branches are useful"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string())
        );
    }
}

/// CMD-1: `/branch` alone and `/branch <name>` both reach the loop, the name unparsed.
#[test]
fn the_branch_command_carries_its_name() {
    for (line, name) in [
        ("/branch", ""),
        ("/branch try the other parser", "try the other parser"),
    ] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Branch(name.to_string())
        );
    }
}

/// `/rewinding the tape` is a longer word, so it is a prompt, as CMD-2 has it.
#[test]
fn a_longer_word_starting_with_rewind_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/rewinding the tape".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/rewinding the tape".to_string())
    );
}

/// `/hunksaw the plank` is a longer word, so it is a prompt, as CMD-2 has it.
#[test]
fn a_longer_word_starting_with_hunks_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/hunksaw the plank".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/hunksaw the plank".to_string())
    );
}
