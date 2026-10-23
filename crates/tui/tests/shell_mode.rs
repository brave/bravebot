//! What shell mode looks like on a real frame.
//!
//! The marker and the hint are the whole of how a user knows the line is going to a shell rather
//! than to the model, so they are asserted against the drawn buffer rather than against the state
//! that feeds it.

use bravebot_tui::render;
use bravebot_tui::state::{Entry, Session};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// The drawn screen, as one string per row, so a marker at the start of a line can be found.
fn rows(session: &Session, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|f| {
            render::draw(f, session);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn screen(session: &Session) -> String {
    rows(session, 80, 24).join("\n")
}

/// The mode has to be discoverable, or a user who never learns it never learns it is there. The
/// hint line no longer lists any binding, so the shortcuts are where this is answered, and the `!`
/// has to be among them.
#[test]
fn the_shortcuts_offer_shell_mode() {
    let mut session = Session::new("kernel-enforced");
    session.type_char('?');
    let drawn = rows(&session, 100, 40).join("\n");
    assert!(
        drawn.contains("run a shell command"),
        "a user has no way to discover the mode:\n{drawn}"
    );
}

/// The prompt marker is how a user knows where the line is going. Drawn as `!` rather than `>`, and
/// coloured, because pressing Enter on the wrong one runs something.
#[test]
fn the_prompt_marker_changes_in_shell_mode() {
    let mut session = Session::new("kernel-enforced");
    session.shell = true;
    session.type_char('l');
    session.type_char('s');

    let drawn = rows(&session, 80, 24);
    assert!(
        drawn.iter().any(|row| row.contains("! ls")),
        "the command was not drawn behind the marker:\n{}",
        drawn.join("\n")
    );
    assert!(
        !drawn.iter().any(|row| row.contains("> ls")),
        "it was drawn as an ordinary prompt:\n{}",
        drawn.join("\n")
    );
}

/// In shell mode the usual bindings are beside the point, and which shell is about to run the line
/// is the thing a user cannot otherwise find out.
#[test]
fn the_hint_names_the_shell_while_in_shell_mode() {
    let mut session = Session::new("kernel-enforced");
    session.shell = true;

    let drawn = screen(&session);
    assert!(
        drawn.contains("esc to cancel"),
        "no way out was offered:\n{drawn}"
    );
    assert!(
        drawn.contains("/") && drawn.contains("sh"),
        "the shell was not named:\n{drawn}"
    );
}

/// A command and its output read as one thing in the scrollback, the command behind the marker it
/// was typed behind.
#[test]
fn a_command_and_its_output_are_shown_together() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("echo hello"));
    session.transcript.push(Entry::output("hello"));

    let drawn = rows(&session, 80, 24);
    let command = drawn
        .iter()
        .position(|row| row.contains("! echo hello"))
        .expect("the command was not shown");
    let output = drawn
        .iter()
        .position(|row| row.trim() == "hello")
        .expect("the output was not shown");
    assert!(
        output > command,
        "the output was not beneath the command:\n{}",
        drawn.join("\n")
    );
}

/// Output is trusted, because the user typed the command that produced it. Drawing it inside the
/// quarantine margin would tell them the opposite of what the kernel decided.
#[test]
fn output_is_not_drawn_as_quarantined() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("cat notes"));
    session.transcript.push(Entry::output("some text"));

    let drawn = screen(&session);
    assert!(
        !drawn.contains("untrusted"),
        "output the user vouched for was marked untrusted:\n{drawn}"
    );
    assert!(
        !drawn.contains('\u{2503}'),
        "the quarantine bar was drawn down the user's own output:\n{drawn}"
    );
}

/// The interface draws its own structure, so text must not be able to draw any of it. An escape
/// sequence would let a file's contents recolour the screen or reposition the cursor, and a forged
/// margin is worse than none: drawing one is only worth anything if content cannot.
#[test]
fn output_cannot_draw_its_own_escapes() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("cat hostile"));
    session
        .transcript
        .push(Entry::output("\u{1b}[31mred\u{1b}[0m and \u{1b}[2Jcleared"));

    let drawn = screen(&session);
    assert!(
        !drawn.contains('\u{1b}'),
        "an escape reached the terminal:\n{drawn}"
    );
    // Shown rather than dropped, so a user can tell the bytes were there.
    assert!(
        drawn.contains('\u{241b}'),
        "the escape was silently swallowed:\n{drawn}"
    );
}

