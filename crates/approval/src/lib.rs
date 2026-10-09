//! A proposed write as the lines a person reads before approving it.
//!
//! One implementation for every surface that shows a write as text. The command-line session
//! draws these lines in a terminal and the editor bridge sends them to an editor, so the two
//! cannot disagree about what an approval shows, which markers a change carries, how much of it is
//! left out, or how a line of quarantined content is made safe to display.
//!
//! The functions here read the content they are given, to replace its control characters and to
//! count its lines. They are called by presentation callers only and run nowhere near the driver.

#![forbid(unsafe_code)]

use bravebot_agent::diff::Change;
use bravebot_i18n::t;

/// Drawn down the margin of everything the planner was not allowed to read.
///
/// The same glyph the interactive transcript uses, on every row of the block, so the mark cannot
/// be ended by anything written inside it. A caption could be imitated; a margin cannot.
pub const QUARANTINE_BAR: &str = "\u{2503}";

/// How many lines of what a question is about are shown before the rest is counted instead.
///
/// A write is approved from the change it would make and a read from the bytes it would release,
/// so the content is what the question is; and the whole of a generated file is not readable as
/// one. A person scrolled past a thousand lines is answering whatever was in front of them at the
/// end of it, which is not the question that was asked.
pub const MOST_CONTENT_LINES: usize = 40;

/// Replace control characters, so shown text cannot move the cursor or recolour the screen.
///
/// The margin in front of every row is written by the caller. An escape sequence in the content
/// would let the content write one instead, and a forged margin is worse than no margin, since
/// drawing one is the whole claim being made about the block.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if !c.is_control() || c == '\t' {
                c
            } else {
                // The Unicode pictures for C0, so an escape reads as ␛ rather than vanishing: a
                // character silently dropped is one nobody can tell was ever in the file.
                char::from_u32(0x2400 + c as u32).unwrap_or('\u{fffd}')
            }
        })
        .collect()
}

/// Quarantined content as rows, each behind a margin, capped.
///
/// The margin is on every row, for the reason [`QUARANTINE_BAR`] is on every row of a block: a
/// caption above the block could be imitated by the block's own first line, and a margin cannot.
/// The cap is because a question has to be readable as one: a person scrolled past a thousand
/// lines of output is answering whatever is in front of them at the end of it.
pub fn quarantined(content: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut counted = content.lines();
    for line in counted.by_ref().take(MOST_CONTENT_LINES) {
        rows.push(format!("{QUARANTINE_BAR} {}", printable(line)));
    }
    let left_out = counted.count();
    if left_out > 0 {
        rows.push(t!(transcript_more_lines, count = left_out).to_string());
    }
    rows
}

/// What a write's approval holds besides the sentence naming it.
pub struct Write<'a> {
    /// The body came from somewhere nobody vouched for.
    pub untrusted: bool,
    /// What the isolated processor that produced the body said about it, already trimmed.
    pub remark: Option<&'a [String]>,
    /// What the scan inferred about the body, each already a kind, a location and a masked preview.
    pub credentials: &'a [String],
    /// The working directory's file was written after the checkout it is being brought back from.
    pub written_since_checkout: bool,
    /// What the write does to the file's line terminators, which `changes` cannot show.
    pub line_endings: Option<&'a str>,
    /// False when the diff had to give up on an exact answer.
    pub exact: bool,
    /// Lines the change adds, for saying what an inexact diff is too large to show.
    pub added: usize,
    /// Lines the change removes.
    pub removed: usize,
    /// The change, already condensed to the context the caller wants around each run.
    pub changes: &'a [Change],
}

