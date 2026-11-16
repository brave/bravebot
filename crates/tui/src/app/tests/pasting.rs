//! PASTE: ctrl+v and pasted text, pictures and copied files, and what a pasted picture does to the command it is named on.

use super::*;
use crate::clipboard::{Image, Pasted};
use bravebot_agent::turn::MAX_PASTED_IMAGE_BYTES;

fn picture(bytes: Vec<u8>) -> Pasted {
    Pasted::Image(Image {
        media_type: "image/png",
        bytes,
    })
}

/// The chord exists because Command-V cannot reach this process, so it has to be answered
/// where the keys are read rather than left to the terminal.
#[test]
fn ctrl_v_asks_for_the_clipboard_to_be_read() {
    let mut session = Session::new("kernel-enforced");
    assert_eq!(handle_key(&mut session, ctrl('v')), Action::Paste);
}

/// A line can be typed while a turn runs, so it can be pasted into while a turn runs. What
/// is refused mid-turn is sending, never writing.
///
/// The picture rather than the answer, because the answer was right all along and every
/// working loop threw it away: the key reached its arm, produced `Action::Paste`, and the
/// clipboard was never read.
#[test]
fn ctrl_v_reads_the_clipboard_during_a_turn_too() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;

    let action = handle_key_while_working(&mut session, ctrl('v'));
    assert_eq!(action, Action::Paste);
    let _ = act_while_working(&mut session, action, || picture(b"pixels".to_vec()));

    assert_eq!(
        session.input(),
        "[Image #1]",
        "the picture the key asked for was not staged"
    );
}

/// A picture pasted with the terminal's own chord arrives as a paste of nothing, and mid-turn
/// is when one is pasted: the person is watching the reply the screenshot is about. The empty
/// paste went into the box instead, which writes nothing and says nothing, so the picture was
/// dropped and the key that would have carried it was never named.
#[test]
fn an_empty_paste_mid_turn_goes_and_reads_the_clipboard_too() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;

    let action = handle_paste(&mut session, "");
    assert!(
        session.transcript[0].text.contains("ctrl-v"),
        "the note did not name the key that works"
    );
    let _ = act_while_working(&mut session, action, || picture(b"pixels".to_vec()));

    assert_eq!(
        session.input(),
        "[Image #1]",
        "the picture the empty paste stood for was not staged"
    );
}

/// Nothing is read for a paste that carried text, mid-turn as at rest: every Command-V of a
/// paragraph would otherwise spawn the platform's clipboard tools for an answer the paste
/// already had.
#[test]
fn a_paste_that_carried_text_mid_turn_is_left_alone() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;

    let action = handle_paste(&mut session, "some words");
    assert_eq!(action, Action::Redraw);
    let _ = act_while_working(&mut session, action, || {
        panic!("the clipboard was read for a paste that carried text")
    });

    assert_eq!(session.input(), "some words");
}

/// A paste that arrives carrying nothing is a Command-V the terminal could not answer: it
/// wrote the markers and found no text between them, which is what a clipboard holding
/// only a picture looks like from in here.
#[test]
fn an_empty_paste_goes_and_reads_the_clipboard_instead() {
    let mut session = Session::new("kernel-enforced");
    assert_eq!(handle_paste(&mut session, ""), Action::Paste);
    assert!(
        session.transcript[0].text.contains("ctrl-v"),
        "the note did not name the key that works"
    );
}

/// Said once and then not again: a user who has been told which key carries a picture does
/// not need telling every time they use the other one.
#[test]
fn which_key_carries_a_picture_is_said_once_per_session() {
    let mut session = Session::new("kernel-enforced");
    handle_paste(&mut session, "");
    handle_paste(&mut session, "");
    handle_paste(&mut session, "");

    assert_eq!(session.transcript.len(), 1, "the note was repeated");
}

