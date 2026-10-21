//! Characters that draw as nothing, and the spelling that lets a person see them.
//!
//! Unicode's default-ignorable characters have no glyph in a terminal or a browser, so text holding
//! them reads the same with or without them while a model reads them as they are. The tag block
//! writes ASCII in characters that draw as nothing, and a model can read one as the letter it
//! stands for. Zero-width characters, bidirectional controls and variation selectors carry bits
//! the same way.
//!
//! [`draws_as_nothing`] is the one list of those characters. The screens that put content in front
//! of a person to decide about, the MCP tool list and a vouch prompt's file name all take their
//! membership from it, so a character added here is hidden from none of them.
//!
//! [`spelled`] is a map from characters to characters. It reads no meaning from the text and
//! decides nothing about any effect. Its only output besides the spelled text is a count, which
//! goes to the person's screen.

/// Whether `c` changes how the text around it reads without being seen: Unicode's default
/// ignorable characters, which hold the bidirectional controls, the zero-width characters, the
/// variation selectors and the tags a model reads as letters, and the separators and annotation
/// marks that break or cover a line without a newline.
pub fn draws_as_nothing(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFFB}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// Text as a person is shown it, and how many characters in it draw as nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelled {
    /// The text with each character [`draws_as_nothing`] holds replaced by a visible token.
    pub text: String,
    /// How many characters were replaced.
    pub hidden: usize,
}

/// `text` with every character that draws as nothing written out, and how many that was.
///
/// Each becomes `‹U+XXXX›`, its code point in hex. A tag character that stands for a printable
/// ASCII character (U+E0020 to U+E007E) also names it, `‹U+E0041 A›`, because that letter is what
/// a model reads. A space is written `␣`. Text with none of them comes back as it went in.
///
/// Line breaks are never replaced, so the text has the lines it had.
pub fn spelled(text: String) -> Spelled {
    if !text.chars().any(draws_as_nothing) {
        return Spelled { text, hidden: 0 };
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut hidden = 0;
    for c in text.chars() {
        if !draws_as_nothing(c) {
            out.push(c);
            continue;
        }
        hidden += 1;
        let code = c as u32;
        out.push('\u{2039}');
        out.push_str(&format!("U+{code:04X}"));
        if let Some(stands_for) = tag_letter(code) {
            out.push(' ');
            out.push(stands_for);
        }
        out.push('\u{203A}');
    }
    Spelled { text: out, hidden }
}

/// The printable ASCII character a tag character stands for, with a space as `␣`.
fn tag_letter(code: u32) -> Option<char> {
    match code {
        0xE0020 => Some('\u{2423}'),
        0xE0021..=0xE007E => char::from_u32(code - 0xE0000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(message: &str) -> String {
        message
            .chars()
            .map(|c| char::from_u32(0xE0000 + c as u32).unwrap())
            .collect()
    }

    /// A character that draws as nothing is written out as its code point, and a tag character
    /// also as the letter it stands for.
    #[test]
    fn each_character_that_draws_as_nothing_is_written_out() {
        for (hidden, written) in [
            ("\u{200B}", "\u{2039}U+200B\u{203A}"),
            ("\u{202E}", "\u{2039}U+202E\u{203A}"),
            ("\u{FE0F}", "\u{2039}U+FE0F\u{203A}"),
            ("\u{FEFF}", "\u{2039}U+FEFF\u{203A}"),
            ("\u{2028}", "\u{2039}U+2028\u{203A}"),
            ("\u{E0041}", "\u{2039}U+E0041 A\u{203A}"),
            ("\u{E0020}", "\u{2039}U+E0020 \u{2423}\u{203A}"),
            ("\u{E0001}", "\u{2039}U+E0001\u{203A}"),
            ("\u{E007F}", "\u{2039}U+E007F\u{203A}"),
        ] {
            let spelled = spelled(format!("a{hidden}b"));
            assert_eq!(spelled.text, format!("a{written}b"), "{hidden:?}");
            assert_eq!(spelled.hidden, 1, "{hidden:?}");
        }
    }

    /// An instruction written in tag characters is readable as the letters it is.
    #[test]
    fn a_message_written_in_tag_characters_is_spelled_letter_by_letter() {
        let spelled = spelled(format!("fine{}", tags("ok")));
        assert_eq!(
            spelled.text,
            "fine\u{2039}U+E006F o\u{203A}\u{2039}U+E006B k\u{203A}"
        );
        assert_eq!(spelled.hidden, 2);
    }

    /// Text with nothing hidden in it, other languages and emoji drawn with a glyph included, is
    /// the text it was, with nothing counted.
    #[test]
    fn text_with_no_hidden_character_is_left_as_it_is() {
        for text in [
            "",
            "plain\nlines\r\nof text\twith a tab",
            "caf\u{e9} \u{4e2d}\u{6587} \u{1F600}",
            "\u{a0}\u{3000}\u{2003}spaces that draw as blank",
        ] {
            let spelled = spelled(text.to_string());
            assert_eq!(spelled.text, text);
            assert_eq!(spelled.hidden, 0);
        }
    }

    /// Line breaks stay where they were, so a prompt that counts the lines of the text counts the
    /// same lines of its spelling.
    #[test]
    fn spelling_keeps_the_lines_the_text_had() {
        let text = format!("one\u{200B}\ntwo{}\r\nthree\n", tags("x"));
        let lines = text.lines().count();
        let spelled = spelled(text);
        assert_eq!(spelled.text.lines().count(), lines);
        assert_eq!(spelled.hidden, 2);
    }

    /// The set is the default-ignorable code points of Unicode and the separators a line is broken
    /// with, so a block it leaves out is a block a model reads and a person does not see. Each end
    /// of each range is named.
    #[test]
    fn the_list_holds_every_block_that_draws_as_nothing_and_no_ordinary_character() {
        for c in [
            '\u{00AD}',
            '\u{034F}',
            '\u{061C}',
            '\u{115F}',
            '\u{1160}',
            '\u{17B4}',
            '\u{17B5}',
            '\u{180B}',
            '\u{180F}',
            '\u{200B}',
            '\u{200F}',
            '\u{2028}',
            '\u{202E}',
            '\u{2060}',
            '\u{206F}',
            '\u{3164}',
            '\u{FE00}',
            '\u{FE0F}',
            '\u{FEFF}',
            '\u{FFA0}',
            '\u{FFF0}',
            '\u{FFFB}',
            '\u{1BCA0}',
            '\u{1BCA3}',
            '\u{1D173}',
            '\u{1D17A}',
            '\u{E0000}',
            '\u{E007F}',
            '\u{E0100}',
            '\u{E01EF}',
            '\u{E0FFF}',
        ] {
            assert!(draws_as_nothing(c), "{:?}", c);
        }
        for c in [
            'a',
            ' ',
            '\n',
            '\t',
            '\u{a0}',
            '\u{ac}',
            '\u{ae}',
            '\u{2027}',
            '\u{202F}',
            '\u{1D172}',
            '\u{1D17B}',
            '\u{FFFC}',
            '\u{FFFD}',
            '\u{E1000}',
            '\u{1F600}',
        ] {
            assert!(!draws_as_nothing(c), "{:?}", c);
        }
    }
}