/// A program that colours its output writes a code around each coloured word, and drawn as
/// pictures they bury the words. The output shows the words and says how many codes it left out
/// (VIEW-30).
#[test]
fn coloured_output_is_drawn_without_its_codes_and_they_are_counted() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("gh run view"));
    session.transcript.push(Entry::output(
        "\u{1b}[0m\u{1b}[36m\u{251c}\u{1b}[0m Announcement: a link\nplain line",
    ));

    let drawn = screen(&session);
    assert!(drawn.contains("\u{251c} Announcement: a link"), "{drawn}");
    assert!(drawn.contains("plain line"), "{drawn}");
    assert!(
        !drawn.contains("[36m") && !drawn.contains("[0m") && !drawn.contains('\u{241b}'),
        "a colour code was drawn as text:\n{drawn}"
    );
    assert!(drawn.contains("3 colour codes not drawn"), "{drawn}");
}

/// Only a colour code is left out. An erase beside one is still shown as what it is, and still
/// cannot reach the terminal.
#[test]
fn output_keeps_every_other_escape_beside_the_codes_it_leaves_out() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("cat hostile"));
    session
        .transcript
        .push(Entry::output("\u{1b}[31mred\u{1b}[0m and \u{1b}[2Jcleared"));

    let drawn = screen(&session);
    assert!(drawn.contains("red and \u{241b}[2Jcleared"), "{drawn}");
    assert!(drawn.contains("2 colour codes not drawn"), "{drawn}");
    assert!(!drawn.contains('\u{1b}'), "{drawn}");
}

/// Output with no colour codes says nothing about them.
#[test]
fn output_without_colour_codes_says_nothing_about_them() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("echo hello"));
    session.transcript.push(Entry::output("hello"));

    assert!(!screen(&session).contains("colour code"));
}

/// A command that printed nothing has to say so. A blank gap would look like a command that never
/// ran, which is the one thing the user cannot tell by looking.
#[test]
fn a_command_that_printed_nothing_says_so() {
    let mut session = Session::new("kernel-enforced");
    session.printed("");

    assert!(
        screen(&session).contains("no output"),
        "silence was indistinguishable from a command that did not run"
    );
}

/// While a command runs the box holds a spinner and the way to stop it, rather than an idle prompt
/// that looks ready for input it would not accept.
#[test]
fn a_running_command_shows_that_it_is_running() {
    let mut session = Session::new("kernel-enforced");
    session.begin_command();

    let drawn = screen(&session);
    assert!(
        drawn.contains("running"),
        "no sign it was working:\n{drawn}"
    );
    assert!(drawn.contains("esc to stop"), "no way to stop it:\n{drawn}");
}

/// A command line waiting for a turn to end is going somewhere different from a prompt waiting
/// beside it, and the row under the box is the only place that says which. Drawn without the
/// marker, `echo pwned` there reads as words on their way to the model.
#[test]
fn a_waiting_command_line_is_drawn_behind_the_marker() {
    let mut session = Session::new("kernel-enforced");
    session.type_char('a');
    session.submit();

    session.shell = true;
    for c in "echo pwned".chars() {
        session.type_char(c);
    }
    assert!(session.queue_shell(), "the command line was not queued");

    let drawn = rows(&session, 80, 24);
    assert!(
        drawn.iter().any(|row| row.contains("! echo pwned")),
        "the waiting command line was not drawn behind the marker:\n{}",
        drawn.join("\n")
    );
    assert!(
        drawn.iter().any(|row| row.contains("QUEUED")),
        "nothing said it was waiting:\n{}",
        drawn.join("\n")
    );
}

