//! What a turn ends with: the limit that stopped it, the error that failed it, and the outcome,
//! wakeup and decisions it hands back.

use bravebot_aichat::protocol::Cached;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_core::value::Labelled;
use bravebot_i18n::t;
use std::fmt;

use super::Task;
use crate::timing::Timing;
use crate::workspace::WorkspaceError;

/// Which of the limits a person gave a run ended it, and what the run had done by then (TURN-14).
///
/// Numbers the driver counted and nothing a reply or a tool result said, so a caller may put them
/// in a sentence or a result object without reading content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitReached {
    /// The planner asked for a round of tool calls past the most the run may make. The round it
    /// asked for was not run.
    Rounds {
        /// The limit, which is also how many rounds were made.
        most: usize,
    },
    /// The run had spent as many tokens as it may when it was about to send another request, which
    /// was not sent.
    Tokens {
        /// The limit.
        most: u64,
        /// Tokens the run had spent, which is at least `most`.
        spent: u64,
        /// Rounds of tool calls made before the run stopped.
        rounds: usize,
    },
}

impl LimitReached {
    /// Rounds of tool calls the run made before it stopped.
    pub fn rounds(&self) -> usize {
        match *self {
            Self::Rounds { most } => most,
            Self::Tokens { rounds, .. } => rounds,
        }
    }
}

impl fmt::Display for LimitReached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Rounds { most } => write!(f, "{}", t!(run_limit_rounds, most = most)),
            Self::Tokens { most, spent, .. } => {
                write!(f, "{}", t!(run_limit_tokens, spent = spent, most = most))
            }
        }
    }
}

#[derive(Debug)]
pub enum TurnError {
    /// The user asked for the turn to stop.
    Cancelled { attempts: Option<u32> },
    /// The run reached a limit its person gave it, and stopped before going past it (TURN-14).
    LimitReached(LimitReached),
    /// Routing could not be precommitted.
    Precommit(String),
    /// A file operation failed or was refused.
    Workspace(WorkspaceError),
    /// The model call failed or was refused.
    Chat(crate::backend::BackendError),
    /// A manifest run stopped before it had a frozen plan, or a step failed.
    ///
    /// Carries what the run produced so a caller can still look at it. A plan that would not
    /// parse has no rendered form, so the model's own words are the only thing left.
    ///
    /// The failure itself travels rather than a sentence about it, so what stopped the run is
    /// still something a caller can decide from. Flattened to a sentence, a step that lost the
    /// backend and a plan that would not parse arrive indistinguishable.
    Manifest {
        attempt: Box<crate::manifest::Attempt>,
        cause: Box<TurnError>,
    },
}

impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled { .. } => write!(f, "cancelled"),
            Self::LimitReached(reached) => write!(f, "{reached}"),
            Self::Precommit(detail) => write!(f, "{detail}"),
            Self::Workspace(e) => write!(f, "{e}"),
            Self::Chat(e) => write!(f, "{e}"),
            Self::Manifest { cause, .. } => write!(f, "{cause}"),
        }
    }
}

impl std::error::Error for TurnError {}

impl TurnError {
    /// Classify a failure or cancellation without copying raw error text.
    pub fn ending(&self) -> crate::outcome::Ending {
        use crate::outcome::{Category, Diagnosis, Ending};
        match self {
            Self::Cancelled { attempts } => Ending::Stopped {
                attempts: *attempts,
            },
            // The person's own limit, so a stop and not a failure of anything. No request was
            // handed to egress after it was reached.
            Self::LimitReached(_) => Ending::Stopped { attempts: Some(0) },
            Self::Chat(error) => Ending::Failed(error.diagnosis()),
            Self::Workspace(_) => Ending::Failed(Diagnosis::of(Category::Workspace)),
            // A precommit that would not hold and a plan that would not run are both this program
            // failing to get a turn off the ground, whatever the wording of either.
            Self::Precommit(_) | Self::Manifest { .. } => {
                Ending::Failed(Diagnosis::of(Category::Internal))
            }
        }
    }

    /// The limit that ended the run, where one did.
    pub fn limit_reached(&self) -> Option<&LimitReached> {
        match self {
            Self::LimitReached(reached) => Some(reached),
            Self::Manifest { cause, .. } => cause.limit_reached(),
            _ => None,
        }
    }

    /// What the reply the output ceiling stopped was doing, where that is what ended the turn.
    ///
    /// Beside [`Self::ending`] rather than in it, since a diagnosis is `Copy` and this carries a
    /// tool's name.
    pub fn cut_off(&self) -> Option<&bravebot_aichat::CutOff> {
        match self {
            Self::Chat(error) => error.cut_off(),
            Self::Manifest { cause, .. } => cause.cut_off(),
            _ => None,
        }
    }
}

impl From<WorkspaceError> for TurnError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<crate::backend::BackendError> for TurnError {
    fn from(value: crate::backend::BackendError) -> Self {
        // A reply stopped part way through is the person's own stop arriving back, not a
        // failure of the call. Reported as one, it would be written into the transcript as
        // something that went wrong with the model.
        if value.is_cancelled() {
            return Self::Cancelled {
                attempts: value.diagnosis().attempts,
            };
        }
        Self::Chat(value)
    }
}

