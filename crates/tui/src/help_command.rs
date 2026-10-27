//! `/help` and `/changelog`: answers taken from the documentation and the release notes this
//! program carries (CMD-24, CMD-25).
//!
//! Both texts are the program's own, compiled in, so neither goes through a trust gate. The
//! documentation is `bravebot_agent::help`'s, and `/help` hands a question to a turn with words the
//! driver wrote, which the planner answers from the pages it loads. The release notes are
//! `bravebot_agent::help`'s too, shared with the desktop's Release Notes window, and `/changelog`
//! shows them without a turn. The sentences that go to the planner are planner-facing text and stay
//! out of the message catalog.

pub use bravebot_agent::help::{Release, release_notes};
use bravebot_agent::help::{SKILL, plain};

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

/// What `/changelog` puts in the transcript for `release`, found for the version `running`.
///
/// A build whose own version has no heading in the changelog shows the newest release's notes and
/// says so, so a person is not told these are the notes of the version they run.
pub fn shown(release: &Release<'_>, running: &str) -> (String, String) {
    let heading = if release.version == running {
        bravebot_i18n::t!(changelog_heading, version = release.version)
    } else {
        bravebot_i18n::t!(
            changelog_newest,
            running = running,
            version = release.version
        )
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
}
