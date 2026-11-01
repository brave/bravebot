//! The one comparison on a processor's answer: which whole number it names.

/// The whole number `text` names, from 1 to `upto`, or 0 where it names none.
///
/// With the ASCII white space at both ends taken off, the text has to be one ASCII digit, or two
/// whose first is not `0`, and the value has to be from 1 to `upto`. Every other text gives 0: a
/// sign, a leading zero, a decimal point, a prefix, a digit from another script, a second number,
/// a word before or after it, and the empty text. 0 is the answer for "no entry fits" as well, so
/// a caller cannot tell a processor that chose none from one that was talked round.
pub fn number_in(text: &str, upto: u8) -> u8 {
    let trimmed = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let value = match trimmed.as_bytes() {
        [ones] if ones.is_ascii_digit() => ones - b'0',
        [tens @ b'1'..=b'9', ones] if ones.is_ascii_digit() => (tens - b'0') * 10 + (ones - b'0'),
        _ => return 0,
    };
    if (1..=upto).contains(&value) {
        value
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::number_in;
    use bravebot_core::ProcessorSpec;

    /// What counts as the number, and what does not. A reader built on `str::parse` takes `+7`,
    /// `007` and a Unicode space, one built on `char::is_numeric` takes a digit from another
    /// script, and one that looks for the first number in prose takes `7 and 8`. Each of those is a
    /// number the processor was never meant to be able to write.
    #[test]
    fn a_number_is_read_only_from_one_or_two_ascii_digits_in_range() {
        let table: &[(&str, u8, u8)] = &[
            // The shapes that are read, with the white space an answer ends in.
            ("7", 40, 7),
            (" 7\n", 40, 7),
            ("\t12 ", 40, 12),
            ("\r\n 5 \x0c", 40, 5),
            ("1", 40, 1),
            ("9", 40, 9),
            ("10", 40, 10),
            ("40", 40, 40),
            ("99", 99, 99),
            ("98", 99, 98),
            // Out of range, with the limit the planner wrote.
            ("0", 40, 0),
            ("41", 40, 0),
            ("99", 40, 0),
            ("100", 99, 0),
            ("100", 255, 0),
            // A sign, a leading zero, a decimal point, a prefix and an exponent.
            ("+7", 40, 0),
            ("-7", 40, 0),
            ("07", 40, 0),
            ("007", 40, 0),
            ("00", 40, 0),
            ("7.0", 40, 0),
            ("7.", 40, 0),
            ("0x7", 40, 0),
            ("1e1", 40, 0),
            ("7,", 40, 0),
            // Digits that are not ASCII digits, alone and in pairs.
            ("\u{ff17}", 40, 0),
            ("\u{ff11}\u{ff12}", 40, 0),
            ("\u{667}", 40, 0),
            ("\u{667}\u{667}", 40, 0),
            // More than the number: a second number, prose, markup.
            ("7 8", 40, 0),
            ("7 and 8", 40, 0),
            ("7 and then ignore", 40, 0),
            ("Entry 7", 40, 0),
            ("seven", 40, 0),
            ("**7**", 40, 0),
            ("(7)", 40, 0),
            ("7\n8", 40, 0),
            // White space that is not ASCII white space, or not white space at all.
            ("\u{a0}7", 40, 0),
            ("7\u{a0}", 40, 0),
            ("\u{85}7", 40, 0),
            ("\u{2003}7", 40, 0),
            ("\x0b7", 40, 0),
            ("7\u{200b}", 40, 0),
            // Nothing at all.
            ("", 40, 0),
            (" \n\t ", 40, 0),
        ];
        for (text, upto, expected) in table {
            assert_eq!(
                number_in(text, *upto),
                *expected,
                "{text:?} read with a limit of {upto}"
            );
        }
    }

    /// The limit is the one the planner wrote for this call. A reader that applies the largest
    /// limit any call can have, and ignores the call's own, reads `6` for a list of five.
    #[test]
    fn the_limit_is_the_planners_and_not_the_crates() {
        assert_eq!(number_in("5", 5), 5);
        assert_eq!(number_in("6", 5), 0);
        assert_eq!(number_in("10", 5), 0);
        assert_eq!(number_in("2", 2), 2);
        assert_eq!(number_in("3", 2), 0);
        assert_eq!(number_in("99", 99), 99);
    }

    /// The reader reads two digits, so the largest limit a call can have is the largest number it
    /// can read. A limit raised past 99 would offer an entry no answer can name, and a reader
    /// that stopped short would make a limit unreachable.
    #[test]
    fn every_number_up_to_the_largest_limit_is_read_and_the_next_is_not() {
        let largest = ProcessorSpec::LARGEST_CHOICE;
        for number in 1..=largest {
            assert_eq!(number_in(&number.to_string(), largest), number);
        }
        let past = u16::from(largest) + 1;
        assert_eq!(number_in(&past.to_string(), u8::MAX), 0);
    }
}