impl From<bravebot_aichat::ChatError> for TurnError {
    fn from(value: bravebot_aichat::ChatError) -> Self {
        Self::from(crate::backend::BackendError::from(value))
    }
}

/// When the next tick of a self-paced loop is due, as the turn that has just ended asked for it.
///
/// A duration and a flag, and deliberately nothing else. What the next tick *says* is the line
/// the person typed, held by whoever started the loop, so there is no field here for a turn to
/// write its own next prompt into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wakeup {
    /// How long to wait, already held to the bounds below.
    pub after: std::time::Duration,
    /// Whether the turn found nothing to do, as it reported.
    pub quiet: bool,
    /// Whether the turn said the loop is finished, so no further tick is wanted.
    ///
    /// One routing boolean that can only end the loop the person started: it cannot start,
    /// lengthen or change what runs.
    pub stop: bool,
}

impl Wakeup {
    /// The shortest wait a turn may ask for.
    ///
    /// The wait is measured from the end of a tick and a tick is a whole turn, so how fast a watch
    /// actually looks is set by how long the turn takes rather than by this. It is not zero because
    /// a loop with no gap at all is a way to spend a rate limit rather than a way to watch
    /// something.
    pub const FLOOR: std::time::Duration = std::time::Duration::from_secs(1);

    /// The longest.
    ///
    /// A person started the loop and is entitled to see it do something. A turn that wants longer
    /// can say so in its answer, where somebody reads it, rather than by going quiet for a day.
    pub const CEILING: std::time::Duration = std::time::Duration::from_secs(3_600);

    /// What a turn asked for, held to the bounds.
    ///
    /// Held here rather than where the loop is kept, so the seconds the planner is told back are
    /// the seconds it gets. A tool that echoed the number it was given would be reporting a wait
    /// that is not going to happen.
    pub fn asked(seconds: u64, quiet: bool) -> Self {
        Self {
            after: std::time::Duration::from_secs(seconds).clamp(Self::FLOOR, Self::CEILING),
            quiet,
            stop: false,
        }
    }

    /// A turn saying the loop is finished.
    ///
    /// The wait is the floor and is never used: the loop that reads this ends instead of arming a
    /// tick.
    pub fn finished(quiet: bool) -> Self {
        Self {
            stop: true,
            ..Self::asked(0, quiet)
        }
    }
}