/// A paste that did carry text is an ordinary paste and must stay one, or every Command-V
/// of a paragraph would go and read the clipboard a second time.
#[test]
fn a_paste_that_carried_text_is_left_alone() {
    let mut session = Session::new("kernel-enforced");
    assert_eq!(handle_paste(&mut session, "some words"), Action::Redraw);
    assert_eq!(session.input(), "some words");
    assert!(
        session.transcript.is_empty(),
        "an ordinary paste said something"
    );
}

/// Shell mode's line is a command, and a marker in one is text the shell would be handed
/// verbatim. Nothing about a picture belongs there.
#[test]
fn a_picture_is_refused_in_shell_mode_rather_than_written_into_the_command() {
    let mut session = Session::new("kernel-enforced");
    session.shell = true;
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));

    assert!(
        session.input().is_empty(),
        "a marker reached the command line"
    );
    assert!(session.transcript[0].text.contains("not a command"));
}

#[test]
fn a_picture_off_the_clipboard_becomes_a_marker_in_the_line() {
    let mut session = Session::new("kernel-enforced");
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "[Image #1]");
}

/// A directory removed when it goes out of scope.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A directory holding a file of each name, and the paths a clipboard would name them by.
fn copied_files(scratch: &str, names: &[&str]) -> (Scratch, Vec<String>) {
    let directory = crate::testutil::scratch_dir(scratch);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let paths = names
        .iter()
        .map(|name| {
            let at = directory.join(name);
            std::fs::write(&at, b"x").unwrap();
            at.to_string_lossy().into_owned()
        })
        .collect();
    (Scratch(directory), paths)
}

/// A copied file is a drop, so the line, the marker and what is staged behind it are the
/// ones a terminal's paste of the same path leaves. The staged file is a `dropped` one and
/// not a pasted picture: the file's bytes were never read off the clipboard, so nothing went
/// through the gate a picture does (PASTE-2).
#[test]
fn a_file_the_clipboard_names_is_staged_as_dropping_it_stages_it() {
    let (_scratch, paths) = copied_files("clipboard-files-staged", &["shot.png"]);
    let mut dropped = Session::new("kernel-enforced");
    handle_paste(&mut dropped, &crate::dropped::line_of(&paths));

    let mut copied = Session::new("kernel-enforced");
    take_from_clipboard(&mut copied, Pasted::Files(paths.clone()));

    assert_eq!(dropped.input(), "[Image #1] ");
    assert_eq!(copied.input(), dropped.input());
    assert_eq!(copied.attached(), dropped.attached());
    assert_eq!(copied.attached()[0].shown, paths[0]);
    assert_eq!(
        copied.attached()[0].kind,
        bravebot_filetype::by_name::Kind::Attachment("image/png")
    );
    assert!(
        copied.pasted_named(copied.input()).is_empty(),
        "a copied file went in as a pasted picture"
    );
}

/// A drop reads a text file as context and writes the path of a type nothing takes, and a
/// copied file is neither more nor less than a drop, whatever its type.
#[test]
fn a_copied_file_is_carried_read_or_written_out_by_its_type_as_a_dropped_one_is() {
    let (_scratch, paths) = copied_files(
        "clipboard-files-types",
        &["a.png", "notes.txt", "installer.dmg"],
    );
    let mut copied = Session::new("kernel-enforced");
    take_from_clipboard(&mut copied, Pasted::Files(paths.clone()));

    assert_eq!(
        copied.input(),
        format!("[Image #1] [File #2] {} ", paths[2])
    );
    assert_eq!(copied.attached().len(), 2);
    assert_eq!(
        copied.attached()[1].kind,
        bravebot_filetype::by_name::Kind::Text
    );
}

/// The paths reach the drop handling as one line each, so a name a shell would split or a
/// quote would end is still one file.
#[test]
fn a_copied_file_named_with_a_space_and_a_quote_is_one_file() {
    let (_scratch, paths) = copied_files("clipboard-files-awkward", &["a b'c.png"]);
    let mut copied = Session::new("kernel-enforced");
    take_from_clipboard(&mut copied, Pasted::Files(paths.clone()));

    assert_eq!(copied.input(), "[Image #1] ");
    assert_eq!(copied.attached()[0].shown, paths[0]);
}

