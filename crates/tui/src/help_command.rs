//! `/help` and `/changelog`: answers taken from the documentation and the release notes this
//! program carries (CMD-24, CMD-25).
//!
//! Both texts are the program's own, compiled in, so neither goes through a trust gate. The
//! documentation is `bravebot_agent::help`'s, and `/help` hands a question to a turn with words the
//! driver wrote, which the planner answers from the pages it loads. The release notes are carried
//! here, and `/changelog` shows them without a turn. The sentences that go to the planner are
//! planner-facing text and stay out of the message catalog.

use bravebot_agent::help::SKILL;

/// The release notes, newest first, as `CHANGELOG.md` holds them.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// One release's notes: the version its heading names and the lines under the heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release<'a> {
    pub version: &'a str,
    pub notes: &'a str,
}

/// The releases `text` holds, in the order it holds them.
///
/// A release starts at a line `## [<version>](<address>)` and runs to the next such line. Anything
/// before the first one belongs to no release, and a line that starts `## [` without an address is
/// part of the notes above it.
pub(crate) fn releases_in(text: &str) -> Vec<Release<'_>> {
    let mut heads = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("## [")
            && let Some((version, after)) = rest.split_once(']')
            && after.starts_with('(')
        {
            heads.push((at, at + line.len(), version));
        }
        at += line.len();
    }
    heads
        .iter()
        .enumerate()
        .map(|(n, &(_, notes_from, version))| {
            let end = heads.get(n + 1).map_or(text.len(), |next| next.0);
            Release {
                version,
                notes: text[notes_from..end].trim_matches(['\n', '\r']),
            }
        })
        .collect()
}

/// The release among `releases` named `version`, or the first of them where none is named that.
///
/// The name has to be the whole version: `0.1` is not the release `0.17.0`.
pub(crate) fn release_for<'a>(releases: &[Release<'a>], version: &str) -> Option<Release<'a>> {
    releases
        .iter()
        .find(|release| release.version == version)
        .or(releases.first())
        .copied()
}

/// The notes of the release this build carries for `version`, or the newest release's where it
/// carries none for it.
///
/// `None` only where the changelog holds no release at all. A version with no heading, such as a
/// development build whose version was set by hand, is answered with the newest release so that
/// `/changelog` shows something; the caller says which version it found.
pub fn release_notes(version: &str) -> Option<Release<'static>> {
    release_for(&releases_in(CHANGELOG), version)
}

/// What the turn is asked, with the person's question after it.
///
/// The skill's own body says how to pick the pages, so this names the skill and the question and
/// repeats none of that. It names no path with `@`, so sending it vouches for nothing.
pub fn prompt(question: &str) -> String {
    format!(
        "Answer this question about using bravebot. Load the skill {SKILL} with load_skill and \
answer from the pages it lists, not from memory.\n\
\n\
Question: {question}"
    )
}

