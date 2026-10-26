//! MOUSE-16: where a click in the box puts the caret, and what a drag over the box copies, for the
//! lines the box wraps, holds control characters in, and draws beside the info panel.

use super::*;

/// A session with `text` in the box, drawn at `width` by 24, and the cells of its first row's words.
fn boxed(text: &str, width: u16) -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("none");
    session.paste(text);
    let buffer = drawn_at(&mut session, width, 24);
    (session, buffer)
}

/// MOUSE-16. Rejects: a click to the right of a row the box broke at a space putting the caret at the
/// start of the next row, which is the same place in the line. The row ends in the blank it was broken
/// at, so the end of what is drawn on it is before the blank, and a press past that is on the row.
#[test]
fn a_click_right_of_a_row_broken_at_a_space_puts_the_caret_at_the_end_of_that_row() {
    // At 20 columns the text has five columns and the prompt, so the words wrap after "hello ".
    let (mut session, buffer) = boxed("hello world and more", 14);
    let (row, _) = find_cell(&buffer, "hello");
    let (below, _) = find_cell(&buffer, "world");
    assert_eq!(
        below,
        row + 1,
        "the line did not wrap where the test needs it"
    );

    click(&mut session, row, 12);
    assert_eq!(
        session.caret(),
        "hello".len(),
        "the caret went to the next row"
    );
    session.last_click = None;
    click(&mut session, row + 1, 12);
    assert_eq!(session.caret(), "hello world".len());
    session.last_click = None;
    // A press on the text of the second row is where it landed, which the first change must not move.
    let (_, at) = find_cell(&buffer, "world");
    click(&mut session, below, at + 2);
    assert_eq!(session.caret(), "hello wo".len());
}

/// MOUSE-16. Rejects: a control character in the line, which the box draws as nothing, being left out
/// of the count of bytes a click passes, so that a click on a character after a tab put the caret
/// before the character before it.
#[test]
fn a_click_after_a_control_character_lands_on_the_character_drawn_there() {
    let (mut session, buffer) = boxed("a\tbcd", 40);
    let (row, column) = find_cell(&buffer, "abcd");
    for (offset, caret) in [(0, 0), (1, 2), (2, 3), (3, 4)] {
        click(&mut session, row, column + offset);
        assert_eq!(session.caret(), caret, "cell {offset}");
        session.last_click = None;
    }
}

/// MOUSE-16. Rejects: a click in a box that has more rows than it shows being placed by the row's
/// place among the rows drawn and not by its place in the line, which is the same only while the
/// first row drawn is the first row of the line. The caret is at the end of a long line, so the box
/// shows its last rows, and a click on each word drawn lands on that word.
#[test]
fn a_click_in_a_box_that_has_scrolled_lands_on_the_word_drawn_there() {
    let text: Vec<String> = (0..100).map(|n| format!("w{n:02}")).collect();
    let (mut session, buffer) = boxed(&text.join(" "), 30);
    let mut tried = 0;
    for n in 80..100 {
        let word = format!("w{n}");
        let Some((row, column)) = (0..buffer.area.height).find_map(|row| {
            let line: String = (0..buffer.area.width)
                .map(|at| buffer[(at, row)].symbol())
                .collect();
            line.find(&word)
                .map(|at| (row, line[..at].chars().count() as u16))
        }) else {
            continue;
        };
        click(&mut session, row, column);
        assert_eq!(
            session.caret(),
            session.input().find(&word).expect("the word"),
            "{word}"
        );
        session.last_click = None;
        tried += 1;
    }
    assert!(tried >= 8, "the box showed too few of the words: {tried}");
}

/// MOUSE-1 and MOUSE-16. Rejects: a drag across the box taking its border, which is drawn and is no
/// text, with the info panel open, where the box ends before the screen does and the border is the
/// last cell the drag reaches.
#[test]
fn a_drag_across_the_box_copies_its_words_and_not_its_border() {
    let (mut session, buffer) = {
        let (mut session, _) = two_panes();
        session.paste("hello box");
        let buffer = drawn_at(&mut session, 120, 24);
        (session, buffer)
    };
    let (row, column) = find_cell(&buffer, "hello box");
    let handed = dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["hello box".to_string()]);
}

/// MOUSE-16. Rejects: a report at the largest column or row a terminal can name being taken for a cell
/// of the box, or moving the caret: where such a cell was reached by an addition that wrapped, it
/// landed on the row it wrapped to. A click at each of the largest coordinates, with the caret
/// somewhere that is not their row, leaves it where it was, and none is a place in the box.
#[test]
fn a_click_at_the_largest_coordinates_is_a_cell_outside_the_box() {
    let (mut session, buffer) = boxed("a line of text in the box", 40);
    let (row, column) = find_cell(&buffer, "a line");
    click(&mut session, row, column + 3);
    let caret = session.caret();
    assert_eq!(caret, 3, "the fixture's click did not land");
    session.last_click = None;

    for (at_row, at_column) in [
        (u16::MAX, u16::MAX),
        (u16::MAX, column),
        (row, u16::MAX),
        (row, u16::MAX - 1),
        (u16::MAX - 1, u16::MAX - 1),
    ] {
        assert_eq!(session.offset_in_the_box(at_row, at_column), None);
        assert!(!session.click_in_the_box(at_row, at_column));
        click(&mut session, at_row, at_column);
        assert_eq!(session.caret(), caret, "({at_row}, {at_column})");
        session.last_click = None;
    }
}
