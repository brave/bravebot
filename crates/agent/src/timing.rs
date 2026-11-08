//! Where a turn's wall clock went.
//!
//! A turn takes a length of time, and until now that was the only thing recorded about it. The
//! figure is unactionable on its own: a turn that took four minutes because the model was slow, one
//! that took four minutes because a test suite ran, and one that took four minutes because it put a
//! diff on the screen and waited for somebody to come back from lunch are the same number and want
//! three different fixes.
//!
//! So the clock is split five ways, and the split is a partition rather than a set of overlapping
//! measures: every millisecond of a turn is charged to exactly one of inference, tools, stalled,
//! file wait, or the remainder. That is what makes the remainder worth reading. Approval prompts and
//! waits to enter the file-authority boundary happen inside tool calls, so what is spent on them is
//! taken out of the tool figure rather than counted in both.
//!
//! Milliseconds, and `u64` rather than `Duration`, because these are written into a session record
//! and read back by whatever is to hand. A `Duration` serialises as a struct of seconds and
//! nanoseconds, which is a poor thing to meet in a JSON file six months later.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// What a turn did, as counts, kept beside where its time went.
///
/// A session's totals cannot say which turn made forty requests, which one wrote the most, or which
/// one the service served from its cache, and none of those can be recovered from the conversation
/// afterwards: a turn that failed or was stopped keeps no messages of its own, and the service's
/// cache figures are in no message. Nothing in here is content, a path, or a label. It is five
/// counts, so it may be written down and shown without any release.
///
/// Delegates' requests and tokens are in it, as they are in the turn's total, since the turn
/// was charged for them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    /// Model requests that completed: the turn's own, a processor's, a summary's and a delegate's.
    #[serde(default)]
    pub requests: u64,
    /// Tokens the model wrote, which is [`crate::outcome::Spent::output_tokens`] for the turn.
    #[serde(default)]
    pub written_tokens: u64,
    /// Of the tokens the model read, those the service answered out of its prompt cache.
    ///
    /// Zero for a service that states no cache figure, which reads the same as a cache that
    /// missed ([`bravebot_aichat::protocol::Cached`]).
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// The tokens the service wrote into its prompt cache.
    #[serde(default)]
    pub cache_written_tokens: u64,
    /// Summaries the turn asked for to make room, which each cost a request.
    #[serde(default)]
    pub compactions: u64,
}

impl Activity {
    /// The counts an account holds, with the `compactions` the clock has made.
    ///
    /// One place that reads an account for these, so that a report summed after its timing was
    /// built (a turn's with its delegates') can have them made again from the sum.
    pub fn of(account: &crate::outcome::Spent, compactions: u64) -> Self {
        Self {
            requests: account.requests,
            written_tokens: account.output_tokens,
            cache_read_tokens: account.cached.read_tokens,
            cache_written_tokens: account.cached.written_tokens,
            compactions,
        }
    }

