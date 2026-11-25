//! The colour codes in output, which are taken out and counted.
//!
//! A program that colours its output writes `ESC [ 36 m` before a word and `ESC [ 0 m` after it.
//! Once the escape is replaced with a picture by `printable` the rest of the code is ordinary text,
//! so a CI log that colours every token draws as `␛[0m␛[36m├␛[0m` and is hard to read. Where output
//! is drawn for a person to read and nobody is being asked to approve it, the complete codes are
//! taken out here first and counted, and the count is drawn under the text so it is still visible
//! that they were there (VIEW-30). The same removal is made on the text of a program's output
//! before it is added to the planner's context (RUN-21), where the codes spend tokens and say
//! nothing.
//!
//! This is a map from text to text and a count. It decides nothing about any effect. It does not
//! make text safe to draw: every escape it leaves in place, and every one it assembles by taking a
//! code out from between two halves of another, is still replaced by the caller's `printable`
//! where the text is drawn.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// The longest run of parameters taken as one colour code.
///
/// A code that sets two colours in a 24-bit palette is 33 parameter characters. A longer run is
/// left for `printable` to draw in full, so digits that are not a colour cannot be hidden by
/// writing them behind `ESC [` and ending them with `m`.
pub const MOST_PARAMETERS: usize = 64;

/// `text` with each complete colour code taken out, and how many that was.
///
/// A colour code is `ESC [`, up to [`MOST_PARAMETERS`] characters from `0-9`, `;` and `:`, and `m`.
/// Anything else that starts with an escape is left as it is: a cursor movement, an erase, a
/// private-mode setting (`ESC [ > 4 ; 2 m` has a parameter outside that set), a window title, a
/// code cut short by the end of the line. Those can move the cursor, repaint what was drawn or
/// ask the terminal something, so the person is shown every byte of them.
#[expect(
    clippy::string_slice,
    reason = "every index is the start of an ESC found by `find`, or one byte past the ASCII `m` that `code_end` found, so it is a char boundary"
)]
pub fn without(text: &str) -> (String, usize) {
    if !text.contains('\u{1b}') {
        return (text.to_string(), 0);
    }

    let bytes = text.as_bytes();
    let mut drawn = String::with_capacity(text.len());
    let mut dropped = 0;
    // The start of the text not yet copied, and where the next escape is looked for from.
    let mut kept = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find("\u{1b}[") {
        let start = from + found;
        match code_end(bytes, start, MOST_PARAMETERS) {
            Some(end) => {
                drawn.push_str(&text[kept..start]);
                kept = end + 1;
                from = kept;
                dropped += 1;
            }
            None => from = start + 1,
        }
    }
    drawn.push_str(&text[kept..]);
    (drawn, dropped)
}

/// The offset of the `m` that ends the colour code whose `ESC [` starts at `start` in `bytes`, or
/// `None` where what follows the `ESC [` is not a colour code.
///
/// The bytes after the `ESC [` are scanned until the first one that is not `0-9`, `;` or `:`, and
/// that one has to be `m`. At most `most + 1` bytes are looked at, so a run of more than `most`
/// parameters is not a code. Every offset it reads is inside `bytes`, and the one it returns is
/// the ASCII `m`, so the offset after it is a char boundary of the text the bytes came from. A
/// `start` with fewer than two bytes after it is `None`.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r is Some ==> start + 2 <= r->Some_0 < bytes.len() && bytes@[r->Some_0 as int] == b'm',
))]
fn code_end(bytes: &[u8], start: usize, most: usize) -> Option<usize> {
    // [`without`] passes the offset of an `ESC [` it found, but Verus does not read `without`, so
    // this checks the offset rather than requiring it of the caller.
    let too_short = start > bytes.len() || bytes.len() - start < 2;
    if too_short {
        return None;
    }
    let parameters = start + 2;
    let room = bytes.len() - parameters;
    let mut taken = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            parameters == start + 2,
            room == bytes.len() - parameters,
            taken <= room,
        decreases room - taken,
    ))]
    while taken < room && taken <= most {
        let at = parameters + taken;
        let byte = bytes[at];
        if byte == b'm' {
            return Some(at);
        }
        if !matches!(byte, b'0'..=b'9' | b';' | b':') {
            return None;
        }
        taken += 1;
    }
    None
}

