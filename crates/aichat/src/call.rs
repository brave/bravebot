//! What a call to a backend has spent so far, and how it waits between attempts.
//!
//! Every client here counts the requests it sent, keeps the reported usage of the attempts that
//! completed, and waits out a pause in slices that a stop can interrupt. The rules are the same
//! for each, so they are written once.

use crate::protocol::{ChatRequest, Usage};
use crate::{Completion, Refusals};
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

/// One optional field of a request that a service may refuse, and that a client can leave out of
/// the request sent next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Concession {
    /// The cache lifetime the settings chose.
    CacheTtl,
    /// The marks on the prefix to cache.
    Breakpoints,
    /// The effort level a turn was given.
    Effort,
    /// The reply ceiling a client assumes where nobody stated one.
    Ceiling,
    /// The ask for a streamed tool's arguments as they are written.
    ArgumentsAsWritten,
    /// The ask to show thinking as it happens.
    ThinkingShown,
}

impl Refusals {
    /// Whether `concession` has been given up.
    pub fn has_given_up(&self, concession: Concession) -> bool {
        match concession {
            Concession::CacheTtl => self.cache_ttl,
            Concession::Breakpoints => self.caching,
            Concession::Effort => self.effort,
            Concession::Ceiling => self.ceiling,
            Concession::ArgumentsAsWritten => self.arguments_as_written,
            Concession::ThinkingShown => self.thinking_shown,
        }
    }

    /// Give `concession` up, so that the next request is built without it.
    pub fn give_up(&mut self, concession: Concession) {
        match concession {
            Concession::CacheTtl => self.cache_ttl = true,
            Concession::Breakpoints => self.caching = true,
            Concession::Effort => self.effort = true,
            Concession::Ceiling => self.ceiling = true,
            Concession::ArgumentsAsWritten => self.arguments_as_written = true,
            Concession::ThinkingShown => self.thinking_shown = true,
        }
    }
}

/// What the attempt driver asks of a client: the state it keeps, how it classifies its own errors,
/// and where it keeps what a model refused.
///
/// A client implements this once, so the loop that sends, concedes, waits and stops is written
/// once and a client differs only in the answers it gives here.
pub trait Driven {
    /// The error the client reports, which is its own type.
    type Error;

    /// The error that says the caller asked to stop.
    fn cancelled() -> Self::Error;

    /// What the call has spent so far.
    fn spent_mut(&mut self) -> &mut Spent;

    /// The stop to watch for and the wait between attempts.
    fn pace(&self) -> &Pace;

    /// What this call has given up so far, which the client builds each request from.
    fn refusals(&self) -> &Refusals;

    /// The same, to give another field up.
    fn refusals_mut(&mut self) -> &mut Refusals;

    /// What the model under `key` is known to refuse.
    fn recall(&self, key: &str) -> Refusals;

    /// Keep what the model under `key` was found to refuse.
    fn remember(&self, key: &str, found: Refusals);

    /// Whether this failure is the service refusing `concession` on `request`.
    ///
    /// Decided from the status and from what the request carried, and never from a body. The driver
    /// has already ruled out a concession that was given up.
    fn refused(&self, request: &ChatRequest, concession: Concession, error: &Self::Error) -> bool;

    /// Whether attempt number `attempt` failing with `error` is worth another attempt.
    fn asks_again(&self, attempt: u32, error: &Self::Error) -> bool;

    /// How long to wait after `failures` failed attempts, `error` being the last.
    fn pause(&self, failures: u32, error: &Self::Error) -> Duration;

    /// Write the decision to send again to the diagnostic log.
    fn log_retry(&self, attempt: u32, pause: Duration);

    /// Repair what made `error` happen, where the client can, and say whether it did.
    ///
    /// True sends the request again at once, counting against no limit and announcing nothing, as
    /// a concession does. A client that repairs nothing keeps this default. An error returned here
    /// ends the call.
    fn mend(&mut self, _error: &Self::Error) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

/// What differs between one call of the driver and another, other than the client.
#[derive(Debug, Clone, Copy)]
pub struct Plan<'a> {
    /// What the model's refusals are remembered under.
    pub key: &'a str,
    /// The fields to give up, first to last. A refusal gives up the first of these the client says
    /// it could be about and that is still being sent, so a field the service is least likely to
    /// refuse comes last and is blamed last.
    pub order: &'a [Concession],
}