/// A command line has no marker, so a file copied while the box is armed for the shell is
/// written as the word a dropped one is.
#[test]
fn a_copied_file_is_written_as_its_path_in_shell_mode_and_nothing_is_staged() {
    let (_scratch, paths) = copied_files("clipboard-files-shell", &["my shot.png"]);
    let mut dropped = Session::new("kernel-enforced");
    dropped.shell = true;
    handle_paste(&mut dropped, &crate::dropped::line_of(&paths));

    let mut copied = Session::new("kernel-enforced");
    copied.shell = true;
    take_from_clipboard(&mut copied, Pasted::Files(paths.clone()));

    assert_eq!(copied.input(), format!("'{}' ", paths[0]));
    assert_eq!(copied.input(), dropped.input());
    assert!(copied.attached().is_empty(), "a marker reached the shell");
}

/// A path that stopped naming a file between the read and the paste attaches nothing, and
/// the person is told so rather than being left with an empty box and no word.
#[test]
fn a_copied_file_that_is_gone_says_there_is_nothing_to_paste() {
    let (_scratch, paths) = copied_files("clipboard-files-gone", &["shot.png"]);
    std::fs::remove_file(&paths[0]).unwrap();
    let mut session = Session::new("kernel-enforced");
    take_from_clipboard(&mut session, Pasted::Files(paths));

    assert!(session.input().is_empty());
    assert!(session.attached().is_empty());
    assert!(
        session.transcript[0]
            .text
            .contains("nothing on the clipboard")
    );
}

/// A list the clipboard reader refused says so, and stages nothing, so the person is not left
/// with an empty box and no word.
#[test]
fn a_refused_list_of_copied_files_says_so_and_stages_nothing() {
    let mut session = Session::new("kernel-enforced");
    take_from_clipboard(&mut session, Pasted::FilesRefused);

    assert!(session.input().is_empty());
    assert!(session.attached().is_empty());
    assert!(
        session.transcript[0]
            .text
            .contains("none of the files were pasted")
    );
}

/// A name holding a control character is a drop's to judge, so a copy of it onto a shell
/// line is refused whole with the note a dragged file gets (DROP-6), and the file beside it
/// is not written without it.
#[cfg(unix)]
#[test]
fn a_copied_file_with_an_escape_in_its_name_refuses_a_shell_line_as_a_dragged_one_does() {
    let (_scratch, paths) = copied_files("clipboard-files-escape", &["fine.png", "a\u{1b}b.png"]);
    let mut dragged = Session::new("kernel-enforced");
    dragged.shell = true;
    handle_paste(&mut dragged, &crate::dropped::line_of(&paths));

    let mut copied = Session::new("kernel-enforced");
    copied.shell = true;
    take_from_clipboard(&mut copied, Pasted::Files(paths));

    assert!(copied.input().is_empty(), "{:?}", copied.input());
    assert_eq!(copied.input(), dragged.input());
    assert!(copied.transcript[0].text.contains("control character"));
    assert_eq!(copied.transcript[0].text, dragged.transcript[0].text);
}

/// Ctrl-V reads the clipboard while a turn runs, and a copied file is put to the drop
/// handling there as it is at rest.
#[test]
fn a_copied_file_is_staged_while_a_turn_runs_too() {
    let (_scratch, paths) = copied_files("clipboard-files-working", &["shot.png"]);
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;

    let _ = act_while_working(&mut session, Action::Paste, || Pasted::Files(paths));

    assert_eq!(session.input(), "[Image #1] ");
    assert_eq!(session.attached().len(), 1);
}

