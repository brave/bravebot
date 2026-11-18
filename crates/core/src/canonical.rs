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

mod decompose;
#[rustfmt::skip]
mod tables;

pub(crate) use decompose::decompose;

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