    /// Whether nothing was counted, which is what a record written before this was kept reads as.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Add another turn's counts, saturating so that a total over a session cannot wrap.
    pub fn add(&mut self, other: Activity) {
        self.requests = self.requests.saturating_add(other.requests);
        self.written_tokens = self.written_tokens.saturating_add(other.written_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_written_tokens = self
            .cache_written_tokens
            .saturating_add(other.cache_written_tokens);
        self.compactions = self.compactions.saturating_add(other.compactions);
    }
}

/// How a turn's wall clock divided up, and what it did in that time.
///
/// Nothing in here is content, a path, or a label. It is five counts of milliseconds and the
/// [`Activity`] counts, so it may be written down and shown without any release: see
/// [`crate::turn::Outcome::timing`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timing {
    /// The whole turn, end to end, as the turn loop saw it.
    ///
    /// The one figure that is not a component: the others are parts of this, and what they do
    /// not account for is [`Timing::overhead_ms`].
    #[serde(default)]
    pub wall_ms: u64,
    /// Waiting on the model, across own requests and the union of requests during delegate joins.
    ///
    /// A turn is several requests when the model calls tools, and a processor is a request of its
    /// own, so a figure for one round would understate this by however long the rest took.
    #[serde(default)]
    pub inference_ms: u64,
    /// Running tools, with time spent waiting for a person taken out.
    ///
    /// An approval prompt is drawn from inside a tool call, so the naive measure would charge the
    /// same seconds here and to [`Timing::stalled_ms`]. Subtracted rather than left in both,
    /// because the five figures are meant to add up.
    #[serde(default)]
    pub tools_ms: u64,
    /// Waiting for a person to answer a prompt.
    ///
    /// Every approval and every question, whatever was answered: a refusal took as long to arrive
    /// as an approval, and this measures the wait rather than the decision.
    #[serde(default)]
    pub stalled_ms: u64,
    /// Blocked entering the file-authority boundary, because another call or a delegate held it.
    ///
    /// Taken out of [`Timing::tools_ms`] for the turn's own calls, and for delegates counted only as
    /// the union of their blocked spans inside the parent's join windows, after the requests in
    /// those windows (`DELEGATE-18`), so the figures still add up with 30 delegates waiting at once.
    /// A record written before this figure existed reads as zero.
    #[serde(default)]
    pub file_wait_ms: u64,
    /// What the turn did while the clock ran. Left out of a record while nothing was counted, and
    /// read as nothing counted from a record written before it was kept.
    #[serde(default, skip_serializing_if = "Activity::is_empty")]
    pub activity: Activity,
}

impl Timing {
    /// What the turn spent on none of the four.
    ///
    /// Reading a session back, this is the figure with nobody to blame for it: not the model, not a
    /// subprocess, not the person. On a turn that ran no delegate, anything large here is time the
    /// harness spent on itself and there is no other way to see it. On a turn that delegated it is
    /// also whatever a delegate spent inside a join on something other than a request, which
    /// `DELEGATE-18` leaves here rather than charging to a figure the parent did not spend it on.
    ///
    /// Saturating, because the parts are measured separately and a clock that steps back must not
    /// produce a wrapped figure in the billions.
    pub fn overhead_ms(&self) -> u64 {
        self.wall_ms
            .saturating_sub(self.inference_ms)
            .saturating_sub(self.tools_ms)
            .saturating_sub(self.stalled_ms)
            .saturating_sub(self.file_wait_ms)
    }

    /// Add another turn's figures, for a total over a session.
    pub fn add(&mut self, other: Timing) {
        self.wall_ms += other.wall_ms;
        self.inference_ms += other.inference_ms;
        self.tools_ms += other.tools_ms;
        self.stalled_ms += other.stalled_ms;
        self.file_wait_ms += other.file_wait_ms;
        self.activity.add(other.activity);
    }

    /// Add another turn's inference, tool, stalled and file-wait figures and its activity, leaving
    /// `wall_ms` alone.
    ///
    /// For a caller that keeps its own wall clock, which is better than the worker's.
    pub fn add_measured(&mut self, other: Timing) {
        self.inference_ms += other.inference_ms;
        self.tools_ms += other.tools_ms;
        self.stalled_ms += other.stalled_ms;
        self.file_wait_ms += other.file_wait_ms;
        self.activity.add(other.activity);
    }
}

/// Durations as the turn loop accumulates them, before they become a [`Timing`].
///
/// The loop adds up `Duration`s because that is what `Instant::elapsed` gives it, and rounding each
/// one to milliseconds as it arrived would lose a fraction per round and end up short by the number
/// of rounds. Rounded once, here.
#[derive(Debug, Clone, Copy, Default)]
pub struct Elapsed {
    pub wall: Duration,
    pub inference: Duration,
    /// Already net of `stalled` and `file_wait`: see [`Timing::tools_ms`].
    pub tools: Duration,
    pub stalled: Duration,
    /// Time blocked entering the file-authority boundary, see [`Timing::file_wait_ms`].
    pub file_wait: Duration,
    /// Summaries the turn has made, see [`Activity::compactions`].
    pub compactions: u64,
}

