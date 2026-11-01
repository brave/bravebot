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
//! membership from it, so a character added here is hidden from none of them. A file name is also
//! spelled for [`draws_as_blank`], which text is not.
//!
//! [`spelled`] is a map from characters to characters. It reads no meaning from the text and
//! decides nothing about any effect. Its only output besides the spelled text is a count, which
//! goes to the person's screen. [`spelled_code`] is the same map for code a person reads character
//! by character, and also remembers how many marks the last visible character carries.

#[rustfmt::skip]
mod marks;

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

/// Whether `c` takes up room and shows no mark although Unicode does not count it as white space:
/// the blank braille pattern.
///
/// A file name made of it draws as spaces, and a name that holds one reads as another. It is not
/// in [`draws_as_nothing`], so [`spelled`] leaves it in text, where a blank cell hides nothing a
/// person reads and braille drawings are written with it.
pub fn draws_as_blank(c: char) -> bool {
    c == '\u{2800}'
}

/// Text as a person is shown it, and how many characters in it draw as nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelled {
    /// The text with each character [`draws_as_nothing`] holds replaced by a visible token.
    pub text: String,
    /// How many characters were replaced. [`spelled_code`] does not count the `‹` it writes out.
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
        write_out(&mut out, c);
    }
    Spelled { text: out, hidden }
}

/// How many combining marks one visible character keeps in [`spelled_code`]. The accents, vowel
/// signs and tone marks of ordinary text come to a few, and each mark beyond them is written out.
const MARKS_PER_CHARACTER: usize = 3;

/// The character a written-out character opens with. [`spelled_code`] writes it out too, so every
/// one in its output opens a character it wrote out.
const OPENING: char = '\u{2039}';

