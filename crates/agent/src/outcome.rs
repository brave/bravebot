//! Safe failure details and distinct turn endings.

use crate::timing::Timing;
use bravebot_aichat::protocol::Cached;

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
}

impl Diagnosis {
    /// A failure of this kind, with nothing else known about it.
    pub fn of(category: Category) -> Self {
        Self {
            category,
            status: None,
            attempts: None,
            ceiling: None,
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

/// Cumulative usage from completed requests, retained even if the turn later fails.
///
/// Reports replace the previous total rather than adding to it. Requests that return an error
/// contribute no usage.
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

#[cfg(test)]
mod tests {
    use super::*;

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