/// The private form is told from the plain one on the drawn frame: a doubled marker on the line and
/// a hint that the output stays on the screen, because pressing Enter on the wrong one either sends
/// a file to the model service or withholds output the person meant to share.
#[test]
fn the_private_form_is_drawn_with_a_doubled_marker_and_says_where_the_output_goes() {
    let mut session = Session::new("kernel-enforced");
    session.type_char('!');
    session.type_char('!');
    session.type_char('l');
    session.type_char('s');

    let drawn = rows(&session, 100, 24);
    assert!(
        drawn.iter().any(|row| row.contains("!!ls")),
        "the private marker was not drawn:\n{}",
        drawn.join("\n")
    );
    assert!(
        drawn
            .iter()
            .any(|row| row.contains("not sent to the model")),
        "nothing said the output stays here:\n{}",
        drawn.join("\n")
    );

    let mut plain = Session::new("kernel-enforced");
    plain.type_char('!');
    plain.type_char('l');
    let drawn = rows(&plain, 100, 24).join("\n");
    assert!(
        drawn.contains("output goes to the model") && !drawn.contains("!!"),
        "the plain form was drawn as private:\n{drawn}"
    );
}

/// What a private command printed is drawn, followed by a line saying it was not sent.
#[test]
fn private_output_is_drawn_and_says_it_went_no_further() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("cat .env"));
    session.printed_privately("SECRET=1");

    let drawn = screen(&session);
    assert!(
        drawn.contains("SECRET=1"),
        "the output was not drawn:\n{drawn}"
    );
    assert!(
        drawn.contains("not sent to the model"),
        "nothing said it was kept private:\n{drawn}"
    );
}

/// While a command runs, what it has printed is on the screen under the command, with the spinner
/// still there since it has not ended (SHELL-7). Without it a slow build is a spinner and nothing
/// to tell it from a hang.
///
/// Rejects: running output not being drawn until the command ends.
#[test]
fn output_that_has_been_printed_is_drawn_while_the_command_runs() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("cargo test"));
    session.begin_command();
    session.command_printing("running 3 tests\n");
    session.command_printing("test one ... ok\n");

    let drawn = rows(&session, 80, 24);
    let command = drawn
        .iter()
        .position(|row| row.contains("cargo test"))
        .expect("the command was not shown");
    let printed = drawn
        .iter()
        .position(|row| row.trim() == "test one ... ok")
        .expect("what it had printed was not drawn");
    assert!(printed > command, "{}", drawn.join("\n"));
    assert!(
        drawn.iter().any(|row| row.contains("running")),
        "the command stopped looking as though it was running:\n{}",
        drawn.join("\n")
    );
}

/// The entry that replaces the rows is drawn where they were, and they are gone, so what a command
/// printed is on the screen once and the screen does not jump at the handover.
///
/// Rejects: the rows outliving the command and drawing the output a second time.
#[test]
fn a_command_that_ended_draws_its_output_once() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("echo only-once"));
    session.begin_command();
    session.command_printing("only-once\n");
    session.finish_command();
    session.printed("only-once\n");

    let drawn = screen(&session);
    assert_eq!(drawn.matches("only-once").count(), 2, "{drawn}");
}

/// What a running command prints is drawn through the step that draws its entry: a colour code is
/// counted and not drawn, and every other escape is replaced by a picture, so a program cannot
/// recolour the screen or move the cursor while it runs any more than after it ends.
///
/// Rejects: running output drawn as it arrived.
#[test]
fn running_output_cannot_draw_its_own_escapes() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("cat hostile"));
    session.begin_command();
    session.command_printing("\u{1b}[31mred\u{1b}[0m and \u{1b}[2Jcleared\n");

    let drawn = screen(&session);
    assert!(
        !drawn.contains('\u{1b}'),
        "an escape reached the terminal:\n{drawn}"
    );
    assert!(drawn.contains("red and \u{241b}[2Jcleared"), "{drawn}");
    assert!(drawn.contains("2 colour codes not drawn"), "{drawn}");
}

