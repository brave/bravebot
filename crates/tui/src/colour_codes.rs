//! The colour codes in output, which are counted instead of drawn.
//!
//! A program that colours its output writes `ESC [ 36 m` before a word and `ESC [ 0 m` after it.
//! Once the escape is replaced with a picture by `printable` the rest of the code is ordinary text,
//! so a CI log that colours every token draws as `␛[0m␛[36m├␛[0m` and is hard to read. Where output
//! is drawn for a person to read and nobody is being asked to approve it, the complete codes are
//! taken out here first and counted, and the count is drawn under the text so it is still visible
//! that they were there (VIEW-30).
//!
//! This is a map from text to text and a count, which goes to the person's screen. It decides
//! nothing about any effect. It does not make text safe to draw: every escape it leaves in place,
//! and every one it assembles by taking a code out from between two halves of another, is still
//! replaced by the caller's `printable`, which always runs after this.

use bravebot_i18n::t;

/// The longest run of parameters taken as one colour code.
///
/// A code that sets two colours in a 24-bit palette is 33 parameter characters. A longer run is
/// left for `printable` to draw in full, so digits that are not a colour cannot be hidden by
/// writing them behind `ESC [` and ending them with `m`.
const MOST_PARAMETERS: usize = 64;

/// `text` with each complete colour code taken out, and how many that was.
///
/// A colour code is `ESC [`, up to [`MOST_PARAMETERS`] characters from `0-9`, `;` and `:`, and `m`.
/// Anything else that starts with an escape is left as it is: a cursor movement, an erase, a
/// private-mode setting (`ESC [ > 4 ; 2 m` has a parameter outside that set), a window title, a
/// code cut short by the end of the line. Those can move the cursor, repaint what was drawn or
/// ask the terminal something, so the person is shown every byte of them.
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
        let parameters = start + 2;
        let ends = bytes[parameters..]
            .iter()
            .take(MOST_PARAMETERS + 1)
            .position(|byte| !matches!(byte, b'0'..=b'9' | b';' | b':'))
            .map(|length| parameters + length)
            .filter(|&end| bytes[end] == b'm');
        match ends {
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

/// The sentence drawn under text that had colour codes taken out, or nothing where none were.
pub fn note(dropped: usize) -> Option<String> {
    (dropped > 0).then(|| t!(colour_codes_not_drawn, count = dropped))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    /// text, which is why the caller's `printable` has to follow.
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

    #[test]
    fn nothing_is_said_where_nothing_was_taken_out() {
        assert_eq!(note(0), None);
        assert!(note(1).expect("one").contains('1'));
        assert!(note(14).expect("many").contains("14"));
    }
}
