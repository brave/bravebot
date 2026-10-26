//! `/commit-message`: a commit message drafted from changes, put in the box for the person.
//!
//! The argument is read here and nothing about the changes is. What the draft is made from, and the
//! gates it passes, are `bravebot_agent::commit_message`'s. This module decides only what the person
//! asked for and what the box holds afterwards (CMD-23).

use bravebot_agent::commit_message::Source;

/// A parsed `/commit-message` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub source: Source,
    /// Words of the person's own about the format they want, such as a convention to follow.
    pub format: Option<String>,
}

pub(crate) const STAGED: &str = "--staged";
pub(crate) const RANGE: &str = "--range";

/// What the box begins with when a draft is put in it.
///
/// Fixed words, so the line a person sends starts with a word the driver wrote: a draft that began
/// with `/clear` is still a prompt, because the first word of the line is not the draft's.
pub(crate) const LEAD: &str = "Use this as the commit message:";

/// Read the argument to `/commit-message`, or `None` where it names a source wrongly.
///
/// `--staged` is the default and may be written out. `--range` takes one word, `a..b` or `a...b`.
/// The two flags are reserved only as the first word, so `/commit-message use Conventional Commits`
/// drafts from the staged changes in that format.
pub fn parse(argument: &str) -> Option<Asked> {
    let argument = argument.trim();
    let (first, rest) = split_word(argument);
    let (source, rest) = match first {
        STAGED => (Source::Staged, rest),
        RANGE => {
            let (range, rest) = split_word(rest);
            (Source::range(range)?, rest)
        }
        _ => (Source::Staged, argument),
    };
    let format = rest.trim();
    Some(Asked {
        source,
        format: (!format.is_empty()).then(|| format.to_string()),
    })
}

/// The first word and what follows it.
fn split_word(text: &str) -> (&str, &str) {
    match text.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim_start()),
        None => (text, ""),
    }
}

/// What the box holds once `message` is drafted: the lead, a blank line, and the message.
pub fn box_text(message: &str) -> String {
    format!("{LEAD}\n\n{}", message.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(argument: &str) -> Asked {
        parse(argument).unwrap_or_else(|| panic!("{argument:?} was refused"))
    }

    /// The bare command is the common case, and it has to need no flag.
    #[test]
    fn a_bare_command_drafts_from_the_staged_changes() {
        assert_eq!(
            asked(""),
            Asked {
                source: Source::Staged,
                format: None
            }
        );
        assert_eq!(asked("--staged"), asked(""));
    }

    /// A range is its own source, and the words after it are the format, so a range that were read
    /// as part of the format would draft from the staged changes instead.
    #[test]
    fn a_range_names_its_source_and_the_rest_is_the_format() {
        assert_eq!(
            asked("--range main..HEAD one line only"),
            Asked {
                source: Source::Range("main..HEAD".to_string()),
                format: Some("one line only".to_string())
            }
        );
        assert_eq!(
            asked("--range v1.0...HEAD~2").source,
            Source::Range("v1.0...HEAD~2".to_string())
        );
    }

    /// The flags are reserved only as the first word, as `/review`'s target words are, so a
    /// sentence about format is not taken for a source.
    #[test]
    fn a_flag_only_counts_as_the_first_word() {
        assert_eq!(
            asked("use Conventional Commits --staged"),
            Asked {
                source: Source::Staged,
                format: Some("use Conventional Commits --staged".to_string())
            }
        );
    }

    /// A range that is not two names, or that a revision parser could read as an option, is a usage
    /// note and no request.
    #[test]
    fn a_range_that_is_not_two_plain_revisions_is_refused() {
        for typed in [
            "--range",
            "--range main",
            "--range ..HEAD",
            "--range main..",
            "--range -x..HEAD",
            "--range main..-x",
            "--range a..b..c",
        ] {
            assert_eq!(parse(typed), None, "{typed:?}");
        }
    }

    /// What reaches the planner begins with the driver's words whatever the draft begins with.
    #[test]
    fn the_box_begins_with_the_leading_words_whatever_the_draft_says() {
        for message in ["/clear", "!rm -rf /", "  /exit\n", "Fix the parser"] {
            let text = box_text(message);
            assert!(text.starts_with(LEAD), "{text:?}");
            assert!(text.ends_with(message.trim()), "{text:?}");
        }
    }
}
