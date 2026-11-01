//! The issue or pull request link a planner asked the session to record (SETLINK-1).
//!
//! The session record lives in a crate this one cannot depend on, so a link leaves a turn as the
//! kind and the text of the address, and whoever owns the record reads it as a link again before
//! keeping it.

/// Which of the session's two links a call is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Issue,
    PullRequest,
}

/// A link the planner asked to have recorded, once it was checked against what the person typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSet {
    pub kind: LinkKind,
    pub url: String,
}

/// Whether `url` is, as a whole, one of the words of a line the person typed.
///
/// A word is a run of non-space characters with the quotes, brackets and trailing punctuation a
/// sentence puts around an address taken off. A substring is not enough: `https://a.example/x`
/// is inside `https://a.example/xyz`, and a link the person did not write would pass.
pub fn typed_in(typed: &[String], url: &str) -> bool {
    typed.iter().any(|line| {
        line.split_whitespace().any(|word| {
            word.trim_matches(|c: char| {
                matches!(
                    c,
                    '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'' | '`' | ',' | ';' | '.'
                )
            }) == url
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(all: &[&str]) -> Vec<String> {
        all.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn a_whole_word_of_a_typed_line_is_found() {
        let typed = lines(&["this is for https://github.com/brave/bravebot/issues/1267."]);
        assert!(typed_in(
            &typed,
            "https://github.com/brave/bravebot/issues/1267"
        ));
    }

    #[test]
    fn an_address_wrapped_in_brackets_or_quotes_is_found() {
        let typed = lines(&["see (<https://a.example/x>), and \"https://b.example/y\""]);
        assert!(typed_in(&typed, "https://a.example/x"));
        assert!(typed_in(&typed, "https://b.example/y"));
    }

    #[test]
    fn the_start_of_a_longer_address_is_not_found() {
        let typed = lines(&["https://a.example/xyz"]);
        assert!(!typed_in(&typed, "https://a.example/x"));
    }

    #[test]
    fn an_address_nobody_typed_is_not_found() {
        assert!(!typed_in(&lines(&["fix the build"]), "https://a.example/x"));
        assert!(!typed_in(&[], "https://a.example/x"));
    }
}
