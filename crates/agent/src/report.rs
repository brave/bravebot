//! Telling the interface what a turn is doing, while it does it.
//!
//! Distinct from [`crate::confirm`], and the difference is consent. A write asks, blocks, and
//! must refuse if nobody can answer. Progress announces: there is no question, no reply, and no
//! answer that could change what happens. So this returns nothing, and a listener that has gone
//! away is not an error.
//!
//! That asymmetry decides the failure behaviour. A closed channel means a write must not happen,
//! but a task list nobody is drawing is merely unseen, and failing the turn over it would let the
//! display outrank the work.

/// What a person is told when `summaryModel` names a model a side request cannot run on.
///
/// Here rather than beside [`crate::compact::SideModelRefusal`] because every string in that module
/// is read by the planner and nothing in it may be looked up in a catalog, and this sentence is
/// read by a person: it names the key to correct. `crates/agent/tests/audience.rs` is the rule.
pub fn summary_model_refusal(refusal: &crate::compact::SideModelRefusal) -> String {
    use crate::compact::SideModelRefusal;
    match refusal {
        SideModelRefusal::NeedsSignIn(model) => {
            bravebot_i18n::t!(summary_model_needs_sign_in, model = model)
        }
        SideModelRefusal::Refused(model) => {
            bravebot_i18n::t!(summary_model_refused, model = model)
        }
        SideModelRefusal::NotServed(model) => {
            bravebot_i18n::t!(summary_model_not_served, model = model)
        }
    }
}

/// Why a request a turn made on its way failed, as a closed value a front end words in its own
/// way ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
///
/// Handed to [`Reporter::summary_failed`] and [`Reporter::fell_back`] rather than a sentence, so the
/// terminal says the account's failure in the terminal's catalog sentence and the window says it in
/// its own, with the command it names drawn as code. Nothing a service wrote is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The ChatGPT account failed in one of its own ways.
    Account(bravebot_aichat::AccountFailure),
    /// Any other failure, by its category.
    Category(crate::outcome::Category),
}

impl Why {
    /// The reason a diagnosis carries.
    pub(crate) fn of(diagnosis: crate::outcome::Diagnosis) -> Self {
        match diagnosis.account {
            Some(account) => Self::Account(account),
            None => Self::Category(diagnosis.category),
        }
    }

    /// The reason a summary could not be made.
    pub(crate) fn of_summary(error: &crate::compact::CompactError) -> Self {
        match error {
            crate::compact::CompactError::Chat(backend) => Self::of(backend.diagnosis()),
            other => Self::Category(other.category()),
        }
    }

    /// The terminal's words for it: the account's catalog sentence, with no status or count after
    /// it, and the name of the category for every other failure. The wait a refusal asked for is
    /// part of the sentence.
    pub fn told(self) -> String {
        match self {
            Self::Account(failure) => crate::chatgpt::said_of_failure(failure),
            Self::Category(category) => category.name().to_string(),
        }
    }
}

/// The line the terminal narrates for a summary that could not be made, which a front end that has
/// no words of its own for the reason says as well.
pub fn summary_failed_in_the_terminals_words(why: Why) -> String {
    t!(compact_failed_and_continuing, problem = why.told())
}

/// The line the terminal narrates for a move to the fallback model, which a front end that has no
/// words of its own for the reason says as well.
pub fn fell_back_in_the_terminals_words(from: &str, to: &str, why: Why) -> String {
    match why {
        Why::Account(failure) => t!(
            fallback_model_in_use_account,
            from = from,
            to = to,
            problem = crate::chatgpt::said_of_failure(failure)
        ),
        Why::Category(category) => t!(
            fallback_model_in_use,
            from = from,
            to = to,
            category = category.name()
        ),
    }
}

/// What a person is told of a summary that could not be made: the sentence for the failure of the
/// ChatGPT account where it was the account that failed
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)), and the name of the failure's
/// category for every other failure.
///
/// Here for the reason [`summary_model_refusal`] is: the sentence is looked up in a catalog, and
/// every string in `compact.rs` is read by the planner. A summary that failed on the account says
/// which command to type where signing in again is the remedy, which the category's name does not.
pub fn why_a_summary_failed(error: &crate::compact::CompactError) -> String {
    Why::of_summary(error).told()
}

/// What a person is told when `processorModel` or `--processor-model` names a model a processor
/// cannot run on.
///
/// Beside [`summary_model_refusal`] for the same reason: it names the key to correct, and a person
/// reads it.
pub fn processor_model_refusal(refusal: &crate::compact::SideModelRefusal) -> String {
    use crate::compact::SideModelRefusal;
    match refusal {
        SideModelRefusal::NeedsSignIn(model) => {
            bravebot_i18n::t!(processor_model_needs_sign_in, model = model)
        }
        SideModelRefusal::Refused(model) => {
            bravebot_i18n::t!(processor_model_refused, model = model)
        }
        SideModelRefusal::NotServed(model) => {
            bravebot_i18n::t!(processor_model_not_served, model = model)
        }
    }
}

use crate::diff::Change;
use bravebot_core::todo::Row;
use bravebot_core::vetting::Checking;
use bravebot_i18n::t;

/// One thing the turn did, shaped for the person watching.
///
/// Every string in here has already been through the display gate, exactly as
/// [`crate::confirm::WriteRequest`] has, so nothing downstream reasons about labels. The
/// release is what the gate exists for: a screen is one of the three destinations untrusted
/// content is allowed to reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    /// What is being done, in the driver's own word.
    ///
    /// A literal chosen by the dispatch table rather than anything the model wrote, so a call
    /// cannot describe itself as something gentler than it is.
    pub verb: &'static str,
    /// What is being acted on, as the model named it. Empty where there is nothing to name.
    pub target: String,
    /// Whether [`Activity::target`] is a file's name or path, which a screen spells with
    /// [`Activity::shown_target`] so a blank or whitespace-ended name is visible (PROMPT-1).
    ///
    /// Set by dispatch from the tool's own name, a literal of this program, and never read out of
    /// the target: a line that decided how to draw a name by looking at the name would be a
    /// branch on content nobody vouched for.
    pub names_a_file: bool,
    /// Why the planner made the call, in its own words. Empty where it gave no reason.
    ///
    /// Carried to a screen and nowhere else: the call runs the same with or without it, and
    /// whether there is anything to draw is the screen's question to ask of the text.
    pub why: String,
    /// The tool's own name, as dispatch matched it. Empty where nothing set one.
    ///
    /// Beside [`Activity::verb`] rather than instead of it, because the two have different
    /// audiences: the verb is a word in the reader's own language, and this is what a program
    /// matches on. A surface that offered only the verb would have a script keying off French.
    pub tool: String,
    /// What came of it, in a few words. `None` while the call is still running, which is what
    /// makes an unfinished line distinguishable from one that finished with nothing to say.
    pub note: Option<String>,
    /// Whether the call was refused or failed, so the line can be coloured as such.
    ///
    /// Set by the driver from which branch it took, never read back out of the note: the note
    /// is prose, and matching on prose is how a message that merely mentions a refusal becomes
    /// one.
    pub failed: bool,
    /// The change a write made, for showing beneath the line. Empty for everything else.
    pub changes: Vec<Change>,
    /// The counts and line numbers the kernel made with the diff behind `changes`, for a write
    /// that changed a file. `None` for everything else, and for a write that was refused, failed or
    /// changed nothing, so a screen listing what was written needs no reading of `changes`.
    pub written: Option<Written>,
    /// Whether those lines are content nobody vouched for.
    ///
    /// Drawn with the same mark the transcript puts on everything the model was not allowed to
    /// read, so one convention covers every place untrusted bytes reach a screen.
    pub untrusted: bool,
    /// How much of the call was spent waiting on a model of its own, where it called one.
    ///
    /// `None` for almost everything: a call that ran a program or read a file waited on this
    /// machine, and the time it took is the time it took. Some calls run a whole request inside
    /// themselves, a confined check before quarantined content may be read and a processor's own
    /// round, and without this the one figure that tells a slow model from a slow program is
    /// measured and then thrown away.
    ///
    /// The duration rather than the boundary, because nothing on a screen can do anything with a
    /// pair of instants: the turn's own clock does the arithmetic that needs them.
    pub waited: Option<std::time::Duration>,
}

