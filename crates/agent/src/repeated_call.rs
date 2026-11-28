//! The same call asked for three times running (TURN-9).
//!
//! The comparison is between two calls the planner wrote, a tool name and its arguments. Nothing
//! here reads a tool result, so the decision depends on no content from the workspace.

use bravebot_aichat::protocol::ToolCall;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_i18n::t;

use crate::report::Reporter;
use crate::tools::{Beside, Named};

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

    /// Record a call that is never held, such as a bounded wait on a job that is still running
    /// (TURN-9). It is a different call from the ones around it, so the count starts again, and it
    /// is not counted toward the next hold.
    pub(crate) fn passed_without_counting(&mut self) {
        self.last = None;
        self.count = 0;
    }

    pub(crate) fn count(&self) -> usize {
        self.count
    }
}

/// What the planner is told in place of a result when its call is not run.
///
/// The driver's own words: the tool's name is the planner's own and no result is quoted. The
/// route is chosen from that name and from whether the turn's list holds `job_output` (TOOL-6).
pub(crate) fn refusal(tool: &str, count: usize, beside: Beside) -> String {
    let route = match crate::tools::strip_namespace(tool) {
        "job_output" => {
            " A background job's end is reported to you in a message of its own while the turn \
             goes on, so do not look at it again to learn that. To wait for a job that is \
             still running, call job_output with wait_seconds. Do not poll with a shell loop \
             that sleeps."
        }
        "run" if beside.holds(Named::JobOutput) => {
            " To wait for a command, start it with background: true, then call job_output with \
             wait_seconds, and add until_exit: true where only the end of the job matters. Do \
             not wait with a shell loop that sleeps. Such a loop is stopped at its deadline, \
             and one that prints only when it finishes has printed nothing when it is stopped."
        }
        "run" => {
            " To wait for a command, run it once with deadline_seconds long enough for it to \
             finish. Do not wait with a shell loop that sleeps. Such a loop is stopped at its \
             deadline, and one that prints only when it finishes has printed nothing when it is \
             stopped."
        }
        _ => " Try a different call, or carry on with the rest of the task.",
    };
    format!(
        "error: tool call repetition limit reached. You have made the same {tool} call {count} \
         times in a row and it was not run.{route}"
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

    /// TURN-9: a call passed without counting is a different call, so the count starts again and
    /// the call is never held. Rejects: leaving the count alone, which holds the call after it.
    #[test]
    fn a_call_passed_without_counting_starts_the_count_again() {
        let mut repeats = Repeats::default();
        let read = call("read_file", r#"{"path":"a.txt"}"#);
        assert!(!repeats.is_held(&read));
        assert!(!repeats.is_held(&read));
        repeats.passed_without_counting();
        assert_eq!(repeats.count(), 0);
        assert!(!repeats.is_held(&read));
        assert!(!repeats.is_held(&read));
        assert!(repeats.is_held(&read));
    }

    /// TURN-9: the refusal names the tool and the count and gives a route chosen from the tool's
    /// name. Rejects: one text for every tool, and a text that drops the count.
    #[test]
    fn the_refusal_names_the_tool_and_the_count_and_gives_a_route() {
        let job = refusal("job_output", 3, Beside::every());
        assert!(job.contains("repetition limit reached"));
        assert!(job.contains("same job_output call 3 times"));
        assert!(job.contains("message of its own"));
        assert!(job.contains("shell loop"));
        let read = refusal("read_file", 4, Beside::every());
        assert!(read.contains("same read_file call 4 times"));
        assert!(!read.contains("job"));
        assert!(read.contains("Try a different call"));
        assert!(refusal("functions.job_output", 3, Beside::every()).contains("shell loop"));
    }

    /// TURN-9: a held `run` call is told how to wait for a command, since a planner told only to
    /// try something else writes a shell loop that sleeps, which is stopped at its deadline.
    /// Where the turn's list does not hold `job_output`, the route names neither it nor
    /// `background` and says to run the command once with a long enough deadline (TOOL-6).
    /// Rejects: the generic text for `run`, a route that drops `background`, `wait_seconds`,
    /// `until_exit` or what a sleeping loop comes to, a route chosen from the name as written so
    /// that `functions.run` or `functions_run` gets the generic text, a route that quotes a figure
    /// for the deadline or the wait, which settings can change, and a route that names
    /// `job_output` to a turn whose list does not hold it.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_refusal_of_a_run_call_says_to_wait_on_a_background_job() {
        let run = refusal("run", 4, Beside::every());
        assert!(run.contains("same run call 4 times"));
        assert!(!run.contains("Try a different call"));
        assert!(run.contains("background: true"));
        assert!(run.contains("job_output with wait_seconds"));
        assert!(run.contains("until_exit"));
        assert!(run.contains("shell loop that sleeps"));
        assert!(run.contains("stopped at its deadline"));
        assert!(run.contains("printed nothing when it is stopped"));
        let (_, route) = run
            .split_once("it was not run.")
            .expect("a route after the count");
        assert!(!route.chars().any(|c| c.is_ascii_digit()), "{route}");
        assert!(refusal("functions.run", 3, Beside::every()).contains("background: true"));
        assert!(refusal("functions_run", 3, Beside::every()).contains("background: true"));

        let without = Beside::every().without(Named::JobOutput);
        let alone = refusal("run", 4, without);
        assert!(alone.contains("same run call 4 times"), "{alone}");
        assert!(!alone.contains("Try a different call"), "{alone}");
        assert!(!alone.contains("job_output"), "{alone}");
        assert!(!alone.contains("background"), "{alone}");
        assert!(alone.contains("deadline_seconds long enough"), "{alone}");
        assert!(alone.contains("shell loop that sleeps"), "{alone}");
        assert!(alone.contains("stopped at its deadline"), "{alone}");
        let (_, route) = alone
            .split_once("it was not run.")
            .expect("a route after the count");
        assert!(!route.chars().any(|c| c.is_ascii_digit()), "{route}");
        assert!(refusal("functions.run", 3, without).contains("deadline_seconds long enough"));
    }
}
