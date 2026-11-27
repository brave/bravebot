//! A short piece of a longer text around one position in it, for a sentence that quotes it.
//!
//! The position is a parser's report on text the planner wrote: a line, and a column counted in
//! bytes from 1. It can name a byte inside a character, a line the text does not have, or a byte
//! past the end. The piece is counted in characters, so it never splits one, and holds at most
//! [`BEFORE`] characters before the position and [`AFTER`] from it, however long the text is.
//! A position one past the last byte, which is where a parser stops at the end of the input, is
//! read as the end, and the piece is the last [`BEFORE`] characters. Where the position names no
//! byte of the text and is not that one, the piece is the first [`HEAD`] characters.
//!
//! [`byte_at`] and [`ends_by`] do the arithmetic on the parser's byte column, and [`window`] the
//! arithmetic on counts of characters. Each carries a Verus contract. [`around`] takes the
//! characters they name with iterators, so nothing here slices a string.
//!
//! This is a map from text to text. It decides nothing about any effect, and a caller hands it only
//! text a gate has released.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// The most characters quoted before the position.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const BEFORE: usize = 40;

/// The most characters quoted from the position on, the character at it included.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const AFTER: usize = 20;

/// The most characters quoted where the position names no character of the text.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const HEAD: usize = BEFORE + AFTER;

/// The characters of a text `len` characters long to quote around character `at`, as the index
/// of the first and one past the last, or the first [`HEAD`] where `at` is `None`.
///
/// An `at` past the end is read as the end, so the window never reaches past `len`.
///
/// The contract states, for every `len` and `at`, that the window is inside `0..=len` and no
/// longer than [`HEAD`]; that a window around a character holds it, with at most [`BEFORE`]
/// characters before it and [`AFTER`] from it, and stops short of either only at an end of the
/// text; and that with no position it is the first [`HEAD`] characters, or the whole text where
/// that is shorter. The indices count characters, so every one of them is a character boundary of
/// the text `len` was counted from.
#[expect(
    clippy::implicit_saturating_sub,
    reason = "a comparison and a subtraction are what a Verus contract on this function reads"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r.0 <= r.1 <= len,
        r.1 - r.0 <= HEAD,
        at is None ==> r.0 == 0 && (r.1 == len || r.1 == HEAD),
        at is Some && at->Some_0 < len ==> r.0 <= at->Some_0 < r.1
            && at->Some_0 - r.0 <= BEFORE && r.1 - at->Some_0 <= AFTER
            && (r.0 == 0 || at->Some_0 - r.0 == BEFORE)
            && (r.1 == len || r.1 - at->Some_0 == AFTER),
        at is Some && at->Some_0 >= len ==> r.1 == len && len - r.0 <= BEFORE
            && (r.0 == 0 || len - r.0 == BEFORE),
))]
pub fn window(len: usize, at: Option<usize>) -> (usize, usize) {
    match at {
        None => {
            let end = if len < HEAD { len } else { HEAD };
            (0, end)
        }
        Some(at) => {
            let at = if at < len { at } else { len };
            let start = if at > BEFORE { at - BEFORE } else { 0 };
            let end = if len - at > AFTER { at + AFTER } else { len };
            (start, end)
        }
    }
}

/// The piece of `text` around the byte a parser reported at `line` and `column`, with `…` where
/// the text goes on past either end, and each control character and each character that draws as
/// nothing written out, so the piece is one line on a screen. A backtick is written out as
/// `‹U+0060›`, so the piece cannot close the backticks a sentence quotes it in.
pub fn around(text: &str, line: usize, column: usize) -> String {
    let len = text.chars().count();
    let (start, end) = window(len, char_index(text, line, column));
    let mut out = String::new();
    if start > 0 {
        out.push('\u{2026}');
    }
    for c in text.chars().skip(start).take(end.saturating_sub(start)) {
        if c.is_control() {
            out.extend(c.escape_debug());
        } else if c == '`' || crate::invisible::draws_as_nothing(c) {
            out.push_str(&crate::invisible::written_out(c));
        } else {
            out.push(c);
        }
    }
    if end < len {
        out.push('\u{2026}');
    }
    out
}

