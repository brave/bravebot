//! The same call asked for three times running (TURN-9).
//!
//! The comparison is between two calls the planner wrote, a tool name and its arguments. Nothing
//! here reads a tool result, so the decision depends on no content from the workspace.

use std::sync::atomic::{AtomicU64, Ordering};

use bravebot_aichat::protocol::ToolCall;
use bravebot_core::ask::{Answer, Asking, Choice, Question, Series};
use bravebot_core::event::Sink;
use bravebot_core::policy::{AfterRepeatedCall, Policy};
use bravebot_i18n::t;

use crate::confirm::Confirmer;
use crate::report::Reporter;

/// How many identical calls in a row the planner may make. The next one is held.
pub(crate) const IN_A_ROW: usize = 3;

/// What the person chose, or the planner is told when nobody could be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    /// The person let the call through, once.
    Run,
    /// The call is not run and the planner is told why.
    Refuse,
    /// The person ended the turn.
    Stop,
}

/// The row that refuses the call. First, so a bare Enter on the question is the safe answer.
const REFUSE: usize = 0;
const RUN_IT: usize = 1;
const STOP: usize = 2;

/// The last call the planner made and how many times in a row it has made it.
#[derive(Debug, Default)]
pub(crate) struct Repeats {
    last: Option<(String, String)>,
    count: usize,
}

impl Repeats {
    /// Count a call and say whether it is one the planner has now made [`IN_A_ROW`] times or more
    /// with nothing different between.
    ///
    /// Arguments are compared as parsed JSON, which has sorted keys, so the same object written
    /// with its keys in another order is the same call. Arguments that are not JSON are compared
    /// as the bytes the planner wrote.
    pub(crate) fn is_held(&mut self, call: &ToolCall) -> bool {
        let arguments = match call.arguments() {
            Ok(value) => value.to_string(),
            Err(_) => call.function.arguments.clone().unwrap_or_default(),
        };
        let key = (call.function.name.clone(), arguments);
        if self.last.as_ref() == Some(&key) {
            self.count += 1;
        } else {
            self.last = Some(key);
            self.count = 1;
        }
        self.count >= IN_A_ROW
    }

    /// Start counting again. The person let the call through, and the answer covers that call.
    pub(crate) fn clear(&mut self) {
        self.last = None;
        self.count = 0;
    }

    pub(crate) fn count(&self) -> usize {
        self.count
    }
}

/// The question, built from a count and the tool's name.
///
/// Its key is new each time, because an interface remembers an answer by key and this one is about
/// the call in front of it.
fn asking(tool: &str, count: usize) -> Asking {
    static ASKED: AtomicU64 = AtomicU64::new(0);
    let mut asking = bravebot_core::ask::asking(&Series::new(vec![Question::new(
        t!(repeated_call_header),
        t!(repeated_call_reached, tool = tool, count = count).to_string(),
        vec![
            Choice::new(t!(repeated_call_refuse), None),
            Choice::new(t!(repeated_call_run_it), None),
            Choice::new(t!(repeated_call_stop), None),
        ],
        false,
    )]));
    for prompt in &mut asking.prompts {
        prompt.key = format!("repeated call {}", ASKED.fetch_add(1, Ordering::Relaxed));
    }
    asking
}

/// What the planner is told in place of a result when its call is not run.
///
/// The driver's own words: the tool's name is the planner's own and no result is quoted.
pub(crate) fn refusal(tool: &str, count: usize) -> String {
    format!(
        "error: tool call repetition limit reached. You have made the same {tool} call {count} \
         times in a row and it was not run. Try a different approach."
    )
}

