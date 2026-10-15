//! Unicode canonical decomposition, which the trust map compares names under.
//!
//! A volume can open one file under more than one spelling of its name. APFS keeps a name as it
//! was created and opens it under any canonically equivalent form, so `cafe` followed by a
//! combining acute accent and `caf` followed by the precomposed letter are one file there. The
//! trust map has to read both as one name wherever a rule withholds trust (TRUST-2), and it cannot
//! ask the volume, because this crate performs no I/O and the question cannot be put without
//! creating a file.
//!
//! Decomposing each name to Normalization Form D gives equivalent names one spelling. The data is
//! Unicode's, generated into [`tables`] from the `unicode-normalization` crate and checked against
//! it over every code point by this module's tests. The crate is a dev-dependency only: this
//! crate has no dependency at runtime (CRED-18).

#[rustfmt::skip]
mod tables;

use tables::{COMBINING_CLASSES, DECOMPOSITIONS};

/// The Hangul syllable block and the constants its arithmetic decomposition is defined with.
const SYLLABLES: std::ops::Range<u32> = 0xAC00..0xD7A4;
const LEADING_BASE: u32 = 0x1100;
const VOWEL_BASE: u32 = 0x1161;
const TRAILING_BASE: u32 = 0x11A7;
const VOWELS: u32 = 21;
const TRAILING: u32 = 28;

/// `text` in Normalization Form D: every character replaced by its full canonical decomposition,
/// and the marks after each letter put in canonical order.
///
/// Two names Unicode makes one name decompose to the same string. Compatibility forms are not
/// canonical equivalents and are left as they are: a full-width letter stays a full-width letter.
pub(crate) fn decompose(text: &str) -> String {
    let mut decomposed: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        push_decomposed(c, &mut decomposed);
    }
    put_marks_in_order(&mut decomposed);
    decomposed.into_iter().collect()
}

fn push_decomposed(c: char, into: &mut Vec<char>) {
    let code = c as u32;
    if code < 0x80 {
        into.push(c);
    } else if SYLLABLES.contains(&code) {
        let index = code - SYLLABLES.start;
        let leading = LEADING_BASE + index / (VOWELS * TRAILING);
        let vowel = VOWEL_BASE + (index % (VOWELS * TRAILING)) / TRAILING;
        let trailing = TRAILING_BASE + index % TRAILING;
        into.extend([leading, vowel].into_iter().filter_map(char::from_u32));
        if trailing != TRAILING_BASE {
            into.extend(char::from_u32(trailing));
        }
    } else if let Ok(found) = DECOMPOSITIONS.binary_search_by_key(&code, |(code, _)| *code) {
        into.extend(
            DECOMPOSITIONS[found]
                .1
                .iter()
                .copied()
                .filter_map(char::from_u32),
        );
    } else {
        into.push(c);
    }
}

/// Order each run of marks by combining class, keeping the order of marks of one class. Two marks
/// on one letter that no precomposed letter holds read the same in either order.
fn put_marks_in_order(text: &mut [char]) {
    let mut start = 0;
    while start < text.len() {
        if combining_class(text[start]) == 0 {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < text.len() && combining_class(text[end]) != 0 {
            end += 1;
        }
        text[start..end].sort_by_key(|mark| combining_class(*mark));
        start = end;
    }
}

fn combining_class(c: char) -> u8 {
    let code = c as u32;
    if code < 0x80 {
        return 0;
    }
    match COMBINING_CLASSES.binary_search_by(|(first, last, _)| {
        if code < *first {
            std::cmp::Ordering::Greater
        } else if code > *last {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(found) => COMBINING_CLASSES[found].2,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_normalization::UnicodeNormalization;

    fn reference(text: &str) -> String {
        text.nfd().collect()
    }

    /// The tables are the crate's, and a single code point missing from them, or one the arithmetic
    /// gets wrong, leaves two spellings of a name apart. Every scalar value is compared, so a
    /// release of the crate that moves to another Unicode version fails here until the tables are
    /// generated again.
    #[test]
    fn every_character_decomposes_as_the_unicode_normalization_crate_decomposes_it() {
        for code in 0..=0x10FFFFu32 {
            let Some(c) = char::from_u32(code) else {
                continue;
            };
            let text = c.to_string();
            assert_eq!(
                decompose(&text),
                reference(&text),
                "U+{code:04X} decomposes differently"
            );
        }
    }

    /// Characters on their own do not reach the ordering of marks, the run of a letter with several
    /// of them, or a syllable beside a mark, so strings built from the characters that matter are
    /// compared too: letters with decompositions, marks of every class, jamo and syllables.
    #[test]
    fn strings_of_letters_and_marks_decompose_as_the_unicode_normalization_crate_decomposes_them() {
        let mut alphabet: Vec<char> = "aeKs/.-"
            .chars()
            .chain([
                '\u{c5}',
                '\u{e9}',
                '\u{1ea0}',
                '\u{1ec7}',
                '\u{1e69}',
                '\u{1f82}',
                '\u{212b}',
                '\u{37e}',
                '\u{344}',
                '\u{f900}',
                '\u{2f800}',
                '\u{1100}',
                '\u{1161}',
                '\u{11a8}',
                '\u{ac00}',
                '\u{d55c}',
                '\u{d7a3}',
                '\u{ff53}',
                '\u{fb01}',
            ])
            .collect();
        alphabet.extend((0x300..0x370).filter_map(char::from_u32));
        alphabet.extend(
            [
                0x5B0, 0x93C, 0xE38, 0x1AB0, 0x1DC0, 0x20D0, 0x3099, 0x1D165, 0x1E944,
            ]
            .map(|c| char::from_u32(c).expect("a scalar value")),
        );

        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = |bound: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % bound as u64) as usize
        };
        for _ in 0..50_000 {
            let length = 1 + next(8);
            let text: String = (0..length)
                .map(|_| alphabet[next(alphabet.len())])
                .collect();
            assert_eq!(
                decompose(&text),
                reference(&text),
                "{text:?} decomposes differently"
            );
        }
    }

    /// The cases the trust map depends on, each written out so a reader can see what the form of a
    /// name is.
    #[test]
    fn names_unicode_makes_one_name_decompose_to_one_string() {
        for (one, other) in [
            ("caf\u{e9}", "cafe\u{301}"),
            ("\u{c5}", "\u{212b}"),
            ("a\u{323}\u{301}", "a\u{301}\u{323}"),
            ("\u{1ec7}", "e\u{323}\u{302}"),
            ("\u{d55c}", "\u{1112}\u{1161}\u{11ab}"),
            ("a\u{37e}b", "a;b"),
        ] {
            assert_eq!(decompose(one), decompose(other), "{one:?} and {other:?}");
        }
    }

    /// Names that only look alike stay apart: another accent, no accent, a compatibility form.
    #[test]
    fn names_that_differ_by_more_than_their_form_decompose_to_different_strings() {
        for (one, other) in [
            ("cafe\u{301}", "cafe"),
            ("cafe\u{301}", "cafe\u{300}"),
            ("\u{ff53}rc", "src"),
            ("\u{fb01}le", "file"),
            ("\u{d55c}", "\u{d55d}"),
        ] {
            assert_ne!(decompose(one), decompose(other), "{one:?} and {other:?}");
        }
    }

    /// A name with nothing to decompose comes back as it went in, which is every ASCII name.
    #[test]
    fn an_ascii_name_decomposes_to_itself() {
        for name in ["", "/", "src/main.rs", "./a b/..", "Docs/README.md"] {
            assert_eq!(decompose(name), name);
        }
    }
}