/// The index of the character holding the byte at `line` and `column` (both from 1, the column in
/// bytes), or `None` where the text has no such line or the byte is past its end.
///
/// A column of 0 is read as the line's first byte. A byte one past the last is the end of the
/// text, and gives the count of its characters.
fn char_index(text: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }
    let mut line_start = if line == 1 { Some(0) } else { None };
    let mut seen = 1usize;
    if line_start.is_none() {
        for (i, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                seen = seen.saturating_add(1);
                if seen == line {
                    line_start = Some(i.saturating_add(1));
                    break;
                }
            }
        }
    }
    let byte = byte_at(text.len(), line_start?, column)?;
    Some(
        text.char_indices()
            .take_while(|(i, c)| ends_by(*i, c.len_utf8(), byte))
            .count(),
    )
}

/// The byte a parser's `column` (from 1, in bytes) names on a line that starts at byte
/// `line_start` of a text `len` bytes long, or `None` where that is past the end. A column of 0 is
/// read as 1. The end itself, `len`, is a byte this answers with.
///
/// The contract states that an answer is never past `len`, that there is one exactly where the
/// line start and the column fit inside the text, and that it is their sum, so no sum wraps.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r is Some ==> r->Some_0 <= len,
        r is Some <==> line_start + (if column == 0 { 0 } else { column - 1 }) <= len,
        r is Some ==> r->Some_0 == line_start + (if column == 0 { 0 } else { column - 1 }),
))]
fn byte_at(len: usize, line_start: usize, column: usize) -> Option<usize> {
    let offset = if column > 0 { column - 1 } else { 0 };
    if line_start > len || offset > len - line_start {
        return None;
    }
    Some(line_start + offset)
}

