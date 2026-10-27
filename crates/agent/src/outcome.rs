//! Safe failure details and distinct turn endings.

use crate::timing::Timing;
use bravebot_aichat::AccountFailure;
use bravebot_aichat::protocol::{Cached, Usage};

/// Fixed failure categories, distinguished by what the user can do about them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The service would not accept the credentials the request was signed with.
    Unauthorized,
    /// The service asked for fewer requests.
    RateLimited,
    /// The service was reached and could not answer.
    Unavailable,
    /// The service rejected the request as it stood.
    Refused,
    /// The request did not complete a round trip.
    Transport,
    /// The reply stopped before the service said it was finished.
    Incomplete,
    /// The reply arrived and was not the shape it had to be, or carried nothing usable.
    Undecodable,
    /// The model reached its output ceiling before finishing.
    TooLong,
    /// Nothing local could send the request: no credential, no model, no usable address.
    Unconfigured,
    /// A gate in this process refused to let the request leave.
    Blocked,
    /// The workspace could not be used.
    Workspace,
    /// A failure inside this program.
    Internal,
}

impl Category {
    /// A fixed name for driver messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::RateLimited => "rate-limited",
            Self::Unavailable => "unavailable",
            Self::Refused => "refused",
            Self::Transport => "transport",
            Self::Incomplete => "incomplete",
            Self::Undecodable => "undecodable",
            Self::TooLong => "too-long",
            Self::Unconfigured => "unconfigured",
            Self::Blocked => "blocked",
            Self::Workspace => "workspace",
            Self::Internal => "internal",
        }
    }
}

/// What is known about a failure, and nothing that is merely available.
///
/// Each field beyond the category is optional, and absent means unknown rather than zero. A
/// connection that never reached a server has no status. A counted call can report zero attempts
/// when preparation failed before egress; an uncounted error leaves attempts absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnosis {
    pub category: Category,
    /// The status the service answered with, where one arrived.
    pub status: Option<u16>,
    /// Requests handed to egress for this call, including retries and capability probes.
    /// Zero means preparation failed first. A policy refusal counts as an attempt.
    pub attempts: Option<u32>,
    /// The output ceiling that stopped the reply, where one did.
    ///
    /// This program's own configured figure rather than anything the service said, which is why
    /// it may be repeated: a person told only that a limit was reached is told nothing they can
    /// act on, and the number is what names the setting to raise. Absent for every other
    /// category, and absent where the ceiling is not known.
    pub ceiling: Option<u64>,
    /// How long the service asked for the next request to wait, where it said.
    ///
    /// A bounded duration the network layer read out of a `Retry-After` header and kept as a
    /// number, so it may be said back as a figure of this program's own wording and never as
    /// anything the service wrote. Absent where no header parsed or the status was not a
    /// rate limit.
    pub retry_after: Option<std::time::Duration>,
    /// Which failure of a ChatGPT account this was, where it was one of them
    /// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
    ///
    /// One of a closed set: a step of the sign-in, or a code the client that sent the request
    /// compared with a closed list. No text from a reply is in it, so a front end may word it as it
    /// words a category, and the category beside it stays the class the failure belongs to. Absent
    /// for every failure that did not come from the account, and never given to the planner.
    pub account: Option<AccountFailure>,
}

/// A wait a service asked for, in the minutes a person is told, rounded up so a limit that lifts in
/// seconds is not read as one that has already lifted.
pub fn minutes_to_wait(wait: std::time::Duration) -> u64 {
    wait.as_secs().div_ceil(60).max(1)
}

impl Diagnosis {
    /// A failure of this kind, with nothing else known about it.
    pub fn of(category: Category) -> Self {
        Self {
            category,
            status: None,
            attempts: None,
            ceiling: None,
            retry_after: None,
            account: None,
        }
    }

    /// The same, with the status a service answered with.
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    /// The same, with the output ceiling that stopped the reply.
    pub fn at_ceiling(mut self, ceiling: u64) -> Self {
        self.ceiling = Some(ceiling);
        self
    }

    /// The same, with the wait the service asked for.
    pub fn asking_for_a_wait_of(mut self, wait: std::time::Duration) -> Self {
        self.retry_after = Some(wait);
        self
    }

    /// The same, with which failure of a ChatGPT account it was.
    pub fn with_account(mut self, account: AccountFailure) -> Self {
        self.account = Some(account);
        self
    }

    /// The same, with how many requests were sent.
    pub fn after(mut self, attempts: u32) -> Self {
        self.attempts = Some(attempts);
        self
    }
}