/// Send a request until it is answered, giving up what the service refuses, waiting out a pause,
/// and stopping when asked.
///
/// The caller has begun the call ([`Spent::begin_call`]). `send` makes one attempt and receives
/// `context` and the number of the attempt; `announce` is told the number of the attempt about to
/// follow a pause, before the wait, so that a caller showing a reply can say why it went quiet.
///
/// A refusal of the request's contents is answered with the next concession in `plan.order` and
/// sent again at once, and counts against no limit. A failure the client mends ([`Driven::mend`])
/// is sent again at once the same way. Anything else the client says is worth another attempt
/// waits `pause` and counts. What answered after a concession is remembered, and a probe
/// that failed as well remembers nothing and puts back what was known before the request.
pub fn drive<C, X, Send, Announce>(
    client: &mut C,
    request: &ChatRequest,
    plan: Plan<'_>,
    context: &mut X,
    mut send: Send,
    mut announce: Announce,
) -> Result<Completion, C::Error>
where
    C: Driven,
    Send: FnMut(&mut C, &mut X, u32) -> Result<Completion, C::Error>,
    Announce: FnMut(&mut X, u32),
{
    let recalled = client.recall(plan.key);
    *client.refusals_mut() = recalled;
    let mut probed = false;
    let mut attempt = 1;
    loop {
        // Moves what the last attempt was billed into the call's total before the next replaces it.
        client.spent_mut().begin_attempt();
        // Before the request is built, let alone sent. A stop that landed while the last attempt was
        // failing is still a stop, and spending a credential on a reply nobody wants is worse than
        // slow.
        let result = if client.pace().cancelled() {
            Err(C::cancelled())
        } else {
            send(client, context, attempt)
        };
        let error = match result {
            Ok(completion) => return Ok(settled(client, plan.key, probed, completion)),
            Err(error) => error,
        };
        if client.mend(&error)? {
            continue;
        }
        let mut conceded = None;
        for &concession in plan.order {
            if !client.refusals().has_given_up(concession)
                && client.refused(request, concession, &error)
            {
                conceded = Some(concession);
                break;
            }
        }
        if let Some(concession) = conceded {
            client.refusals_mut().give_up(concession);
            probed = true;
        } else if client.asks_again(attempt, &error) {
            let pause = client.pause(attempt, &error);
            client.log_retry(attempt + 1, pause);
            announce(context, attempt + 1);
            if !client.pace().wait(pause) {
                return Err(C::cancelled());
            }
            attempt += 1;
        } else {
            // A probe that failed as well says none of what it gave up was what the service
            // refused, so each goes back to what was known before the request and nothing is
            // written.
            if probed {
                *client.refusals_mut() = recalled;
            }
            return Err(error);
        }
    }
}