/// The sentence a result puts after text that had `dropped` colour codes taken out, in the driver's
/// own words with the number and nothing else. Empty where none were, so the text of output with no
/// colour code is the text and nothing is said about them (R5).
pub fn planner_note(dropped: usize) -> String {
    match dropped {
        0 => String::new(),
        1 => "\n\n1 colour code was taken out of this output.".to_string(),
        many => format!("\n\n{many} colour codes were taken out of this output."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `code_end` checks the offset it is given rather than trusting [`without`] to pass one with
    /// `ESC [` after it, so an offset at or past the end of the bytes is no code and reads nothing
    /// out of bounds.
    #[test]
    fn an_offset_without_two_bytes_after_it_ends_no_code() {
        let bytes = b"ab\x1b[";

        assert_eq!(code_end(bytes, 2, MOST_PARAMETERS), None);
        assert_eq!(code_end(bytes, 3, MOST_PARAMETERS), None);
        assert_eq!(code_end(bytes, 4, MOST_PARAMETERS), None);
        assert_eq!(code_end(bytes, 9, MOST_PARAMETERS), None);
    }

    /// The line from a CI log that colours the glyph of a tree and nothing around it.
    #[test]
    fn the_codes_of_a_coloured_line_are_taken_out_and_counted() {
        let (drawn, dropped) = without("\u{1b}[0m\u{1b}[36m├\u{1b}[0m Announcement: a link");

        assert_eq!(drawn, "├ Announcement: a link");
        assert_eq!(dropped, 3);
    }

    /// A reset with no parameters, the sub-parameters of an underline style and a 24-bit colour are
    /// all colour codes, so each is taken out whole and none leaves its tail behind as text.
    #[test]
    fn every_spelling_of_a_colour_code_is_taken_out_whole() {
        for code in [
            "\u{1b}[m",
            "\u{1b}[1m",
            "\u{1b}[1;31m",
            "\u{1b}[4:3m",
            "\u{1b}[38;2;255;128;0;48;2;0;0;0m",
            "\u{1b}[38:2::255:128:0m",
        ] {
            let (drawn, dropped) = without(&format!("a{code}b"));
            assert_eq!((drawn.as_str(), dropped), ("ab", 1), "{code:?}");
        }
    }

    /// Text with no escape in it comes back as it went in, so ordinary output is not touched.
    #[test]
    fn text_without_an_escape_is_not_changed() {
        assert_eq!(
            without("cargo test [m] 1;2m"),
            ("cargo test [m] 1;2m".to_string(), 0)
        );
    }

    /// Only a colour code is taken out. Each of these can do something other than colour text, so
    /// each stays whole for `printable` to draw as pictures the person can read.
    #[test]
    fn a_sequence_that_is_not_a_colour_code_is_left_in_place() {
        for sequence in [
            "\u{1b}[2J",
            "\u{1b}[A",
            "\u{1b}[2K",
            "\u{1b}[?25l",
            "\u{1b}[>4;2m",
            "\u{1b}[=1m",
            "\u{1b}[31;1H",
            "\u{1b}[31",
            "\u{1b}[31;",
            "\u{1b}]0;title\u{7}",
            "\u{1b}m",
            "\u{1b}",
        ] {
            let text = format!("a{sequence}b");
            assert_eq!(without(&text), (text.clone(), 0), "{sequence:?}");
        }
    }

    /// A code with more parameters than any colour has is left for `printable`, so a long run of
    /// digits cannot be hidden by wrapping it in the two ends of a colour code.
    #[test]
    fn a_run_of_parameters_longer_than_a_colour_has_is_left_in_place() {
        let longest = format!("\u{1b}[{}m", "1".repeat(MOST_PARAMETERS));
        let too_long = format!("\u{1b}[{}m", "1".repeat(MOST_PARAMETERS + 1));

        assert_eq!(without(&longest), (String::new(), 1));
        assert_eq!(without(&too_long), (too_long.clone(), 0));
    }

    /// Taking a code out from between the two halves of another leaves the halves next to each
    /// other, and they read as a code of their own. The escape at their head is still in the
    /// text, which is why the caller's `printable` has to follow where text is drawn.
    #[test]
    fn taking_a_code_out_can_join_two_halves_and_leaves_their_escape_for_printable() {
        let (drawn, dropped) = without("a\u{1b}[3\u{1b}[0m1mb");

        assert_eq!((drawn.as_str(), dropped), ("a\u{1b}[31mb", 1));
        assert!(drawn.contains('\u{1b}'));
    }

    /// Several codes in a row, and codes beside text that is not ASCII, are each taken out without
    /// cutting a character in half.
    #[test]
    fn codes_beside_wide_characters_are_taken_out_cleanly() {
        let (drawn, dropped) = without("\u{1b}[31m日本\u{1b}[0m\u{1b}[1m語é\u{1b}[0m");

        assert_eq!((drawn.as_str(), dropped), ("日本語é", 4));
    }
}