/// Put the question about a held call, record the answer and say what came of it.
///
/// A delegate is not asked: its questions would reach a person who is watching the session and not
/// this worker, so its call is refused. So is the call of an interface that cannot ask, and any
/// answer that is not one of the rows.
pub(crate) fn hold<S, C, R>(
    call: &ToolCall,
    count: usize,
    round: usize,
    may_ask: bool,
    policy: &mut Policy<'_, S>,
    confirmer: &mut C,
    reporter: &mut R,
) -> Held
where
    S: Sink,
    C: Confirmer + ?Sized,
    R: Reporter + ?Sized,
{
    let tool = call.function.name.as_str();
    let (held, then) = if may_ask {
        match confirmer.ask_user(&asking(tool, count)).as_slice() {
            [Answer::Chosen(rows)] if rows.as_slice() == [RUN_IT] => {
                (Held::Run, AfterRepeatedCall::LetThrough)
            }
            [Answer::Chosen(rows)] if rows.as_slice() == [STOP] => {
                (Held::Stop, AfterRepeatedCall::Stopped)
            }
            [Answer::Chosen(rows)] if rows.as_slice() == [REFUSE] => {
                (Held::Refuse, AfterRepeatedCall::Refused)
            }
            _ => (Held::Refuse, AfterRepeatedCall::Unanswered),
        }
    } else {
        (Held::Refuse, AfterRepeatedCall::Unanswered)
    };
    policy.record_repeated_call(round, count, then);
    match held {
        Held::Run => {}
        Held::Refuse => reporter.notice(t!(repeated_call_refused, tool = tool, count = count)),
        Held::Stop => reporter.notice(t!(repeated_call_stopped, tool = tool, count = count)),
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: &str) -> ToolCall {
        serde_json::from_value(serde_json::json!({
            "id": "c1",
            "type": "function",
            "function": {"name": name, "arguments": arguments},
        }))
        .expect("a tool call")
    }

    #[test]
    fn the_third_identical_call_is_held_and_the_first_two_are_not() {
        let mut repeats = Repeats::default();
        let read = call("read_file", r#"{"path":"a.txt"}"#);
        assert!(!repeats.is_held(&read));
        assert!(!repeats.is_held(&read));
        assert!(repeats.is_held(&read));
        assert_eq!(repeats.count(), 3);
    }

    #[test]
    fn a_different_argument_starts_the_count_again() {
        let mut repeats = Repeats::default();
        assert!(!repeats.is_held(&call("read_file", r#"{"path":"a.txt"}"#)));
        assert!(!repeats.is_held(&call("read_file", r#"{"path":"a.txt"}"#)));
        assert!(!repeats.is_held(&call("read_file", r#"{"path":"b.txt"}"#)));
        assert!(!repeats.is_held(&call("read_file", r#"{"path":"a.txt"}"#)));
    }

    #[test]
    fn the_same_arguments_in_another_key_order_are_the_same_call() {
        let mut repeats = Repeats::default();
        assert!(!repeats.is_held(&call("grep", r#"{"pattern":"x","path":"a"}"#)));
        assert!(!repeats.is_held(&call("grep", r#"{"path":"a","pattern":"x"}"#)));
        assert!(repeats.is_held(&call("grep", r#"{ "pattern": "x", "path": "a" }"#)));
    }

    #[test]
    fn the_same_arguments_to_another_tool_are_another_call() {
        let mut repeats = Repeats::default();
        assert!(!repeats.is_held(&call("read_file", r#"{"path":"a"}"#)));
        assert!(!repeats.is_held(&call("list_dir", r#"{"path":"a"}"#)));
        assert!(!repeats.is_held(&call("read_file", r#"{"path":"a"}"#)));
    }

    #[test]
    fn clearing_starts_the_count_again() {
        let mut repeats = Repeats::default();
        let read = call("read_file", r#"{"path":"a.txt"}"#);
        repeats.is_held(&read);
        repeats.is_held(&read);
        assert!(repeats.is_held(&read));
        repeats.clear();
        assert!(!repeats.is_held(&read));
    }

    #[test]
    fn every_question_has_a_key_of_its_own() {
        let first = asking("read_file", 3);
        let second = asking("read_file", 3);
        assert_ne!(first.prompts[0].key, second.prompts[0].key);
    }

    #[test]
    fn the_first_row_is_the_one_that_refuses() {
        let asked = asking("read_file", 3);
        assert_eq!(
            asked.prompts[0].rows[REFUSE].label,
            t!(repeated_call_refuse)
        );
        assert_eq!(
            asked.prompts[0].rows[RUN_IT].label,
            t!(repeated_call_run_it)
        );
        assert_eq!(asked.prompts[0].rows[STOP].label, t!(repeated_call_stop));
    }
}
