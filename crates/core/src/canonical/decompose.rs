//! The decomposition itself, in a file of its own so that `bravebot-sandbox`, which has no
//! dependency on this crate, compiles the same code over the same tables for the names a `denyRead`
//! glob is compared with (SANDBOX-25).

use super::tables::{COMBINING_CLASSES, DECOMPOSITIONS};

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

/// The combining class of `c`: 0 for a character that stands on its own, and the class that orders
/// it among the marks after a letter otherwise.
pub(crate) fn combining_class(c: char) -> u8 {
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
