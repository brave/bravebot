//! The same call asked for three times running (TURN-9).
//!
//! The comparison is between two calls the planner wrote, a tool name and its arguments. Nothing
//! here reads a tool result, so the decision depends on no content from the workspace.

use bravebot_aichat::protocol::ToolCall;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_i18n::t;

use crate::report::Reporter;

/// How many identical calls in a row the planner may make. The next one is held.
pub(crate) const IN_A_ROW: usize = 3;

/// The last call the planner made and how many times in a row it has made it.
#[derive(Debug, Default, Clone)]
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

    pub(crate) fn count(&self) -> usize {
        self.count
    }
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

/// Record a held call and say on screen that it was not run.
///
/// Nobody is asked. A person who started a long task and left would come back to a turn waiting on
/// a question, and the planner is told in place of a result, so it can try another approach.
pub(crate) fn hold<S, R>(
    call: &ToolCall,
    count: usize,
    round: usize,
    policy: &mut Policy<'_, S>,
    reporter: &mut R,
) where
    S: Sink,
    R: Reporter + ?Sized,
{
    policy.record_repeated_call(round, count);
    reporter.notice(t!(
        repeated_call_refused,
        tool = call.function.name.as_str(),
        count = count
    ));
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
}
