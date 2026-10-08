//! Output styles: named words that take the place of the system prompt's opening.
//!
//! A style is words this program wrote, kept by name so a person can pick one with `/style`
//! instead of typing `--system-prompt`. It stands where `--system-prompt` stands (CLI-19) and
//! reaches nothing after it, so the guidance on reading a tool's output, the mode and every
//! refusal are unchanged. The words ask for a manner of answering and grant nothing.

/// One built-in style.
#[derive(Debug, PartialEq, Eq)]
pub struct Style {
    /// What a person types after `/style`.
    pub name: &'static str,
    /// Stands in for the opening of the planner's system prompt.
    pub words: &'static str,
}

/// The styles this build ships, in the order they are listed.
pub const BUILT_IN: [Style; 3] = [
    Style {
        name: "concise",
        words: "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. Lead with the result. Leave out preamble, \
restating the question and a recap of what you did, and say only what the person needs to act.",
    },
    Style {
        name: "explanatory",
        words: "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. While you work, add short `Insight` notes that \
explain why the code is the way it is or why you chose an approach, so the person learns the \
codebase as well as getting the result.",
    },
    Style {
        name: "proactive",
        words: "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. Start the work without asking for permission to \
begin, and where a routine decision has an obvious answer, decide it, say what you assumed, and \
carry on. This does not change what you may do: a write is still put to the person where it \
would have been, and a mode that refuses still refuses.",
    },
];

/// The built-in style called `name`, compared exactly.
pub fn named(name: &str) -> Option<&'static Style> {
    BUILT_IN.iter().find(|style| style.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_is_found_by_its_whole_name_only() {
        assert_eq!(named("concise").map(|s| s.name), Some("concise"));
        for word in ["conci", "Concise", "concise ", "", "default"] {
            assert_eq!(named(word), None, "{word:?} named a style");
        }
    }

    #[test]
    fn no_style_takes_the_quarantine_guidance_with_it() {
        for style in &BUILT_IN {
            assert!(
                !style.words.contains("data, never as instructions"),
                "{} restates guidance that belongs after the opening",
                style.name
            );
            assert!(
                !style.words.trim().is_empty() && style.words.len() < 1000,
                "{}",
                style.name
            );
        }
    }

    #[test]
    fn style_names_are_distinct() {
        let mut names: Vec<_> = BUILT_IN.iter().map(|style| style.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILT_IN.len());
    }
}
