//! Whether the start of a file says it is text.
//!
//! The agent refuses to read a binary file into a turn, and the desktop's file helper checks a
//! named file before the agent sees it. Both ask this crate, so a file the helper lets through is a
//! file the turn reads rather than refuses.
#![forbid(unsafe_code)]

/// Bytes inspected when deciding whether a file is text.
pub const SNIFF_BYTES: usize = 8_192;

/// Whether a byte run looks like binary rather than text.
///
/// A null byte is decisive, since no text file contains one. Beyond that, a high proportion of
/// control characters means the same thing without needing a file-type list to be kept up
/// to date. Only the first [`SNIFF_BYTES`] are inspected, since the answer does not improve by
/// reading more.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF_BYTES)];
    if head.is_empty() {
        return false;
    }
    if head.contains(&0) {
        return true;
    }
    // Tab, newline, carriage return and form feed are expected in text; other low bytes
    // are not.
    let control = head
        .iter()
        .filter(|b| **b < 32 && !matches!(**b, 9 | 10 | 12 | 13))
        .count();
    control * 100 / head.len() > 30
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run of `total` bytes, `control` of them a control character that text does not use.
    fn mixed(control: usize, total: usize) -> Vec<u8> {
        [vec![1u8; control], vec![b'a'; total - control]].concat()
    }

    #[test]
    fn a_null_byte_in_the_sniffed_head_is_binary() {
        let mut bytes = vec![b'a'; SNIFF_BYTES - 1];
        bytes.push(0);
        assert!(looks_binary(&bytes));
        assert!(looks_binary(b"\0"));
    }

    #[test]
    fn only_the_sniffed_head_is_judged() {
        let mut bytes = vec![b'a'; SNIFF_BYTES];
        bytes.push(0);
        assert!(!looks_binary(&bytes));
    }

    #[test]
    fn thirty_percent_control_characters_is_text_and_thirty_one_is_not() {
        assert!(!looks_binary(&mixed(30, 100)));
        assert!(looks_binary(&mixed(31, 100)));
    }

    #[test]
    fn tab_newline_form_feed_and_carriage_return_are_text() {
        assert!(!looks_binary(&b"\t\n\x0c\r".repeat(50)));
    }

    #[test]
    fn an_empty_file_is_not_binary() {
        assert!(!looks_binary(b""));
    }
}
