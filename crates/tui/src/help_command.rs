//! `/help`: the turn that answers a question about bravebot from the documentation this program
//! carries (CMD-24).
//!
//! The documentation is `bravebot_agent::help`'s, compiled in, so it goes through no trust gate.
//! `/help` hands a question to a turn with words the driver wrote, which the planner answers from the
//! pages it loads. The sentence that goes to the planner is planner-facing text and stays out of the
//! message catalog.

use bravebot_agent::help::SKILL;

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
}