/// A tool's interrupted backend call, carried separately from its planner-facing result.
#[derive(Debug, Clone, Copy)]
pub struct Cancellation {
    pub attempts: Option<u32>,
}

/// Distinguish an answer, a deliberate stop, and a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// It answered.
    Done,
    /// Somebody stopped it.
    Stopped {
        /// Attempts for the interrupted backend call, not a total over earlier rounds.
        /// None means no count was available; zero means no request was handed to egress.
        attempts: Option<u32>,
    },
    /// It failed, with what is known about why.
    Failed(Diagnosis),
}

impl Ending {
    /// Whether the turn ended without an answer for the caller to judge or use.
    pub fn unanswered(self) -> bool {
        !matches!(self, Self::Done)
    }

    /// What is known about the failure, where it was one.
    pub fn diagnosis(self) -> Option<Diagnosis> {
        match self {
            Self::Failed(diagnosis) => Some(diagnosis),
            Self::Done | Self::Stopped { .. } => None,
        }
    }
}

/// How many calls changed a file and how many ran a program, and whether a program ran after the
/// last change, which is bravebot's own record of its work and never what a model said of it
/// (GROUND-1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Calls that changed a file.
    pub writes: usize,
    /// Calls that ran a program. A line is one call however many programs it starts.
    pub commands: usize,
    /// Whether the last call that ran a program came after the last call that changed a file.
    pub ran_after_last_write: bool,
}

impl Work {
    /// Count one call. A call that both ran a program and changed a file is counted as the change
    /// coming last, since nothing ran after what it wrote.
    pub fn count(&mut self, changed_a_file: bool, ran_a_program: bool) {
        self.then(Work {
            writes: usize::from(changed_a_file),
            commands: usize::from(ran_a_program),
            ran_after_last_write: ran_a_program && !changed_a_file,
        });
    }

    /// Add work that came after this, such as a delegate taken back or the next turn: a write in it
    /// is the last write, and a program it ran after its own last write ran after this one's too.
    pub fn then(&mut self, later: Work) {
        self.writes += later.writes;
        self.commands += later.commands;
        if later.writes > 0 {
            self.ran_after_last_write = later.ran_after_last_write;
        } else if later.commands > 0 {
            self.ran_after_last_write = true;
        }
    }
}

/// Cumulative usage from completed requests, retained even if the turn later fails.
///
/// Reports replace the previous total rather than adding to it. Failed requests contribute only
/// usage measured by completed replies, including replies that could not be used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent {
    /// Every round added together, as the service counted them.
    pub tokens: u64,
    /// Of those, the ones the model wrote.
    pub output_tokens: u64,
    /// What the last completed request came to, which is occupancy rather than cost.
    pub context_tokens: u64,
    /// How much of the total the service served out of its prompt cache.
    pub cached: Cached,
    /// Time measured so far. The session uses its own wall clock, starting when Enter is pressed.
    pub timing: Timing,
}

impl Spent {
    /// Add one request's usage to the cost: its total, what the model wrote and what the cache
    /// served. The context reading and the timing are left as they are.
    pub fn add_usage(&mut self, usage: Usage) {
        self.tokens += usage.total();
        self.output_tokens += usage.completion_tokens;
        self.cached.add(usage.cached);
    }

    /// Add another run's cost, such as a delegate's, to this one: its total, what the model wrote
    /// and what the cache served. The context reading is that run's own conversation and the timing
    /// overlaps this one's clock, so both are left as they are.
    pub fn add_spent(&mut self, other: &Spent) {
        self.tokens += other.tokens;
        self.output_tokens += other.output_tokens;
        self.cached.add(other.cached);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_usage(prompt: u64, completion: u64, read: u64, written: u64) -> Usage {
        Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            cached: Cached {
                read_tokens: read,
                written_tokens: written,
            },
        }
    }

