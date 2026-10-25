//! What a call to a backend has spent so far, and how it waits between attempts.
//!
//! Every client here counts the requests it sent, keeps the reported usage of the attempts that
//! completed, and waits out a pause in slices that a stop can interrupt. The rules are the same
//! for each, so they are written once.

use crate::protocol::{ChatRequest, Usage};
use crate::{ChatError, Completion, Refusals, refuses_the_body, remember, remembered};
use bravebot_core::cancel::Cancel;
use std::time::Duration;

/// How often a wait looks up to see whether it should stop.
const SLICE: Duration = Duration::from_millis(50);

/// What one call has spent: the requests sent and the usage the service reported for them.
#[derive(Debug, Default, Clone, Copy)]
pub struct Spent {
    /// Requests handed to egress in the last call, including capability probes.
    pub attempts: u32,
    /// The usage the attempt in progress, or the one that just ended, reported.
    pub completed: Option<Usage>,
    /// The usage of the attempts before that one, which a new attempt sets aside so that
    /// `completed` holds only the attempt that just ended.
    pub retried: Option<Usage>,
}

impl Spent {
    /// Forget everything a previous call spent.
    pub fn begin_call(&mut self) {
        self.attempts = 0;
        self.completed = None;
        self.retried = None;
    }

    /// Set aside what the call has been billed so far, so that what the next attempt reports
    /// replaces nothing.
    pub fn begin_attempt(&mut self) {
        self.retried = self.total();
        self.completed = None;
    }

    /// Count a request handed to egress.
    pub fn sent(&mut self) {
        self.attempts += 1;
    }

    /// Keep what the service reported for the attempt in progress. `None` leaves the attempt with
    /// no measured usage.
    pub fn record(&mut self, usage: Option<Usage>) {
        self.completed = usage;
    }

    /// Total reported usage of completed attempts in the call, including retries. Incomplete
    /// attempts add nothing. `None` if no usage was reported.
    pub fn total(&self) -> Option<Usage> {
        match (self.retried, self.completed) {
            (Some(mut earlier), Some(current)) => {
                earlier.add(current);
                Some(earlier)
            }
            (earlier, current) => earlier.or(current),
        }
    }

    /// The final attempt's measured prompt size, separate from the cost of earlier attempts.
    pub fn last_prompt_tokens(&self) -> Option<u64> {
        self.completed.map(|usage| usage.prompt_tokens)
    }

    /// Add the usage of earlier attempts to the reply the last one produced.
    pub fn bill(&self, completion: &mut Completion) {
        if let Some(usage) = self.retried {
            completion.usage.add(usage);
        }
    }
}

/// How a client waits between attempts and learns that it should stop.
#[derive(Debug, Clone)]
pub struct Pace {
    /// Checked between attempts, between chunks and during a wait.
    pub cancel: Option<Cancel>,
    /// How long to wait before the first retry. Doubled for each retry after that.
    pub backoff: Duration,
}

impl Pace {
    pub fn new(backoff: Duration) -> Self {
        Self {
            cancel: None,
            backoff,
        }
    }

    /// Whether the caller has said to stop.
    pub fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(Cancel::is_cancelled)
    }

    /// Wait out a backoff, or give up on it when the caller says to stop.
    ///
    /// Slept in slices, because a stop landing in the middle of a seconds-long pause would otherwise
    /// wait the rest of it out with nothing to stop but a sleep. Returns whether the wait finished,
    /// so a stop is answered rather than swallowed.
    pub fn wait(&self, how_long: Duration) -> bool {
        let until = std::time::Instant::now() + how_long;
        loop {
            if self.cancelled() {
                return false;
            }
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return true;
            }
            std::thread::sleep(left.min(SLICE));
        }
    }
}

/// Whether an effort-only client still sends the level a turn asks for, and what its model was
/// known to refuse before the request.
///
/// The Google and Responses clients give up one thing on a refusal, the thinking level, so they
/// share this. The clients that give up several things keep their own state.
#[derive(Debug, Clone)]
pub struct EffortRefused {
    /// True until a model refuses the level, which is the only way to find out that it reads none:
    /// nothing in a roster says which models do.
    pub effort: bool,
    /// What that model was known to refuse before this request, which a probe that settled
    /// nothing puts back.
    pub recalled: Refusals,
}

impl Default for EffortRefused {
    fn default() -> Self {
        Self {
            effort: true,
            recalled: Refusals::default(),
        }
    }
}

impl EffortRefused {
    /// Start a request from what the model under `key` has already been found to refuse.
    pub fn recall(&mut self, key: &str) {
        let refusals = remembered(key);
        self.effort = !refusals.effort;
        self.recalled = refusals;
    }