impl Activity {
    /// A call that has begun and has not finished.
    pub fn running(verb: &'static str, target: impl Into<String>) -> Self {
        Self {
            verb,
            target: target.into(),
            names_a_file: false,
            why: String::new(),
            tool: String::new(),
            note: None,
            failed: false,
            changes: Vec::new(),
            written: None,
            untrusted: false,
            waited: None,
        }
    }

    /// Say which tool this is, by the name dispatch matched rather than the word shown.
    pub fn of_tool(mut self, tool: &str) -> Self {
        self.tool = tool.to_string();
        self
    }

    /// Say that the target is a file's name or path, where the tool's own name says it is
    /// ([`crate::tools::names_a_file`]).
    pub fn naming_a_file(mut self, names_a_file: bool) -> Self {
        self.names_a_file = names_a_file;
        self
    }

    /// Say why the planner made the call.
    pub fn saying_why(mut self, why: impl Into<String>) -> Self {
        self.why = why.into();
        self
    }

    /// The same call, finished, with what came of it.
    pub fn done(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// The same call, refused or failed, with why.
    pub fn failed(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self.failed = true;
        self
    }

    /// Attach the change a write made.
    pub fn with_changes(mut self, changes: Vec<Change>) -> Self {
        self.changes = changes;
        self
    }

    /// Attach the counts and line numbers the write's diff was made with.
    pub fn with_written(mut self, written: Option<Written>) -> Self {
        self.written = written;
        self
    }

    /// Say that the lines beneath this call are content nobody vouched for.
    pub fn marked_untrusted(mut self, untrusted: bool) -> Self {
        self.untrusted = untrusted;
        self
    }

    /// Say how long the call waited on a model of its own.
    pub fn after_waiting(mut self, waited: Option<std::time::Duration>) -> Self {
        self.waited = waited;
        self
    }

    /// Whether this line is still waiting on the call it describes.
    pub fn is_running(&self) -> bool {
        self.note.is_none()
    }

    /// The target as a person is shown it: a file's name spelled as
    /// [`crate::confirm::shown_name`] spells it, anything else as it is.
    ///
    /// For a screen only. A name that needs no spelling comes back as it was, so the plain case
    /// reads as it always did, and [`Activity::target`] stays what programs and the files view
    /// match on. A call with nothing to name has no target, and it is not drawn as the empty name.
    pub fn shown_target(&self) -> String {
        match self.names_a_file && !self.target.is_empty() {
            true => crate::confirm::shown_name(&self.target),
            false => self.target.clone(),
        }
    }

    /// The file this call is about as a list of files names it: [`Activity::shown_target`] of the
    /// target with the slot a reference was drawn under and a leading `./` taken off, so a file
    /// the planner named two ways is one row, spelled as the confirmation for it is.
    ///
    /// For a screen only, as [`Activity::shown_target`] is. A call that names no file comes back
    /// as it is.
    pub fn shown_file(&self) -> String {
        match self.names_a_file && !self.target.is_empty() {
            true => crate::confirm::shown_name(without_reference_slot(&self.target)),
            false => self.target.clone(),
        }
    }

    /// The line as one string, for a display with nowhere to put the parts separately.
    pub fn line(&self) -> String {
        if self.target.is_empty() {
            self.verb.to_string()
        } else {
            format!("{}({})", self.verb, self.shown_target())
        }
    }
}

/// A target without the `ref:N(label):` a reference is drawn under, and without a leading `./`.
///
/// Both come off one time, the slot first, as a list of files has always matched a call to the
/// path it was confirmed for. Only the way the name is drawn is decided here, on text already
/// released to a screen.
fn without_reference_slot(target: &str) -> &str {
    let after_slot = || -> Option<&str> {
        let rest = target.strip_prefix("ref:")?;
        let after_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        if after_digits.len() == rest.len() {
            return None;
        }
        let rest = after_digits.strip_prefix('(')?;
        let rest = rest.strip_prefix(['T', 'U'])?.strip_prefix(',')?;
        rest.strip_prefix("pub")
            .or_else(|| rest.strip_prefix("priv"))?
            .strip_prefix("):")
    };
    let target = after_slot().unwrap_or(target);
    target.strip_prefix("./").unwrap_or(target)
}

/// What the kernel counted when it made the diff behind a write's [`Activity::changes`].
///
/// Counted over the two texts before anything was released for a screen, so no interface counts
/// the rows it was given or counts them to find a line number ([FILES-2](../../../docs/specs/files-mode.md#FILES-2)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    /// The file the write landed on, as the workspace resolved it. Two calls that landed on one
    /// file carry the same value, whatever each was called by. Where it came from a reference it
    /// is a name out of a directory nobody vouched for, so it is a key and is never drawn.
    pub landed: std::path::PathBuf,
    /// Lines the write added.
    pub added: usize,
    /// Lines the write removed.
    pub removed: usize,
    /// The line numbers of each row of [`Activity::changes`], in the same order.
    pub numbers: Vec<crate::diff::Numbered>,
}

/// What the turn is waiting on.
///
/// The driver's own words, chosen from the round number and nothing else. A wait that says what
/// it is a wait for is the difference between a slow turn and an apparently stuck one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The first call to the model. It has the task and nothing else, so what it is doing is
    /// working out what to do.
    Planning,
    /// A later call, with tool results in hand.
    Thinking,
    /// The conversation is being summarised so that it stops growing.
    ///
    /// Worth its own word for the same reason as reconnecting: nothing is being worked out, and
    /// a pause the user cannot account for is the difference between a slow turn and a stuck one.
    Compacting,
    /// The request failed in transit and is being sent again.
    ///
    /// Worth its own word because the pause looks like the others and is not one: nothing is
    /// being worked out, and what the model had written has been thrown away.
    Reconnecting,
}

impl Phase {
    /// Which phase a round is, counting rounds already taken.
    pub fn of_round(rounds_taken: usize) -> Self {
        if rounds_taken == 0 {
            Self::Planning
        } else {
            Self::Thinking
        }
    }

    /// The word to show, as a verb someone can read beside a spinner.
    pub fn word(&self) -> &'static str {
        match self {
            Self::Planning => "Planning",
            Self::Thinking => "Thinking",
            Self::Compacting => "Compacting",
            Self::Reconnecting => "Reconnecting",
        }
    }
}

/// How long ago something happened, in the words a person says it in.
///
/// Lives here rather than in the interface because two things need it and a phrase written twice
/// is a phrase that will disagree with itself: the list of sessions saying when one was last
/// touched, and a write saying how old the file it replaced was.
/// Deliberately not from a catalog. This reads back as part of the note a write answers the
/// planner with, in tools.rs, so the words are interface to the model rather than prose for a
/// person. The session list has its own, in the interface, which is translated.
pub fn how_long_ago(age: std::time::Duration) -> String {
    let seconds = age.as_secs();

    let (count, unit) = match seconds {
        0..=59 => return "just now".to_string(),
        60..=3_599 => (seconds / 60, "minute"),
        3_600..=86_399 => (seconds / 3_600, "hour"),
        86_400..=2_591_999 => (seconds / 86_400, "day"),
        _ => (seconds / 2_592_000, "month"),
    };

    if count == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{count} {unit}s ago")
    }
}

/// Something that can be told about progress.
///
/// A trait so a turn does not depend on a terminal: the interactive session draws, a one-shot run
/// ignores, and tests record.
/// How far the content in a [`Shown`] block reaches.
///
/// There is more than one model in a turn, so "not shown to the model" says nothing useful. The
/// driver is not a model at all: it carries bytes it may not read, and has no context to put
/// them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Never in the planner's context. An isolated processor can be sent to read it, if the
    /// planner names it.
    NotThePlanner,
    /// In no model's context at all: nothing can be sent to read it, and it is not part of any
    /// file. What a processor says about its own work is this.
    NoModel,
}

impl Reach {
    /// The sentence a block's heading adds to "untrusted", where it adds one.
    ///
    /// Content kept from the planner is the ordinary reason for a block, and the heading and the
    /// landing line under the call already say it, so repeating it on every block says nothing
    /// new. Content in no model's context is the exception and states it.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Self::NotThePlanner => None,
            Self::NoModel => Some(t!(reach_no_model)),
        }
    }
}