    #[test]
    fn adding_usage_or_another_spent_sums_the_cost_and_leaves_context_and_timing() {
        let timing = Timing {
            wall_ms: 900,
            inference_ms: 500,
            ..Timing::default()
        };
        let mut spent = Spent {
            tokens: 1_000,
            output_tokens: 100,
            context_tokens: 700,
            cached: Cached {
                read_tokens: 10,
                written_tokens: 20,
            },
            timing,
        };

        spent.add_usage(a_usage(300, 50, 5, 7));
        assert_eq!(
            spent.tokens, 1_350,
            "a request counts its prompt and completion"
        );
        assert_eq!(spent.output_tokens, 150);
        assert_eq!(spent.cached.read_tokens, 15);
        assert_eq!(spent.cached.written_tokens, 27);

        spent.add_spent(&Spent {
            tokens: 2_000,
            output_tokens: 400,
            context_tokens: 9_999,
            cached: Cached {
                read_tokens: 100,
                written_tokens: 200,
            },
            timing: Timing {
                wall_ms: 5_000,
                ..Timing::default()
            },
        });
        assert_eq!(spent.tokens, 3_350);
        assert_eq!(spent.output_tokens, 550);
        assert_eq!(spent.cached.read_tokens, 115);
        assert_eq!(spent.cached.written_tokens, 227);

        assert_eq!(spent.context_tokens, 700, "a context reading is not a cost");
        assert_eq!(spent.timing, timing, "the clock is the turn's own");
    }

    #[test]
    fn a_stop_is_neither_of_the_other_two_endings() {
        assert!((Ending::Stopped { attempts: None }).unanswered());
        assert!((Ending::Stopped { attempts: None }).diagnosis().is_none());
        assert!(!Ending::Done.unanswered());
        assert!(
            Ending::Failed(Diagnosis::of(Category::Transport))
                .diagnosis()
                .is_some()
        );
    }

    #[test]
    fn every_category_is_written_down_distinctly() {
        let all = [
            Category::Unauthorized,
            Category::RateLimited,
            Category::Unavailable,
            Category::Refused,
            Category::Transport,
            Category::Incomplete,
            Category::Undecodable,
            Category::TooLong,
            Category::Unconfigured,
            Category::Blocked,
            Category::Workspace,
            Category::Internal,
        ];
        let mut names: Vec<&str> = all.iter().map(|it| it.name()).collect();
        names.sort_unstable();
        let written = names.len();
        names.dedup();
        assert_eq!(names.len(), written, "two categories share a name");
    }

    #[test]
    fn cancellation_conversion_keeps_counts_from_both_clients() {
        for attempts in [0, 1, 2] {
            for cause in [
                crate::backend::BackendError::from(bravebot_aichat::ChatError::Cancelled),
                crate::backend::BackendError::from(bravebot_bedrock::BedrockError::Cancelled),
            ] {
                let error = crate::turn::TurnError::from(cause.counted(attempts, None, None));
                assert_eq!(
                    error.ending(),
                    Ending::Stopped {
                        attempts: Some(attempts)
                    }
                );
            }
        }
    }
}

#[cfg(test)]
mod work_tests {
    use super::Work;

    /// GROUND-1. Each call that changed a file is a write and each that ran a program is a
    /// command, so a refused write, which changed nothing, is not counted.
    #[test]
    fn each_write_that_changed_a_file_is_counted() {
        let mut work = Work::default();
        work.count(true, false);
        work.count(true, false);
        work.count(false, false);
        work.count(false, true);
        assert_eq!((work.writes, work.commands), (2, 1));
    }

    /// GROUND-1. A command after the last write is recorded as after it, a write after that
    /// command is not, and a line that ran a program and changed a file left nothing run after
    /// what it wrote.
    #[test]
    fn a_command_after_the_last_write_is_recorded_as_after_it() {
        let mut work = Work::default();
        work.count(true, false);
        work.count(false, true);
        assert!(work.ran_after_last_write);
        work.count(true, false);
        assert!(!work.ran_after_last_write);
        work.count(true, true);
        assert!(!work.ran_after_last_write);
    }

    /// GROUND-1, GROUND-4. A later record is after an earlier one: a command in a later turn ran
    /// after a write in an earlier one, a later write is the last write, and a later record with
    /// a write of its own settles the order by its own.
    #[test]
    fn a_command_in_a_later_turn_is_after_a_write_in_an_earlier_one() {
        let wrote = Work {
            writes: 1,
            commands: 0,
            ran_after_last_write: false,
        };
        let ran = Work {
            writes: 0,
            commands: 1,
            ran_after_last_write: true,
        };
        let mut record = wrote;
        record.then(ran);
        assert!(record.ran_after_last_write);
        record.then(wrote);
        assert!(!record.ran_after_last_write);
        record.then(Work::default());
        assert!(!record.ran_after_last_write);
        record.then(Work {
            writes: 2,
            commands: 3,
            ran_after_last_write: true,
        });
        assert_eq!(
            record,
            Work {
                writes: 4,
                commands: 4,
                ran_after_last_write: true,
            }
        );
    }
}