/// The reply of an attempt that answered, with the cost of the earlier ones added, after
/// remembering what the concessions it was sent without proved.
fn settled<C: Driven>(
    client: &mut C,
    key: &str,
    probed: bool,
    mut completion: Completion,
) -> Completion {
    if probed {
        client.remember(key, *client.refusals());
    }
    client.spent_mut().bill(&mut completion);
    completion
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Effort;
    use crate::{remember, remembered};

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

    /// How an attempt of the fake client ended.
    #[derive(Debug, PartialEq, Clone, Copy)]
    enum Ended {
        Answered,
        /// The service refused the contents of the request.
        Refused,
        /// A rate limit, asking for this long a wait.
        Throttled(Duration),
        /// A failure nobody can do anything about.
        Failed,
        /// A failure the client repairs once and sends the request again after.
        Rejected,
        Stopped,
    }

    /// A client that answers from a script, so the driver is exercised without a network.
    struct Fake {
        spent: Spent,
        pace: Pace,
        refusals: Refusals,
        /// What each attempt reports as its usage and how it ends, first to last.
        script: std::collections::VecDeque<(Option<Usage>, Ended)>,
        /// How many requests may be sent in all.
        allowed: u32,
        /// The pause the driver asked this client for, per wait.
        paused: std::cell::RefCell<Vec<Duration>>,
        /// How many failures have been repaired.
        mended: u32,
    }

    impl Fake {
        fn new(script: Vec<(Option<Usage>, Ended)>) -> Self {
            Self {
                spent: Spent::default(),
                pace: Pace::new(Duration::ZERO),
                refusals: Refusals::default(),
                script: script.into(),
                allowed: 3,
                paused: std::cell::RefCell::new(Vec::new()),
                mended: 0,
            }
        }

        fn ending(ends: Vec<Ended>) -> Self {
            Self::new(ends.into_iter().map(|end| (None, end)).collect())
        }
    }

    /// What the driver's closures see: the refusals each request was sent with, and when the
    /// driver announced an attempt.
    struct Seen {
        started: std::time::Instant,
        sent_with: Vec<Refusals>,
        announced: Vec<(u32, Duration)>,
    }

    impl Seen {
        fn new() -> Self {
            Self {
                started: std::time::Instant::now(),
                sent_with: Vec::new(),
                announced: Vec::new(),
            }
        }
    }

    impl Driven for Fake {
        type Error = Ended;

        fn cancelled() -> Ended {
            Ended::Stopped
        }

        fn spent_mut(&mut self) -> &mut Spent {
            &mut self.spent
        }

        fn pace(&self) -> &Pace {
            &self.pace
        }

        fn refusals(&self) -> &Refusals {
            &self.refusals
        }

        fn refusals_mut(&mut self) -> &mut Refusals {
            &mut self.refusals
        }

        fn recall(&self, key: &str) -> Refusals {
            remembered(key)
        }

        fn remember(&self, key: &str, found: Refusals) {
            remember(key, found);
        }

        fn refused(&self, _: &ChatRequest, _: Concession, error: &Ended) -> bool {
            *error == Ended::Refused
        }

        fn asks_again(&self, attempt: u32, error: &Ended) -> bool {
            attempt < self.allowed && matches!(error, Ended::Throttled(_))
        }

        fn pause(&self, _: u32, error: &Ended) -> Duration {
            let pause = match error {
                Ended::Throttled(asked) => *asked,
                _ => Duration::ZERO,
            };
            self.paused.borrow_mut().push(pause);
            pause
        }

        fn log_retry(&self, _: u32, _: Duration) {}

        /// Repairs the first rejection only, as a token is replaced once.
        fn mend(&mut self, error: &Ended) -> Result<bool, Ended> {
            if *error == Ended::Rejected && self.mended == 0 {
                self.mended += 1;
                return Ok(true);
            }
            Ok(false)
        }
    }

    /// One attempt of the fake: note what the request carried, count it, and end as scripted.
    fn attempt_of(client: &mut Fake, seen: &mut Seen, _: u32) -> Result<Completion, Ended> {
        seen.sent_with.push(client.refusals);
        client.spent.sent();
        let Some((reported, end)) = client.script.pop_front() else {
            panic!("the script ran out");
        };
        client.spent.record(reported);
        match end {
            Ended::Answered => Ok(completion_with(reported.unwrap_or_default())),
            other => Err(other),
        }
    }

    fn run_fake(
        client: &mut Fake,
        key: &str,
        order: &[Concession],
        seen: &mut Seen,
    ) -> Result<Completion, Ended> {
        client.spent.begin_call();
        drive(
            client,
            &request_with(None),
            Plan { key, order },
            seen,
            attempt_of,
            |seen, attempt| seen.announced.push((attempt, seen.started.elapsed())),
        )
    }

    fn effort_only() -> Refusals {
        Refusals {
            effort: true,
            ..Refusals::default()
        }
    }

    fn caching_only() -> Refusals {
        Refusals {
            caching: true,
            ..Refusals::default()
        }
    }

    /// Rejects a driver that gives fields up in an order of its own: the same two refusals must
    /// give up a different field first when the caller passes a different order.
    #[test]
    fn the_order_fields_are_given_up_in_is_the_callers() {
        for (order, first, key) in [
            (
                [Concession::Effort, Concession::Breakpoints],
                effort_only(),
                "driver-test\norder-effort-first",
            ),
            (
                [Concession::Breakpoints, Concession::Effort],
                caching_only(),
                "driver-test\norder-breakpoints-first",
            ),
        ] {
            let mut client = Fake::ending(vec![Ended::Refused, Ended::Answered]);
            let mut seen = Seen::new();
            run_fake(&mut client, key, &order, &mut seen).expect("the second request answers");
            assert_eq!(
                seen.sent_with,
                [Refusals::default(), first],
                "the first refusal gave up the wrong field for {order:?}"
            );
            assert_eq!(remembered(key), first, "what answered was not remembered");
        }

        // Two refusals give up both, the second field being the next in the order.
        let key = "driver-test\norder-both";
        let mut client = Fake::ending(vec![Ended::Refused, Ended::Refused, Ended::Answered]);
        let mut seen = Seen::new();
        let order = [Concession::Breakpoints, Concession::Effort];
        run_fake(&mut client, key, &order, &mut seen).expect("the third request answers");
        assert_eq!(
            seen.sent_with.last(),
            Some(&Refusals {
                caching: true,
                effort: true,
                ..Refusals::default()
            })
        );
    }

    /// Rejects a driver that writes down what a failed probe gave up, or leaves it given up: a
    /// refusal that outlived every concession proves nothing about any of them.
    #[test]
    fn a_probe_that_failed_as_well_remembers_nothing_and_restores_what_was_known() {
        let order = [Concession::Effort, Concession::Breakpoints];

        let key = "driver-test\nprobe-fails-unknown";
        let mut client = Fake::ending(vec![Ended::Refused, Ended::Refused, Ended::Refused]);
        let mut seen = Seen::new();
        let result = run_fake(&mut client, key, &order, &mut seen);
        assert_eq!(result.unwrap_err(), Ended::Refused);
        assert_eq!(seen.sent_with.len(), 3, "both fields were not tried");
        assert_eq!(
            remembered(key),
            Refusals::default(),
            "a failed probe was written"
        );
        assert_eq!(
            client.refusals,
            Refusals::default(),
            "the fields stayed given up"
        );

        // What was known before the request is what comes back, and stays known.
        let known = "driver-test\nprobe-fails-known";
        remember(known, effort_only());
        let mut client = Fake::ending(vec![Ended::Refused, Ended::Refused]);
        let mut seen = Seen::new();
        let result = run_fake(&mut client, known, &order, &mut seen);
        assert_eq!(result.unwrap_err(), Ended::Refused);
        assert_eq!(
            seen.sent_with,
            [
                effort_only(),
                Refusals {
                    caching: true,
                    effort: true,
                    ..Refusals::default()
                }
            ]
        );
        assert_eq!(client.refusals, effort_only());
        assert_eq!(remembered(known), effort_only());
    }

    /// Rejects a driver that ignores a repair, one that waits or announces before sending again
    /// after it, one that counts it against the attempts, and one that repairs without end.
    #[test]
    fn a_mended_failure_is_sent_again_at_once_and_counts_against_nothing() {
        // Three throttled attempts after the repair still fit the three allowed: the repair is not
        // one of them.
        let mut client = Fake::ending(vec![
            Ended::Rejected,
            Ended::Throttled(Duration::ZERO),
            Ended::Throttled(Duration::ZERO),
            Ended::Answered,
        ]);
        let mut seen = Seen::new();
        run_fake(&mut client, "driver-test\nmended", &[], &mut seen)
            .expect("the repair was counted against the attempts");
        assert_eq!(seen.sent_with.len(), 4);
        assert_eq!(client.mended, 1);
        // Only the two rate limits were waited out and announced; the repair was neither.
        assert_eq!(*client.paused.borrow(), [Duration::ZERO; 2]);
        assert_eq!(seen.announced.len(), 2);

        // A failure the client does not repair a second time ends the call.
        let mut client = Fake::ending(vec![Ended::Rejected, Ended::Rejected, Ended::Answered]);
        let mut seen = Seen::new();
        let result = run_fake(&mut client, "driver-test\nmended-once", &[], &mut seen);
        assert_eq!(result.unwrap_err(), Ended::Rejected);
        assert_eq!(seen.sent_with.len(), 2);
    }

    /// Rejects a driver that sends again without looking at the stop. The stop lands while a
    /// refused attempt is ending, so no wait is there to notice it.
    #[test]
    fn a_stop_that_lands_between_attempts_is_answered_before_the_next_is_sent() {
        let cancel = Cancel::new();
        let mut client = Fake::ending(vec![Ended::Refused, Ended::Answered]);
        client.pace.cancel = Some(cancel.clone());
        let mut seen = Seen::new();
        client.spent.begin_call();
        let result = drive(
            &mut client,
            &request_with(None),
            Plan {
                key: "driver-test\nstop-between",
                order: &[Concession::Effort],
            },
            &mut seen,
            |client: &mut Fake, seen, attempt| {
                let result = attempt_of(client, seen, attempt);
                cancel.cancel();
                result
            },
            |_, _| {},
        );
        assert_eq!(result.unwrap_err(), Ended::Stopped);
        assert_eq!(seen.sent_with.len(), 1, "a request went out after the stop");
        assert_eq!(remembered("driver-test\nstop-between"), Refusals::default());
    }

    /// Rejects a driver that bills only the last attempt, or that bills the first twice.
    #[test]
    fn usage_of_the_earlier_attempts_is_carried_into_the_reply() {
        let mut client = Fake::new(vec![
            (Some(usage(10, 1)), Ended::Throttled(Duration::ZERO)),
            (Some(usage(20, 2)), Ended::Throttled(Duration::ZERO)),
            (Some(usage(5, 3)), Ended::Answered),
        ]);
        let mut seen = Seen::new();
        let reply = run_fake(&mut client, "driver-test\nusage", &[], &mut seen)
            .expect("the third request answers");
        assert_eq!(reply.usage, usage(35, 6));
        assert_eq!(client.spent.attempts, 3);
        assert_eq!(client.spent.last_prompt_tokens(), Some(5));
    }

    /// Rejects a driver that ignores the pause the client asks for (a 429 naming how long to wait),
    /// and one that announces the attempt after the wait rather than before it.
    #[test]
    fn a_rate_limit_is_waited_out_and_announced_before_the_wait() {
        let asked = Duration::from_millis(300);
        let mut client = Fake::ending(vec![Ended::Throttled(asked), Ended::Answered]);
        let mut seen = Seen::new();
        run_fake(&mut client, "driver-test\nrate-limit", &[], &mut seen)
            .expect("the second request answers");
        assert_eq!(*client.paused.borrow(), [asked]);
        let [(attempt, announced_at)] = seen.announced[..] else {
            panic!("expected one announcement, got {:?}", seen.announced);
        };
        assert_eq!(
            attempt, 2,
            "the announcement names the attempt about to follow"
        );
        assert!(
            announced_at < asked / 2,
            "announced after the wait: {announced_at:?}"
        );
        assert!(
            seen.started.elapsed() >= asked,
            "the wait the service asked for was not taken: {:?}",
            seen.started.elapsed()
        );
    }

    /// Rejects a driver with no cap on attempts, and one that counts a given-up field against it.
    #[test]
    fn attempts_are_capped_and_a_concession_is_not_one() {
        let mut client = Fake::ending(vec![Ended::Throttled(Duration::ZERO); 10]);
        let mut seen = Seen::new();
        let result = run_fake(&mut client, "driver-test\ncap", &[], &mut seen);
        assert_eq!(result.unwrap_err(), Ended::Throttled(Duration::ZERO));
        assert_eq!(seen.sent_with.len(), 3, "more requests than are allowed");

        // Two refusals first: still three attempts at the rate limit before it gives up.
        let mut ends = vec![Ended::Refused, Ended::Refused];
        ends.extend([
            Ended::Throttled(Duration::ZERO),
            Ended::Throttled(Duration::ZERO),
        ]);
        ends.push(Ended::Answered);
        let mut client = Fake::ending(ends);
        let mut seen = Seen::new();
        let order = [Concession::Effort, Concession::Breakpoints];
        run_fake(
            &mut client,
            "driver-test\ncap-concessions",
            &order,
            &mut seen,
        )
        .expect("a concession was counted against the attempts");
        assert_eq!(seen.sent_with.len(), 5);

        // Not worth asking again: no wait and no second request.
        let mut client = Fake::ending(vec![Ended::Failed, Ended::Answered]);
        let mut seen = Seen::new();
        let result = run_fake(&mut client, "driver-test\nfailed", &[], &mut seen);
        assert_eq!(result.unwrap_err(), Ended::Failed);
        assert_eq!(seen.sent_with.len(), 1);
    }
}