/// The lines a proposed write is read before approving: the note on where the body came from,
/// then the change itself.
pub fn write_lines(write: &Write<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    if write.untrusted {
        lines.push(t!(write_untrusted).to_string());
    }
    // What the isolated processor that produced the body said about it, beside the diff rather than
    // somewhere up the scrollback: a remark saying a typo was fixed is only a claim worth anything
    // while the lines it describes are in front of the person reading it. It decides nothing, and
    // it is free text a processor authored, so it goes behind the margin with the content.
    if let Some(remark) = write.remark {
        lines.push(t!(write_remark).to_string());
        lines.extend(quarantined(&remark.join("\n")));
    }
    // What the scan inferred, beside the lines it read it from. These are the driver's own words
    // about its own findings, each already a kind, a location and a masked preview, so no part of
    // the value is repeated here and none of it needs the margin content sits behind.
    if !write.credentials.is_empty() {
        lines.push(t!(write_credentials).to_string());
        lines.extend(write.credentials.iter().map(|found| printable(found)));
    }

    if write.written_since_checkout {
        lines.push(t!(write_since_checkout).to_string());
    }
    lines.extend(write.line_endings.map(str::to_string));

    // A change too large to diff says so rather than showing a guess at it. The summary the caller
    // puts above still counts the lines.
    if !write.exact {
        lines.push(t!(
            write_too_large_to_show,
            added = write.added,
            removed = write.removed
        ));
        return lines;
    }

    let mut changed = 0usize;
    let mut left_out = 0usize;
    for held in write.changes {
        if changed == MOST_CONTENT_LINES {
            left_out += 1;
            continue;
        }
        changed += 1;
        lines.push(match held {
            Change::Added(line) => format!("+ {}", printable(line)),
            Change::Removed(line) => format!("- {}", printable(line)),
            Change::Kept(line) => format!("  {}", printable(line)),
            Change::Elided(count) => t!(write_unchanged, count = *count).to_string(),
        });
    }
    if left_out > 0 {
        lines.push(t!(transcript_more_lines, count = left_out).to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write<'a>(changes: &'a [Change]) -> Write<'a> {
        Write {
            untrusted: false,
            remark: None,
            credentials: &[],
            written_since_checkout: false,
            line_endings: None,
            exact: true,
            added: 0,
            removed: 0,
            changes,
        }
    }

    #[test]
    fn each_kind_of_change_has_its_own_marker_and_an_escape_is_pictured() {
        let changes = [
            Change::Kept("same".to_string()),
            Change::Removed("old".to_string()),
            Change::Added("new\u{1b}[2J".to_string()),
            Change::Elided(3),
        ];
        assert_eq!(
            write_lines(&write(&changes)),
            [
                "  same".to_string(),
                "- old".to_string(),
                "+ new\u{241b}[2J".to_string(),
                t!(write_unchanged, count = 3).to_string(),
            ]
        );
    }

    #[test]
    fn a_body_nobody_vouched_for_says_so_before_the_change() {
        let changes = [Change::Added("body".to_string())];
        let trusted = write_lines(&write(&changes));
        let untrusted = write_lines(&Write {
            untrusted: true,
            ..write(&changes)
        });
        assert_eq!(trusted, ["+ body".to_string()]);
        assert_eq!(
            untrusted,
            [t!(write_untrusted).to_string(), "+ body".to_string()]
        );
    }

    #[test]
    fn a_change_longer_than_the_cap_counts_what_it_leaves_out() {
        let changes: Vec<Change> = (0..MOST_CONTENT_LINES + 7)
            .map(|n| Change::Added(format!("line {n}")))
            .collect();
        let lines = write_lines(&write(&changes));
        assert_eq!(lines.len(), MOST_CONTENT_LINES + 1);
        assert_eq!(lines[MOST_CONTENT_LINES - 1], "+ line 39");
        assert_eq!(
            lines[MOST_CONTENT_LINES],
            t!(transcript_more_lines, count = 7).to_string()
        );
    }

    #[test]
    fn a_change_too_large_to_diff_shows_no_guess_at_it() {
        let changes = [Change::Added("a guess".to_string())];
        let lines = write_lines(&Write {
            exact: false,
            added: 90,
            removed: 80,
            ..write(&changes)
        });
        assert_eq!(
            lines,
            [t!(write_too_large_to_show, added = 90usize, removed = 80usize).to_string()]
        );
    }

    #[test]
    fn what_a_processor_said_sits_behind_the_margin_and_its_escapes_are_pictured() {
        let remark = ["fixed a typo\u{1b}[31m".to_string()];
        let credentials = ["API_KEY at line 2 (sk-\u{2026})".to_string()];
        let lines = write_lines(&Write {
            remark: Some(&remark),
            credentials: &credentials,
            written_since_checkout: true,
            line_endings: Some("line endings: LF kept"),
            ..write(&[])
        });
        assert_eq!(
            lines,
            [
                t!(write_remark).to_string(),
                format!("{QUARANTINE_BAR} fixed a typo\u{241b}[31m"),
                t!(write_credentials).to_string(),
                credentials[0].clone(),
                t!(write_since_checkout).to_string(),
                "line endings: LF kept".to_string(),
            ]
        );
    }
}
