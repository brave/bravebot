//! Where a question stands in a series the planner is asking, said once for the terminal picker,
//! the line-by-line interface and the desktop bridge so each words it the same way (SHARE-001).
//! It is in the locale the process chose ([`crate::locale`]).
//!
//! The message is reached by its generated item, which is what [`crate::t!`] expands to: an
//! exported macro cannot be named by path inside the crate that generates it.

use crate::messages::AskTitleNumbered;

/// Where the question at `at` (counted from zero) stands among the `total` being put, in the words
/// the picker's title uses, or nothing for a lone question.
///
/// `total` is the number of questions put to the person: one a remembered answer settled is not
/// among them.
pub fn place(at: usize, total: usize) -> Option<String> {
    (total > 1).then(|| AskTitleNumbered { at: at + 1, total }.render())
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
}