/// Quarantined content, released so the person watching can see it.
///
/// The planner is never shown this and neither is a processor: it goes to a screen and stops
/// there. That is not a hole in the confinement, it is what the confinement is for. The user owns
/// the directory and is entitled to know what their agent is working on; what must not happen is
/// those bytes reaching a model's context, and a screen is not a context.
///
/// Marked wherever it is drawn, and marked structurally rather than by a line of text the content
/// could imitate. See the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// Where it came from, in words a person can act on: a path, or what produced it.
    pub origin: String,
    /// Which contexts this is kept out of.
    pub reach: Reach,
    /// The label it carries, in the same short form the trail uses.
    pub label: String,
    /// The first lines of it, each already trimmed to a sensible width.
    pub preview: Vec<String>,
    /// How many lines there are altogether, so a preview can say what it left out.
    pub lines: usize,
}

/// How a command ended, said from the exit codes and the clock.
///
/// Structure rather than content: nothing here was read out of a byte the program printed, so it
/// may be drawn on a row and told to the planner alike.
///
/// Not a boolean, because a run stopped at the wall-clock limit is neither of the first two, and a
/// background job looked at while it goes on running is none of the three. A server told to serve a
/// page serves it, prints as it goes and never exits, and reporting that as a failure would be
/// wrong. See [tools/run.md](../../../docs/specs/tools/run.md) RUN-11 and RUN-17.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every stage exited zero, or a line with branches did what it was told.
    Succeeded,
    /// A stage exited non-zero or was killed, named as the driver names them.
    Failed {
        detail: String,
        /// What the programs were confined to, where they were, for the planner to read beside
        /// the exit codes. Left out of [`Outcome::summary`]: a person watching chose the
        /// confinement, and the planner is the reader who does not know it applies.
        confinement: Option<String>,
    },
    /// It outstayed the limit and was stopped, with what it had run for by then.
    Stopped(std::time::Duration),
    /// A background job the person asked to stop, with what it had run for by then.
    ///
    /// Separate from [`Outcome::Stopped`] because the planner did not ask for it and no limit was
    /// reached: a planner told its job outstayed a limit tries it again with a longer one.
    StoppedByTheUser(std::time::Duration),
    /// It was still going when it was looked at, and was left going.
    ///
    /// Separate from [`Outcome::Stopped`] because nothing stopped it: a look at a background job is
    /// not the end of the job, and a caller told it was stopped stops asking about a program that
    /// is still printing.
    Running {
        /// How long it has been going, which is not how long the look waited.
        ran_for: std::time::Duration,
        /// How long this look waited for it, where it waited at all.
        ///
        /// The window the answer accounts for. Without it, nothing new reads as a standing account
        /// of the job rather than as an account of some seconds of it.
        waited: Option<std::time::Duration>,
    },
}

/// What the planner is told a stop can leave behind, wherever a run is reported as stopped.
///
/// A stage that does not end when it is asked to is killed, and a program that was killed has not
/// undone what it held: a `git` killed in the middle of a commit leaves its `index.lock`, and the
/// next `git` command fails on it.
const WHAT_A_STOP_LEAVES: &str = "A stop can leave lock files or partly written output behind, \
     and programs the line started may need checking.";

/// A whole number of seconds with its unit, pluralised.
///
/// Every duration these sentences quote goes through here, so none of them can say "1 seconds".
fn seconds(duration: std::time::Duration) -> String {
    crate::tools::tally(duration.as_secs() as usize, "second", "seconds")
}

impl Outcome {
    /// How the caller is told it ended.
    ///
    /// A whole sentence rather than the few words a row has room for, and said beside output the
    /// caller may read and beside a reference to output it may not alike: a program's own bytes
    /// do not say whether it did what it was asked, and a command that fails silently prints
    /// nothing at all.
    pub fn describe(&self) -> String {
        match self {
            Self::Succeeded => "It exited 0.".to_string(),
            Self::Failed {
                detail,
                confinement: None,
            } => format!("It failed: {detail}."),
            Self::Failed {
                detail,
                confinement: Some(confinement),
            } => format!("It failed: {detail}. {confinement}"),
            Self::Stopped(after) => format!(
                "It was still running after {} seconds and was stopped, so this is what it had \
                 printed by then and not the whole of what it would print. {WHAT_A_STOP_LEAVES}",
                after.as_secs()
            ),
            Self::StoppedByTheUser(after) => format!(
                "The user stopped it after {}, so this is what it had printed by then and not the \
                 whole of what it would print. {WHAT_A_STOP_LEAVES} Do not start it again unless \
                 the user asks you to.",
                seconds(*after)
            ),
            Self::Running {
                ran_for,
                waited: None,
            } => format!(
                "It is still running after {} and was left running, so this is what it had \
                 printed by the moment you looked and not the whole of what it will print.",
                seconds(*ran_for)
            ),
            // The window as well as the warning. Both, because each answers a different wrong
            // reading: without the warning a truncated log looks like the whole of one, and without
            // the window a look that came back with nothing looks like an account of the whole job
            // rather than of the seconds it watched.
            Self::Running {
                ran_for,
                waited: Some(waited),
            } => format!(
                "It is still running after {} and was left running. This look waited {} for it, so \
                 this is what it had printed by the end of that wait and not the whole of what it \
                 will print, and where nothing is here nothing arrived in those seconds rather \
                 than nothing at all. Nothing is watching it now.",
                seconds(*ran_for),
                seconds(*waited)
            ),
        }
    }

    /// The driver's few words about how it ended, for the line the person watching reads.
    pub fn summary(&self) -> String {
        match self {
            // Named first among the branches that produce this, because it explains the missing
            // codes that would otherwise be reported as steps killed for no stated reason.
            Self::Stopped(after) => format!(
                "still running after {} seconds, so it was stopped; what it printed first is here",
                after.as_secs()
            ),
            Self::StoppedByTheUser(after) => format!("stopped by you after {}", seconds(*after)),
            Self::Running {
                ran_for,
                waited: None,
            } => format!("still running after {}", seconds(*ran_for)),
            Self::Running {
                ran_for,
                waited: Some(waited),
            } => format!(
                "still running after {}; waited {} for it",
                seconds(*ran_for),
                seconds(*waited)
            ),
            Self::Succeeded => "succeeded".to_string(),
            Self::Failed { detail, .. } => detail.clone(),
        }
    }
}

/// What a command printed, kept whole enough for a person to open.
///
/// Sent for every run, whichever way the label went. What the planner may read decides what
/// enters a model's context; this is a screen, and a person who owns the directory is entitled to
/// read what their agent just ran. "12 lines, quarantined" does not tell them that.
///
/// Released for display by the turn before it gets here, exactly as [`Shown`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed {
    /// The command, as the plan the person endorsed showed it.
    pub command: String,
    /// What it printed, as far back as is kept, each line already trimmed to a sensible width.
    pub lines: Vec<String>,
    /// How many lines there were altogether, so a view can say what it left out.
    pub total: usize,
    /// Whether the planner was allowed to read it.
    ///
    /// The one thing a person cannot work out from the bytes, and the thing the whole design
    /// turns on: the same output either reached a model's context or did not.
    pub read_by_the_planner: bool,
    /// How the command ended.
    ///
    /// The other thing a person cannot work out from the bytes: a build that printed twelve lines
    /// and failed prints much the same twelve lines when it passes.
    pub outcome: Outcome,
    /// The background job this is a look at, where it is one: the name the driver minted for it,
    /// so a display can keep one row per job rather than one per look.
    pub job: Option<String>,
}

/// What happened to a background job, for the person watching (RUN-26).
///
/// Every field but one is the driver's own: a name it minted, the line the person endorsed, the
/// clock and the exit codes. Nothing in them was read out of a byte the job printed, so a display
/// may draw them and decide from them. The exception is [`JobEvent::Printed`], whose lines are
/// bytes the job printed, released for the screen and fit only to be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    /// A job exists: started in the background, or moved there part way through a run.
    Started {
        name: String,
        /// The line as the person endorsed it.
        line: String,
        /// How long the run had been waited for when the person moved it, where they did.
        moved_after: Option<std::time::Duration>,
        /// The token that asks the turn to stop this job and no other.
        stop: bravebot_core::cancel::JobStop,
    },
    /// It exited, or was stopped at the planner's or the person's asking, and somebody has been
    /// told how.
    Ended { name: String, outcome: Outcome },
    /// The turn ended with it still running, and it is stopped with the turn.
    Dropped { name: String },
    /// What a running job has printed that no look has handed the planner, for the screen only
    /// (WATCH-24).
    ///
    /// Released through the same gate as a look's lines, so a display draws them and decides
    /// nothing from them: no mark, count or name is made or changed from what is in `lines`. The
    /// planner's cursor does not move for them, and a look afterwards returns every one.
    Printed {
        name: String,
        /// The last of the unread lines, each already trimmed to a sensible width.
        lines: Vec<String>,
        /// How many unread lines there were altogether, so the row can say what it left out.
        total: usize,
    },
}