/// The release notes as the transcript shows them: each markdown link written as its text, so a
/// bullet ending `([#1551](https://github.com/brave/bravebot/issues/1551))` ends `(#1551)`.
///
/// Anything that is not a whole link is left as it was written: a bracket with no address after
/// it, an image, and a link inside a code span.
pub fn plain(notes: &str) -> String {
    notes
        .split('\n')
        .map(plain_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One line of the notes. A code span and a link end with their line, so nothing is carried over
/// from the line before, and a backtick left open on one bullet does not hide the links on the rest.
fn plain_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    let mut in_code = false;
    while let Some(at) = rest.find(['[', '`']) {
        out.push_str(&rest[..at]);
        let here = &rest[at..];
        if let Some(after) = here.strip_prefix('`') {
            in_code = !in_code;
            out.push('`');
            rest = after;
            continue;
        }
        let image = rest[..at].ends_with('!');
        match link_at(here) {
            Some((label, length)) if !in_code && !image => {
                out.push_str(label);
                rest = &here[length..];
            }
            _ => {
                out.push('[');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The label of the link `text` starts with and how many bytes the link takes up.
///
/// The label holds no bracket. The address ends at the parenthesis that closes the one opening it,
/// so a `(bar)` inside an address belongs to it.
fn link_at(text: &str) -> Option<(&str, usize)> {
    let (label, after) = text.strip_prefix('[')?.split_once("](")?;
    if label.is_empty() || label.contains(['[', ']']) {
        return None;
    }
    let mut depth = 1;
    for (at, c) in after.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((label, "[".len() + label.len() + "](".len() + at + ")".len()));
                }
            }
            _ => {}
        }
    }
    None
}

/// What `/changelog` puts in the transcript for `release`, found for the version `running`.
///
/// A build whose own version has no heading in the changelog shows the newest release's notes and
/// says so, so a person is not told these are the notes of the version they run.
pub fn shown(release: &Release<'_>, running: &str) -> (String, String) {
    let heading = if release.version == running {
        bravebot_i18n::t!(changelog_heading, version = release.version).to_string()
    } else {
        bravebot_i18n::t!(
            changelog_newest,
            running = running,
            version = release.version
        )
        .to_string()
    };
    (heading, plain(release.notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The turn can only answer from the pages if it is told which skill holds them, and it can
    /// only answer the person's question if the question arrives as typed.
    #[test]
    fn the_prompt_names_the_help_skill_and_carries_the_question_as_typed() {
        let asked = "how do I stop a background job?  (with 2 spaces)";
        let text = prompt(asked);
        assert!(text.contains(&format!("skill {SKILL} ")), "{text}");
        assert!(text.contains("load_skill"), "{text}");
        assert!(text.ends_with(&format!("Question: {asked}")), "{text}");
    }

    /// A path written with `@` in a sentence the driver wrote would open a file on the driver's
    /// word, so the fixed part of the prompt may contain none.
    #[test]
    fn the_prompt_names_no_path_for_the_person_to_vouch_for() {
        assert!(!prompt("").contains('@'));
    }

    /// The release notes link every issue number, and a line drawn with the link's address in it
    /// is a line most of which is a URL.
    #[test]
    fn a_link_is_drawn_as_its_text_and_the_rest_of_the_line_is_untouched() {
        assert_eq!(
            plain(
                " - Fixed a hang. ([#2122](https://github.com/brave/bravebot/issues/2122), \
[#2123](https://github.com/brave/bravebot/issues/2123))"
            ),
            " - Fixed a hang. (#2122, #2123)"
        );
        assert_eq!(plain("no link here"), "no link here");
    }

    /// Brackets that are not a link are the release notes' own words, and dropping them would
    /// change what a line says.
    #[test]
    fn brackets_that_are_not_a_link_are_kept() {
        for text in [
            "Added `[a | b]` to the hint",
            "[unclosed](https://example.org",
            "[](https://example.org/empty)",
            "a [b] c (d)",
        ] {
            assert_eq!(plain(text), text);
        }
        assert_eq!(plain("[a [b](https://x.test/y)"), "[a b");
    }

    /// An address that is not closed on its line is not an address, so the lines after it, and the
    /// parenthesis that closes some later sentence, are not swallowed into one.
    #[test]
    fn an_address_that_is_not_closed_takes_nothing_after_it() {
        for text in [
            "a [b](c\n - Added foo (#1)",
            "a [b](c\n - Added foo (a)) more",
            "Added `[a](b` and later (see c)",
            "[b](prose that opens (like this)",
        ] {
            assert_eq!(plain(text), text);
        }
    }

    /// A backtick left open on one bullet ends with that bullet. Carried on, it would hide the links
    /// of every bullet after it and leave their addresses in the lines.
    #[test]
    fn a_backtick_left_open_does_not_hide_the_links_on_the_next_line() {
        assert_eq!(
            plain("Added the ` key and [x](y)\n - Later [z](w)"),
            "Added the ` key and [x](y)\n - Later z"
        );
    }

    /// A parenthesis inside an address closes only its own pair, or the rest of the address would
    /// be printed after the label.
    #[test]
    fn an_address_with_parentheses_of_its_own_is_one_address() {
        assert_eq!(
            plain("[wiki](https://en.wikipedia.org/wiki/Foo_(bar)) and [b](c)"),
            "wiki and b"
        );
    }

    /// An image and a link in a code span are not links the notes ask to have followed, so they
    /// are shown as written.
    #[test]
    fn an_image_and_a_link_in_a_code_span_are_kept_and_a_link_after_them_is_not() {
        assert_eq!(plain("![alt](u) [z](w)"), "![alt](u) z");
        assert_eq!(plain("`[x](y)` then [z](w)"), "`[x](y)` then z");
        assert_eq!(plain("[a!](u)[b](v)"), "a!b");
    }

    /// The running version's notes are named for it, and a version with none of its own is said
    /// to be showing another's.
    #[test]
    fn a_heading_says_whether_the_notes_are_the_running_versions() {
        let release = Release {
            version: "0.17.0",
            notes: " - Added a thing.",
        };
        let (own, body) = shown(&release, "0.17.0");
        assert_eq!(
            own,
            bravebot_i18n::t!(changelog_heading, version = "0.17.0")
        );
        assert_eq!(body, " - Added a thing.");
        let (other, _) = shown(&release, "0.18.0-dev");
        assert_eq!(
            other,
            bravebot_i18n::t!(changelog_newest, running = "0.18.0-dev", version = "0.17.0")
        );
        assert_ne!(own, other);
    }

    const FIXTURE: &str = "\
Release notes.

## [0.18.0](https://example.test/v0.18.0)

 - Added a thing. ([#1](https://example.test/1))
### [not a release](https://example.test/x)
 - Mentions a `## [0.0.0]` heading in the middle of a line.
## [Unreleased]
 - Under a heading with no address.
## [Docs] see [the docs](https://example.test/docs)
 - Under a heading with prose after the bracket.

## [0.17.0](https://example.test/v0.17.0)

 - Fixed a thing.

## [0.1.0](https://example.test/v0.1.0)
 - First.
";

    /// A release is the lines under its own heading and no others, so the notes of one version are
    /// never shown under another's name.
    #[test]
    fn a_release_is_the_lines_under_its_heading_up_to_the_next_heading() {
        let releases = releases_in(FIXTURE);
        let versions: Vec<&str> = releases.iter().map(|release| release.version).collect();
        assert_eq!(versions, ["0.18.0", "0.17.0", "0.1.0"]);
        assert_eq!(releases[1].notes, " - Fixed a thing.");
        assert_eq!(releases[2].notes, " - First.");
    }

    /// A subheading, a heading with no address and a heading written in the middle of a line are
    /// part of a release's notes. Splitting on them would cut a release short and invent a version.
    #[test]
    fn only_a_line_that_begins_with_the_heading_mark_starts_a_release() {
        let notes = releases_in(FIXTURE)[0].notes;
        assert!(notes.contains("### [not a release]"), "{notes}");
        assert!(notes.contains("a `## [0.0.0]` heading"), "{notes}");
        assert!(notes.contains("## [Unreleased]"), "{notes}");
        assert!(notes.contains("## [Docs] see [the docs]"), "{notes}");
        assert!(notes.starts_with(" - Added a thing."), "{notes}");
    }

    /// Text before the first heading belongs to no release, and a text with no heading holds none.
    #[test]
    fn text_before_the_first_heading_is_no_release() {
        assert!(releases_in("Release notes.\n\nNo headings.\n").is_empty());
        assert!(releases_in("").is_empty());
        assert!(
            !releases_in(FIXTURE)
                .iter()
                .any(|r| r.notes.contains("Release notes."))
        );
    }

    /// The version has to be the whole of a heading's version. A prefix match would answer a build
    /// running `0.1` with the notes of `0.17.0`.
    #[test]
    fn a_release_is_found_by_its_whole_version_and_else_the_newest_is_given() {
        let releases = releases_in(FIXTURE);
        assert_eq!(
            release_for(&releases, "0.1.0").map(|r| r.notes),
            Some(" - First.")
        );
        for unnamed in ["0.1", "0.17", "0.17.0-dev", "9.9.9", ""] {
            assert_eq!(
                release_for(&releases, unnamed).map(|r| r.version),
                Some("0.18.0"),
                "{unnamed:?}"
            );
        }
        assert_eq!(release_for(&[], "0.17.0"), None);
    }

    /// `/changelog` reads the carried file with this parser, so a heading it stops being written
    /// with would leave the command with nothing to show and the tests above passing. The first
    /// release is the one shown where a version has no heading, so it is the newest.
    #[test]
    fn the_carried_changelog_lists_releases_newest_first_and_the_newest_has_notes() {
        fn number(version: &str) -> Vec<u32> {
            version
                .split(['.', '-'])
                .filter_map(|part| part.parse().ok())
                .take(3)
                .collect()
        }
        let releases = releases_in(CHANGELOG);
        assert!(releases.len() > 1);
        assert!(
            !releases[0].notes.is_empty(),
            "{} has no notes",
            releases[0].version
        );
        for pair in releases.windows(2) {
            assert!(
                number(pair[0].version) >= number(pair[1].version),
                "{} is listed before {}",
                pair[0].version,
                pair[1].version
            );
        }
        assert_eq!(release_notes(releases[1].version), Some(releases[1]));
    }
}