/// A read can end inside a colour code. Until the rest arrives the part is drawn as the pictures an
/// escape is made into and never as an escape, and once it arrives the code is whole and taken out.
///
/// Rejects: a piece being made safe by itself when it arrives and the result kept, which leaves the
/// tail of the code drawn as text when the next piece completes it, and a piece that is drawn raw.
#[test]
fn a_colour_code_split_between_two_pieces_is_counted_once_it_is_whole() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("ls --color"));
    session.begin_command();

    session.command_printing("a\u{1b}[3");
    let halfway = screen(&session);
    assert!(
        !halfway.contains('\u{1b}'),
        "an escape reached the terminal:\n{halfway}"
    );
    assert!(halfway.contains('\u{241b}'), "{halfway}");

    session.command_printing("1mred\n");
    let whole = screen(&session);
    assert!(whole.contains("ared"), "{whole}");
    assert!(
        !whole.contains("[31m") && !whole.contains('\u{241b}'),
        "{whole}"
    );
    assert!(whole.contains("1 colour code not drawn"), "{whole}");
}

/// A command can print without end, and a frame draws everything held, so only the end of it is
/// kept while it runs: the lines before are counted and said, and the whole is drawn when it ends.
///
/// Rejects: every line of a long output being held and drawn on every frame.
#[test]
fn only_the_end_of_a_long_output_is_held_while_it_runs() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("seq 1 1000"));
    session.begin_command();
    let extra = 50;
    for number in 0..bravebot_tui::state::COMMAND_LINES + extra {
        session.command_printing(&format!("line {number}\n"));
    }

    let (kept, earlier) = session.command_so_far().expect("it has printed");
    assert_eq!(earlier, extra);
    assert_eq!(kept.lines().count(), bravebot_tui::state::COMMAND_LINES);
    assert!(kept.starts_with(&format!("line {extra}\n")), "{kept}");

    let drawn = rows(&session, 80, 260);
    let has_row = |text: &str| drawn.iter().any(|row| row.trim() == text);
    let screen = drawn.join("\n");
    assert!(has_row("\u{2026} 50 earlier lines"), "{screen}");
    assert!(
        !has_row("line 49"),
        "a line that was cut was drawn:\n{screen}"
    );
    assert!(has_row("line 50"), "{screen}");
    let last = bravebot_tui::state::COMMAND_LINES + extra - 1;
    assert!(has_row(&format!("line {last}")), "{screen}");
}

/// A line with no end is bounded by its bytes, so a command that never prints a newline does not
/// make every frame draw all it has printed.
///
/// Rejects: no bound on one long line.
#[test]
fn one_endless_line_is_held_to_a_bounded_size() {
    let mut session = Session::new("kernel-enforced");
    session.begin_command();
    for _ in 0..30 {
        session.command_printing(&"x".repeat(bravebot_tui::state::COMMAND_BYTES / 10));
    }

    let (kept, earlier) = session.command_so_far().expect("it has printed");
    assert_eq!(kept.len(), bravebot_tui::state::COMMAND_BYTES);
    assert_eq!(earlier, 0);
}

/// A bound that falls inside a character moves to the end of it, so what is kept is text and not
/// a cut character. Two bytes to the character here and one byte too many, so the bound is at an
/// odd byte, which is inside the first character.
///
/// Rejects: a cut at the byte the bound names, which splits the character and panics.
#[test]
fn a_bound_inside_a_character_moves_to_the_end_of_it() {
    let mut session = Session::new("kernel-enforced");
    session.begin_command();
    session.command_printing(&"\u{e9}".repeat(bravebot_tui::state::COMMAND_BYTES / 2));
    session.command_printing("a");

    let (kept, _) = session.command_so_far().expect("it has printed");
    let whole = bravebot_tui::state::COMMAND_BYTES / 2 - 1;
    assert_eq!(kept, format!("{}a", "\u{e9}".repeat(whole)));
}