/// A few lines of a result the planner read, for the person watching to see beside the call.
///
/// The counterpart of [`Shown`] for content the planner may read, and released the same way. The
/// planner's reading is not what makes a screen safe to draw on; the display gate is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Returned {
    /// The lines, each already trimmed to a sensible width.
    pub lines: Vec<String>,
    /// How many lines there were altogether, so the glimpse can say what it left out.
    pub total: usize,
    /// The last lines rather than the first, because how a command went is at the end of it.
    pub from_the_end: bool,
    /// The same lines as far as the agent kept them, for a key that expands the glimpse. Never
    /// shorter than `lines`, and shorter than `total` where the cap on what is kept cut it.
    pub whole: Vec<String>,
}

/// The command a result came from, and how it ended.
///
/// The two travel together because a run is reported by both: the person watching is shown the
/// line they endorsed beside how it went, and the slot the output lands in records the same line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The command, as the plan the person endorsed showed it.
    pub line: String,
    /// How it ended.
    pub outcome: Outcome,
    /// The background job this is a look at, where it is one.
    pub job: Option<String>,
}

/// What a delegate handed back, in the shape the person may read it.
///
/// Which of the two it is was settled by the gate that decided what the planner got, and not
/// here. A delegate whose own context stayed trusted hands back words, and the person may read
/// exactly what the planner reads; one that met something untrusted hands the planner a reference,
/// and the words are released for a screen and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reported {
    /// The words, as the planner has them.
    Said(String),
    /// Kept from the planner, and drawn in the marked block every quarantined thing is drawn in.
    Kept(Shown),
}

/// Where a tool's result went, which is the thing a person cannot otherwise tell.
///
/// "Read(index.html)" says nothing about whether the model can now read that file, and the
/// difference is the whole design: one of these puts a file in front of the model, one puts it
/// somewhere only an isolated processor can be sent, and one reads nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landing {
    /// Into the planner's context. The model has read this.
    Context,
    /// Into a slot. Nothing has read it but the driver, and the only thing that can be sent to
    /// read it is an isolated processor.
    Quarantined,
    /// Nowhere. The file is named and has not been opened.
    Reserved,
}

impl Landing {
    /// How it reads at the end of a line about a call.
    ///
    /// Names which context, because there is more than one kind of model here and the driver is
    /// not one of them: the planner is the model holding the conversation, and a processor is an
    /// isolated model that is handed slots and nothing else. "The model" answers neither
    /// question a person is asking.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Context => t!(landed_in_the_planner),
            Self::Quarantined => t!(landed_quarantined),
            Self::Reserved => t!(landed_reserved),
        }
    }
}

/// Which run a report describes.
///
/// The kernel's own number, because a gate records under it too: the audit trail names the run
/// whose decision it holds with the same number the screen names the run whose work it shows.
pub use bravebot_core::delegate::DelegateId;

/// One delegate, for the person watching it work.
///
/// Everything here reaches a screen and nothing else. The kind is the driver's own word out of
/// the enumerated set, so it names what the delegate holds; the task is what the planner wrote,
/// released for a screen exactly as the target of a tool call is. Neither is compared, matched
/// or routed anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegation {
    /// Which delegate this is, for everything that follows from it.
    pub id: DelegateId,
    /// Which definition it is: the name the planner selected, which is its kind's own name
    /// where nothing was defined.
    pub kind: String,
    /// What it was asked to do.
    pub task: String,
    /// The token that asks this delegate to stop, and no other and not the turn (DELEGATE-25).
    pub stop: bravebot_core::cancel::DelegateStop,
}

/// A message from another session, as it reached the planner of this one (MSG-11).
///
/// Drawn apart from the person's own prompts. The sender's id and name are the roster's and the
/// label is the one the policy layer gave the message, so none of the three is the sender's word;
/// the text is the sender's, released for a screen only where the planner was given it, and kept
/// in the marked block every quarantined thing is drawn in where it was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeardMessage {
    pub from: String,
    /// The sender's name, as the roster holds it.
    pub name: String,
    /// The label the message was given.
    pub label: String,
    /// The words, or the block that stands for them.
    pub body: Reported,
}

/// A message this session's planner put to another session (MSG-11).
///
/// Drawn where the person reads the transcript, and drawn for every message, so one sent under a
/// standing answer is seen as it goes. The target's id and name are the roster's, the label is the
/// one the policy layer gave the words, and the words are the planner's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMessage {
    /// The receiver's id.
    pub to: String,
    /// The receiver's name, as the roster holds it.
    pub name: String,
    /// The label the message was sent under.
    pub label: String,
    /// What the planner wrote.
    pub text: String,
}

pub trait Reporter {
    /// A measured model call, including retries and their waits, even on failure or stop.
    fn inference_interval(&mut self, _interval: crate::timing::Interval) {}

    /// Whether this interface shows the request a turn built, so a turn builds the view of it only
    /// for one that does.
    fn wants_request_view(&self) -> bool {
        false
    }

    /// The request a turn is about to send the planner, read from the request itself.
    fn request_built(&mut self, _view: crate::request_view::RequestView) {}

    /// The parent is about to join a delegate, which may already have finished.
    fn delegate_waiting(&mut self, _delegate: DelegateId) {}

    /// The submitted prompt was appended at this position in the recounted conversation.
    /// Context may precede it, and a context read can fail before it is appended.
    fn prompt_recorded(&mut self, _at: usize) {}

    /// Cumulative usage from completed requests, the delegates' included as each of theirs
    /// completes. Each report replaces the previous total.
    fn spent(&mut self, _spent: crate::outcome::Spent) {}

    /// The task list changed. Rows are already shaped for display and released.
    fn todos(&mut self, rows: Vec<Row>);

    /// The model has written more of its reply.
    ///
    /// A count and nothing else. The reply is untrusted model output, so passing the text here
    /// would put untrusted content in the driver's hands; how much was written is not content.
    fn output_tokens(&mut self, _written: u64) {}

    /// The turn is waiting on the model again.
    ///
    /// Sent before each request, so the wait before the first tool call says what it is: the
    /// model working out a plan, which is the longest silence in a turn and used to be the
    /// least explained.
    fn phase(&mut self, _phase: Phase) {}

    /// The model said something on its way to calling more tools.
    ///
    /// Released model output, so it is shown rather than acted on, exactly like the final
    /// reply. This text used to be discarded: a turn that explained each step as it went had
    /// every one of those explanations thrown away, and the user saw a spinner instead.
    ///
    /// Sent whether or not there is anything in it. Deciding that from the text would be the
    /// driver taking a decision from untrusted bytes, and an empty line is the interface's to
    /// leave undrawn.
    fn narration(&mut self, _text: String) {}

    /// A summary of the conversation could not be made, so the turn goes on with the history it has.
    ///
    /// The reason is the closed value and not a sentence, so a front end words it in its own way. The
    /// default says it as a narration in the terminal's words.
    fn summary_failed(&mut self, why: Why) {
        self.narration(summary_failed_in_the_terminals_words(why));
    }

    /// The turn's model kept failing and the person's `fallbackModel` takes over for the rest of the
    /// turn (BACKEND-53).
    ///
    /// The reason is the closed value, for the reason [`Reporter::summary_failed`] takes one. The
    /// default says it as a narration in the terminal's words.
    fn fell_back(&mut self, from: &str, to: &str, why: Why) {
        self.narration(fell_back_in_the_terminals_words(from, to, why));
    }

    /// The reply as the model writes it, in the words added since the last call.
    ///
    /// Released for a screen and nowhere else, exactly as the finished text is. Sent whether or
    /// not it is empty: whether there is anything to draw is a question about the text, and the
    /// driver does not get to ask questions about it. An interface that draws this replaces what
    /// it drew when the round's finished text arrives, so nothing is said twice.
    fn streaming(&mut self, _text: String) {}

