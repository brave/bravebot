//! MOUSE-8 and MOUSE-40: what a line the transcript wrapped copies as and opens as, whatever the
//! blanks at the breaks.

use super::*;

/// A session whose one reply is `text`, drawn at `width` by 24.
fn replying(text: &str, width: u16) -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("none");
    session
        .transcript
        .push(crate::state::Entry::assistant(text, Vec::new()));
    let buffer = drawn_at(&mut session, width, 24);
    (session, buffer)
}

/// What a click with ctrl held on the cell `(row, column)` opens.
fn opened_by(session: &mut Session, (row, column): (u16, u16)) -> Action {
    let mut down = drag(MouseEventKind::Down(MouseButton::Left), row, column);
    down.modifiers = KeyModifiers::CONTROL;
    handle_mouse(session, down);
    let mut up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    up.modifiers = KeyModifiers::CONTROL;
    handle_mouse(session, up)
}

/// MOUSE-40. Rejects: two addresses a blank apart, with a run of blanks before the first at a break
/// the wrap made, being read as one address, so that a click on either opens the first address's host
/// with the second's text, which nobody drew. A run of blanks at a break is dropped, kept or left at the
/// start of the next row, and a bookkeeping that counts one blank dropped at every break takes the
/// next break, which is the one between the addresses, for a cut inside a word. The line is written
/// with every length of what comes before the blanks, so that the run falls at a break at some widths,
/// and each address opens as it is drawn.
#[test]
fn two_addresses_a_blank_apart_open_as_each_is_drawn_whatever_the_blanks_before_them() {
    const FIRST: &str = "https://a.test/xx";
    const SECOND: &str = "https://b.test/yy";
    let mut told_apart_at_a_break = 0;
    for blanks in 1..=4 {
        for before in 1..=30 {
            for width in 14..=48 {
                let text = format!(
                    "{}{}{FIRST} {SECOND} end",
                    "x".repeat(before),
                    " ".repeat(blanks)
                );
                let (mut session, buffer) = replying(&text, width);
                let first = find_cell(&buffer, "https://a");
                let second = find_cell(&buffer, "https://b");
                let at = format!("{blanks} blanks after {before} letters at {width}");
                assert_eq!(
                    opened_by(&mut session, first),
                    Action::OpenUrl(FIRST.to_string()),
                    "{at}"
                );
                session.last_click = None;
                assert_eq!(
                    opened_by(&mut session, second),
                    Action::OpenUrl(SECOND.to_string()),
                    "{at}"
                );
                told_apart_at_a_break += usize::from(second.0 > first.0);
            }
        }
    }
    assert!(told_apart_at_a_break > 500, "the wrap hardly broke between");
}

/// `text` with each run of blanks as one, which is what a copy keeps of a run the wrap broke at.
fn collapsed(text: &str) -> String {
    text.split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// MOUSE-8. Rejects: a copy of a wrapped reply that glues the words either side of a break after two
/// blanks, drops a no-break space the wrap left at the end of a row, or adds a blank inside a word
/// the wrap cut. Swept with the mouse from the first word to the last, through the loop at rest.
#[test]
fn a_wrapped_reply_copies_as_its_words_whatever_the_blanks_and_the_width() {
    const TEXT: &str = "aaaaaaaa  bbbbbbbb   cccccccc dddddddd\u{a0}eeeeeeee  ffffffff";
    let mut wrapped = 0;
    for width in 14..=40 {
        let (mut session, buffer) = replying(TEXT, width);
        wrapped += usize::from(find_cell(&buffer, "ffffffff").0 > find_cell(&buffer, "aaaaaaaa").0);
        let first = find_cell(&buffer, "aaaaaaaa");
        let last = find_cell(&buffer, "ffffffff");
        let handed =
            dragged_and_released_at_rest(&mut session, (width, 24), first, (last.0, last.1 + 8));
        assert_eq!(handed.len(), 1, "at {width}");
        assert_eq!(collapsed(&handed[0]), collapsed(TEXT), "at {width}");
    }
    assert!(wrapped > 10, "the reply hardly wrapped: {wrapped}");
}
