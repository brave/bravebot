//! Where a question stands in a series the planner is asking, said once for the terminal picker,
//! the line-by-line interface and the desktop bridge so each words it the same way (SHARE-001).
//! It is in the locale the process chose ([`crate::locale`]).
//!
//! The message is reached by its generated item, which is what [`crate::t!`] expands to: an
//! exported macro cannot be named by path inside the crate that generates it.

use crate::messages::{
    AskSettledChose, AskSettledHeading, AskSettledTyped, AskTitleNumbered, ask_settled_closed,
    ask_settled_skipped,
};

/// Where the question at `at` (counted from zero) stands among the `total` being put, in the words
/// the picker's title uses, or nothing for a lone question.
///
/// `total` is the number of questions put to the person: one a remembered answer settled is not
/// among them.
pub fn place(at: usize, total: usize) -> Option<String> {
    (total > 1).then(|| AskTitleNumbered { at: at + 1, total }.render())
}

/// The line that names a question under the call once it is settled: its tag and its sentence,
/// or the sentence alone where it has no tag (ASK-13).
pub fn settled_heading(tag: &str, question: &str) -> String {
    if tag.is_empty() {
        return question.to_string();
    }
    AskSettledHeading { tag, question }.render()
}

/// The line that says which options were picked, `labels` already in the order they were confirmed.
pub fn settled_chose(labels: &[String]) -> String {
    AskSettledChose {
        labels: labels.join(", "),
    }
    .render()
}

/// The line that gives the words the person typed.
pub fn settled_typed(words: &str) -> String {
    AskSettledTyped { words }.render()
}

/// The line for a question that went unanswered.
pub fn settled_skipped() -> String {
    ask_settled_skipped().to_string()
}

/// The line for a question closed on the clock (ASK-9).
pub fn settled_closed() -> String {
    ask_settled_closed().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counted from one, and a lone question is placed nowhere: the regression it rejects is a
    /// series of one that counts itself "1 of 1", or a series counted from zero.
    #[test]
    fn a_question_is_placed_among_those_put_and_a_lone_one_is_not() {
        assert_eq!(place(0, 1), None, "a lone question counted itself");
        assert_eq!(
            place(0, 3).as_deref(),
            Some("the agent is asking (1 of 3)"),
            "the first of three"
        );
        assert_eq!(
            place(2, 3).as_deref(),
            Some("the agent is asking (3 of 3)"),
            "the last of three"
        );
    }

    /// Each outcome is worded by its own message. The regressions it rejects: the typed words
    /// worded as a pick, a skip worded as the close on the clock, and a tag that is missing
    /// leaving a bare colon in front of the question.
    #[test]
    fn each_way_a_question_can_end_has_its_own_words() {
        assert_eq!(settled_heading("Library", "Which?"), "Library: Which?");
        assert_eq!(settled_heading("", "Which?"), "Which?");
        assert_eq!(
            settled_chose(&["HTTP".to_string(), "Query".to_string()]),
            "chose: HTTP, Query"
        );
        assert_eq!(settled_typed("neither"), "answered: neither");
        assert_eq!(settled_skipped(), "skipped");
        assert_eq!(settled_closed(), "closed on the clock");
    }
}