    /// What the model thought since the last call, for a service that sends its thinking as it
    /// goes (BACKEND-46).
    ///
    /// Released for a screen and nowhere else, through the gate [`Reporter::streaming`] passes,
    /// and sent whether or not it is empty for the same reason. It is no part of the reply: it is
    /// never written into the conversation or the session record and never reaches a later
    /// request, so a caller that draws it draws it only while it is wanted and drops it when the
    /// round ends. A service that sends no thinking never calls this with anything in it.
    fn thinking(&mut self, _text: String) {}

    /// The tool call the model is writing, by the word its row will start with, or `None` once
    /// there is no longer one.
    ///
    /// Released for a screen and nowhere else, as [`Reporter::streaming`] is, and only as a word
    /// from [`verb_for`]'s fixed table: the name is the model's own output and nothing has been
    /// dispatched on it yet, so what it spelt never reaches an interface. Sent when it changes,
    /// from the piece of the reply that opened the call, because a service may send nothing more
    /// until the argument is whole and this is then the only thing there is to say about a wait
    /// that can last minutes. `None` follows an attempt the client threw away. An interface keeps
    /// the last one until then, until the turn's phase is announced, or until its call starts.
    fn composing(&mut self, _call: Option<&'static str>) {}

    /// Say what the turn found before it started: which standing instructions and skills loaded,
    /// and which did not.
    ///
    /// Sent when it is learned, which is before the first request goes out. The outcome carries
    /// the same lines for a caller with no live display, but a caller that has one and waits for
    /// the outcome draws them after every tool line, describing what the turn began with as
    /// though it were the last thing that happened.
    ///
    /// Also what a hook that went wrong is said with, at the moment it goes wrong (HOOK-7). A turn
    /// that fails produces no outcome at all, so a caller that reports at the end rather than
    /// drawing as it goes has to keep these to have anything to say beside the failure.
    ///
    /// The driver's own words about a file it enumerated, never anything read out of one, so
    /// there is nothing here for a display gate to release.
    fn notice(&mut self, _text: String) {}

    /// Untrusted content, for the person watching to read.
    ///
    /// Sent whenever a result is quarantined, which is exactly when the planner is told a
    /// reference and nothing else. The user is not the planner: they own the workspace, they are
    /// the one who can tell whether the agent is working on the right file, and leaving them with
    /// "2 files, quarantined" told them nothing they could use.
    fn quarantined(&mut self, _shown: Shown) {}

    /// What a command printed, for the view a person can open over it.
    ///
    /// Separate from [`Reporter::quarantined`] because the two answer different questions. That
    /// one says what the planner was kept from; this one says what a program printed, whether or
    /// not the planner read it.
    fn printed(&mut self, _output: Printed) {}

    /// A glimpse of a result the planner read, so a person sees what the call found and not only
    /// that it ran.
    fn returned(&mut self, _returned: Returned) {}

    /// Where the result of the call just finished ended up.
    ///
    /// Sent after the kernel has decided, which is why it is not part of the finished call: what
    /// happens to a result is settled by its label, after the tool that produced it has returned.
    fn landed(&mut self, _landing: Landing) {}

    /// A tool call has begun.
    ///
    /// Sent before the call runs, so a slow one is visible while it is slow rather than only
    /// once it is over. That is the whole point: a turn that reads twenty files used to show
    /// nothing at all until it finished.
    fn tool_started(&mut self, _activity: Activity) {}

    /// The call [`Reporter::tool_started`] last announced has finished.
    ///
    /// Paired by position rather than by an identifier because dispatch runs one call at a
    /// time: there is never a second call in flight for this to be ambiguous between.
    fn tool_finished(&mut self, _activity: Activity) {}

    /// The command the call [`Reporter::tool_started`] last announced can be moved to the
    /// background, by requesting this token.
    ///
    /// Sent before the command starts, and only for a line a job can hold. The token is good for
    /// that one run and is read by nothing else, so a display holding it after the call finished
    /// can request it and change nothing.
    fn movable(&mut self, _handoff: bravebot_core::cancel::Handoff) {}

    /// A background job started, ended, or is being stopped with its turn.
    ///
    /// The one account of a job a display gets that does not wait for the planner to look at it:
    /// without it a job moved to the background leaves nothing on the screen saying it runs.
    fn job(&mut self, _event: JobEvent) {}

    /// A confined check has begun, over this many lines of quarantined content, or over a
    /// picture or a PDF.
    ///
    /// A whole model call runs inside the tool call, and the verb already on the screen names
    /// the thing that has not happened yet: a person staring at "Read output" cannot tell a
    /// check that is working from a backend that is hanging. The shape and nothing else, since
    /// how many lines were sent, or which of two kinds of file, is structure rather than any part
    /// of the content.
    fn check_started(&mut self, _checking: Checking) {}

    /// The check [`Reporter::check_started`] announced is over.
    ///
    /// Nothing about what it decided. A verdict reaches a person on the prompt it is drawn on
    /// and reaches no model at all (`CHECK-9`), and this says only that the wait has ended, so
    /// that whatever was drawn for it stops being drawn. Sent however the check ended, including
    /// on the failure that becomes an inconclusive verdict: a display left saying a check is
    /// running because the backend was down is the fault this pair exists to remove.
    fn check_finished(&mut self) {}

    /// A hook is about to start, for this moment, running this program.
    ///
    /// A hook holds the turn open while it runs, and nothing else on screen says so: a slow
    /// formatter looks like a slow model (HOOK-8). The moment's own word and the program as the
    /// person's hooks file spells it, both of which the person wrote, and never anything the
    /// hook printed. Transient: a display shows it for as long as the hook runs and leaves no
    /// line behind, which is what separates it from [`Reporter::notice`].
    fn hook_started(&mut self, _moment: &'static str, _program: String) {}

    /// The hook [`Reporter::hook_started`] announced is over.
    ///
    /// Sent however it ended, a hook that was stopped at its bound or never started included: a
    /// display left saying a hook is running after it has gone is the fault this pair removes.
    fn hook_finished(&mut self) {}

    /// A prompt the person typed mid-turn has reached the planner.
    ///
    /// The user's own words on their way back to them, so there is nothing to release: this is the
    /// one text a display gate has never had anything to say about. Sent because the moment is
    /// what matters. A prompt that waited above the box and then went into the middle of a turn
    /// needs to be seen going in, or the answer that changes course reads as the planner changing
    /// its mind unprompted.
    fn interjected(&mut self, _said: String) {}

    /// A message another session sent has reached the planner.
    ///
    /// Sent at the moment it goes in, for the reason [`Reporter::interjected`] is: a planner that
    /// changes course on a message nobody saw arrive reads as one that changed its mind unprompted.
    fn session_message(&mut self, _heard: HeardMessage) {}

    /// A message this session's planner sent to another session has gone.
    ///
    /// Said once it is queued in the other session's process, and for every message: the person is
    /// asked about the first to a session in a turn and may have said to send the rest, so this is
    /// the only place the later ones are seen.
    fn message_sent(&mut self, _sent: SentMessage) {}

    /// Whose work the reports that follow describe: one delegate, or the turn itself.
    ///
    /// Set by the driver immediately before each report, and the only thing that says where a
    /// line belongs. Delegates run alongside each other and alongside the turn, so the lines
    /// arrive interleaved and the order they arrive in says nothing about whose they are.
    ///
    /// Announced rather than worked out from the line. A line is prose a model had a hand in, and
    /// an interface reading one to decide which run it belonged to would be taking that decision
    /// from model output, which is the thing this repository refuses everywhere else.
    fn reporting_for(&mut self, _delegate: Option<DelegateId>) {}

    /// A delegate has begun.
    fn delegate_started(&mut self, _delegation: Delegation) {}

    /// One delegate has finished, with the words the turn is told about it and what it reported.
    ///
    /// Named rather than paired by position: several run at once, so the one that finishes first
    /// is not the one that started first.
    ///
    /// The note is the driver's own sentence, and the report is the delegate's. They are separate
    /// because they answer different questions: how the run ended, and what it found. A delegate
    /// that could not finish reported nothing, so it carries no report.
    fn delegate_finished(
        &mut self,
        _delegate: DelegateId,
        _note: String,
        _failed: bool,
        _reported: Option<Reported>,
    ) {
    }
}

/// Discards every report.
///
/// The right behaviour where there is no live display: a one-shot command, a pipeline. Unlike
/// refusing a write, discarding a progress report costs nothing, since it was never going to
/// change what the turn did.
#[derive(Debug, Default)]
pub struct IgnoreReports;

impl Reporter for IgnoreReports {
    fn todos(&mut self, _rows: Vec<Row>) {}
}

/// Keeps what it was told, for tests.
#[derive(Debug, Default)]
pub struct RecordingReporter {
    /// Every update in order, so a test can assert on the sequence rather than the end state.
    pub updates: Vec<Vec<Row>>,
    /// Every output-token count reported, in order.
    pub written: Vec<u64>,
    pub spent: Vec<crate::outcome::Spent>,
    pub prompts: Vec<usize>,
    /// Every tool call announced as starting, in order.
    pub started: Vec<Activity>,
    /// Every tool call announced as finished, in order.
    pub finished: Vec<Activity>,
    /// Every token offered for moving a command to the background, in order.
    pub movable: Vec<bravebot_core::cancel::Handoff>,
    /// Every background job event, in order.
    pub jobs: Vec<JobEvent>,
    /// Every check announced as starting, in order, by what it was given.
    pub checks: Vec<Checking>,
    /// How many checks were announced as over.
    pub checks_finished: usize,
    /// Each hook announced as about to start, as its moment and program.
    pub hooks_started: Vec<(&'static str, String)>,
    /// How many hooks were announced over.
    pub hooks_finished: usize,
    /// Every phase the turn entered, in order.
    pub phases: Vec<Phase>,
    /// Everything the model said between tool calls, in order.
    pub narration: Vec<String>,
    /// Every fragment of a reply as it arrived, in order.
    pub streamed: Vec<String>,
    /// Every fragment of the model's thinking as it arrived, in order.
    pub thought: Vec<String>,
    /// The call being written, each time it changed.
    pub composing: Vec<Option<&'static str>>,
    /// What loaded and what did not, in order.
    pub notices: Vec<String>,
    /// Quarantined content released for the screen.
    pub shown: Vec<Shown>,
    /// What each command printed, in the order they ran.
    pub printed: Vec<Printed>,
    /// A glimpse of each result the planner read, in order.
    pub returned: Vec<Returned>,
    /// Where each result went.
    pub landed: Vec<Landing>,
    /// Everything the person said mid-turn, in the order it reached the planner.
    pub interjected: Vec<String>,
    /// Every message from another session, in the order it reached the planner.
    pub heard: Vec<HeardMessage>,
    /// Every message this session's planner sent to another session, in the order it was sent.
    pub sent: Vec<SentMessage>,
    /// Every delegate announced as starting, in order.
    pub delegated: Vec<Delegation>,
    /// How each delegate ended, in the order they finished.
    pub delegates_finished: Vec<(DelegateId, String, bool)>,
    /// What each delegate reported, in the order they finished.
    pub delegates_reported: Vec<(DelegateId, Option<Reported>)>,
    /// Whose work the reports that follow belong to.
    pub attributed_to: Option<DelegateId>,
    /// Every request view a turn published, in order.
    pub requests: Vec<crate::request_view::RequestView>,
}

impl Reporter for RecordingReporter {
    fn wants_request_view(&self) -> bool {
        true
    }

    fn request_built(&mut self, view: crate::request_view::RequestView) {
        self.requests.push(view);
    }

    fn prompt_recorded(&mut self, at: usize) {
        self.prompts.push(at);
    }

    fn spent(&mut self, spent: crate::outcome::Spent) {
        self.spent.push(spent);
    }

    fn todos(&mut self, rows: Vec<Row>) {
        self.updates.push(rows);
    }

    fn output_tokens(&mut self, written: u64) {
        self.written.push(written);
    }

    fn phase(&mut self, phase: Phase) {
        self.phases.push(phase);
    }

    fn narration(&mut self, text: String) {
        self.narration.push(text);
    }

    fn streaming(&mut self, text: String) {
        self.streamed.push(text);
    }

    fn thinking(&mut self, text: String) {
        self.thought.push(text);
    }

    fn composing(&mut self, call: Option<&'static str>) {
        self.composing.push(call);
    }

    fn notice(&mut self, text: String) {
        self.notices.push(text);
    }

    fn tool_started(&mut self, activity: Activity) {
        self.started.push(activity);
    }

    fn tool_finished(&mut self, activity: Activity) {
        self.finished.push(activity);
    }

    fn movable(&mut self, handoff: bravebot_core::cancel::Handoff) {
        self.movable.push(handoff);
    }

    fn job(&mut self, event: JobEvent) {
        self.jobs.push(event);
    }

    fn check_started(&mut self, checking: Checking) {
        self.checks.push(checking);
    }

    fn check_finished(&mut self) {
        self.checks_finished += 1;
    }

    fn hook_started(&mut self, moment: &'static str, program: String) {
        self.hooks_started.push((moment, program));
    }

    fn hook_finished(&mut self) {
        self.hooks_finished += 1;
    }

    fn quarantined(&mut self, shown: Shown) {
        self.shown.push(shown);
    }

    fn printed(&mut self, output: Printed) {
        self.printed.push(output);
    }

    fn returned(&mut self, returned: Returned) {
        self.returned.push(returned);
    }

    fn landed(&mut self, landing: Landing) {
        self.landed.push(landing);
    }

    fn interjected(&mut self, said: String) {
        self.interjected.push(said);
    }

    fn session_message(&mut self, heard: HeardMessage) {
        self.heard.push(heard);
    }

    fn message_sent(&mut self, sent: SentMessage) {
        self.sent.push(sent);
    }

    fn reporting_for(&mut self, delegate: Option<DelegateId>) {
        self.attributed_to = delegate;
    }

    fn delegate_started(&mut self, delegation: Delegation) {
        self.delegated.push(delegation);
    }

    fn delegate_finished(
        &mut self,
        delegate: DelegateId,
        note: String,
        failed: bool,
        reported: Option<Reported>,
    ) {
        self.delegates_finished.push((delegate, note, failed));
        self.delegates_reported.push((delegate, reported));
    }
}

/// The driver's word for what a tool does.
///
/// Chosen from the tool's name, which dispatch already matches on, so this decides nothing new.
/// A literal rather than the raw name because the line is read by a person: "Read(src/main.rs)"
/// says what happened and "read_file" says what was typed.
pub(crate) fn verb_for(tool: &str) -> &'static str {
    match tool {
        "read_file" => t!(verb_read_file),
        "list_files" => t!(verb_list_files),
        "search" => t!(verb_search),
        "read_git" => t!(verb_read_git),
        "repo_map" => t!(verb_repo_map),
        "lsp" => t!(verb_lsp),
        "write_file" => t!(verb_write_file),
        "edit_file" => t!(verb_edit_file),
        "apply_checkout" => t!(verb_apply_checkout),
        "todo_write" => t!(verb_todo_write),
        // Named for what it is rather than for what it does: every one of these is a model
        // with no tools, no memory and one round, and a person watching a line go by should not
        // have to remember which of the verbs meant that.
        "spawn_processor" => t!(verb_spawn_processor),
        "load_skill" => t!(verb_load_skill),
        "search_skills" => t!(verb_search_skills),
        "load_tool" => t!(verb_load_tool),
        "ask_user" => t!(verb_ask_user),
        "request_path" => t!(verb_request_path),
        "run" => t!(verb_run),
        "read_output" => t!(verb_read_output),
        "vet_content" => t!(verb_vet_content),
        "fetch_url" => t!(verb_fetch_url),
        "download_url" => t!(verb_download_url),
        "job_output" => t!(verb_job_output),
        // Named for what it is rather than for what it does, exactly as a processor is: a person
        // watching a line go by should be able to see that the work moved somewhere else.
        "spawn_agent" => t!(verb_spawn_agent),
        "schedule_next" => t!(verb_schedule_next),
        "watch_file" => t!(verb_watch_file),
        "advisor" => t!(verb_advisor),
        "propose_plan" => t!(verb_propose_plan),
        "message_session" => t!(verb_message_session),
        "remember" => t!(verb_remember),
        "propose_rule" => t!(verb_propose_rule),
        "recall" => t!(verb_recall),
        // Every server's tool is offered under a wire name of this shape, and no built-in is.
        server if server.starts_with("mcp__") => t!(verb_mcp_call),
        _ => t!(verb_unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::todo::{Item, List, Status, rows};

    #[test]
    fn a_recording_reporter_keeps_each_update_in_order() {
        let mut reporter = RecordingReporter::default();
        reporter.todos(rows(&List::new(vec![Item::new("one", Status::Pending)])));
        reporter.todos(rows(&List::new(vec![Item::new("one", Status::Done)])));

        assert_eq!(reporter.updates.len(), 2);
        assert!(!reporter.updates[0][0].struck());
        assert!(reporter.updates[1][0].struck());
    }

    /// Nothing to draw is not a failure. A reporter has no way to refuse, by design: there is no
    /// return value it could refuse with.
    #[test]
    fn ignoring_reports_is_infallible() {
        IgnoreReports.todos(rows(&List::new(vec![Item::new("x", Status::Active)])));
        IgnoreReports.tool_started(Activity::running("Read", "src/main.rs"));
        IgnoreReports.tool_finished(Activity::running("Read", "src/main.rs").done("12 lines"));
    }

    /// The distinction the display draws everything else from: a line with no note is a call
    /// still in flight, and one with a note is over.
    #[test]
    fn an_activity_is_running_until_it_has_a_note() {
        let started = Activity::running("Read", "src/main.rs");
        assert!(started.is_running());
        assert!(!started.clone().done("12 lines").is_running());
        assert!(!started.failed("refused").is_running());
    }

    /// A refusal has to be distinguishable from a success without reading the note, or the
    /// interface would be matching on prose to decide what colour to draw.
    #[test]
    fn a_refusal_is_marked_as_one_rather_than_described_as_one() {
        let refused = Activity::running("Update", "a.rs").failed("refused: not approved");
        assert!(refused.failed);
        assert!(!Activity::running("Update", "a.rs").done("+1 -0").failed);
    }

    /// Two wrong readings, so two sentences. Without the warning a log that stops in the middle
    /// reads as the whole of one; without the window a look that came back with nothing reads as an
    /// account of the whole job rather than of the seconds it watched.
    #[test]
    fn a_look_that_waited_is_described_with_both_the_window_and_the_warning() {
        let said = Outcome::Running {
            ran_for: std::time::Duration::from_secs(90),
            waited: Some(std::time::Duration::from_secs(30)),
        }
        .describe();

        assert!(
            said.contains("waited 30 seconds"),
            "the window the look watched is not in what the caller is told: {said}"
        );
        assert!(
            said.contains("not the whole of what it will print"),
            "a partial log is described as the whole of one: {said}"
        );
    }

    /// A wait that ended early on output reads like one that sat out its bound, and nothing new
    /// reads like a standing account of the job. Saying that nothing is watching now is what makes
    /// the answer an account of those seconds only. The sentence belongs to a look that watched,
    /// whatever it waited for, and not to one that only took a snapshot.
    #[test]
    fn a_look_that_waited_says_nothing_is_watching_the_job_now() {
        for waited in [1, 30, 600] {
            let said = Outcome::Running {
                ran_for: std::time::Duration::from_secs(900),
                waited: Some(std::time::Duration::from_secs(waited)),
            }
            .describe();
            assert!(
                said.contains("Nothing is watching it now."),
                "a wait of {waited}s does not say the job is no longer watched: {said}"
            );
        }

        let snapshot = Outcome::Running {
            ran_for: std::time::Duration::from_secs(900),
            waited: None,
        }
        .describe();
        assert!(
            !snapshot.contains("watching"),
            "a look that watched nothing claims a watch ended: {snapshot}"
        );
    }

    /// Every duration in these sentences is a number the planner reads, and "1 seconds" in one of
    /// them is a driver that cannot count reporting on a program.
    #[test]
    fn one_second_is_described_in_the_singular() {
        let said = Outcome::Running {
            ran_for: std::time::Duration::from_secs(1),
            waited: Some(std::time::Duration::from_secs(1)),
        }
        .describe();

        assert!(
            !said.contains("1 seconds"),
            "a one-second duration is described in the plural: {said}"
        );
        assert_eq!(
            said.matches("1 second").count(),
            2,
            "both of the durations should read as one second: {said}"
        );
    }

    /// A planner told only that a run was stopped treats the repository as it was before the run,
    /// and the next `git` command fails on the lock the stopped one left. The sentence is for a
    /// stop, at the deadline or at the person's asking, and for no other ending: a job left running
    /// has been stopped by nothing.
    #[test]
    fn a_stop_says_it_can_leave_files_behind_and_a_run_still_going_does_not() {
        for (stopped, how) in [
            (
                Outcome::Stopped(std::time::Duration::from_secs(300)).describe(),
                "at the deadline",
            ),
            (
                Outcome::StoppedByTheUser(std::time::Duration::from_secs(40)).describe(),
                "by the person",
            ),
        ] {
            assert!(
                stopped.contains("A stop can leave lock files or partly written output behind"),
                "a run stopped {how} does not say what a stop can leave behind: {stopped}"
            );
            assert!(
                stopped.contains("programs the line started may need checking"),
                "a run stopped {how} does not say the programs it started may be running: {stopped}"
            );
        }

        let going = Outcome::Running {
            ran_for: std::time::Duration::from_secs(90),
            waited: None,
        }
        .describe();
        assert!(
            !going.contains("lock files"),
            "a run nothing stopped is warned about the effects of a stop: {going}"
        );
    }

    /// The first wait is the one that needs explaining: the model has the task and nothing
    /// else, and there is no tool call yet to show for it.
    #[test]
    fn the_first_round_is_planning_and_the_rest_are_not() {
        assert_eq!(Phase::of_round(0), Phase::Planning);
        assert_eq!(Phase::of_round(1), Phase::Thinking);
        assert_eq!(Phase::of_round(9), Phase::Thinking);
        assert_ne!(Phase::Planning.word(), Phase::Thinking.word());
    }

    #[test]
    fn ages_read_the_way_a_person_says_them() {
        use std::time::Duration;
        assert_eq!(how_long_ago(Duration::from_secs(3)), "just now");
        assert_eq!(how_long_ago(Duration::from_secs(60)), "1 minute ago");
        assert_eq!(how_long_ago(Duration::from_secs(13 * 60)), "13 minutes ago");
        assert_eq!(how_long_ago(Duration::from_secs(2 * 3_600)), "2 hours ago");
        assert_eq!(how_long_ago(Duration::from_secs(86_400)), "1 day ago");
        assert_eq!(
            how_long_ago(Duration::from_secs(40 * 86_400)),
            "1 month ago"
        );
    }

    #[test]
    fn a_line_names_what_was_acted_on() {
        assert_eq!(
            Activity::running("Read", "src/main.rs").line(),
            "Read(src/main.rs)"
        );
    }

    /// A line that names a file spells a name a person could not otherwise tell from another
    /// (PROMPT-1), and one that names anything else draws it as written. The target itself is
    /// left as it was, since a window matches a call to its confirmation on it.
    #[test]
    fn a_line_spells_the_name_of_a_file_and_nothing_else() {
        let read = |target: &str| Activity::running("Read", target).naming_a_file(true);

        assert_eq!(read("src/main.rs").line(), "Read(src/main.rs)");
        assert_eq!(read("note.md ").line(), "Read(\"note.md\u{2423}\")");
        assert_eq!(read(" ").line(), "Read(\"\u{2423}\")");
        assert_eq!(read("a\u{200b}b").line(), "Read(\"a\\u{200b}b\")");
        assert_eq!(read("note.md ").target, "note.md ");
        assert_eq!(read("note.md ").shown_target(), "\"note.md\u{2423}\"");

        // A search for `todo ` looks for the space too, and a line saying otherwise is wrong.
        let search = Activity::running("Search", "todo ");
        assert_eq!(search.line(), "Search(todo )");
        assert_eq!(search.shown_target(), "todo ");

        // A call that named nothing has no target to spell, so nothing is drawn for it.
        assert_eq!(read("").shown_target(), "");
        assert_eq!(read("").line(), "Read");
    }

    /// A list of files names a file once however the planner named it, and spells the name as a
    /// prompt about it does (PROMPT-1): the slot a reference was drawn under and a leading `./`
    /// come off, once each and in that order, and what is left is spelled. Anything that is not
    /// a file's name keeps its target whole.
    #[test]
    fn a_list_of_files_names_a_file_without_its_slot_and_spelled() {
        let file = |target: &str| Activity::running("Write", target).naming_a_file(true);

        assert_eq!(file("src/main.rs").shown_file(), "src/main.rs");
        assert_eq!(
            file("ref:3(U,priv):note.md ").shown_file(),
            "\"note.md\u{2423}\""
        );
        assert_eq!(file("ref:12(T,pub):note.md").shown_file(), "note.md");
        assert_eq!(file("./other.md ").shown_file(), "\"other.md\u{2423}\"");
        assert_eq!(file("ref:1(U,priv):./a.md").shown_file(), "a.md");
        // Once each: a second `./` is part of the name, and so is a slot that is not one.
        assert_eq!(file(".//a.md").shown_file(), "/a.md");
        assert_eq!(file("ref:(U,priv):a.md").shown_file(), "ref:(U,priv):a.md");
        assert_eq!(
            file("ref:1(X,priv):a.md").shown_file(),
            "ref:1(X,priv):a.md"
        );
        assert_eq!(
            file("ref:1(U,secret):a.md").shown_file(),
            "ref:1(U,secret):a.md"
        );
        // Taking the slot off a name that is nothing but one leaves nothing to draw as a name.
        assert_eq!(file("ref:1(U,priv):").shown_file(), "\"\"");
        assert_eq!(file("").shown_file(), "");

        let search = Activity::running("Search", "./todo ");
        assert_eq!(search.shown_file(), "./todo ");
    }

    /// CHATGPT-8: a summary that failed on the ChatGPT account is told in that failure's sentence,
    /// which holds the wait a refusal asked for and has no status after it, and every other failure
    /// keeps the name of its category, since nothing in a reply may be put in the line. The
    /// regressions it rejects are the category's name for a failure of the account, a sentence for a
    /// failure that is not the account's, such as a gateway's refused key, and the status of a
    /// refusal put after the sentence, which `/compact` and the narrated line leave out.
    #[test]
    fn a_failed_summary_says_the_accounts_sentence_where_it_was_the_account_and_the_category_otherwise()
     {
        use crate::backend::BackendError;
        use crate::compact::{CompactError, SideModelRefusal};
        use bravebot_aichat::{AccountFailure, ChatError};

        let chat = |error: ChatError| CompactError::Chat(BackendError::from(error));
        for failure in [AccountFailure::Ended, AccountFailure::NotSignedIn] {
            assert_eq!(
                why_a_summary_failed(&chat(ChatError::Account(failure))),
                crate::chatgpt::said_of_failure(failure),
                "{failure:?}"
            );
        }
        // A refusal that came as a 429 with a wait has the wait in its sentence and no status after it.
        for refusal in [
            bravebot_aichat::responses::protocol::Refusal::UsageLimit,
            bravebot_aichat::responses::protocol::Refusal::NotPermitted,
        ] {
            let failure = AccountFailure::Named {
                refusal,
                status: Some(429),
                retry_after: Some(std::time::Duration::from_secs(600)),
            };
            let told = why_a_summary_failed(&chat(ChatError::Account(failure)));
            assert_eq!(
                told,
                crate::chatgpt::said_of_failure(failure),
                "{refusal:?}"
            );
            assert!(told.contains("a wait of about 10 minutes"), "{told}");
            assert!(!told.contains("HTTP") && !told.contains("429"), "{told}");
        }
        let status = |status: u16| {
            chat(ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://api.example.invalid/v1/responses".to_string(),
                status,
                retry_after: None,
                body: None,
            }))
        };
        assert_eq!(why_a_summary_failed(&status(401)), "unauthorized");
        assert_eq!(why_a_summary_failed(&status(503)), "unavailable");
        assert_eq!(
            why_a_summary_failed(&CompactError::SummaryModel(SideModelRefusal::NeedsSignIn(
                "a-model".to_string()
            ))),
            "unconfigured"
        );
    }

    /// CHATGPT-8: the line a turn narrates for a summary the account could not make is the account's
    /// sentence, and for a refusal that came as a 429 with a wait it holds the wait and has no status
    /// after it. The failure is handed to the reporter as the closed value the turn makes from the
    /// error, as `round_steps` hands it. The regression it rejects is the status of the refusal put
    /// after the sentence, as the terminal's line for a failed turn puts it.
    #[test]
    fn the_line_for_a_summary_the_account_refused_has_the_wait_and_no_status() {
        use crate::backend::BackendError;
        use crate::compact::CompactError;
        use bravebot_aichat::responses::protocol::Refusal;
        use bravebot_aichat::{AccountFailure, ChatError};

        for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
            let failure = AccountFailure::Named {
                refusal,
                status: Some(429),
                retry_after: Some(std::time::Duration::from_secs(600)),
            };
            let error = CompactError::Chat(BackendError::from(ChatError::Account(failure)));
            let mut reporter = RecordingReporter::default();
            reporter.summary_failed(Why::of_summary(&error));

            let [line] = reporter.narration.as_slice() else {
                panic!("{refusal:?}: {:?}", reporter.narration);
            };
            assert_eq!(
                *line,
                t!(
                    compact_failed_and_continuing,
                    problem = crate::chatgpt::said_of_failure(failure)
                ),
                "{refusal:?}"
            );
            assert!(line.contains("a wait of about 10 minutes"), "{line}");
            assert!(!line.contains("HTTP") && !line.contains("429"), "{line}");
        }
    }

    /// XAI-15: a summary the xAI account could not make, and a move to the fallback model off it, are
    /// told in the sentence of the xAI failure and not in the category's name, and not in ChatGPT's. A
    /// reporter that fell back to `Why::Category` for the xAI value would say "unauthorized", which names
    /// no command, and one that used ChatGPT's wording would send a person to sign in to ChatGPT.
    #[test]
    fn a_failure_of_the_xai_account_is_told_in_its_own_sentence_in_a_summary_and_a_fallback() {
        use crate::backend::BackendError;
        use crate::compact::CompactError;
        use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure, XaiFailure};

        for failure in [
            XaiFailure::NotSignedIn,
            XaiFailure::Rejected,
            XaiFailure::NotPermitted,
            XaiFailure::Renewal(RenewalFailure::RateLimited),
        ] {
            let said = crate::xai::said_of_failure(failure);
            let error = CompactError::Chat(BackendError::from(ChatError::Account(
                AccountFailure::Xai(failure),
            )));
            assert_eq!(why_a_summary_failed(&error), said, "{failure:?}");

            let mut reporter = RecordingReporter::default();
            reporter.summary_failed(Why::of_summary(&error));
            assert_eq!(
                reporter.narration,
                [t!(compact_failed_and_continuing, problem = said.clone())],
                "{failure:?}"
            );
            let moved = fell_back_in_the_terminals_words(
                "xai/grok-4.5",
                "xai/grok-4.6",
                Why::Account(AccountFailure::Xai(failure)),
            );
            assert!(moved.contains(&said), "{failure:?}: {moved}");
            assert!(!moved.contains("ChatGPT"), "{failure:?}: {moved}");
        }
    }

