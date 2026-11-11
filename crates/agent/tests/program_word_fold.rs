//! The program word of a deny or ask rule, held to the volume it is run from (PERM-5).
//!
//! A rule written `Bash(git push *)` has to cover every spelling the volume opens as `git`. The
//! volume is asked rather than a table read: a file is made for each ASCII letter, every character
//! outside ASCII is then opened by name, and the letter whose file comes back is the letter the
//! volume took the character for. The rule has to cover each of those. A character the rule covers
//! and the volume does not open is the safe side of [PERM-4](../../../docs/specs/permissions.md)
//! and is not asserted.
//!
//! The answer is the volume's, so this reads a different set on NTFS, on APFS and on a volume that
//! does not fold case. On a case-sensitive volume nothing folds, the probe finds no character and
//! the first test asserts nothing about the rule. CI runs it on Windows in the `Awkward names on
//! Windows` job, which is the one place this reads NTFS.
//!
//! What holds the rule on every host is the unit tests in `crates/core/src/permissions.rs` and
//! `crates/core/src/ambient.rs`, which name the spellings and fail without the fold.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_core::permissions::{Anchors, Decision, Permissions, Ruling};
use std::fs;

fn denies(rule: &str) -> Permissions {
    let (permissions, rejected) =
        Permissions::parse(&[rule.to_string()], &[], &[], &Anchors::none());
    assert!(rejected.is_empty(), "{rule} did not parse");
    permissions
}

/// The characters outside ASCII that the volume opens as an ASCII letter, with the letter.
///
/// A file named for a letter opens by that name, which shows the probe reads what it wrote.
fn letters_the_volume_folds_to_ascii() -> Vec<(char, char)> {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for letter in 'a'..='z' {
        fs::write(
            directory.path().join(letter.to_string()),
            letter.to_string(),
        )
        .expect("a file named for the letter");
    }
    assert_eq!(
        fs::read_to_string(directory.path().join("k")).expect("the file opens by its own name"),
        "k"
    );
    (0x80..=0xFFFF_u32)
        .filter_map(char::from_u32)
        .filter_map(|spelled| {
            let opened = fs::read_to_string(directory.path().join(spelled.to_string())).ok()?;
            let mut letters = opened.chars();
            match (letters.next(), letters.next()) {
                (Some(letter), None) => Some((spelled, letter)),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn a_deny_rule_covers_every_spelling_the_volume_opens_as_the_program() {
    let folded = letters_the_volume_folds_to_ascii();
    println!("the volume opens as an ASCII letter: {folded:?}");
    let mut uncovered = Vec::new();
    for (spelled, letter) in &folded {
        let rule = denies(&format!("Bash({letter}{letter} *)"));
        let program = format!("{spelled}{letter}");
        let line = [program.as_str(), "x"];
        if rule.for_command(&line) != Decision::Ruled(Ruling::Deny) {
            uncovered.push(format!("U+{:04X} for {letter}", *spelled as u32));
        }
    }
    assert!(
        uncovered.is_empty(),
        "the volume opens these as an ASCII letter and the rule does not cover them: {uncovered:?}"
    );
}

/// The opposite direction: the characters below U+10000 outside ASCII that the rule covers for an
/// ASCII letter are exactly dotless i for `i`, long s for `s` and the Kelvin sign for `k`. The
/// literal set does not depend on the volume. Those the volume does not open as the letter are
/// printed: a rule that restricts may cover more than the volume folds
/// ([PERM-4](../../../docs/specs/permissions.md)).
#[test]
fn a_deny_rule_covers_exactly_the_pinned_spellings_and_the_extra_ones_are_listed() {
    let opened: Vec<(char, char)> = letters_the_volume_folds_to_ascii();
    let mut extra = Vec::new();
    let mut covered = Vec::new();
    for letter in 'a'..='z' {
        let rule = denies(&format!("Bash({letter}{letter} *)"));
        for spelled in (0x80..=0xFFFF_u32).filter_map(char::from_u32) {
            let program = format!("{spelled}{letter}");
            let line = [program.as_str(), "x"];
            if rule.for_command(&line) != Decision::Ruled(Ruling::Deny) {
                continue;
            }
            covered.push((spelled, letter));
            if !opened.contains(&(spelled, letter)) {
                extra.push((spelled, letter));
            }
        }
    }
    println!("covered and not opened as that letter by the volume: {extra:?}");
    assert_eq!(
        covered,
        [('\u{131}', 'i'), ('\u{212a}', 'k'), ('\u{17f}', 's')],
        "the characters a deny rule covers for an ASCII letter"
    );
}