/// A command the box carries out itself has nowhere to put a picture: `/goal` names a
/// condition that a turn which has not happened yet is judged against, so a marker in one
/// would send whoever judges it looking for a picture no turn ever carried. What goes in its
/// place says a picture was meant and cannot be shown, which is something a judge can
/// answer, and the picture stops being staged behind a box that no longer names it.
#[test]
fn a_picture_a_command_line_named_is_carried_out_as_words_rather_than_as_its_marker() {
    let mut session = Session::new("kernel-enforced");
    type_line(&mut session, "/goal the screen matches ");
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "/goal the screen matches [Image #1]");

    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(
        session.goal().map(crate::goals::Running::condition),
        Some(
            "the screen matches ([Image #1] was pasted here, but it cannot be shown to you: \
             a picture cannot join that command. Ask for it again if you need to see it.)"
        )
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|said| said.text.contains("does not go with that command")),
        "nothing said the picture had stayed behind"
    );
    assert!(
        session.pasted_named("[Image #1]").is_empty(),
        "the picture is still staged behind a marker no line names"
    );
}

/// `/after` sends its line from the loop's clock and not from the line that was typed, so
/// what the line named cannot travel with it: a picture becomes words saying it cannot be
/// shown, a dropped file becomes its name, and the person is told about the picture. A wrong
/// implementation holds the literal marker, which names a screenshot nothing is sent with.
#[test]
fn what_an_after_line_named_is_held_as_words_and_names_and_not_as_markers() {
    let mut session = Session::new("kernel-enforced");
    let action = dispatch_command(
        &mut session,
        crate::state::Commanded {
            line: "/after 5m look at [Image #1] and [Image #2]".to_string(),
            pasted: vec![crate::state::AttachedImage {
                marker: "[Image #1]".to_string(),
                media_type: "image/png",
                bytes: b"pixels".to_vec(),
            }],
            attached: vec![crate::state::Attached {
                marker: "[Image #2]".to_string(),
                name: "shot.png".to_string(),
                shown: "/home/me/shot.png".to_string(),
                kind: bravebot_filetype::by_name::Kind::Attachment("image/png"),
            }],
            typed_at: std::time::Instant::now(),
        },
    );

    assert_eq!(action, Action::Redraw);
    let prompt = session
        .looping()
        .map(crate::loops::Running::prompt)
        .expect("the line was not held");
    assert_eq!(
        prompt,
        "look at ([Image #1] was pasted here, but it cannot be shown to you: a picture \
         cannot join that command. Ask for it again if you need to see it.) and shot.png"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|said| said.text.contains("does not go with that command")),
        "nothing said the picture had stayed behind"
    );
}

/// Waiting changes nothing about what a command can carry, and by the time the queue reaches
/// one nobody is there to be asked about it. So a command the box carries out itself is
/// settled where it is carried out, and reads the same as one dispatched at rest.
#[test]
fn a_picture_named_on_a_command_that_waited_is_words_by_the_time_it_is_carried_out() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/goal the screen matches ".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "/goal the screen matches [Image #1]");
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(session.input().is_empty(), "the command stayed in the box");

    session.complete("answered", Vec::new(), 0);
    queued_next(&mut session);

    assert_eq!(
        session.goal().map(crate::goals::Running::condition),
        Some(
            "the screen matches ([Image #1] was pasted here, but it cannot be shown to you: \
             a picture cannot join that command. Ask for it again if you need to see it.)"
        )
    );
}

/// A question is a prompt with the conversation behind it, so the picture pasted beside the
/// words goes where the words go. Settling it here would answer a question about a picture
/// by saying the picture cannot be shown, which is the answer to nothing.
#[test]
fn a_picture_pasted_into_a_question_goes_with_it() {
    let mut session = Session::new("kernel-enforced");
    type_line(&mut session, "/btw what is in ");
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "/btw what is in [Image #1]");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside(
            "what is in [Image #1]".to_string(),
            vec![crate::state::AttachedImage {
                marker: "[Image #1]".to_string(),
                media_type: "image/png",
                bytes: b"pixels".to_vec(),
            }],
            Vec::new()
        )
    );
    assert!(
        !session
            .transcript
            .iter()
            .any(|said| said.text.contains("does not go with that command")),
        "a picture that went with the question was spoken of as left behind"
    );
}