    /// Some work has nothing to name, and an empty pair of brackets reads as a bug rather than
    /// as an absent target.
    #[test]
    fn a_line_with_nothing_to_name_is_the_verb_alone() {
        assert_eq!(Activity::running("Plan", "").line(), "Plan");
    }

    /// Every tool the model is offered needs a word of its own. Without this a new tool
    /// shows up in the transcript as the fallback, which tells the user nothing.
    #[test]
    fn every_offered_tool_has_its_own_verb() {
        for tool in crate::tools::available(
            crate::tools::Scheduling::ArrangingALook,
            crate::watch::Arming::Allowed { free: 1 },
            crate::exec::Deadlines::BUILT_IN,
            crate::tools::Running::Offered,
        )
        .into_iter()
        .chain(crate::tools::available(
            crate::tools::Scheduling::PacingALoop,
            crate::watch::Arming::Allowed { free: 1 },
            crate::exec::Deadlines::BUILT_IN,
            crate::tools::Running::Offered,
        ))
        .chain({
            let mut added = Vec::new();
            crate::tools::offer_advisor(&mut added);
            crate::tools::offer_message_session(&mut added);
            crate::tools::offer_remember(&mut added);
            crate::tools::offer_propose_rule(&mut added);
            crate::tools::offer_recall(&mut added);
            added
        }) {
            let name = &tool.function.name;
            assert_ne!(
                verb_for(name),
                verb_for("something nobody wrote"),
                "{name} has no verb of its own"
            );
        }
    }
}
