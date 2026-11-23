//! Dropping a file onto a command line, which is a paste the session recognises as a drop.
//!
//! Real files in a real directory, because whether a path names something is the whole question
//! a drop asks. The commands are exercised through [`handle_key`] rather than through
//! [`Session::take_command`], because the defect was in the wiring between them: the box took a
//! picture off the line and the dispatch was handed pictures alone.

use super::*;

/// A scratch directory with one file in it, and a session working there.
///
/// The workspace matters: it is what decides the name the task is given, and a session with
/// none names every drop absolutely.
fn a_session_with(name: &str, file: &str) -> (Session, std::path::PathBuf, String) {
    let directory = crate::testutil::scratch_dir(&format!("bravebot-app-drop-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    std::fs::write(directory.join(file), [0x89u8, 0x50]).expect("write");
    let at = directory.join(file).to_string_lossy().to_string();
    (
        Session::new("kernel-enforced").in_workspace(&directory),
        directory,
        at,
    )
}

/// The question the issue reports. A dropped picture and a pasted one read the same in the
/// box, so a question that carries one and not the other answers about nothing and tells the
/// person no image came through about a screenshot that is plainly in their line.
#[test]
fn a_picture_dropped_onto_a_question_goes_with_it() {
    let (mut session, directory, path) = a_session_with("btw", "shot.png");
    type_line(&mut session, "/btw what is in ");
    assert!(session.drop_files(&path), "not recognised as a drop");
    assert_eq!(session.input(), "/btw what is in [Image #1] ");

    let action = handle_key(&mut session, key(KeyCode::Enter));
    let Action::Aside(question, pasted, attached) = action else {
        panic!("a question was not asked: {action:?}");
    };
    assert_eq!(question, "what is in [Image #1]");
    assert!(pasted.is_empty(), "a paste was invented: {pasted:?}");
    assert_eq!(attached.len(), 1, "the picture did not go: {attached:?}");
    assert_eq!(attached[0].name, "shot.png");

    let _ = std::fs::remove_dir_all(&directory);
}

/// A picture of the thing to be built is the task whichever gesture put it there.
#[test]
fn a_picture_dropped_onto_a_task_goes_with_the_plan() {
    let (mut session, directory, path) = a_session_with("manifest", "shot.png");
    type_line(&mut session, "/manifest build ");
    session.drop_files(&path);

    let action = handle_key(&mut session, key(KeyCode::Enter));
    let Action::Manifest(task, _, attached) = action else {
        panic!("a plan was not asked for: {action:?}");
    };
    assert_eq!(task, "build [Image #1]");
    assert_eq!(attached.len(), 1, "the picture did not go: {attached:?}");

    let _ = std::fs::remove_dir_all(&directory);
}

/// A tick is an ordinary turn, so the first one carries the file the person dropped on the
/// line they armed the loop from. Settled at the dispatch instead, the loop would spend its
/// whole life asking about a file no tick was given.
#[test]
fn a_picture_dropped_onto_a_loop_goes_with_its_first_tick() {
    let (mut session, directory, path) = a_session_with("loop", "shot.png");
    type_line(&mut session, "/loop 15m look at ");
    session.drop_files(&path);

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("look at [Image #1]".to_string())
    );
    assert_eq!(
        session.sent_attachments().len(),
        1,
        "the first tick went without the file"
    );
    assert_eq!(session.sent_attachments()[0].name, "shot.png");

    let _ = std::fs::remove_dir_all(&directory);
}

/// Waiting is not being refused, so a question typed while a turn ran carries what was
/// dropped on it when the queue reaches it. What makes that possible is that the file is held
/// with the queued line rather than in the box, which the press that queued it emptied.
#[test]
fn a_picture_dropped_onto_a_question_that_waited_still_goes_with_it() {
    let (mut session, directory, path) = a_session_with("queued", "shot.png");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/manifest build ".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, &path);
    assert_eq!(session.input(), "/manifest build [Image #1] ");
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(session.input().is_empty(), "the command stayed in the box");

    session.complete("answered", Vec::new(), 0);

    let action = queued_next(&mut session);
    let Some(Action::Manifest(question, _, attached)) = action else {
        panic!("a waiting task was lost: {action:?}");
    };
    assert_eq!(question, "build [Image #1]");
    assert_eq!(attached.len(), 1, "the picture did not go: {attached:?}");

    let _ = std::fs::remove_dir_all(&directory);
}

/// Deleting the marker is how a file is taken off a line, so what a command carries is read
/// out of the line rather than off what the session has staged. Read off the staging, a drop
/// somebody rubbed out would ride along with every command they typed afterwards.
#[test]
fn a_command_line_whose_dropped_marker_was_deleted_carries_no_file() {
    let (mut session, directory, path) = a_session_with("deleted", "shot.png");
    type_line(&mut session, "/btw ");
    session.drop_files(&path);
    // The marker and the space a drop leaves after itself.
    handle_key(&mut session, key(KeyCode::Backspace));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert_eq!(session.input(), "/btw ", "the marker outlived the press");
    type_line(&mut session, "what is this");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside("what is this".to_string(), Vec::new(), Vec::new())
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// A command the driver carries out itself has no request for a file to travel in, so the
/// marker becomes the file's name: that is what a planner is handed everywhere else the file
/// itself cannot go. Left as it was, `/rename` would call the session `[Image #1]`.
#[test]
fn a_file_dropped_onto_a_command_line_is_carried_out_as_its_name() {
    let (mut session, directory, path) = a_session_with("rename", "shot.png");
    type_line(&mut session, "/rename ");
    session.drop_files(&path);

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Rename("shot.png".to_string())
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// A command line reaches the shell as it stands, so a marker in it would be run as the text
/// `[Image #1]`. The path is written instead, and nothing is staged behind a line that is
/// about to leave the box.
#[test]
fn a_file_dropped_onto_a_shell_line_is_run_as_its_path() {
    let (mut session, directory, path) = a_session_with("shell", "shot.png");
    type_line(&mut session, "!");
    assert!(session.shell, "shell mode was not armed");
    type_line(&mut session, "file ");
    assert!(session.drop_files(&path), "not recognised as a drop");
    assert_eq!(session.input(), format!("file {path} "));

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run(format!("file {path}"))
    );
    assert!(
        session.attached().is_empty(),
        "the record outlived the line: {:?}",
        session.attached()
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// A path the way macOS Terminal hands over a dropped file: a backslash before every
/// character a shell would not read as part of a word.
fn escaped_by_the_terminal(path: &str) -> String {
    let mut escaped = String::new();
    for c in path.chars() {
        if !(c.is_ascii_alphanumeric() || "/._-".contains(c)) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// A path the way a terminal that hands over a URI writes it, percent-encoded.
fn as_a_file_uri(path: &str) -> String {
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/.-_".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// A file of this name dropped onto `printf '%s\n' ` in shell mode and run: the dropped
/// path, what the shell printed, one line per argument, and the names in the directory
/// after it ran.
fn printed_for_a_dropped(name: &str, file: &str) -> (String, String, Vec<String>) {
    let (mut session, directory, path) = a_session_with(name, file);
    type_line(&mut session, "!");
    type_line(&mut session, "printf '%s\\n' ");
    assert!(
        session.drop_files(&escaped_by_the_terminal(&path)),
        "not recognised as a drop"
    );
    let action = handle_key(&mut session, key(KeyCode::Enter));
    let Action::Run(line) = action else {
        panic!("the line was not run: {action:?}");
    };
    let ran = bravebot_agent::shell::run(&line, &directory, &Cancel::new()).expect("it runs");
    let mut names: Vec<String> = std::fs::read_dir(&directory)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    let _ = std::fs::remove_dir_all(&directory);
    (path, ran.stdout, names)
}

/// A screenshot's name has spaces in it, and written bare the shell hands the command a
/// word for each, none of which names the file.
#[test]
fn a_dropped_name_with_a_space_reaches_the_shell_as_one_word() {
    let file = "Screen Shot 1.png";
    let (path, printed, names) = printed_for_a_dropped("shell-space", file);
    assert_eq!(printed, format!("{path}\n"));
    assert_eq!(names, [file]);
}

/// A name is chosen by whoever made the file, which can be a repository, an archive or a
/// download. Written bare or in double quotes, this one runs two commands nobody typed.
#[test]
fn a_dropped_name_holding_shell_syntax_runs_none_of_it() {
    let file = "a;touch semi;b$(touch sub)c`touch tick`.png";
    let (path, printed, names) = printed_for_a_dropped("shell-syntax", file);
    assert_eq!(printed, format!("{path}\n"));
    assert_eq!(names, [file], "the name ran as a command");
}

/// Single quotes hold everything but a quote, so a quote in the name has to be written
/// outside them, and a backslash beside it has to come back as one backslash.
#[test]
fn a_quote_in_a_dropped_name_does_not_end_its_quoting() {
    let file = r"it's a\';touch pwned;'.png";
    let (path, printed, names) = printed_for_a_dropped("shell-quote", file);
    assert_eq!(printed, format!("{path}\n"));
    assert_eq!(names, [file], "the name ran as a command");
}

/// What a person cannot see on the line they cannot have meant to run, and taking the
/// character out would name some other file. So the drop writes nothing and says why,
/// whichever spelling the terminal handed the name over in.
#[test]
fn a_dropped_name_holding_an_invisible_character_writes_nothing_and_says_why() {
    let (mut session, directory, path) = a_session_with("shell-invisible", "shot\u{202E}gnp.png");
    type_line(&mut session, "!");
    type_line(&mut session, "printf '%s\\n' ");

    for dropped in [escaped_by_the_terminal(&path), as_a_file_uri(&path)] {
        let notes = session.transcript.len();
        assert!(
            session.drop_files(&dropped),
            "not taken as a drop: {dropped}"
        );
        assert_eq!(session.input(), "printf '%s\\n' ", "written from {dropped}");
        assert_eq!(
            session.transcript.len(),
            notes + 1,
            "nothing said for {dropped}"
        );
        assert_eq!(
            session.transcript.last().expect("a note").text,
            t!(drop_name_not_written)
        );
    }

    let _ = std::fs::remove_dir_all(&directory);
}

/// A carriage return reaches a name through a `file://` URI, and the box writes it as a
/// newline, so the word on the line would name another file. The whole drop writes
/// nothing, the ordinary name beside it included, since a command missing one of the files
/// it was dropped with is not the command the person made.
#[test]
fn a_drop_with_a_control_character_in_a_name_writes_none_of_its_paths() {
    let (mut session, directory, path) = a_session_with("shell-control", "a\rb.png");
    let plain = directory.join("plain.png");
    std::fs::write(&plain, b"x").expect("write");
    type_line(&mut session, "!");
    type_line(&mut session, "ls ");

    let dropped = format!("{} {}", plain.to_string_lossy(), as_a_file_uri(&path));
    assert!(session.drop_files(&dropped), "not taken as a drop");
    assert_eq!(session.input(), "ls ");
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(drop_name_not_written)
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// A text file's contents would be a context message, and a planner asked a question or a
/// task is precommitted to a context holding the task and the driver's own words. So what it
/// is given is the name, which it can act on, rather than a marker standing for nothing.
#[test]
fn a_text_file_dropped_onto_a_question_is_sent_as_its_name() {
    let (mut session, directory, path) = a_session_with("text", "notes.md");
    type_line(&mut session, "/btw what is in ");
    session.drop_files(&path);
    assert_eq!(session.input(), "/btw what is in [File #1] ");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside("what is in notes.md".to_string(), Vec::new(), Vec::new())
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// Taking the line clears what it named, the way taking a prompt does. Left staged, a
/// session accumulates a record for every command anybody dropped on, each one behind a box
/// that cannot name it again, which is the state `dropping.md` DROP-6 exists to rule out.
/// Nothing on the screen says so, which is why this is asserted against the session.
#[test]
fn dispatching_a_command_clears_what_was_dropped_on_its_line() {
    let (mut session, directory, path) = a_session_with("cleared", "shot.png");
    type_line(&mut session, "/btw what is in ");
    session.drop_files(&path);
    assert_eq!(session.attached().len(), 1);

    handle_key(&mut session, key(KeyCode::Enter));
    assert!(
        session.attached().is_empty(),
        "the record outlived the line: {:?}",
        session.attached()
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// DROP-1. A task is given the files the person dropped on the line and nothing else: the paths
/// it carries come from what the drop staged, so a path the line only names in words, or a marker
/// typed by hand, reaches neither list. The line here drops a picture and a text file, then types
/// the path of a real picture and a marker nothing staged. The control half is the pair that was
/// dropped, which a supplier that carried nothing would also fail.
///
/// Rejects a supplier that reads paths out of the prompt (or out of any text a model or a file
/// supplied) and gives them the unconfined, trusted read a drop gets, and one that carries every
/// marker the line shows.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn a_task_carries_the_files_dropped_on_its_line_and_no_path_the_line_only_names() {
    let (mut session, directory, shot) = a_session_with("supplier", "shot.png");
    let notes = directory.join("notes.md");
    std::fs::write(&notes, "the notes").expect("write");
    let typed = directory.join("typed.png");
    std::fs::write(&typed, [0x89u8, 0x50]).expect("write");
    let typed = typed.to_string_lossy().to_string();

    type_line(&mut session, "compare ");
    assert!(session.drop_files(&shot), "the picture was not a drop");
    assert!(
        session.drop_files(&notes.to_string_lossy()),
        "the text file was not a drop"
    );
    type_line(&mut session, &format!("with {typed} and [Image #9]"));
    let action = handle_key(&mut session, key(KeyCode::Enter));
    let Action::Submit(prompt) = action else {
        panic!("the line was not sent: {action:?}");
    };

    let task = crate::app::with_submitted_attachments(bravebot_agent::Task::new(prompt), &session);
    let attached: Vec<&str> = task.attachments.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(attached, ["shot.png"], "carried more or less than the drop");
    assert_eq!(task.dropped_text, ["notes.md"]);

    let (mut session, directory_aside, shot) = a_session_with("supplier-aside", "shot.png");
    type_line(&mut session, &format!("/btw what is {typed} next to "));
    assert!(session.drop_files(&shot), "the picture was not a drop");
    let action = handle_key(&mut session, key(KeyCode::Enter));
    let Action::Aside(_, _, attached) = action else {
        panic!("a question was not asked: {action:?}");
    };
    let names: Vec<&str> = attached.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        ["shot.png"],
        "a question carried a path it only named"
    );

    std::fs::remove_dir_all(&directory).ok();
    std::fs::remove_dir_all(&directory_aside).ok();
}