impl Elapsed {
    /// The figures so far, with the requests and tokens `account` has counted.
    ///
    /// Takes the account rather than leaving the caller to copy from it, so that a report built
    /// from the clock alone cannot be written down without the counts that belong with it.
    pub fn finish(self, account: &crate::outcome::Spent) -> Timing {
        Timing {
            wall_ms: millis(self.wall),
            inference_ms: millis(self.inference),
            tools_ms: millis(self.tools),
            stalled_ms: millis(self.stalled),
            file_wait_ms: millis(self.file_wait),
            activity: Activity::of(account, self.compactions),
        }
    }
}

/// Whole milliseconds, held at `u64::MAX` for a duration too long for one.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// A request's original timing boundary, on the process-wide monotonic clock.
#[derive(Debug, Clone, Copy)]
pub struct Interval {
    pub start: Instant,
    pub end: Instant,
}

impl Interval {
    pub fn since(start: Instant) -> Self {
        Self {
            start,
            end: Instant::now(),
        }
    }

    pub fn duration(self) -> Duration {
        self.end.duration_since(self.start)
    }
}

impl From<bravebot_core::file_authority::Wait> for Interval {
    fn from(wait: bravebot_core::file_authority::Wait) -> Self {
        Self {
            start: wait.start,
            end: wait.end,
        }
    }
}

/// How long, from `from` to now, at least one of `waits` was blocked: their union clipped to that
/// window, so captures running at once are not counted once each.
pub(crate) fn blocked_since(
    waits: Vec<bravebot_core::file_authority::Wait>,
    from: Instant,
) -> Duration {
    let window = Interval::since(from);
    let waits: Vec<Interval> = waits.into_iter().map(Interval::from).collect();
    covered(&waits, window)
}

/// The union of requests, restricted to one actual parent wait.
fn covered(requests: &[Interval], wait: Interval) -> Duration {
    let mut clipped: Vec<_> = requests
        .iter()
        .filter_map(|request| {
            let start = request.start.max(wait.start);
            let end = request.end.min(wait.end);
            (start < end).then_some(Interval { start, end })
        })
        .collect();
    clipped.sort_unstable_by_key(|interval| interval.start);
    let mut through = wait.start;
    let mut total = Duration::ZERO;
    for interval in clipped {
        let start = interval.start.max(through);
        if start < interval.end {
            total += interval.end.duration_since(start);
            through = interval.end;
        }
    }
    total
}

/// What one join added to the turn's figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Collected {
    pub inference: Duration,
    pub file_wait: Duration,
}

/// Join windows never overlap: the parent joins on one thread. A later join may return a
/// request that covered an earlier window, so retain both until all delegates are collected.
#[derive(Default)]
pub(crate) struct DelegateWait {
    requests: Vec<Interval>,
    /// Spans delegates spent blocked entering the file-authority boundary.
    blocked: Vec<Interval>,
    waits: Vec<Interval>,
    /// What has been charged to inference so far across all joins.
    charged: Duration,
    /// What has been charged to file wait so far across all joins. Never reduced: a span charged
    /// to file wait stays there, and a request returned later that covers it is not charged again.
    charged_blocked: Duration,
    /// When the turn began waiting for a delegate to end, until the first join after it takes it.
    window_from: Option<Instant>,
    /// How many typed lines the buffer had counted when the last wait ended, so a wait wakes for a
    /// line added after it and not again for one it already woke for.
    typed_seen: u64,
}

impl DelegateWait {
    pub fn typed_seen(&self) -> u64 {
        self.typed_seen
    }

    pub fn note_typed(&mut self, typed: u64) {
        self.typed_seen = typed;
    }

    /// Whether a wake has announced the delegate it waits on and no join has taken its window yet.
    pub fn wake_announced(&self) -> bool {
        self.window_from.is_some()
    }

    /// Note that the turn begins waiting now, so the next join's window starts here and covers the
    /// whole wait rather than only the instant of the join.
    pub fn begin_wait(&mut self) {
        self.window_from = Some(Instant::now());
    }