/// Whether a character `width` bytes long that starts at byte `at` ends at or before byte `byte`,
/// asked without a sum that can wrap.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == (at + width <= byte),
))]
fn ends_by(at: usize, width: usize, byte: usize) -> bool {
    width <= byte && at <= byte - width
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window never reaches past the text and is never longer than the cap, wherever the
    /// position is. The regression it rejects is `at - BEFORE` or `at + AFTER` taken without a
    /// bound, which underflows at the start or reaches past the end.
    #[test]
    fn the_window_stays_inside_the_text_and_under_the_cap() {
        for len in [0, 1, 19, 20, 21, 59, 60, 61, 1000] {
            for at in [
                None,
                Some(0),
                Some(1),
                Some(39),
                Some(40),
                Some(41),
                Some(len),
                Some(len + 5),
                Some(usize::MAX),
            ] {
                let (start, end) = window(len, at);
                assert!(start <= end && end <= len, "{len} {at:?}: {start}..{end}");
                assert!(end - start <= HEAD, "{len} {at:?}: {start}..{end}");
            }
        }
        assert_eq!(window(1000, Some(500)), (460, 520));
        assert_eq!(window(1000, Some(10)), (0, 30));
        assert_eq!(window(1000, None), (0, 60));
        assert_eq!(window(30, None), (0, 30));
    }

    /// The piece is counted in characters, and the column is a count of bytes. The text puts many
    /// two-byte characters before the error and more text after it, so a column read as a
    /// character index lands past the error, and a byte slice at the column would split a
    /// character. The piece holds the bad key and every `é` whole.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_column_in_bytes_is_read_as_the_character_it_falls_in() {
        let text = format!(
            r#"{{"a":"{}","b">1,"c":"{}"}}"#,
            "é".repeat(50),
            "z".repeat(100)
        );
        let column = text.find('>').expect("the bad byte") + 1;
        let piece = around(&text, 1, column);
        assert!(piece.contains(r#""b">1,"#), "{piece}");
        assert!(piece.starts_with('\u{2026}'), "{piece}");
        assert!(
            piece.trim_start_matches('\u{2026}').starts_with('é'),
            "{piece}"
        );
    }

    /// A column inside a character, at the first byte, at the end and past it, and a line the text
    /// does not have, each give a piece of whole characters and no panic.
    #[test]
    fn a_position_at_either_end_or_outside_the_text_quotes_whole_characters() {
        let text = "😀a😀";
        for (line, column) in [
            (1, 0),
            (1, 1),
            (1, 2),
            (1, 3),
            (1, 9),
            (1, 10),
            (1, 99),
            (2, 1),
            (0, 1),
            (usize::MAX, usize::MAX),
        ] {
            let piece = around(text, line, column);
            assert_eq!(piece, text, "{line}:{column}");
        }
        assert_eq!(around("", 1, 1), "");
    }

    /// A text longer than the cap is cut on both sides of the position, each cut end marked.
    /// A position past the end quotes the first characters instead of nothing.
    #[test]
    fn a_long_text_is_cut_around_the_position() {
        let text = format!("{}X{}", "a".repeat(100_000), "b".repeat(100_000));
        let piece = around(&text, 1, 100_001);
        assert_eq!(
            piece,
            format!(
                "\u{2026}{}X{}\u{2026}",
                "a".repeat(BEFORE),
                "b".repeat(AFTER - 1)
            )
        );
        let head = around(&text, 1, text.len() + 2);
        assert_eq!(head, format!("{}\u{2026}", "a".repeat(HEAD)));
    }

    /// A position one past the last byte, where a parser stops at the end of the input, is read as
    /// the end, so the piece is the last characters and not the first. The position one further
    /// names no byte and gives the first. The regression it rejects is reading the end as a
    /// position past the text, which quotes the start of a long text and not where it stopped.
    #[test]
    fn a_position_at_the_end_quotes_the_end() {
        let text = format!("{}{}", "a".repeat(100), "b".repeat(BEFORE));
        assert_eq!(
            around(&text, 1, text.len() + 1),
            format!("\u{2026}{}", "b".repeat(BEFORE))
        );
        assert_eq!(
            around(&text, 1, text.len() + 2),
            format!("{}\u{2026}", "a".repeat(HEAD))
        );
        let on_a_second_line = format!("{{\n{}", "b".repeat(BEFORE));
        assert_eq!(
            around(&on_a_second_line, 2, BEFORE + 1),
            format!("\u{2026}{}", "b".repeat(BEFORE))
        );
    }

    /// The column arithmetic answers with a byte inside the text or nothing, for every line start
    /// and column, and never wraps. The regression it rejects is an unchecked `line_start + column
    /// - 1`, which wraps for a column near `usize::MAX`.
    #[test]
    fn the_byte_a_column_names_is_inside_the_text_or_none() {
        assert_eq!(byte_at(10, 0, 1), Some(0));
        assert_eq!(byte_at(10, 0, 0), Some(0));
        assert_eq!(byte_at(10, 4, 3), Some(6));
        assert_eq!(byte_at(10, 4, 7), Some(10));
        assert_eq!(byte_at(10, 4, 8), None);
        assert_eq!(byte_at(10, 11, 1), None);
        assert_eq!(byte_at(10, 1, usize::MAX), None);
        assert!(ends_by(3, 2, 5));
        assert!(!ends_by(3, 2, 4));
        assert!(!ends_by(usize::MAX, 4, 2));
        assert!(!ends_by(1, usize::MAX, usize::MAX));
    }

    /// A backtick in the text is written out, so the piece cannot end the backticks a sentence
    /// quotes it in. The regression it rejects is quoting a backtick as it is.
    #[test]
    fn a_backtick_is_written_out() {
        let piece = around("{\"a\":`x`>1}", 1, 9);
        assert!(!piece.contains('`'), "{piece}");
        assert!(piece.contains("U+0060"), "{piece}");
    }

    /// A newline, a control character and a line separator are written out, so the piece stays
    /// one line where it is drawn. The regression it rejects is quoting them as they are.
    #[test]
    fn a_newline_or_a_control_character_is_written_out() {
        let piece = around("{\"a\":\n\"x\u{1}y\u{2028}z\"", 2, 1);
        assert!(
            !piece.contains('\n') && !piece.contains('\u{1}') && !piece.contains('\u{2028}'),
            "{piece:?}"
        );
        assert!(
            piece.contains("\\n") && piece.contains("\\u{1}") && piece.contains("U+2028"),
            "{piece}"
        );
    }
}