/// A command that has printed nothing draws nothing but its spinner, and one that printed only
/// blank lines draws no empty block under the command. The transcript is longer than the screen, so
/// a blank row added at its end moves every row above it and the frame differs.
///
/// Rejects: a block drawn for output that is only blank lines.
#[test]
fn a_command_that_has_printed_only_blank_lines_draws_no_output() {
    let mut session = Session::new("kernel-enforced");
    for number in 0..30 {
        session
            .transcript
            .push(Entry::user(format!("earlier prompt {number}")));
    }
    session.transcript.push(Entry::shell("echo"));
    session.begin_command();
    let before = screen(&session);
    session.command_printing("\n\n");

    assert_eq!(screen(&session), before);
}

/// The blank lines at the end of what a command has printed are not drawn while it runs, as they are
/// not drawn in the entry that replaces the rows, so the screen does not move when the command ends.
/// The transcript is longer than the screen, so a blank row at its end moves every row above it.
///
/// Rejects: running output drawn with the blank lines the finished entry leaves out.
#[test]
fn the_blank_lines_ending_running_output_are_not_drawn() {
    let session_printing = |printed: &str| {
        let mut session = Session::new("kernel-enforced");
        for number in 0..30 {
            session
                .transcript
                .push(Entry::user(format!("earlier prompt {number}")));
        }
        session.transcript.push(Entry::shell("seq 1 2"));
        session.begin_command();
        session.command_printing(printed);
        session
    };

    assert_eq!(
        screen(&session_printing("1\n2\n\n\n")),
        screen(&session_printing("1\n2"))
    );
}

/// A last line with no newline yet is one of the lines kept, so what is held is never more than the
/// lines the bound names, whatever the command was in the middle of printing.
///
/// Rejects: a bound that counts only the lines that have ended and keeps one more.
#[test]
fn a_line_still_being_written_counts_as_one_of_the_lines_held() {
    let mut session = Session::new("kernel-enforced");
    session.begin_command();
    let extra = 10;
    for number in 0..bravebot_tui::state::COMMAND_LINES + extra {
        session.command_printing(&format!("line {number}\n"));
    }
    session.command_printing("half a li");

    let (kept, earlier) = session.command_so_far().expect("it has printed");
    assert_eq!(kept.lines().count(), bravebot_tui::state::COMMAND_LINES);
    assert!(kept.ends_with("\nhalf a li"), "{kept}");
    assert_eq!(earlier, extra + 1);
}

/// A command that prints faster than frames are drawn hands over a great deal in one piece. The
/// piece is cut from the end, and what was held and what the piece dropped are both counted.
///
/// Rejects: a large piece being kept whole, and the lines it pushed out of the bound, or the lines
/// that were held before it, going uncounted.
#[test]
fn a_piece_larger_than_the_bound_is_cut_from_its_end_and_counted() {
    let mut session = Session::new("kernel-enforced");
    session.begin_command();
    session.command_printing("held before\n");
    let lines = 100_000;
    session.command_printing(&"x\n".repeat(lines));

    let (kept, earlier) = session.command_so_far().expect("it has printed");
    assert_eq!(kept.lines().count(), bravebot_tui::state::COMMAND_LINES);
    assert!(kept.lines().all(|line| line == "x"), "{kept}");
    assert_eq!(earlier, 1 + lines - bravebot_tui::state::COMMAND_LINES);
}

/// A command that is stopped after printing more than the rows held leaves an entry that begins
/// with the line saying how many lines came before, so the screen does not read as the whole of it.
///
/// Rejects: the entry made of the rows alone, which reads as though the command had printed only
/// those lines.
#[test]
fn a_stopped_command_keeps_the_count_of_the_lines_before_the_rows() {
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(Entry::shell("seq 1 1000"));
    session.begin_command();
    let extra = 50;
    for number in 0..bravebot_tui::state::COMMAND_LINES + extra {
        session.command_printing(&format!("line {number}\n"));
    }

    session.finish_stopped_command();

    let kept = session.transcript.last().expect("an entry");
    assert!(
        kept.text
            .starts_with("\u{2026} 50 earlier lines\nline 50\n"),
        "{}",
        kept.text
    );
    assert!(session.command_so_far().is_none());
}
