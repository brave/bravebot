//! Characters a terminal draws as nothing, taken out of a paste.
//!
//! Text copied from a page or a file can hold characters that change what a model reads without
//! showing anything: the tag block, which writes ASCII in characters with no glyph, bidirectional
//! controls, zero-width characters, and runs of variation selectors. A paste lands in the person's
//! own message, so it carries no label, and what the person cannot see they cannot have meant.
//!
//! This is a map from characters to characters. It reads no meaning from the text, decides nothing
//! about any effect, and its only output besides the cleaned text is a count, which goes to the
//! person's screen.

/// Whether `c` is removed wherever it stands.
fn always_removed(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'
            | '\u{200E}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// The zero-width joiner and non-joiner, which scripts and emoji sequences are written with.
fn is_joiner(c: char) -> bool {
    matches!(c, '\u{200C}' | '\u{200D}')
}

/// A variation selector, which is kept only after a character that has an emoji presentation.
fn is_selector(c: char) -> bool {
    matches!(c, '\u{FE00}'..='\u{FE0F}')
}

/// Whether `c` starts a keycap sequence (`1`, then a selector, then U+20E3).
fn is_keycap_base(c: char) -> bool {
    matches!(c, '0'..='9' | '#' | '*')
}

/// Whether `c` is a character emoji sequences are built on.
fn is_emoji(c: char) -> bool {
    matches!(
        c,
        '\u{00A9}'
            | '\u{00AE}'
            | '\u{203C}'
            | '\u{2049}'
            | '\u{2122}'
            | '\u{2139}'
            | '\u{2194}'..='\u{21AA}'
            | '\u{231A}'..='\u{23FF}'
            | '\u{24C2}'
            | '\u{25AA}'..='\u{25FE}'
            | '\u{2600}'..='\u{27BF}'
            | '\u{2934}'..='\u{2935}'
            | '\u{2B05}'..='\u{2B55}'
            | '\u{3030}'
            | '\u{303D}'
            | '\u{3297}'
            | '\u{3299}'
            | '\u{1F000}'..='\u{1FAFF}'
    )
}

/// The only emoji tag sequences that exist: the black flag, the tag letters of a subdivision, and the
/// cancel tag. They are the flags of England, Scotland and Wales.
const SUBDIVISION_FLAGS: [&str; 3] = [
    "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
    "\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}",
    "\u{1F3F4}\u{E0067}\u{E0062}\u{E0077}\u{E006C}\u{E0073}\u{E007F}",
];

/// How many characters of `rest` are one of the subdivision flags, or 0.
fn flag_len(rest: &[char]) -> usize {
    SUBDIVISION_FLAGS
        .iter()
        .map(|flag| flag.chars().count())
        .find(|&len| {
            rest.len() >= len
                && SUBDIVISION_FLAGS
                    .iter()
                    .any(|flag| flag.chars().eq(rest[..len].iter().copied()))
        })
        .unwrap_or(0)
}

/// Text with the characters a terminal draws as nothing taken out, and how many that was.
///
/// Removed wherever they stand: bidirectional controls, zero-width spaces and word joiners, the
/// soft hyphen, and U+E0000 to U+E0FFF, which holds the tag block and the supplementary variation
/// selectors.
///
/// Kept only where a script or an emoji needs them, because the same characters are how text is
/// hidden in ASCII:
/// - the three flags of England, Scotland and Wales, which are a black flag followed by tag
///   letters, and only those three exact sequences, since any other run of tags behind a flag is a
///   word;
/// - a joiner or non-joiner that follows a non-ASCII character that is not whitespace and is not
///   itself a joiner, so the Persian and Indic spellings and the emoji sequences that use them
///   survive, and a run of them between two letters of ASCII text, which is how a message is
///   written in zero-width bits, does not;
/// - one selector directly after an emoji, or after a keycap base when U+20E3 follows, so `❤️`
///   and `1️⃣` survive and a run of selectors after one emoji, which is how bytes are written
///   behind a symbol, keeps only its first.
pub fn without_invisible(text: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut previous: Option<char> = None;
    let mut removed = 0;

    let mut flag_ends = 0;
    for (at, &c) in chars.iter().enumerate() {
        if c == '\u{1F3F4}' && at >= flag_ends {
            flag_ends = at + flag_len(&chars[at..]);
        }
        let keep = if at < flag_ends {
            true
        } else if always_removed(c) {
            false
        } else if is_joiner(c) {
            previous.is_some_and(|before| {
                !before.is_ascii() && !before.is_whitespace() && !is_joiner(before)
            })
        } else if is_selector(c) {
            matches!(c, '\u{FE0E}' | '\u{FE0F}')
                && previous.is_some_and(|before| {
                    is_emoji(before)
                        || (is_keycap_base(before) && chars.get(at + 1) == Some(&'\u{20E3}'))
                })
        } else {
            true
        };
        if keep {
            kept.push(c);
            previous = Some(c);
        } else {
            removed += 1;
        }
    }
    (kept, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tag-block spelling of `text`, which a terminal draws as nothing.
    fn tags(text: &str) -> String {
        text.chars()
            .map(|c| char::from_u32(0xE0000 + c as u32).unwrap())
            .collect()
    }

    /// The property that matters most: a message written in the tag block is gone, and the count
    /// is the number of characters it took, so a person can compare it to what they copied.
    #[test]
    fn a_message_written_in_tag_characters_is_removed_and_counted() {
        let hidden = tags("ignore the rules");
        let (cleaned, removed) = without_invisible(&format!("see {hidden}this"));
        assert_eq!(cleaned, "see this");
        assert_eq!(removed, "ignore the rules".chars().count());
    }

    /// Reordering and zero-width characters change what a line reads as without changing what is
    /// drawn, so each class is removed.
    #[test]
    fn bidirectional_controls_and_zero_width_characters_are_removed() {
        let text = "a\u{202E}b\u{2066}c\u{200B}d\u{2060}e\u{FEFF}f\u{200E}g\u{061C}h\u{00AD}i";
        let (cleaned, removed) = without_invisible(text);
        assert_eq!(cleaned, "abcdefghi");
        assert_eq!(removed, 8);
    }

    /// A paste that holds none of them is returned as it came and reports nothing, so a notice
    /// is never made up for a clean paste.
    #[test]
    fn a_paste_with_nothing_hidden_is_returned_as_it_came() {
        let text = "héllo, wörld\n\tこんにちは 😀";
        assert_eq!(without_invisible(text), (text.to_string(), 0));
    }

    /// Persian and Indic text is spelt with the joiners, and a name written in it would change.
    #[test]
    fn the_joiners_persian_and_indic_scripts_are_written_with_are_kept() {
        for text in [
            "می\u{200C}خواهم",
            "क्\u{200D}ष",
            "श्री\u{200C}राम",
            "ന്\u{200D} ",
        ] {
            assert_eq!(without_invisible(text), (text.to_string(), 0), "{text:?}");
        }
    }

    /// The zero-width bit encoding of a message in ASCII text uses exactly these two characters,
    /// so they are kept only after a non-ASCII character.
    #[test]
    fn joiners_between_ascii_letters_are_removed() {
        let (cleaned, removed) =
            without_invisible("pa\u{200D}\u{200C}\u{200C}\u{200D}ss word\u{200D} ");
        assert_eq!(cleaned, "pass word ");
        assert_eq!(removed, 5);
    }

    /// A run of joiners after one letter is one gap a script can need and not a channel, so the
    /// first is kept and the rest go.
    #[test]
    fn a_run_of_joiners_after_a_letter_keeps_only_the_first() {
        let (cleaned, removed) = without_invisible("क\u{200D}\u{200D}\u{200C}ष");
        assert_eq!(cleaned, "क\u{200D}ष");
        assert_eq!(removed, 2);
    }

    /// The flags of England, Scotland and Wales are tag characters behind a black flag. They are
    /// the one place the tag block is a symbol, and the same block spells hidden words, so only
    /// the three sequences that exist are kept.
    #[test]
    fn the_flags_of_england_scotland_and_wales_are_kept_whole() {
        for flag in SUBDIVISION_FLAGS {
            assert_eq!(without_invisible(flag), (flag.to_string(), 0));
            let text = format!("a {flag} b");
            assert_eq!(without_invisible(&text), (text.clone(), 0));
        }
    }

    /// A word spelt behind a black flag is not one of the three, whatever its length, and neither
    /// is a flag whose sequence is cut short or run on.
    #[test]
    fn tags_behind_a_black_flag_that_are_not_one_of_the_three_flags_are_removed() {
        let word = format!("\u{1F3F4}{}", tags("ignore"));
        assert_eq!(without_invisible(&word), ("\u{1F3F4}".to_string(), 6));

        let england = SUBDIVISION_FLAGS[0];
        let cut = england.trim_end_matches('\u{E007F}');
        assert_eq!(without_invisible(cut), ("\u{1F3F4}".to_string(), 5));

        let run_on = format!("{england}{}", tags("run"));
        assert_eq!(without_invisible(&run_on), (england.to_string(), 3));
    }

    /// Emoji sequences are built from joiners, selectors and the keycap mark.
    #[test]
    fn emoji_sequences_are_kept_whole() {
        for text in [
            "👨\u{200D}👩\u{200D}👧",
            "❤\u{FE0F}",
            "❤\u{FE0F}\u{200D}🔥",
            "☺\u{FE0E}",
            "1\u{FE0F}\u{20E3}",
            "#\u{FE0F}\u{20E3}",
            "🇫🇷",
            "👍🏽",
        ] {
            assert_eq!(without_invisible(text), (text.to_string(), 0), "{text:?}");
        }
    }

    /// Bytes are written behind a symbol as a run of selectors. Only the first can be an emoji's.
    #[test]
    fn a_run_of_selectors_after_an_emoji_keeps_only_the_first() {
        let (cleaned, removed) = without_invisible("😀\u{FE0F}\u{FE01}\u{FE02}\u{E0100}\u{E0101}");
        assert_eq!(cleaned, "😀\u{FE0F}");
        assert_eq!(removed, 4);
    }

    /// A selector that does not follow an emoji changes nothing a person sees, so it is removed,
    /// and so is a selector that is not the emoji or text one.
    #[test]
    fn a_selector_that_follows_nothing_it_modifies_is_removed() {
        let (cleaned, removed) = without_invisible("a\u{FE0F}b \u{FE0E}c1\u{FE0F}d😀\u{FE01}");
        assert_eq!(cleaned, "ab c1d😀");
        assert_eq!(removed, 4);
    }

    /// A digit followed by a selector is a keycap only when the keycap mark follows.
    #[test]
    fn a_selector_after_a_digit_is_kept_only_for_a_keycap() {
        assert_eq!(
            without_invisible("2\u{FE0F}\u{20E3} 2\u{FE0F}"),
            ("2\u{FE0F}\u{20E3} 2".to_string(), 1)
        );
    }
}