    /// Where the window of a join starting now begins: the start of the wait noted by
    /// [`DelegateWait::begin_wait`] for the first join after it, and now for any other.
    pub fn join_window_start(&mut self) -> Instant {
        self.window_from.take().unwrap_or_else(Instant::now)
    }

    /// Close a wait that ended with nothing joined, so requests a later collection returns are
    /// still charged for the part of it they covered.
    pub fn end_wait(&mut self) {
        if let Some(from) = self.window_from.take() {
            self.waits.push(Interval::since(from));
        }
    }

    /// Charge a join: the part of its window, and of every earlier window, that a delegate was
    /// inside a request or blocked at the boundary, each instant charged once.
    ///
    /// Inside a join, a request wins where it overlaps a blocked span. A request returned by a
    /// later join can cover a span an earlier join already charged to file wait. That span stays
    /// charged to file wait, the charge for it is not repeated for inference, and the sum of the
    /// two charges stays equal to the union of the requests and blocked spans over the windows.
    pub fn collected(
        &mut self,
        wait: Interval,
        requests: Vec<Interval>,
        blocked: Vec<Interval>,
    ) -> Collected {
        self.waits.push(wait);
        self.requests.extend(requests);
        self.blocked.extend(blocked);
        let inference: Duration = self
            .waits
            .iter()
            .map(|wait| covered(&self.requests, *wait))
            .sum();
        let both: Vec<Interval> = self
            .requests
            .iter()
            .chain(self.blocked.iter())
            .copied()
            .collect();
        let either: Duration = self.waits.iter().map(|wait| covered(&both, *wait)).sum();
        let file_wait = either.saturating_sub(inference);
        // Charges only grow, so what an earlier join charged to file wait is kept. The rest of the
        // union goes to inference.
        let blocked_total = file_wait.max(self.charged_blocked);
        let inference_total = either.saturating_sub(blocked_total);
        let added = Collected {
            inference: inference_total.saturating_sub(self.charged),
            file_wait: blocked_total.saturating_sub(self.charged_blocked),
        };
        self.charged = inference_total;
        self.charged_blocked = blocked_total;
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interval(base: Instant, start: u64, end: u64) -> Interval {
        Interval {
            start: base + Duration::from_millis(start),
            end: base + Duration::from_millis(end),
        }
    }

    /// Elapsed inference is a union clipped to the wait, including neither gaps nor background work.
    #[test]
    fn delegate_requests_cover_only_their_union_inside_a_wait() {
        let base = Instant::now();
        for (name, requests, expected) in [
            ("disjoint", vec![(110, 120), (140, 160)], 30),
            ("overlap", vec![(80, 150), (120, 180)], 80),
            ("nested", vec![(110, 190), (120, 130)], 80),
            ("identical", vec![(110, 190), (110, 190)], 80),
            ("spanning", vec![(80, 220)], 100),
            ("left clip", vec![(80, 130)], 30),
            ("right clip", vec![(170, 230)], 30),
            ("outside", vec![(20, 90), (210, 230)], 0),
            ("adjacent", vec![(100, 150), (150, 200)], 100),
            ("empty", vec![(120, 120), (200, 220), (80, 100)], 0),
            ("unordered", vec![(160, 190), (110, 130), (120, 170)], 80),
        ] {
            let requests: Vec<_> = requests
                .into_iter()
                .map(|(start, end)| interval(base, start, end))
                .collect();
            assert_eq!(
                covered(&requests, interval(base, 100, 200)),
                Duration::from_millis(expected),
                "{name}"
            );
            assert_eq!(
                covered(&requests, interval(base, 150, 150)),
                Duration::ZERO,
                "empty wait: {name}"
            );
        }
    }

    /// A later-collected delegate can cover an earlier wait, but cannot charge it twice.
    #[test]
    fn successive_collections_charge_each_covered_instant_once() {
        let base = Instant::now();
        let mut waits = DelegateWait::default();
        assert_eq!(
            waits
                .collected(
                    interval(base, 100, 150),
                    vec![interval(base, 120, 140)],
                    vec![]
                )
                .inference,
            Duration::from_millis(20)
        );
        // The second delegate was already working during the first join. Its request also
        // spans a gap between joins; that gap is the parent's own work, not delegate wait.
        assert_eq!(
            waits
                .collected(
                    interval(base, 170, 200),
                    vec![interval(base, 80, 190)],
                    vec![]
                )
                .inference,
            Duration::from_millis(50)
        );
        assert_eq!(
            waits
                .collected(
                    interval(base, 220, 230),
                    vec![interval(base, 100, 190)],
                    vec![]
                )
                .inference,
            Duration::ZERO
        );
        assert_eq!(waits.charged, Duration::from_millis(70));
    }

    /// A file-authority wait is charged where no request covers it, and once however many delegates
    /// were blocked at the same time: 30 delegates blocked for the same 40 ms are 40 ms, not 1200.
    #[test]
    fn delegates_blocked_together_charge_the_union_once_and_a_request_wins() {
        let base = Instant::now();
        let mut waits = DelegateWait::default();
        let blocked: Vec<_> = (0..30).map(|_| interval(base, 110, 150)).collect();
        let added = waits.collected(
            interval(base, 100, 200),
            vec![interval(base, 140, 160)],
            blocked,
        );
        // 110..140 is blocked with no request; 140..150 is also inside a request, which wins.
        assert_eq!(added.file_wait, Duration::from_millis(30));
        assert_eq!(added.inference, Duration::from_millis(20));
        // A span returned at a later join that also covers an earlier window charges only the part
        // of that window not yet covered: 100..110 (the earlier join already counted 110..150).
        let later = waits.collected(
            interval(base, 300, 320),
            vec![],
            vec![interval(base, 100, 150)],
        );
        assert_eq!(later.file_wait, Duration::from_millis(10));
        // Seen again, and with one new span inside a new window, only the new span is charged.
        let again = waits.collected(
            interval(base, 400, 410),
            vec![],
            vec![interval(base, 100, 150), interval(base, 400, 405)],
        );
        assert_eq!(again.file_wait, Duration::from_millis(5));
    }

    /// A request returned at a later join that covers a span an earlier join charged to file wait
    /// does not charge that span to inference as well: the two charges never come to more than the
    /// union of the requests and blocked spans over the windows.
    #[test]
    fn a_late_request_over_an_earlier_blocked_span_is_not_charged_twice() {
        let base = Instant::now();
        let mut waits = DelegateWait::default();
        let first = waits.collected(
            interval(base, 100, 140),
            vec![],
            vec![interval(base, 100, 140)],
        );
        assert_eq!(first.file_wait, Duration::from_millis(40));
        assert_eq!(first.inference, Duration::ZERO);
        // The request ran through the span the first join charged to file wait.
        let second = waits.collected(
            interval(base, 200, 210),
            vec![interval(base, 100, 140), interval(base, 200, 205)],
            vec![],
        );
        assert_eq!(second.file_wait, Duration::ZERO);
        assert_eq!(second.inference, Duration::from_millis(5));
        // 40 ms of the first window and 5 ms of the second were covered, 45 ms in all.
        assert_eq!(
            first.inference + first.file_wait + second.inference + second.file_wait,
            Duration::from_millis(45)
        );
    }

    /// A record written before the file-wait figure existed reads it as zero, not as an error.
    #[test]
    fn a_record_without_a_file_wait_figure_reads_it_as_zero() {
        let timing: Timing =
            serde_json::from_str(r#"{"wall_ms":10,"inference_ms":4,"tools_ms":3,"stalled_ms":1}"#)
                .expect("parses");
        assert_eq!(timing.file_wait_ms, 0);
        assert_eq!(timing.wall_ms, 10);
        assert_eq!(timing.overhead_ms(), 2);
    }

    /// The figure is part of the partition: what it takes up is not left in the remainder.
    #[test]
    fn the_file_wait_is_taken_out_of_the_remainder() {
        let timing = Timing {
            wall_ms: 1_000,
            inference_ms: 100,
            tools_ms: 100,
            stalled_ms: 100,
            file_wait_ms: 300,
            ..Default::default()
        };
        assert_eq!(timing.overhead_ms(), 400);
    }

    /// The union of captures that overlapped is clipped to the window it is asked about.
    #[test]
    fn blocked_since_counts_overlapping_waits_once() {
        use bravebot_core::file_authority::Wait;
        let from = Instant::now();
        let at = |ms| from + Duration::from_millis(ms);
        let waits = vec![
            Wait {
                start: at(0),
                end: at(30),
            },
            Wait {
                start: at(10),
                end: at(40),
            },
            Wait {
                start: at(10),
                end: at(40),
            },
        ];
        // `now` is later than every span, so the window holds all of them.
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(blocked_since(waits, from), Duration::from_millis(40));
    }

    /// The point of the whole type: what is left when the four known costs are taken out.
    #[test]
    fn the_remainder_is_what_nothing_else_accounts_for() {
        let timing = Timing {
            wall_ms: 10_000,
            inference_ms: 6_000,
            tools_ms: 1_500,
            stalled_ms: 2_000,
            file_wait_ms: 0,
            ..Default::default()
        };
        assert_eq!(timing.overhead_ms(), 500);
    }

    /// A clock that steps backwards, or three parts measured slightly over the whole, must not turn
    /// a small discrepancy into a figure in the billions.
    #[test]
    fn parts_exceeding_the_whole_do_not_wrap() {
        let timing = Timing {
            wall_ms: 100,
            inference_ms: 200,
            tools_ms: 200,
            stalled_ms: 200,
            file_wait_ms: 0,
            ..Default::default()
        };
        assert_eq!(timing.overhead_ms(), 0);
    }

    #[test]
    fn a_session_total_adds_every_turn() {
        let mut total = Timing::default();
        total.add(Timing {
            wall_ms: 100,
            inference_ms: 60,
            tools_ms: 10,
            stalled_ms: 20,
            file_wait_ms: 0,
            ..Default::default()
        });
        total.add(Timing {
            wall_ms: 50,
            inference_ms: 30,
            tools_ms: 5,
            stalled_ms: 5,
            file_wait_ms: 0,
            ..Default::default()
        });
        assert_eq!(total.wall_ms, 150);
        assert_eq!(total.inference_ms, 90);
        assert_eq!(total.tools_ms, 15);
        assert_eq!(total.stalled_ms, 25);
        assert_eq!(total.overhead_ms(), 20);
    }

    /// A caller with a better wall clock of its own adds the four measured parts and keeps its figure.
    #[test]
    fn add_measured_leaves_the_wall_clock_alone() {
        let mut total = Timing {
            wall_ms: 7,
            inference_ms: 1,
            tools_ms: 2,
            stalled_ms: 3,
            file_wait_ms: 0,
            ..Default::default()
        };
        total.add_measured(Timing {
            wall_ms: 999_999,
            inference_ms: 10,
            tools_ms: 20,
            stalled_ms: 30,
            file_wait_ms: 0,
            ..Default::default()
        });
        assert_eq!(
            total,
            Timing {
                wall_ms: 7,
                inference_ms: 11,
                tools_ms: 22,
                stalled_ms: 33,
                file_wait_ms: 0,
                ..Default::default()
            }
        );
    }

    /// Rounded once at the end rather than per round, or a turn of many short rounds comes back
    /// short by a fraction of a millisecond per round.
    #[test]
    fn durations_are_rounded_once_rather_than_per_round() {
        let mut elapsed = Elapsed::default();
        for _ in 0..10 {
            elapsed.inference += Duration::from_micros(1_900);
        }
        assert_eq!(
            elapsed
                .finish(&crate::outcome::Spent::default())
                .inference_ms,
            19
        );
    }

    /// The clock counts the summaries and the account counts the requests, so a report that takes
    /// either from the wrong one writes down zero, or another turn's count.
    #[test]
    fn finishing_takes_the_requests_tokens_and_cache_from_the_account_and_compactions_from_the_clock()
     {
        let account = crate::outcome::Spent {
            tokens: 9_000,
            output_tokens: 40,
            cached: bravebot_aichat::protocol::Cached {
                read_tokens: 300,
                written_tokens: 20,
            },
            requests: 5,
            ..Default::default()
        };
        let elapsed = Elapsed {
            compactions: 2,
            ..Elapsed::default()
        };
        assert_eq!(
            elapsed.finish(&account).activity,
            Activity {
                requests: 5,
                written_tokens: 40,
                cache_read_tokens: 300,
                cache_written_tokens: 20,
                compactions: 2,
            }
        );
    }

    /// A total over turns, and a failed turn charged beside the time it was measured with, both
    /// add the counts as they add the milliseconds.
    #[test]
    fn activity_is_added_with_the_time_by_both_adders_and_saturates() {
        let turn = |requests, compactions| Timing {
            wall_ms: 10,
            activity: Activity {
                requests,
                written_tokens: 7,
                cache_read_tokens: 11,
                cache_written_tokens: 13,
                compactions,
            },
            ..Default::default()
        };
        let mut total = turn(3, 1);
        total.add(turn(4, 0));
        assert_eq!(total.activity.requests, 7);
        assert_eq!(total.activity.written_tokens, 14);
        assert_eq!(total.activity.cache_read_tokens, 22);
        assert_eq!(total.activity.cache_written_tokens, 26);
        assert_eq!(total.activity.compactions, 1);

        let mut kept = turn(1, 0);
        kept.add_measured(turn(2, 3));
        assert_eq!(kept.wall_ms, 10, "the wall clock is the caller's");
        assert_eq!((kept.activity.requests, kept.activity.compactions), (3, 3));

        let mut full = turn(u64::MAX, 0);
        full.add(turn(1, 0));
        assert_eq!(full.activity.requests, u64::MAX);
    }

    /// An empty activity adds nothing to the record a turn is written in, a record from before
    /// the counts reads as none counted, and counts survive being written.
    #[test]
    fn activity_is_left_out_while_empty_and_survives_a_round_trip() {
        let none = serde_json::to_string(&Timing::default()).expect("serialises");
        assert!(!none.contains("activity"), "{none}");
        let old: Timing = serde_json::from_str(
            r#"{"wall_ms":10,"inference_ms":4,"tools_ms":3,"stalled_ms":1,"file_wait_ms":0}"#,
        )
        .expect("parses");
        assert!(old.activity.is_empty());

        let counted = Timing {
            activity: Activity {
                requests: 6,
                written_tokens: 70,
                cache_read_tokens: 800,
                cache_written_tokens: 90,
                compactions: 1,
            },
            ..Default::default()
        };
        let body = serde_json::to_string(&counted).expect("serialises");
        assert_eq!(
            serde_json::from_str::<Timing>(&body).expect("parses"),
            counted
        );
        let partial: Timing =
            serde_json::from_str(r#"{"activity":{"requests":2}}"#).expect("parses");
        assert_eq!(partial.activity.requests, 2);
        assert_eq!(partial.activity.written_tokens, 0);
    }

    /// The figures are written into a session record, so they have to survive being one.
    #[test]
    fn timing_survives_a_round_trip_as_json() {
        let timing = Timing {
            wall_ms: 1,
            inference_ms: 2,
            tools_ms: 3,
            stalled_ms: 4,
            file_wait_ms: 0,
            ..Default::default()
        };
        let body = serde_json::to_string(&timing).expect("serialises");
        assert_eq!(
            serde_json::from_str::<Timing>(&body).expect("parses"),
            timing
        );
    }

    /// A record written before this was kept reads as a turn that took no time, which is the same
    /// way an absent token breakdown reads: nothing recorded rather than an error.
    #[test]
    fn a_record_without_timing_reads_as_zero() {
        let timing: Timing = serde_json::from_str("{}").expect("parses");
        assert_eq!(timing, Timing::default());
    }
}
