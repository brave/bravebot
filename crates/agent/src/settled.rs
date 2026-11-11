//! What the person sees under an `ask_user` call once its questions are settled (ASK-13).
//!
//! The call's own line says only how many questions were asked (ASK-10). Without the questions
//! and the answers a person scrolling back has to ask the planner what was decided. The rows are
//! built here once from the [`Asking`] that was shown and the [`Answer`] values the interface
//! returned, so the terminal, the line interface and the desktop window word them alike
//! (SHARE-001).
//!
//! For a screen only. Nothing here is given to the planner, written to the session record or
//! sent in a request, and nothing here looks at what a question or an answer says: the rows are
//! assembled by the shape of each [`Outcome`].

use crate::report::Returned;
use bravebot_core::ask::{Answer, Asking, Outcome, settled_rows};
use bravebot_i18n::ask as words;

/// The margin before an outcome line, which sets it under its question.
const UNDER: &str = "  ";

/// One text on one line: a newline or any other control character becomes a space, so a typed
/// answer or a planner's question cannot start a line of its own.
fn flat(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The outcome of a question in words, on one line.
fn outcome(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Chose(labels) => {
            let labels: Vec<String> = labels.iter().map(|label| flat(label)).collect();
            words::settled_chose(&labels)
        }
        Outcome::Typed(text) => words::settled_typed(&flat(text)),
        Outcome::Skipped => words::settled_skipped(),
        Outcome::ClosedOnTheClock => words::settled_closed(),
    }
}

/// The question and what became of it, each as one line, for every question put. The terminal and
/// the line interface print these as a pair of lines, and the desktop window draws the pair.
pub fn pairs(asking: &Asking, answers: &[Answer]) -> Vec<(String, String)> {
    settled_rows(asking, answers)
        .iter()
        .map(|row| {
            (
                words::settled_heading(&flat(&row.header), &flat(&row.question)),
                outcome(&row.outcome),
            )
        })
        .collect()
}

/// The lines for a series: for each question put, its tag and sentence, then its outcome.
///
/// `asking` is the questions that were put to the person and `answers` what the interface gave
/// back for them. A question a remembered answer settled is not among them (ASK-8).
pub fn lines(asking: &Asking, answers: &[Answer]) -> Vec<String> {
    pairs(asking, answers)
        .into_iter()
        .flat_map(|(heading, outcome)| [heading, format!("{UNDER}{outcome}")])
        .collect()
}

/// The lines as a glimpse to put under the call, which every row of the glimpse is shown whole.
pub fn returned(asking: &Asking, answers: &[Answer]) -> Returned {
    let lines = lines(asking, answers);
    Returned {
        total: lines.len(),
        whole: lines.clone(),
        lines,
        from_the_end: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::ask::{Choice, Question, Series, asking};

    fn series() -> Asking {
        asking(&Series::new(vec![
            Question::new(
                "Library",
                "Which library?",
                vec![Choice::new("serde", None), Choice::new("miniserde", None)],
                false,
            ),
            Question::new("Name", "What to call it?", Vec::new(), false),
            Question::new("", "Ship it?", vec![Choice::new("yes", None)], false),
        ]))
    }

    /// Question then outcome for each, in the order asked. The regressions it rejects: an
    /// outcome drawn above its question, a tagless question drawn with a bare colon, the typed
    /// words drawn as a pick, and a row per answer rather than per question.
    #[test]
    fn each_question_is_followed_by_what_became_of_it() {
        let said = lines(
            &series(),
            &[
                Answer::Chosen(vec![1]),
                Answer::Typed("own words".into()),
                Answer::TimedOut,
            ],
        );
        assert_eq!(
            said,
            [
                "Library: Which library?",
                "  chose: miniserde",
                "Name: What to call it?",
                "  answered: own words",
                "Ship it?",
                "  closed on the clock",
            ]
        );
    }

    /// A skipped question and one nobody answered both read as skipped; neither is given an
    /// answer. The regression it rejects: a missing answer drawn as an empty pick.
    #[test]
    fn a_question_with_no_answer_reads_as_skipped() {
        let said = lines(&series(), &[Answer::Declined]);
        assert_eq!(said[1], "  skipped");
        assert_eq!(said[3], "  skipped");
        assert_eq!(said[5], "  skipped");
    }

    /// Typed words and a planner's question stay on the line they were put on. The regression it
    /// rejects: a newline in either starting a line the person could read as another question or
    /// as the session's own words.
    #[test]
    fn nothing_typed_or_asked_starts_a_line_of_its_own() {
        let said = lines(
            &series(),
            &[
                Answer::Declined,
                Answer::Typed("one\nSkipped\ntwo".into()),
                Answer::Declined,
            ],
        );
        assert_eq!(said.len(), 6);
        assert_eq!(said[3], "  answered: one Skipped two");
    }

    /// The glimpse holds the same lines whole, so nothing is left for a key to expand.
    #[test]
    fn the_glimpse_is_the_lines_and_nothing_cut() {
        let shown = returned(&series(), &[Answer::Declined]);
        assert_eq!(shown.lines, shown.whole);
        assert_eq!(shown.total, 6);
        assert!(!shown.from_the_end);
    }
}