/// Whether `c` is a space other than the plain one (Unicode's `Zs`): the no-break space, the Ogham
/// space mark, the spaces of fixed width, the narrow no-break space, the medium mathematical space
/// and the ideographic space. Each draws as a blank, and none is the character a program reads as
/// a space.
fn draws_as_another_space(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// Whether `c` is a format character (Unicode's `Cf`): one that changes how the text around it is
/// drawn or joined and has no glyph of its own. [`draws_as_nothing`] holds most of them. The rest
/// are the marks Arabic and Syriac put before a number or at the end of a verse, and the controls
/// of Kaithi and of Egyptian hieroglyphs.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

/// Whether `c` is a private use character (Unicode's `Co`): it has no meaning of its own and draws
/// as whatever the font of the reader gives it.
fn is_private_use(c: char) -> bool {
    matches!(
        c,
        '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}'
    )
}

/// Whether `c` is a mark that draws on the character before it (Unicode's `Mn` and `Me`).
fn is_stacking_mark(c: char) -> bool {
    let code = c as u32;
    marks::MARKS
        .binary_search_by(|(first, last)| {
            if code < *first {
                std::cmp::Ordering::Greater
            } else if code > *last {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Whether a person reading code cannot tell `c` from another character or from nothing: a control
/// character other than the line feed, a character [`draws_as_nothing`] holds, the blank braille
/// pattern, a format character, a space that is not the plain one and a private use character.
fn unseen_in_code(c: char) -> bool {
    (c != '\n' && c.is_control())
        || draws_as_nothing(c)
        || draws_as_blank(c)
        || is_format(c)
        || draws_as_another_space(c)
        || is_private_use(c)
}

/// `text`, a string a person reads character by character, with each character they could not
/// tell from another or from nothing written out as [`spelled`] writes it, and how many that was.
///
/// This is wider than [`spelled`], because a program changes what it does with a character that
/// reads as a space or as nothing. Also written out are a control character, the blank braille
/// pattern, a format character, a space that is not the plain one, a private use character, and a
/// combining mark that has no visible character before it or is past the third on one. A line feed
/// is kept. The character a written-out one opens with is written out too, as `‹U+2039›`, and is
/// not counted, so text that holds the written-out form of a character reads as the characters it
/// holds.
pub fn spelled_code(text: &str) -> Spelled {
    let mut out = String::with_capacity(text.len() + 16);
    let mut hidden = 0;
    // Whether the last character put in was a visible one for a mark to draw on, and how many
    // marks it has.
    let mut based = false;
    let mut marks = 0;
    for c in text.chars() {
        if c == OPENING {
            write_out(&mut out, c);
            based = false;
            marks = 0;
        } else if unseen_in_code(c) {
            hidden += 1;
            write_out(&mut out, c);
            based = false;
            marks = 0;
        } else if is_stacking_mark(c) {
            if based && marks < MARKS_PER_CHARACTER {
                out.push(c);
                marks += 1;
            } else {
                hidden += 1;
                write_out(&mut out, c);
            }
        } else {
            out.push(c);
            based = c != ' ' && c != '\n';
            marks = 0;
        }
    }
    Spelled { text: out, hidden }
}

/// `c` as `‹U+XXXX›`, with the letter a tag character stands for.
fn write_out(out: &mut String, c: char) {
    let code = c as u32;
    out.push(OPENING);
    out.push_str(&format!("U+{code:04X}"));
    if let Some(stands_for) = tag_letter(code) {
        out.push(' ');
        out.push(stands_for);
    }
    out.push('\u{203A}');
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

    /// The first and last code point of each range under `key` in `tests/draws-as-nothing.json`,
    /// which holds one range to a line.
    fn listed(key: &str) -> Vec<(u32, u32)> {
        let opening = format!("\"{key}\": [");
        include_str!("../tests/draws-as-nothing.json")
            .lines()
            .skip_while(|line| line.trim() != opening)
            .skip(1)
            .take_while(|line| !line.trim().starts_with(']'))
            .map(|line| {
                let range = line.trim().trim_end_matches(',');
                let (first, last) = range.trim_matches(['[', ']']).split_once(',').unwrap();
                (first.trim().parse().unwrap(), last.trim().parse().unwrap())
            })
            .collect()
    }

    /// The set is the default-ignorable code points of Unicode and the separators a line is broken
    /// with, so a block it leaves out is a block a model reads and a person does not see. Every
    /// code point is held to the ranges of `tests/draws-as-nothing.json`, which
    /// `extension/tests/unseen.test.mjs` holds both copies of `shown.js` to, so the confirmation
    /// window marks what this function names. The characters the list leaves unmarked, the object
    /// replacement character among them, are left as they are by [`spelled`] and [`spelled_code`].
    #[test]
    fn the_list_holds_every_block_that_draws_as_nothing_and_no_ordinary_character() {
        let ranges = listed("draws_as_nothing");
        assert!(ranges.len() > 10, "{} ranges were read", ranges.len());
        for point in 0..=0x10_FFFF_u32 {
            let Some(c) = char::from_u32(point) else {
                continue;
            };
            let inside = ranges
                .iter()
                .any(|(first, last)| (*first..=*last).contains(&point));
            assert_eq!(draws_as_nothing(c), inside, "U+{point:04X}");
        }
        let unmarked = listed("left_unmarked");
        assert!(!unmarked.is_empty(), "no character is left unmarked");
        for (first, last) in unmarked {
            for c in (first..=last).filter_map(char::from_u32) {
                let text = format!("a{c}b");
                assert_eq!(spelled(text.clone()).text, text, "{c:?} in text");
                assert_eq!(spelled_code(&text).text, text, "{c:?} in code");
            }
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

    /// The blank braille pattern is the one character that draws as an empty cell without being
    /// white space or default ignorable, and the patterns with a dot are marks. Text is not spelled
    /// for it, since a braille drawing is written with it.
    #[test]
    fn only_the_blank_braille_pattern_is_a_cell_with_no_mark() {
        assert!(draws_as_blank('\u{2800}'));
        for c in [
            'a', ' ', '\u{a0}', '\u{27ff}', '\u{2801}', '\u{28ff}', '\u{3000}',
        ] {
            assert!(!draws_as_blank(c), "{c:?}");
        }
        let text = "\u{2800}\u{2801}\u{2800}".to_string();
        assert_eq!(spelled(text.clone()).text, text);
        assert_eq!(spelled(text).hidden, 0);
    }
    /// The written-out form of `c`, as every drawing of code spells it.
    fn written(c: char) -> String {
        format!("\u{2039}U+{:04X}\u{203A}", c as u32)
    }

    /// Code is read character by character, and a program treats a space that is not the plain
    /// one, a format character, a private use character and the blank braille pattern as what
    /// they are while they read as a blank or as nothing. Each is written out where it stands, and
    /// counted, as a character that draws as nothing is. A plain space, a tab written as a control,
    /// letters of other scripts, an emoji, the object replacement character (which a terminal draws
    /// as a box) and a line feed are the cases that decide whether the list is wider than it should
    /// be. A list that stayed [`spelled`]'s would leave the first group unmarked.
    #[test]
    fn code_has_every_character_it_cannot_be_read_by_written_out() {
        for c in [
            '\u{a0}',
            '\u{1680}',
            '\u{2000}',
            '\u{2003}',
            '\u{200a}',
            '\u{202f}',
            '\u{205f}',
            '\u{3000}',
            '\u{600}',
            '\u{605}',
            '\u{6dd}',
            '\u{70f}',
            '\u{890}',
            '\u{8e2}',
            '\u{110bd}',
            '\u{13430}',
            '\u{1343f}',
            '\u{2800}',
            '\u{e000}',
            '\u{f8ff}',
            '\u{f0000}',
            '\u{ffffd}',
            '\u{100000}',
            '\u{10fffd}',
            '\u{9}',
            '\u{d}',
            '\u{1b}',
            '\u{7f}',
            '\u{85}',
            '\u{200b}',
            '\u{202e}',
            '\u{2066}',
            '\u{feff}',
            '\u{fe0f}',
            '\u{e0100}',
            '\u{2028}',
        ] {
            let spelled = spelled_code(&format!("a{c}b"));
            assert_eq!(spelled.text, format!("a{}b", written(c)), "{c:?}");
            assert_eq!(spelled.hidden, 1, "{c:?}");
        }
        for text in [
            "plain words and \"quotes\" with ()[]{} and ; and 1+1",
            "caf\u{e9} \u{4e2d}\u{6587} \u{1f600} \u{5d0}\u{5d1} \u{627}",
            "\u{fffc}\u{fffd}\u{27ff}\u{2801}\u{1d172}",
            "one\ntwo\n",
        ] {
            let spelled = spelled_code(text);
            assert_eq!(spelled.text, text);
            assert_eq!(spelled.hidden, 0, "{text:?}");
        }
        assert_eq!(spelled_code("a\nb").text, "a\nb");
    }

    /// A character that draws as nothing is written out for code as it is for text, with the letter
    /// a tag character stands for, and a mix of everything is counted once per character.
    #[test]
    fn code_spells_what_text_spells_the_same_way() {
        for text in [
            format!("fine{}", tags("ok")),
            "a\u{200b}b\u{202e}c\u{e0041}".to_string(),
            "\u{e0020}\u{e0001}\u{e007f}".to_string(),
        ] {
            let text_spelled = spelled(text.clone());
            let code_spelled = spelled_code(&text);
            assert_eq!(code_spelled, text_spelled);
        }
    }

    /// An accent, a vowel sign and a tone mark are ordinary, so up to three marks on a letter are
    /// shown as they are. A fourth and the rest are written out, so a stack of them cannot draw
    /// over the rows around it. A mark with no letter to draw on (at the start of a line, after a
    /// space, after a character that was written out) is written out whatever its place. An
    /// accented, Vietnamese, Thai and Arabic word keeps its marks. The mark after a plain letter
    /// that is the fourth of a stack is the one a counter that never reset, or one that reset at a
    /// space and not at a letter, would get wrong.
    #[test]
    fn marks_past_the_third_on_a_letter_and_marks_on_nothing_are_written_out() {
        let marks = |n: usize, mark: char| mark.to_string().repeat(n);
        let three = format!("e{}", marks(3, '\u{301}'));
        assert_eq!(spelled_code(&three).text, three);
        assert_eq!(spelled_code(&three).hidden, 0);

        let four = format!("e{}", marks(4, '\u{301}'));
        let spelled = spelled_code(&four);
        assert_eq!(
            spelled.text,
            format!("e{}{}", marks(3, '\u{301}'), written('\u{301}'))
        );
        assert_eq!(spelled.hidden, 1);

        let stack = format!("x{}y", marks(200, '\u{301}'));
        let spelled = spelled_code(&stack);
        assert_eq!(spelled.hidden, 197);
        assert_eq!(
            spelled.text,
            format!(
                "x{}{}y",
                marks(3, '\u{301}'),
                written('\u{301}').repeat(197)
            )
        );

        // The count starts again at the next letter, and a letter that is not preceded by marks
        // is shown whole.
        let two_letters = format!("e{}e{}", marks(3, '\u{301}'), marks(3, '\u{300}'));
        assert_eq!(spelled_code(&two_letters).text, two_letters);

        // Marks with nothing to draw on.
        for (text, expected) in [
            ("\u{301}a", format!("{}a", written('\u{301}'))),
            ("a \u{301}", format!("a {}", written('\u{301}'))),
            (
                "a\u{200b}\u{301}",
                format!("a{}{}", written('\u{200b}'), written('\u{301}')),
            ),
            ("a\n\u{301}", format!("a\n{}", written('\u{301}'))),
            (
                "a\u{2039}\u{301}",
                format!("a{}{}", written('\u{2039}'), written('\u{301}')),
            ),
        ] {
            assert_eq!(spelled_code(text).text, expected, "{text:?}");
        }

        // An enclosing mark is one too, and the fourth mark of any kind goes the same way.
        let enclosed = "a\u{20dd}\u{20e3}\u{301}\u{302}".to_string();
        let spelled = spelled_code(&enclosed);
        assert_eq!(
            spelled.text,
            format!("a\u{20dd}\u{20e3}\u{301}{}", written('\u{302}'))
        );

        // Ordinary text in scripts that stack marks.
        for word in [
            "cafe\u{301} \u{4e2d}",
            "Vi\u{1ec7}t Nam, to\u{1ea1}\u{300}",
            "\u{e01}\u{e49}\u{e34}\u{e38} \u{e17}\u{e35}\u{e48}",
            "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}\u{64e}\u{651}\u{64b}",
            "\u{915}\u{93f} \u{939}\u{93f}\u{902}\u{926}\u{940}",
        ] {
            assert_eq!(spelled_code(word).hidden, 0, "{word:?}");
            assert_eq!(spelled_code(word).text, word);
        }

        // A spacing mark takes a cell of its own and is not counted among the marks drawn on a
        // letter.
        let spacing = format!("\u{915}{}", marks(10, '\u{93e}'));
        assert_eq!(spelled_code(&spacing).text, spacing);
    }

    /// Text that holds the written-out form of a character must not read as one the drawing wrote:
    /// the character a written-out one opens with is written out too, not counted, so each one in
    /// the output opens a character that was written out. A literal `‹U+200B›` and a zero-width
    /// space draw differently, and the sentence that counts what a person cannot see counts the
    /// zero-width space and not the bracket. A closing bracket alone is left as it is.
    #[test]
    fn a_written_out_form_in_the_text_cannot_pass_for_a_character_that_was_written_out() {
        let literal = "\u{2039}U+200B\u{203a}".to_string();
        let spelled = spelled_code(&literal);
        assert_eq!(spelled.text, "\u{2039}U+2039\u{203a}U+200B\u{203a}");
        assert_eq!(spelled.hidden, 0);
        assert_ne!(spelled.text, spelled_code("\u{200b}").text);

        let both = spelled_code(&format!("{literal} \u{200b}"));
        assert_eq!(
            both.text,
            "\u{2039}U+2039\u{203a}U+200B\u{203a} \u{2039}U+200B\u{203a}"
        );
        assert_eq!(both.hidden, 1);
        // Every opening in the output is followed by "U+", so each one begins a written form.
        for output in [spelled.text, both.text] {
            for (at, _) in output.match_indices('\u{2039}') {
                assert!(
                    output[at + '\u{2039}'.len_utf8()..].starts_with("U+"),
                    "{output}"
                );
            }
        }
        assert_eq!(spelled_code("a\u{203a}b").text, "a\u{203a}b");
    }

    /// The table of marks is Unicode's nonspacing and enclosing marks and nothing else. It is
    /// sorted and its runs do not touch, every code point in it is a mark to the
    /// `unicode-normalization` crate, which also holds the spacing marks, and the code points it
    /// leaves out of that list are the 471 spacing marks of this Unicode version. A table that
    /// held the spacing marks too, or that missed a run, fails the counts.
    #[test]
    fn the_table_of_marks_is_the_nonspacing_and_enclosing_marks() {
        use unicode_normalization::char::is_combining_mark;

        let table = marks::MARKS;
        assert_eq!(table.len(), 362);
        for pair in table.windows(2) {
            assert!(pair[0].1 + 1 < pair[1].0, "{pair:?}");
        }
        let mut held = 0;
        let mut spacing = 0;
        for code in 0..=0x10FFFFu32 {
            let Some(c) = char::from_u32(code) else {
                continue;
            };
            let in_table = is_stacking_mark(c);
            if in_table {
                held += 1;
                assert!(is_combining_mark(c), "U+{code:04X}");
            } else if is_combining_mark(c) {
                spacing += 1;
            }
        }
        assert_eq!(held, 2072);
        assert_eq!(spacing, 471);
        for c in [
            '\u{300}',
            '\u{36f}',
            '\u{483}',
            '\u{489}',
            '\u{e31}',
            '\u{20dd}',
            '\u{e01ef}',
        ] {
            assert!(is_stacking_mark(c), "{c:?}");
        }
        for c in [
            'a',
            '\u{2ff}',
            '\u{370}',
            '\u{903}',
            '\u{93e}',
            '\u{1d165}',
            '\u{e01f0}',
        ] {
            assert!(!is_stacking_mark(c), "{c:?}");
        }
    }

    /// The set of characters code writes out holds, besides what draws as nothing, exactly the
    /// categories named for it, and every end of every run is named. A format character the list
    /// forgot, or a run that stopped one short, is a character a program reads that a person does
    /// not.
    #[test]
    fn the_extra_lists_hold_their_categories_and_no_neighbour() {
        for c in [
            '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{202f}', '\u{205f}', '\u{3000}',
        ] {
            assert!(draws_as_another_space(c), "{c:?}");
        }
        for c in [
            ' ', '\u{9f}', '\u{a1}', '\u{167f}', '\u{1681}', '\u{1fff}', '\u{200b}', '\u{2010}',
            '\u{202e}', '\u{2030}', '\u{205e}', '\u{2060}', '\u{2fff}', '\u{3001}',
        ] {
            assert!(!draws_as_another_space(c), "{c:?}");
        }
        for c in [
            '\u{ad}',
            '\u{600}',
            '\u{605}',
            '\u{61c}',
            '\u{6dd}',
            '\u{70f}',
            '\u{890}',
            '\u{891}',
            '\u{8e2}',
            '\u{180e}',
            '\u{200b}',
            '\u{200f}',
            '\u{202a}',
            '\u{202e}',
            '\u{2060}',
            '\u{2064}',
            '\u{2066}',
            '\u{206f}',
            '\u{feff}',
            '\u{fff9}',
            '\u{fffb}',
            '\u{110bd}',
            '\u{110cd}',
            '\u{13430}',
            '\u{1343f}',
            '\u{1bca0}',
            '\u{1bca3}',
            '\u{1d173}',
            '\u{1d17a}',
            '\u{e0001}',
            '\u{e0020}',
            '\u{e007f}',
        ] {
            assert!(is_format(c), "{c:?}");
        }
        for c in [
            'a',
            '\u{ac}',
            '\u{ae}',
            '\u{5ff}',
            '\u{606}',
            '\u{6de}',
            '\u{710}',
            '\u{88f}',
            '\u{892}',
            '\u{8e3}',
            '\u{180d}',
            '\u{2065}',
            '\u{2028}',
            '\u{202f}',
            '\u{fffc}',
            '\u{110be}',
            '\u{13440}',
            '\u{1d172}',
            '\u{e0000}',
            '\u{e0002}',
            '\u{e0080}',
        ] {
            assert!(!is_format(c), "{c:?}");
        }
        for c in [
            '\u{e000}',
            '\u{f8ff}',
            '\u{f0000}',
            '\u{ffffd}',
            '\u{100000}',
            '\u{10fffd}',
        ] {
            assert!(is_private_use(c), "{c:?}");
        }
        for c in [
            '\u{d7ff}',
            '\u{f900}',
            '\u{efffd}',
            '\u{e0fff}',
            '\u{ffffe}',
            '\u{10fffe}',
        ] {
            assert!(!is_private_use(c), "{c:?}");
        }
    }
}