    /// Settle what a probe found, once the request it was part of has finished one way or the other.
    ///
    /// A probe that answered leaves what it gave up dropped, which is the model saying it does not
    /// read it. A probe that failed as well says the level was not what the service refused, so it
    /// goes back to what was known before the request: a request can be refused on its contents
    /// for reasons that have nothing to do with it.
    pub fn probe_settled(&mut self, key: &str, probed: bool, failed: bool) {
        if !probed {
            return;
        }
        if failed {
            self.effort = !self.recalled.effort;
            return;
        }
        remember(
            key,
            Refusals {
                effort: !self.effort,
                ..Refusals::default()
            },
        );
    }

    /// Whether this failure is worth sending the same request again without its level.
    ///
    /// Only where the request carried one: a request without a level was not refused for it, and
    /// giving it up there would record a refusal nobody saw.
    pub fn worth_dropping(&self, request: &ChatRequest, error: &ChatError) -> bool {
        self.effort && request.effort.is_some() && refuses_the_body(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Effort;
    use bravebot_net::EgressError;

    fn usage(prompt: u64, completion: u64) -> Usage {
        Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            ..Usage::default()
        }
    }

    fn completion_with(usage: Usage) -> Completion {
        Completion {
            content: bravebot_core::value::Labelled::new(
                String::new(),
                bravebot_core::label::Label::untrusted_public(),
            ),
            model: String::new(),
            calls: Vec::new(),
            context_tokens: 0,
            usage,
            cut_off: None,
        }
    }

    #[test]
    fn the_total_is_the_retried_usage_plus_the_completed_usage() {
        let spent = Spent {
            attempts: 2,
            retried: Some(usage(10, 1)),
            completed: Some(usage(20, 2)),
        };
        assert_eq!(spent.total(), Some(usage(30, 3)));
        // The prompt size is the last attempt's alone.
        assert_eq!(spent.last_prompt_tokens(), Some(20));
    }

    #[test]
    fn a_measured_zero_stays_known_and_nothing_measured_stays_unknown() {
        let zero = Spent {
            completed: Some(Usage::default()),
            ..Spent::default()
        };
        assert_eq!(zero.total(), Some(Usage::default()));
        assert_eq!(zero.last_prompt_tokens(), Some(0));

        let nothing = Spent::default();
        assert_eq!(nothing.total(), None);
        assert_eq!(nothing.last_prompt_tokens(), None);
    }

    #[test]
    fn beginning_an_attempt_keeps_what_the_earlier_ones_were_billed() {
        let mut spent = Spent::default();
        spent.begin_call();
        spent.sent();
        spent.record(Some(usage(10, 1)));
        spent.begin_attempt();
        spent.sent();
        assert_eq!(spent.completed, None);
        assert_eq!(spent.total(), Some(usage(10, 1)));
        spent.record(Some(usage(5, 2)));
        assert_eq!(spent.total(), Some(usage(15, 3)));
        assert_eq!(spent.last_prompt_tokens(), Some(5));
        assert_eq!(spent.attempts, 2);

        // A third attempt that reports nothing still carries both earlier bills.
        spent.begin_attempt();
        spent.sent();
        assert_eq!(spent.completed, None);
        assert_eq!(spent.last_prompt_tokens(), None);
        assert_eq!(spent.total(), Some(usage(15, 3)));
        spent.record(Some(usage(1, 1)));
        assert_eq!(spent.total(), Some(usage(16, 4)));
        assert_eq!(spent.attempts, 3);
    }

    #[test]
    fn an_attempt_that_reports_nothing_replaces_the_one_before_it_with_nothing() {
        let mut spent = Spent::default();
        spent.record(Some(usage(10, 1)));
        spent.record(None);
        assert_eq!(spent.completed, None);
        assert_eq!(spent.total(), None);
    }

    fn request_with(effort: Option<Effort>) -> ChatRequest {
        ChatRequest::new("model", Vec::new()).with_effort(effort)
    }

    fn status(code: u16) -> ChatError {
        ChatError::Egress(EgressError::Status {
            url: "https://example.invalid/".to_string(),
            status: code,
            retry_after: None,
            body: None,
        })
    }

    #[test]
    fn the_level_is_dropped_only_for_a_refusal_of_a_request_that_carried_one() {
        let refused = EffortRefused::default();
        let with_level = request_with(Some(Effort::High));
        assert!(refused.worth_dropping(&with_level, &status(400)));
        assert!(refused.worth_dropping(&with_level, &status(422)));
        // Not a refusal of the contents.
        assert!(!refused.worth_dropping(&with_level, &status(500)));
        assert!(!refused.worth_dropping(&with_level, &status(401)));
        // Nothing was sent, so nothing was refused.
        assert!(!refused.worth_dropping(&request_with(None), &status(400)));
        // Already given up.
        let given_up = EffortRefused {
            effort: false,
            ..EffortRefused::default()
        };
        assert!(!given_up.worth_dropping(&with_level, &status(400)));
    }

    #[test]
    fn a_request_starts_from_what_the_model_was_known_to_refuse() {
        let key = "call-test\nrecall-known";
        remember(
            key,
            Refusals {
                effort: true,
                ..Refusals::default()
            },
        );
        let mut refused = EffortRefused::default();
        refused.recall(key);
        assert!(!refused.effort);
        assert!(refused.recalled.effort);

        let mut fresh = EffortRefused {
            effort: false,
            ..EffortRefused::default()
        };
        fresh.recall("call-test\nrecall-unknown");
        assert!(fresh.effort, "an unknown model is asked with the level");
        assert_eq!(fresh.recalled, Refusals::default());
    }

    #[test]
    fn a_probe_that_answered_remembers_the_refusal() {
        let key = "call-test\nprobe-answered";
        let mut refused = EffortRefused::default();
        refused.recall(key);
        refused.effort = false;
        refused.probe_settled(key, true, false);
        assert!(remembered(key).effort);
    }

    #[test]
    fn a_probe_that_failed_as_well_remembers_nothing_and_goes_back() {
        let key = "call-test\nprobe-failed";
        let mut refused = EffortRefused::default();
        refused.recall(key);
        refused.effort = false;
        refused.probe_settled(key, true, true);
        assert!(refused.effort, "the level goes back to what was known");
        assert_eq!(remembered(key), Refusals::default());

        // Known to be refused before the request: a failed probe puts that back.
        let known = "call-test\nprobe-failed-known";
        remember(
            known,
            Refusals {
                effort: true,
                ..Refusals::default()
            },
        );
        let mut again = EffortRefused::default();
        again.recall(known);
        again.probe_settled(known, true, true);
        assert!(!again.effort);
        assert!(remembered(known).effort);
    }

    #[test]
    fn a_request_that_probed_nothing_settles_nothing() {
        let key = "call-test\nnot-probed";
        let mut refused = EffortRefused::default();
        refused.recall(key);
        refused.effort = false;
        refused.probe_settled(key, false, false);
        refused.probe_settled(key, false, true);
        assert!(!refused.effort);
        assert_eq!(remembered(key), Refusals::default());
    }

    #[test]
    fn beginning_a_call_clears_the_count_and_both_usages() {
        let mut spent = Spent {
            attempts: 3,
            retried: Some(usage(1, 1)),
            completed: Some(usage(2, 2)),
        };
        spent.begin_call();
        assert_eq!(spent.attempts, 0);
        assert_eq!(spent.total(), None);
        assert_eq!(spent.last_prompt_tokens(), None);
    }

    #[test]
    fn billing_adds_the_earlier_attempts_to_the_reply() {
        let spent = Spent {
            attempts: 2,
            retried: Some(usage(10, 1)),
            completed: Some(usage(20, 2)),
        };
        let mut completion = completion_with(usage(20, 2));
        spent.bill(&mut completion);
        assert_eq!(completion.usage, usage(30, 3));

        let mut alone = completion_with(usage(20, 2));
        Spent::default().bill(&mut alone);
        assert_eq!(alone.usage, usage(20, 2));
    }

    #[test]
    fn a_wait_ends_early_and_says_so_when_a_stop_lands_in_the_middle_of_it() {
        let cancel = Cancel::new();
        let mut pace = Pace::new(Duration::ZERO);
        pace.cancel = Some(cancel.clone());
        let stopper = std::thread::spawn({
            let cancel = cancel.clone();
            move || {
                std::thread::sleep(Duration::from_millis(120));
                cancel.cancel();
            }
        });
        let started = std::time::Instant::now();
        let finished = pace.wait(Duration::from_secs(20));
        stopper.join().expect("the stopper thread panicked");
        assert!(!finished, "a stopped wait must say it did not finish");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the wait slept through the stop"
        );
    }

    #[test]
    fn a_wait_nobody_stops_finishes_and_says_so() {
        let pace = Pace::new(Duration::ZERO);
        assert!(pace.wait(Duration::from_millis(60)));
        assert!(pace.wait(Duration::ZERO));
        let mut idle = Pace::new(Duration::ZERO);
        idle.cancel = Some(Cancel::new());
        assert!(idle.wait(Duration::from_millis(60)));
    }

    #[test]
    fn a_wait_started_after_a_stop_does_not_sleep() {
        let cancel = Cancel::new();
        cancel.cancel();
        let mut pace = Pace::new(Duration::ZERO);
        pace.cancel = Some(cancel);
        let started = std::time::Instant::now();
        assert!(!pace.wait(Duration::from_secs(20)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