/// A picture of the thing to be built is the task, so it reaches the planner rather than a
/// sentence saying a picture was meant. What stops the plan from being made of what somebody
/// else wrote is [`crate::state::Session::trust`], which a paste does not lower.
#[test]
fn a_picture_pasted_into_a_task_goes_with_the_plan() {
    let mut session = Session::new("kernel-enforced");
    type_line(&mut session, "/manifest build ");
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "/manifest build [Image #1]");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Manifest(
            "build [Image #1]".to_string(),
            vec![crate::state::AttachedImage {
                marker: "[Image #1]".to_string(),
                media_type: "image/png",
                bytes: b"pixels".to_vec(),
            }],
            Vec::new()
        )
    );
}

/// A tick is a prompt, so the first one is a turn the person pasted into and carries the
/// picture. The two halves of that are wired apart, the command handing the pictures over
/// and the loop handing them to one tick, so this is what says the word is on the carrying
/// side of the dispatch: settled there, the loop would spend its whole life asking about a
/// screenshot no tick was given.
#[test]
fn a_picture_pasted_into_a_loop_goes_with_its_first_tick() {
    let mut session = Session::new("kernel-enforced");
    type_line(&mut session, "/loop 15m look at ");
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "/loop 15m look at [Image #1]");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("look at [Image #1]".to_string())
    );
    assert_eq!(
        session.sent_pasted().len(),
        1,
        "the first tick went without the picture"
    );
    assert_eq!(session.sent_pasted()[0].bytes, b"pixels".to_vec());
}

/// Waiting is not the same as being refused, so a question that sat in the queue carries the
/// picture it was typed beside when the queue reaches it. The picture is held with the line
/// rather than in the box, which is what makes that possible: the box was cleared by the
/// press that queued it.
#[test]
fn a_picture_named_on_a_question_that_waited_still_goes_with_it() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/manifest build ".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    assert_eq!(session.input(), "/manifest build [Image #1]");
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(session.input().is_empty(), "the command stayed in the box");

    session.complete("answered", Vec::new(), 0);

    assert_eq!(
        queued_next(&mut session),
        Some(Action::Manifest(
            "build [Image #1]".to_string(),
            vec![crate::state::AttachedImage {
                marker: "[Image #1]".to_string(),
                media_type: "image/png",
                bytes: b"pixels".to_vec(),
            }],
            Vec::new()
        ))
    );
}

/// Deleting the marker is how a picture is taken off a line, so what a command carries is
/// read out of the line rather than off what the session has staged. Read off the staging
/// and a paste the person rubbed out would ride along with every command they typed
/// afterwards.
#[test]
fn a_command_line_whose_marker_was_deleted_says_nothing_about_a_picture() {
    let mut session = Session::new("kernel-enforced");
    type_line(&mut session, "/btw ");
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert_eq!(session.input(), "/btw ", "the marker outlived the press");
    type_line(&mut session, "what is this");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside("what is this".to_string(), Vec::new(), Vec::new())
    );
    assert!(
        !session
            .transcript
            .iter()
            .any(|said| said.text.contains("does not go with that command")),
        "a picture the person took off the line was still spoken for"
    );
}

/// Refused rather than truncated, and with the size, because half a picture would be sent,
/// rejected by the endpoint, and reported as a fault of the request.
#[test]
fn a_picture_too_large_to_send_says_so_with_its_size() {
    let mut session = Session::new("kernel-enforced");
    take_from_clipboard(&mut session, Pasted::TooLarge(MAX_PASTED_IMAGE_BYTES * 2));

    assert!(
        session.input().is_empty(),
        "an oversized picture reached the line"
    );
    assert!(
        session.transcript[0].text.contains("20.0 MB"),
        "the size was not reported: {}",
        session.transcript[0].text
    );
}

/// Carrying on saying it once the picture is in the prompt is nagging, and the answer is
/// stale in any case: the hint is about what a key would do, and the key has been pressed.
#[test]
fn a_paste_clears_the_hint_that_prompted_it() {
    let mut session = Session::new("kernel-enforced");
    session.image_on_clipboard = true;
    take_from_clipboard(&mut session, picture(b"pixels".to_vec()));

    assert!(!session.image_on_clipboard, "the hint outlived the paste");
}