/// The result of a turn.
#[derive(Debug)]
pub struct Outcome {
    /// The assistant's reply. Untrusted, since it is model output.
    pub reply: Labelled<String>,
    /// The same reply, as the kernel labelled it from the context that produced it.
    ///
    /// [`Outcome::reply`] carries the transport's label, which is pessimistic because a JSON
    /// string arrives with no provenance. This one carries the kernel's, which tracked what
    /// entered the context and is the only thing that can say. A nested run's caller presents
    /// this one: presenting the other would quarantine every report a delegate ever made,
    /// whatever its context had actually met.
    pub answer: Labelled<String>,
    /// The model the server reported using.
    pub model: String,
    /// How many tool-calling rounds the turn took.
    pub steps: usize,
    /// Whether no gate refused anything during the turn.
    pub clean: bool,
    /// The refusals no later call to the same tool got past, each as `gate: reason`, in order.
    /// Empty where `clean` is false means the turn cannot say which refusals those were.
    pub standing_refusals: Vec<String>,
    /// How many refusals a later call to the same tool got past.
    pub worked_around: usize,
    /// The trust map after the turn, including any rule the turn recorded itself.
    pub trust: TrustStore,
    /// The programs vouched for after the turn, including any the user vouched for during it.
    ///
    /// Travels back rather than being recorded by whoever drew the prompt, so there is one copy
    /// of the answer and nothing to disagree with it.
    pub programs: TrustedPrograms,
    /// The run prompts put to the person after the turn, including any this one drew.
    ///
    /// Travels back for the reason [`Outcome::programs`] does, and grants nothing at all: a caller
    /// that drops it loses a sentence of advice at a later prompt and nothing else.
    pub asked_about: bravebot_core::programs::AskedAbout,
    /// The files agreed to be shown to the planner after the turn, including any it asked about.
    ///
    /// Travels back for the reason [`Outcome::asked_about`] does, and grants as little: a caller
    /// that drops it asks about the same file once more next turn and nothing else.
    pub exposed: bravebot_core::credentials::Exposed,
    /// Tokens the turn cost in total, summed over every round.
    ///
    /// A turn is several requests when the model calls tools, and each re-sends the whole
    /// history, so one round's count understates what the turn actually cost.
    pub tokens: u64,
    /// Of those, the ones the model wrote.
    ///
    /// Kept apart from the total because it answers a different question: the total is dominated by
    /// the history each round re-sends, while this tracks how much the model actually produced.
    pub output_tokens: u64,
    /// What the last round's request came to, as the server counted it.
    ///
    /// Occupancy rather than cost, and the difference matters. [`Outcome::tokens`] adds every
    /// round together and so says what the turn spent; this says how full the context was when it
    /// ended, which is the only figure worth comparing against
    /// [`bravebot_config::Config::context_budget`].
    pub context_tokens: u64,
    /// Of [`Outcome::tokens`], how much the backend served out of its prompt cache.
    ///
    /// Summed over the rounds like the total, and for the same reason: a turn's first round writes
    /// the prefix that the rest of them read back, so the round that paid for it and the rounds
    /// that profited are only the same figure once added together.
    ///
    /// Both figures zero from a backend that reports nothing about a cache, which is the same thing
    /// this says about a turn whose cache missed.
    pub cached: Cached,
    /// Whether this turn's requests went out on the premium tier.
    ///
    /// A fact about what happened rather than about the configuration. Every build that knows a
    /// premium host used to report itself as premium, so a session whose credentials could not be
    /// read said "premium" while being answered by a weaker model.
    pub premium: bool,
    /// When the planner asked to be asked again, whether or not this turn was a tick of a loop.
    ///
    /// A tick is setting the pace of a loop already running. Any other turn is asking to start
    /// one, which is how a turn told to report a change gets the later look that would catch it.
    ///
    /// `None` from a turn that was offered the chance and said nothing. The caller decides what
    /// that silence means; nothing here waits for it.
    pub wakeup: Option<Wakeup>,
    /// The paths this turn asked to have standing watches armed on, in the order it asked.
    ///
    /// Travels back for the reason a wakeup does: a watch outlives the turn, so the session is
    /// what holds one. Each has been through the gate a read of that path goes through, and the
    /// session applies the bound on how many may be live.
    pub watches: Vec<crate::watch::Armed>,
    /// Where the turn's wall clock went.
    ///
    /// Beside the token figures because it answers the other half of the same question. Tokens say
    /// what a turn cost the endpoint; this says what it cost the person in front of it, and the two
    /// have no relation: the cheapest turn in a session can be the one that took ten minutes
    /// because it stopped and waited to be allowed to run a command.
    pub timing: Timing,
    /// The reply, released for display while the policy was still open.
    pub(crate) display: String,
    /// What to tell the person watching about standing instructions and skills.
    ///
    /// The driver's own words about what loaded and what did not, never anything read out of a
    /// file, so they may go straight to a screen.
    pub notices: Vec<String>,
    /// What a manifest run produced, when this outcome came from one.
    ///
    /// Absent for a turn. On failure the same value is on [`TurnError::Manifest`].
    pub attempt: Option<crate::manifest::Attempt>,
    /// The definition this turn ran under, where the person's line addressed one.
    ///
    /// The kernel's match rather than anything the reply says about itself, so an interface
    /// drawing the reply under a name is drawing the driver's word for it (ADDRESS-12).
    pub addressed: Option<bravebot_core::delegate::Addressed>,
    /// Whether the turn compared the model that answered with one it asked for itself: an
    /// addressed definition's (ADDRESS-11) or a loaded skill's (SKILL-15).
    pub(crate) compared_the_model_itself: bool,
}

impl Outcome {
    /// The reply as text, for showing to the user.
    ///
    /// Authorised inside [`resume`], while the policy is still alive, so the release is
    /// recorded in the audit trail rather than happening implicitly after the fact.
    pub fn reply_for_display(&self) -> &str {
        &self.display
    }

    /// Whether the turn's last round was asked of the session's own model, so that a front end
    /// may compare [`Outcome::model`] with it.
    ///
    /// False where an addressed definition's model was asked for, a loaded skill named the
    /// model, or the turn moved to the fallback model (BACKEND-53). The first two have already been
    /// compared with the one that answered (ADDRESS-11, SKILL-15), and a comparison with the
    /// session's would report a substitution that did not happen. The fallback is the model the
    /// person named for the turn to move to, and the session's is the one that failed. True for a definition whose model the command line's outranked, since the
    /// command line's is what was asked for.
    pub fn ran_on_the_sessions_model(&self) -> bool {
        !self.compared_the_model_itself
    }
}

/// Session decisions after an ordinary turn ending. Advice and exposure answers are live-only.
#[derive(Debug, Clone)]
pub struct Decisions {
    pub trust: TrustStore,
    pub programs: TrustedPrograms,
    pub asked_about: bravebot_core::programs::AskedAbout,
    pub exposed: bravebot_core::credentials::Exposed,
}

impl Decisions {
    pub(super) fn initial(task: &Task, trust: TrustStore, programs: TrustedPrograms) -> Self {
        Self {
            trust: task
                .file_authority
                .as_ref()
                .map(|files| files.snapshot())
                .unwrap_or(trust),
            programs,
            asked_about: task.asked_about.clone(),
            exposed: task.exposed.clone(),
        }
    }
}

/// A continuing caller adopts `decisions` before handling `outcome` or saving the session.
/// Every field is returned after child cleanup, including on early context-loading errors.
#[must_use = "adopt the decisions before handling the outcome"]
pub struct CompletedTurn {
    pub outcome: Result<Outcome, TurnError>,
    pub decisions: Decisions,
    /// What the turn and the delegates it took back did, however it ended (GROUND-4).
    pub work: crate::outcome::Work,
}
