//! Sessions kept on disk, so one can be picked up again tomorrow.
//!
//! Under `~/.bravebot/sessions`, one directory per working directory, because a session belongs to
//! the checkout it happened in: the list worth seeing when resuming in one project is not the
//! list from another. The directory is named after the path it stands for, mangled into one
//! segment, and the real path is written inside the record as well, since the mangling is not
//! reversible.
//!
//! Two files per session. The **record** holds what the picker shows and what a resume needs:
//! the conversation, and what the transcript showed beside it, which is the plan each turn worked
//! to and what the whole session has spent. The **audit** holds every gate decision the session
//! made, one JSON object per line, which is the file to read when the question is what the agent
//! was allowed to do and why. It is also read back on a resume, since a trail under the turns
//! from this process and nothing under the earlier ones is a worse account than either.
//!
//! The trust map is in the record too, and belongs there rather than to the directory: a map kept
//! per directory would answer the startup question for a user who was never asked. A fresh session
//! asks; a resumed one inherits what its own user answered.
//!
//! # What is written, and what is not
//!
//! Every message in the record has already been past the present gate, so what lands on disk is
//! what the planner was allowed to hold: no untrusted bytes, by construction rather than by
//! filtering. The same goes for the task lists, which came out of the render gate on their way to
//! the screen. The quarantine is not written at all, and the audit is labels and gate names with
//! no content in it. See [`bravebot_agent::conversation::Snapshot`].
//!
//! # Permissions
//!
//! Session files and directories are restricted to the current user. On Unix systems, session
//! directories are created with mode 0700 and session records, temporary files, and audit trails
//! are written with mode 0600. A conversation record contains source code from private
//! repositories, secrets appearing in context, and standing permissions accumulated during
//! execution; keeping permissions restricted prevents other local accounts on shared machines from
//! reading them.
//!
//! Everything degrades to doing nothing. A missing home, a full disk, a corrupt record: a
//! session that cannot be written down still runs, and one that cannot be read is left out of
//! the list rather than taken as a reason to fail.

use bravebot_agent::conversation::Snapshot;
use bravebot_agent::workspace::Workspace;
use bravebot_core::command::Spelling;
use bravebot_core::label::Integrity;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::todo::{self, Item, List, Row, Status};
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A question asked beside the work, and the answer it came back with.
///
/// Never in the conversation. The question forked the exchange, was answered over the copy, and
/// the copy went: the planner picking the work up has read neither half, which is what makes an
/// aside a question rather than a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aside {
    /// What the person asked, in their own words.
    pub question: String,
    /// The answer, as the person may read it.
    ///
    /// `None` for one brought back from a record that could not hold it, where the view says so
    /// rather than drawing an answer that is not there. Absence rather than emptiness, because a
    /// model that answered with nothing at all is a different thing from an answer that did not
    /// come back, and the interface must not have to tell the two apart by reading the words.
    pub answer: Option<String>,
    /// Whether the record keeps the answer, so that a resume brings it back.
    ///
    /// `false` where the exchange had met something untrusted when the question was asked: the
    /// planner's own words are quarantined then, like anything else, and a record is read back.
    pub kept: bool,
}

/// A checkpoint of session state captured before a turn begins, for `/undo`.
#[derive(Debug, Clone)]
pub struct TurnSnapshot {
    /// The conversation state (messages, references, context).
    pub conversation: bravebot_agent::conversation::Snapshot,
    /// Completed turns count before this turn.
    pub turns: usize,
    /// Cumulative tokens before this turn.
    pub tokens: u64,
    /// Spend by turn before this turn.
    pub spend: std::collections::BTreeMap<usize, u64>,
    /// Timing by turn before this turn.
    pub timing: std::collections::BTreeMap<usize, bravebot_agent::timing::Timing>,
    /// What the turn before this one read out of the cache, where this process measured it.
    ///
    /// Kept with the spend it belongs beside: undoing a turn that is no longer in the token count
    /// must not leave the panel reporting the cache that turn hit. `None` for the first turn of a
    /// session, and for every point a resume brought back, [`StoredRewind`] keeping no figure.
    pub cached: Option<bravebot_aichat::protocol::Cached>,
    /// Prompt tokens behind [`TurnSnapshot::cached`], for its hit rate.
    pub cached_prompt_tokens: Option<u64>,
    /// Trust map rules before this turn.
    pub trust: bravebot_core::trust::TrustStore,
    /// Trusted programs before this turn.
    pub programs: bravebot_core::programs::TrustedPrograms,
    /// Length of transcript entries before this turn.
    pub transcript_len: usize,
    /// Stored session title before this turn.
    pub title: String,
    /// Whether the session had already been written to disk before this turn.
    pub was_wrote: bool,
}

/// How many turns back a rewind may reach.
///
/// A point holds a copy of the conversation as well as the bytes the turn wrote over, and every
/// one of them is written into the record after every turn, so depth is paid for continuously by
/// sessions that never rewind at all. Five is set at the case a rewind exists for, which is a
/// mistake noticed a few prompts after it was made rather than one noticed an hour later: past
/// that the conversation has usually moved somewhere a wholesale rewind would not be wanted.
pub const MAX_REWIND_POINTS: usize = 5;

/// One point a session can be put back to, and what it would take to get there.
///
/// The snapshot is taken before the turn begins and the backups arrive when it ends, so a point
/// exists for the whole of the turn it describes and is only complete afterwards.
#[derive(Debug, Clone)]
pub struct RewindPoint {
    /// Positive evidence that no uncovered effect has begun.
    pub coverage: bravebot_agent::workspace::RewindCoverage,
    /// What the session held before the turn.
    pub snapshot: TurnSnapshot,
    /// What the files that turn wrote to held before it wrote to them.
    pub backups: Vec<bravebot_agent::workspace::Backup>,
    /// The prompt the turn began with.
    ///
    /// Kept here rather than read back out of the transcript, because the list is offered after
    /// the transcript has been rewound past other points and a turn is named by what was asked
    /// of it.
    pub prompt: String,
}

/// Where sessions live inside the state directory.
const SESSIONS: &str = "sessions";

/// Which front end wrote a record.
///
/// Two programs share `~/.bravebot` and each writes the same kind of record into it, so a
/// transcript read after the fact is a transcript of one of them and there is nothing in the
/// content to say which. The build stamp answers which code ran; this answers which of the two
/// surfaces that code drew, and the pair is what a reader needs before treating the transcript as
/// evidence of anything.
///
/// Supplied by the caller for the same reason the build is: this crate is below both front ends
/// and cannot see which one is calling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    /// The `bravebot` command: its full-screen interface, and the runs it takes from the command
    /// line without drawing one.
    ///
    /// One variant rather than two, because both are the same binary reading the same terminal,
    /// and a manifest run started from a session and one started from the command line are
    /// deliberately written down the same way.
    Terminal,
    /// The desktop application, through the bridge it links the agent into.
    Desktop,
}

impl Front {
    /// The word written into the record.
    ///
    /// Stable across releases: a record outlives the build that wrote it, and a word that changed
    /// would make every record written before the change read as a front end nothing recognises.
    pub fn recorded(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Desktop => "desktop",
        }
    }

    /// What to call it to a person, for a word read back out of a record.
    ///
    /// A word this build does not know is shown as it was written rather than dropped. Such a
    /// record comes from a front end added after this build, and naming it as it named itself says
    /// more than saying nothing does.
    fn named(word: &str) -> String {
        match word {
            "terminal" => t!(session_front_terminal).to_string(),
            "desktop" => t!(session_front_desktop).to_string(),
            other => other.to_string(),
        }
    }
}

/// A session as it is written down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Unique within its project directory, and sortable, since it starts with the time.
    pub id: String,
    /// The working directory the session ran in, as it was.
    pub directory: String,
    /// The branch checked out at the time, where there was one.
    #[serde(default)]
    pub branch: Option<String>,
    /// The issue the person said the session is for, with `/issue`.
    ///
    /// `None` for a record written before this was kept, or a session nobody gave one.
    #[serde(default)]
    pub issue: Option<String>,
    /// The pull request the person said the session is for, with `/pr`.
    #[serde(default)]
    pub pull_request: Option<String>,
    /// What to call it in a list: the first thing the user asked.
    pub title: String,
    /// When it began and when it was last written, in seconds since the epoch.
    pub started: u64,
    pub updated: u64,
    /// How many turns it has had, for a reader of the file.
    #[serde(default)]
    pub turns: usize,
    /// What the session has spent, in tokens, across every turn it has had.
    ///
    /// The figure answers "what has this cost me", which is a question about the session rather
    /// than about the process that happened to be running it.
    #[serde(default)]
    pub tokens: u64,
    /// The model the server reported answering with, as of the last turn.
    ///
    /// What answered rather than what was asked for, since those differ: an endpoint may serve
    /// something other than the name it was given, and the one that did the work is the one a
    /// reader of this file needs. The name asked for is not kept, being a setting rather than a
    /// fact about the session.
    ///
    /// `None` for a record written before this was kept, or one whose turns never reached a
    /// server.
    #[serde(default)]
    pub model: Option<String>,
    /// What each turn cost, by turn number, and what was spent before the first turn under zero.
    ///
    /// The total says what the session cost; this says which turn cost it. Reading a session back
    /// to find out why it was expensive, a total cannot tell twenty even turns from one that ran
    /// away, and those are different problems. A key of zero is what an aside or a run asked for
    /// as the first thing the session did cost, which belongs to no turn and is in the total all
    /// the same.
    ///
    /// Empty for a record written before this was kept, which needs no question: the total is
    /// still there and only the breakdown is missing.
    #[serde(default)]
    pub spend: BTreeMap<usize, u64>,
    /// Where each turn's wall clock went, by turn number.
    ///
    /// The other half of the question [`Record::spend`] answers. Tokens say what a turn cost the
    /// endpoint; this says what it cost the person sitting in front of it, and the two need not
    /// agree at all: the cheapest turn in a session is often the one that stopped, put a command on
    /// the screen, and waited twenty minutes to be allowed to run it.
    ///
    /// Empty for a record written before this was kept, which is not the same as a session that
    /// took no time.
    #[serde(default)]
    pub timing: BTreeMap<usize, bravebot_agent::timing::Timing>,
    /// The task list each turn worked to, by turn number.
    ///
    /// Kept per turn rather than as one list, because that is how the transcript shows it: the
    /// plan a turn set out with sits beneath what that turn produced.
    #[serde(default)]
    pub todos: BTreeMap<usize, Vec<StoredTask>>,
    /// Which paths this session's user vouched for, and what its writes recorded since.
    ///
    /// Belongs to the session rather than to the directory, and that is the whole point. A map
    /// kept per directory would answer the startup question on behalf of a user who was never
    /// asked, which is trust assumed from silence. Resuming inherits it because the person
    /// resuming is the person who gave it; a session started fresh in the same directory is
    /// asked, and answers for itself.
    ///
    /// `None` for a record written before this was kept, which is asked about rather than read
    /// as an empty map: nothing recorded is not the same as nothing trusted.
    #[serde(default)]
    pub trust: Option<Vec<StoredRule>>,
    /// Which commands this session's user vouched for: resolved path and exact arguments.
    ///
    /// Belongs to the session for the same reason the trust map does: vouching for a command is a
    /// standing permission over both its side effects and the trust of its output, and a list kept
    /// per directory would grant that on behalf of a user who was never asked. Resuming inherits
    /// it because the person resuming is the person who gave it.
    ///
    /// Absent, unlike the map, needs no question: an empty list means every run asks and no
    /// output is trusted, which is what a session that recorded nothing should do.
    #[serde(default)]
    pub programs: Vec<StoredCommand>,
    /// Which directories this session opened with `/add-dir`, canonical.
    ///
    /// The trust map carries only half of what `/add-dir` did. The other half is reachability: an
    /// absolute path is refused whatever the map says unless the directory is open, so a record
    /// with the rule and not the directory resumes holding a rule about files nothing can open.
    /// Both halves are written, and both come back together.
    ///
    /// Empty for a record written before this was kept, which needs no question: such a session
    /// resumes with the rules it recorded and nothing open, exactly as it did before.
    #[serde(default)]
    pub directories: Vec<String>,
    /// Which build wrote this record: the version, the commit, and whether the tree was
    /// modified, as the front end that wrote it stamps itself.
    ///
    /// A transcript is read after the fact, usually because something in it went wrong, and the
    /// first question is whether the code that produced it is the code in front of you. Without
    /// this that has to be inferred from the transcript's own symptoms.
    ///
    /// `None` for a record written before this was kept.
    #[serde(default)]
    pub build: Option<String>,
    /// Which front end wrote this record: the word [`Front::recorded`] gives for it.
    ///
    /// Beside the build rather than folded into it, because the two go stale independently: the
    /// same build ships both surfaces, and the same surface is shipped by every build. A reader
    /// asking why a transcript looks the way it does needs both answers, and a resume in the
    /// other surface is as much a caveat on what is above it as a resume on other code is.
    ///
    /// `None` for a record written before this was kept, which is not read as either surface.
    #[serde(default)]
    pub front: Option<String>,
    /// The conversation, which is what resuming restores.
    pub conversation: Snapshot,
    /// Explicit display history; absent in older records.
    #[serde(default)]
    pub history: Option<Vec<StoredTurn>>,
    /// Questions asked beside the work, and their answers, oldest first.
    ///
    /// Beside the conversation and never in it. A resume puts these back into the view a person
    /// opens with Ctrl-L, and there is no path from here into a conversation: an aside the planner
    /// went on to read would be the digression the whole feature exists to keep out of its
    /// context.
    ///
    /// An answer is written only where the planner could have held it, which is what keeps
    /// SESSION-2 true of this field as much as of the conversation above it. Where it could not,
    /// the question is still recorded and the answer is not.
    ///
    /// Empty for a record written before this was kept.
    #[serde(default)]
    pub asides: Vec<StoredAside>,
    /// What a manifest run produced, where this was one.
    ///
    /// Its presence is what makes a record a manifest run, and its absence a turn session. A
    /// manifest run has no conversation to restore, so [`Record::conversation`] is empty for one
    /// and resuming is refused rather than attempted. What it has instead is this: the goal in
    /// plain words, the manifest the planner proposed, the plan that was frozen, and what each
    /// step did.
    ///
    /// Written whether the run finished or failed, because the run somebody needs to read is the
    /// one that stopped.
    #[serde(default)]
    pub manifest: Option<StoredManifest>,
    /// The turns a rewind can go back to, oldest first.
    ///
    /// Here rather than only in memory because a mistake is often noticed after closing the
    /// program and opening it again, and a resumed session with nothing to undo is a session
    /// whose whole history of what it wrote has been thrown away while the transcript describing
    /// it was kept.
    ///
    /// Empty for a record written before this was kept, and for a session that has had no turn
    /// a rewind may reach.
    #[serde(default)]
    pub rewind: Vec<StoredRewind>,
    /// Server descendants may outlive both their server and the last undo point.
    #[serde(default)]
    pub server_children_may_run: bool,
    /// The checkouts the session's delegates were given and it keeps (CHECKOUT-15).
    ///
    /// Beside the trust map, which holds the rules copied for them: a resume brings the checkouts
    /// back with their candidate paths, and a fork brings neither (CHECKOUT-16).
    ///
    /// Empty for a record written before this was kept, and for a session that kept none.
    #[serde(default)]
    pub checkouts: Vec<StoredCheckout>,
}

/// A kept checkout as it is written down.
///
/// What the driver recorded and nothing a checkout's own files say. A resume takes it as a claim
/// to check against the disk, not as an answer ([`Workspace::restore_session_checkouts`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCheckout {
    /// Its number, `c1` and on.
    pub id: String,
    pub path: String,
    /// The commit it holds, in full.
    pub commit: String,
    /// The delegate it was made for, as [`bravebot_core::delegate::DelegateId`] spells it.
    pub delegate: String,
    /// Whether the driver recorded a file effect in it or a program started in it.
    pub worked_in: bool,
    /// The names the planner typed for files it wrote there.
    #[serde(default)]
    pub written: Vec<String>,
    /// How many writes there went through a reference.
    #[serde(default)]
    pub referenced: usize,
    /// What it took on disk as its delegate ended, where that was measured.
    #[serde(default)]
    pub size: Option<StoredSize>,
}

/// [`bravebot_agent::git::checkout::Size`] as it is written down.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StoredSize {
    pub bytes: u64,
    pub whole: bool,
}

impl StoredCheckout {
    fn of(checkout: &bravebot_agent::workspace::SessionCheckout) -> Self {
        Self {
            id: checkout.id.clone(),
            path: checkout.path.display().to_string(),
            commit: checkout.commit.clone(),
            delegate: checkout.delegate.to_string(),
            worked_in: checkout.worked_in,
            written: checkout.candidates.named.iter().cloned().collect(),
            referenced: checkout.candidates.referenced,
            size: checkout.size.map(|size| StoredSize {
                bytes: size.bytes,
                whole: size.whole,
            }),
        }
    }

    /// The checkout this describes, or nothing where its delegate's number is not one.
    fn read(&self, repository: &Path) -> Option<bravebot_agent::workspace::SessionCheckout> {
        Some(bravebot_agent::workspace::SessionCheckout {
            id: self.id.clone(),
            path: PathBuf::from(&self.path),
            commit: self.commit.clone(),
            delegate: bravebot_core::delegate::DelegateId::parse(&self.delegate)?,
            worked_in: self.worked_in,
            candidates: bravebot_agent::workspace::Candidates {
                named: self.written.iter().cloned().collect(),
                referenced: self.referenced,
            },
            repository: repository.join(".git"),
            size: self.size.map(|size| bravebot_agent::git::checkout::Size {
                bytes: size.bytes,
                whole: size.whole,
            }),
        })
    }
}

/// Display-only turn boundaries in the recounted conversation. These never enter planner context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTurn {
    pub number: usize,
    /// Absent when cancellation returned the prompt to the editor.
    pub prompt: Option<String>,
    /// Inclusive start and exclusive end in `Conversation::recounted`, including its archive.
    /// Messages are kept in the conversation alone, not copied from display-only output.
    pub start: usize,
    pub end: usize,
    /// The submitted prompt's offset in this span, or absent if it never entered the context.
    pub prompt_offset: Option<usize>,
    /// The worker lost its conversation. Earlier ranges belong to the context before this reset.
    #[serde(default)]
    pub reset_context: bool,
    /// Absent when the turn ended with no recorded ending.
    pub outcome: Option<StoredOutcome>,
}

/// A recorded ending, with only the safe explanation already composed by the interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredOutcome {
    Completed,
    Failed { reason: String },
    Cancelled { reason: String },
}

impl StoredOutcome {
    /// How turn `turn` ended, in the words the interface uses for it.
    pub fn ended(
        turn: usize,
        ending: bravebot_agent::Ending,
        cut_off: Option<&bravebot_aichat::CutOff>,
    ) -> Self {
        match ending {
            bravebot_agent::Ending::Done => Self::Completed,
            bravebot_agent::Ending::Failed(diagnosis) => Self::Failed {
                reason: failure_reason(diagnosis, cut_off),
            },
            bravebot_agent::Ending::Stopped { .. } => Self::Cancelled {
                reason: t!(turn_cancelled, turn = turn),
            },
        }
    }
}

/// Compose a localized failure reason from safe fields, without raw backend error text.
///
/// `cut_off` is what a reply the output ceiling stopped was doing, whose one name is a tool the
/// request offered, spelt as it offered it.
pub fn failure_reason(
    diagnosis: bravebot_agent::Diagnosis,
    cut_off: Option<&bravebot_aichat::CutOff>,
) -> String {
    use bravebot_agent::Category;
    use bravebot_aichat::OpenCall;
    let what: std::borrow::Cow<'_, str> = match diagnosis.category {
        Category::Unauthorized => t!(failure_unauthorized).into(),
        Category::RateLimited => t!(failure_rate_limited).into(),
        Category::Unavailable => t!(failure_unavailable).into(),
        Category::Refused => t!(failure_refused).into(),
        Category::Transport => t!(failure_transport).into(),
        Category::Incomplete => t!(failure_incomplete).into(),
        Category::Undecodable => t!(failure_undecodable).into(),
        // The one category that says a number. It is this program's own configured ceiling, not
        // anything the service reported, and without it the sentence names no remedy.
        // What the reply was writing is said as well, since a reply that spent the ceiling on one
        // file's worth of argument is asked for in parts, and one that spent it thinking is not.
        Category::TooLong => match (diagnosis.ceiling, cut_off) {
            (None, _) => t!(failure_too_long).into(),
            (Some(tokens), None) => t!(failure_too_long_at, tokens = tokens).into(),
            (Some(tokens), Some(cut_off)) => match (&cut_off.call, cut_off.thought) {
                (
                    Some(OpenCall {
                        tool: Some(tool), ..
                    }),
                    _,
                ) => t!(
                    failure_too_long_in_call,
                    tokens = tokens,
                    tool = tool.as_str()
                )
                .into(),
                (Some(OpenCall { tool: None, .. }), _) => {
                    t!(failure_too_long_in_a_call, tokens = tokens).into()
                }
                (None, true) => t!(failure_too_long_thinking, tokens = tokens).into(),
                (None, false) => t!(failure_too_long_at, tokens = tokens).into(),
            },
        },
        Category::Unconfigured => t!(failure_unconfigured).into(),
        Category::Blocked => t!(failure_blocked).into(),
        Category::Workspace => t!(failure_workspace).into(),
        Category::Internal => t!(failure_internal).into(),
    };
    let mut said = what.to_string();
    if let Some(status) = diagnosis.status {
        said = t!(failure_with_status, what = said, status = status);
    }
    // Said only where there was more than one, since "after 1 attempts" is a worse sentence than
    // the silence it replaces, and one attempt is what an unremarkable failure took.
    if let Some(attempts) = diagnosis.attempts.filter(|count| *count > 1) {
        said = t!(failure_with_attempts, what = said, attempts = attempts);
    }
    t!(session_error, problem = said)
}

/// One point a rewind can go back to, as it is written down.
///
/// Its own type rather than the interface's, for the reason [`StoredAside`] is: a record on disk
/// outlives the shape of a struct in memory. The conversation is the same [`Snapshot`] the record
/// keeps for the session itself, since it is the same thing a turn earlier.
///
/// It holds the counts the point goes back to but not the cache figure [`TurnSnapshot`] carries
/// beside them, which BACKEND-31 keeps out of a record: a figure measured by the process that
/// wrote it says nothing about what this one would pay, so a rewind after a resume reports
/// nothing rather than what a session that is no longer running read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRewind {
    /// Versioned coverage used for warnings, never for checkpoint eligibility.
    #[serde(default, deserialize_with = "read_coverage")]
    pub coverage: Option<StoredCoverage>,
    /// The exchange as it stood before the turn.
    pub conversation: Snapshot,
    /// Completed turns before the turn.
    pub turns: usize,
    /// Cumulative tokens before the turn.
    pub tokens: u64,
    #[serde(default)]
    pub spend: BTreeMap<usize, u64>,
    #[serde(default)]
    pub timing: BTreeMap<usize, bravebot_agent::timing::Timing>,
    /// The trust map before the turn, written the way [`Record::trust`] is.
    ///
    /// `None` reads as a map with nothing in it rather than as a question, unlike the record's
    /// own: a point that recorded no rules is one nothing had been vouched for before, and a
    /// rewind that put back the live map instead would keep a permission the turn granted.
    #[serde(default)]
    pub trust: Option<Vec<StoredRule>>,
    /// The programs vouched for before the turn.
    #[serde(default)]
    pub programs: Vec<StoredCommand>,
    /// The session's name before the turn.
    pub title: String,
    /// Whether a record for this session was on disk before the turn.
    #[serde(default)]
    pub wrote: bool,
    /// What the turn was asked, for the line that lists this point.
    pub prompt: String,
    /// What the files the turn wrote to held before it wrote to them.
    #[serde(default)]
    pub wrote_over: Vec<StoredBackup>,
}

/// Required restorations, recorded independently of the backup payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCoverage {
    pub version: u32,
    pub paths: Vec<String>,
    /// Missing evidence differs from an explicitly empty version 2 gap list.
    #[serde(
        default,
        deserialize_with = "read_coverage_gaps",
        skip_serializing_if = "Option::is_none"
    )]
    pub gaps: Option<std::collections::BTreeSet<bravebot_agent::rewind::CoverageGap>>,
}

// Gap formats can evolve without discarding required paths or recognized causes.
fn read_coverage_gaps<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<std::collections::BTreeSet<bravebot_agent::rewind::CoverageGap>>, D::Error> {
    use bravebot_agent::rewind::CoverageGap;
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.map(|value| match value {
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(|value| serde_json::from_value(value).unwrap_or(CoverageGap::Unknown))
            .collect(),
        _ => [CoverageGap::Unknown].into(),
    }))
}

// Unknown metadata leaves undo available with an unknown-coverage warning.
fn read_coverage<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<StoredCoverage>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// What one path held before a turn wrote to it, as it is written down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBackup {
    /// The file, relative to the project where it is inside it, so a record survives the
    /// checkout being moved the way [`Record::trust`] does.
    pub path: String,
    /// What was there: [`NOTHING`] for a path the turn created, [`BYTES`] for one whose contents
    /// are here, and anything else for one this session did not keep.
    pub before: String,
    /// Capture provenance is independent of coverage. Older records do not establish it.
    #[serde(default)]
    pub captured_trust: Option<String>,
    /// Those contents, base64, where `before` says they are here.
    ///
    /// Only where the map that stood before the turn vouched for the path, which makes SESSION-2
    /// true of this field as much as of the conversation: what a file nobody vouched for held is
    /// bytes the planner was never allowed to see, and this is on disk.
    #[serde(default)]
    pub bytes: Option<String>,
}

/// The word for a path the turn created, so a rewind removes it again.
const NOTHING: &str = "nothing";
/// The word for a path whose contents are in the record.
const BYTES: &str = "bytes";
/// The word for a path whose contents this session did not keep, so a rewind says it did not go
/// back.
const NOT_KEPT: &str = "not-kept";

/// A question asked beside the work, as it is written down.
///
/// Its own type rather than the interface's, because a record on disk outlives the shape of a
/// struct in memory, and because the two hold different things: the interface holds the answer it
/// is drawing, and this holds the answer the planner could have held.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAside {
    /// What the person asked, which is their own typed line.
    pub question: String,
    /// The answer, where it was one the planner could have held.
    ///
    /// `None` where the exchange had met something untrusted, so the answer was quarantined like
    /// any other model output over such a context. The question is still here: that it was asked
    /// is worth keeping even where what came back is not.
    #[serde(default)]
    pub answer: Option<String>,
}

impl StoredRewind {
    fn coverage_gaps(&self) -> std::collections::BTreeSet<bravebot_agent::rewind::CoverageGap> {
        match &self.coverage {
            Some(coverage) if matches!(coverage.version, 1 | 2) => {
                let mut gaps = coverage
                    .gaps
                    .clone()
                    .unwrap_or_else(|| [bravebot_agent::rewind::CoverageGap::Unknown].into());
                if coverage.version == 1 {
                    gaps.insert(bravebot_agent::rewind::CoverageGap::Unknown);
                }
                gaps
            }
            _ => [bravebot_agent::rewind::CoverageGap::Unknown].into(),
        }
    }

    /// Write one down, with the paths inside the project kept relative to it.
    fn of(point: &RewindPoint, project: &Path) -> Self {
        use base64::Engine;
        use bravebot_agent::workspace::Before;

        let snapshot = &point.snapshot;
        Self {
            coverage: Some(StoredCoverage {
                version: 2,
                gaps: Some(point.coverage.gaps()),
                paths: point
                    .backups
                    .iter()
                    .map(|backup| {
                        backup
                            .path
                            .strip_prefix(project)
                            .unwrap_or(&backup.path)
                            .display()
                            .to_string()
                    })
                    .collect(),
            }),
            conversation: snapshot.conversation.clone(),
            turns: snapshot.turns,
            tokens: snapshot.tokens,
            spend: snapshot.spend.clone(),
            timing: snapshot.timing.clone(),
            trust: Some(stored_rules(&snapshot.trust)),
            programs: stored_programs(&snapshot.programs, project),
            title: snapshot.title.clone(),
            wrote: snapshot.was_wrote,
            prompt: point.prompt.clone(),
            wrote_over: point
                .backups
                .iter()
                .map(|backup| {
                    let relative = backup.path.strip_prefix(project).unwrap_or(&backup.path);
                    let path = relative.display().to_string();
                    // Both maps have to vouch for the path. The capture's own trust is asked
                    // first, since a pre-turn snapshot may predate another writer and cannot
                    // label bytes captured afterwards. The map that stood before the turn is
                    // asked as well: the bytes are what the file held before it, so that map is
                    // the one that labelled them, and a path it does not vouch for held bytes the
                    // planner was never allowed to see. What neither vouches for stays in memory
                    // for a rewind in this session and goes no further.
                    let (before, bytes) = match &backup.was {
                        Before::Nothing => (NOTHING, None),
                        Before::Bytes(held)
                            if backup.captured_trust == Integrity::Trusted
                                && vouched_for(
                                    &snapshot.trust,
                                    relative,
                                    bravebot_agent::workspace::BACKSLASH_SEPARATES,
                                ) =>
                        {
                            (
                                BYTES,
                                Some(base64::engine::general_purpose::STANDARD.encode(held)),
                            )
                        }
                        Before::Bytes(_) | Before::NotKept => (NOT_KEPT, None),
                    };
                    StoredBackup {
                        path,
                        before: before.to_string(),
                        captured_trust: Some(
                            if backup.captured_trust == Integrity::Trusted {
                                TRUSTED
                            } else {
                                UNTRUSTED
                            }
                            .into(),
                        ),
                        bytes,
                    }
                })
                .collect(),
        }
    }

    /// Read one back, for a session working in `root`.
    ///
    /// Anything this build cannot read means the path will not go back: a word it does not know,
    /// and base64 it cannot decode, both land there rather than on "the file was never here",
    /// which is the direction that would have a rewind delete work it merely could not hold.
    ///
    /// The place in the transcript is not read from here and is left at nothing. It is an index
    /// into the list one process drew, and the session reading this draws another; the turn
    /// number is the fact that survives, and the session restoring these points finds the index
    /// again from it.
    ///
    /// The cache figure is left at nothing for a reason of its own: a record keeps none, so
    /// rewinding to a point a resume brought back reports nothing about a cache, where rewinding
    /// to one this process made puts back the figure it is still holding in memory.
    fn into_point(self, root: &Path) -> RewindPoint {
        use base64::Engine;
        use bravebot_agent::workspace::{Backup, Before};

        let gaps = self.coverage_gaps();
        let mut wrote_over = self.wrote_over;
        if let Some(coverage) = self.coverage {
            for path in coverage.paths {
                if !wrote_over.iter().any(|backup| backup.path == path) {
                    wrote_over.push(StoredBackup {
                        path,
                        before: NOT_KEPT.into(),
                        captured_trust: None,
                        bytes: None,
                    });
                }
            }
        }
        let trust = restored_rules(root, self.trust.as_deref().unwrap_or_default());

        RewindPoint {
            coverage: bravebot_agent::rewind::RewindCoverage::restored(gaps),
            snapshot: TurnSnapshot {
                conversation: self.conversation,
                turns: self.turns,
                tokens: self.tokens,
                spend: self.spend,
                timing: self.timing,
                cached: None,
                cached_prompt_tokens: None,
                trust,
                programs: restored_programs(&self.programs, root),
                transcript_len: 0,
                title: self.title,
                was_wrote: self.wrote,
            },
            backups: wrote_over
                .into_iter()
                .map(|held| {
                    let was = match held.before.as_str() {
                        NOTHING => Before::Nothing,
                        BYTES => held
                            .bytes
                            .and_then(|encoded| {
                                base64::engine::general_purpose::STANDARD
                                    .decode(encoded)
                                    .ok()
                            })
                            .map_or(Before::NotKept, Before::Bytes),
                        _ => Before::NotKept,
                    };
                    Backup {
                        captured_trust: if held.captured_trust.as_deref() == Some(TRUSTED) {
                            Integrity::Trusted
                        } else {
                            Integrity::Untrusted
                        },
                        path: root.join(held.path),
                        was,
                    }
                })
                .collect(),
            prompt: self.prompt,
        }
    }
}

/// Whether `trust` vouches for `path`, asked of every spelling a rewind's path can arrive in.
///
/// A path inside the project is relative, which is the spelling a rule about it is written in. One
/// outside arrives whole, and a `/`-joined name built from its components is not always the
/// platform's own spelling, which is how one derived from an absolute name reaches it. The two
/// reach one rule where `/` is already the separator, since the leading empty segment an absolute
/// name joins with is dropped on the way to a key. On Windows they do not, where a rule written in
/// one spelling is invisible to a question asked in the other, and what such a question falls back
/// to is the rule about the directory above the file: after somebody vouches for their project,
/// that answer is "trusted". Neither is the key a rule about a file on a drive letter is held
/// under, which is spelled from `/` (TRUST-18), so that key is asked as well: without it a file in
/// a directory opened by name is decided by the answer about the project.
///
/// So the weakest of the answers is the one taken. A rule marking this path untrusted keeps its
/// bytes out of the record whichever spelling recorded it, and a path no rule covers at all is not
/// vouched for either, since nobody has said anything about it. What that costs on Windows is a
/// path somebody did vouch for under another spelling: its bytes stay out of the record, so the
/// session holding them can still put them back and a resumed one cannot, which is the direction
/// that keeps untrusted bytes out rather than the one that hands them to a planner.
///
/// The host's answer is supplied, so the Windows one can be asked for where the tests run.
fn vouched_for(trust: &TrustStore, path: &Path, backslash_separates: bool) -> bool {
    let joined = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let whole = path.to_string_lossy();
    trust.is_trusted(&joined)
        && trust.is_trusted(&whole)
        && trust.is_trusted(&bravebot_core::spelling::to_key(
            &whole,
            backslash_separates,
        ))
}

/// A fork carries no checkout and none of the rules copied for them, so no two records list one
/// directory (CHECKOUT-16). A rule that distrusts a path stays, as it does when a checkout is
/// removed.
fn drop_checkouts(record: &mut Record) {
    let kept = std::mem::take(&mut record.checkouts);
    let Some(rules) = record.trust.as_ref().filter(|_| !kept.is_empty()) else {
        return;
    };
    let mut trust = restored_rules(Path::new(&record.directory), rules);
    for checkout in &kept {
        trust.withdraw_beneath(&checkout.path);
    }
    record.trust = Some(stored_rules(&trust));
}

/// Load weaker decisions last so equivalent path spellings cannot hide them.
fn restored_rules(root: &Path, rules: &[StoredRule]) -> TrustStore {
    let mut trust = bravebot_agent::workspace::trust_store(root);
    for rule in rules.iter().filter(|rule| rule.integrity == TRUSTED) {
        trust.trust(&replayed(&rule.path));
    }
    for rule in rules.iter().filter(|rule| rule.integrity == "undecided") {
        trust.undecide(&replayed(&rule.path));
    }
    for rule in rules
        .iter()
        .filter(|rule| rule.integrity != TRUSTED && rule.integrity != "undecided")
    {
        trust.distrust(&replayed(&rule.path));
    }
    trust
}

/// A trust map as it is written down.
fn stored_rules(trust: &TrustStore) -> Vec<StoredRule> {
    trust
        .rules()
        .map(|(path, integrity)| StoredRule {
            path: path.to_string(),
            integrity: match integrity {
                Some(Integrity::Trusted) => TRUSTED,
                Some(Integrity::Untrusted) => UNTRUSTED,
                None => "undecided",
            }
            .to_string(),
        })
        .collect()
}

/// A list of vouched-for commands as it is written down, with a tree inside the project kept
/// relative to it the way a trust rule's path and a rewind's are.
fn stored_programs(programs: &TrustedPrograms, project: &Path) -> Vec<StoredCommand> {
    programs
        .iter()
        .map(|c| StoredCommand {
            program: StoredPath::of(&c.program),
            started_as: (c.started_as != c.program).then(|| StoredPath::of(&c.started_as)),
            args: c.args.clone(),
            directory: Some(StoredPath::of(
                c.directory.strip_prefix(project).unwrap_or(&c.directory),
            )),
        })
        .collect()
}

/// A list of vouched-for commands read back, for a session working in `root`.
///
/// The one place a missing tree is filled in, so the reading that predates the field lives here
/// rather than at each caller. A tree that no longer exists is written down as it was and comes
/// back as it was: it matches no run, so every run asks, which is the direction to fail in.
///
/// A tree written down relative is joined to `root`, so it names the checkout being resumed rather
/// than the one the entry was granted in. An absolute one is read as it stands, which is both the
/// tree outside the project this build writes in full and the tree inside it that a record written
/// by the build before this one holds.
///
/// An entry whose spelling of a path names no path is dropped rather than restored: see
/// [`StoredPath`]. It is the same direction a missing list takes, which is that the run asks.
fn restored_programs(programs: &[StoredCommand], root: &Path) -> TrustedPrograms {
    TrustedPrograms::from_iter(programs.iter().filter_map(|c| {
        let directory = match &c.directory {
            None => root.to_path_buf(),
            Some(written) => match written.to_path()? {
                written if written.is_absolute() => written,
                written => root.join(written),
            },
        };
        let program = c.program.to_path()?;
        let started_as = match &c.started_as {
            Some(written) => written.to_path()?,
            None => program.clone(),
        };
        Some(
            bravebot_core::programs::Command::new(program, c.args.clone(), directory)
                .started_as(started_as),
        )
    }))
}

impl StoredAside {
    /// Write one down, keeping the answer only where the record may hold it.
    fn of(aside: &Aside) -> Self {
        Self {
            question: aside.question.clone(),
            answer: aside.kept.then(|| aside.answer.clone()).flatten(),
        }
    }

    /// Read one back, for the view rather than for any conversation.
    fn into_aside(self) -> Aside {
        Aside {
            kept: self.answer.is_some(),
            answer: self.answer,
            question: self.question,
        }
    }
}

/// A manifest run as it is written down.
///
/// The same fields as `bravebot_agent::manifest::Attempt`, flattened for the file. Kept as its own
/// type rather than serialising the agent's, because a record on disk outlives the shape of a
/// struct in memory and a `#[serde(default)]` field here is cheaper than a migration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredManifest {
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub proposed: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub steps: Vec<String>,
    /// Why the run stopped, where it did.
    #[serde(default)]
    pub failure: Option<String>,
}

impl StoredManifest {
    /// Write down what a run produced, finished or not.
    pub fn of(attempt: &bravebot_agent::manifest::Attempt, failure: Option<String>) -> Self {
        Self {
            shape: attempt.shape.clone(),
            proposed: attempt.proposed.clone(),
            plan: attempt.plan.clone(),
            steps: attempt.steps.clone(),
            failure,
        }
    }

    /// How the record reads, for `--resume` of a run that cannot be continued.
    ///
    /// The picker only has a footer. Naming the session on the command line is the way to see
    /// what it produced, so this is the same report a failed live run printed, plus why it
    /// stopped where that was kept.
    pub fn describe(&self) -> String {
        let mut out = bravebot_agent::manifest::Attempt {
            shape: self.shape.clone(),
            proposed: self.proposed.clone(),
            plan: self.plan.clone(),
            steps: self.steps.clone(),
        }
        .describe();
        if let Some(failure) = &self.failure {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(failure);
            out.push('\n');
        }
        out
    }
}

/// A finished outcome replaces cumulative progress; an interrupted run keeps the last report.
pub fn manifest_usage(
    outcome: &Result<bravebot_agent::Outcome, bravebot_agent::TurnError>,
    retained: Option<bravebot_agent::Spent>,
) -> Option<bravebot_agent::Spent> {
    outcome
        .as_ref()
        .ok()
        .map(|done| bravebot_agent::Spent {
            tokens: done.tokens,
            output_tokens: done.output_tokens,
            context_tokens: done.context_tokens,
            cached: done.cached,
            timing: done.timing,
        })
        .or(retained)
}

/// Write a manifest run into the session store, finished or not, and say what it is called.
///
/// One function for both callers, because a run started from a session and a run started from the
/// command line are the same run and have to be written down the same way: a reader opening one
/// with `--resume` should not be able to tell which of the two started it.
///
/// `None` where there is nothing worth writing: a run the person cancelled, or a failure that
/// produced no attempt to look at. Every other outcome is written, because the run somebody needs
/// to read is the one that stopped. The id it returns is how a session names the run it started.
///
/// Best-effort, like everything else under `~/.bravebot`: a run that cannot be written down still
/// ran, and failing the command because the record did not save would be the wrong trade. The id
/// comes back regardless, since there is no reading of the disk here to tell.
pub fn record_manifest_run(
    project: &Path,
    prompt: &str,
    outcome: &Result<bravebot_agent::Outcome, bravebot_agent::TurnError>,
    retained: Option<bravebot_agent::Spent>,
    front: Front,
    build: &str,
) -> Option<String> {
    let (stored, trust) = match outcome {
        Ok(finished) => (
            finished
                .attempt
                .as_ref()
                .map(|attempt| StoredManifest::of(attempt, None)),
            finished.trust.clone(),
        ),
        Err(bravebot_agent::TurnError::Manifest { attempt, cause }) => (
            Some(StoredManifest::of(attempt, Some(cause.to_string()))),
            TrustStore::new(bravebot_agent::workspace::key_of(project)),
        ),
        // Cancelled, or a failure with nothing to show. Nothing worth a record.
        Err(_) => (
            None,
            TrustStore::new(bravebot_agent::workspace::key_of(project)),
        ),
    };

    let stored = stored?;

    let conversation = bravebot_agent::Conversation::new();
    let snapshot = conversation.snapshot();
    let todos = BTreeMap::new();
    let programs = TrustedPrograms::new();
    let usage = manifest_usage(outcome, retained);
    let tokens = usage.map_or(0, |s| s.tokens);
    // One turn, so the breakdown and the total say the same thing. Written anyway, because a
    // reader comparing runs should not have to special-case where the figure came from.
    let spend = BTreeMap::from([(1, tokens)]);
    // Where that one turn's time went, on the same footing. A manifest run is the case where this
    // matters most: a run nobody is watching that spent its afternoon blocked on an approval
    // nobody was there to give leaves this as the only trace of it.
    let measured = usage.map(|s| s.timing);
    let timing = measured
        .map(|timing| BTreeMap::from([(1, timing)]))
        .unwrap_or_default();
    let mut handle = Handle::begin(project, front, build);
    handle.save(
        prompt,
        Standing {
            history: None,
            // Empty, and it has to be: a manifest run has no conversation, which is the same
            // fact that makes it unresumable. Filling this with something conversation-shaped
            // would make the picker offer to continue a run that cannot be continued.
            conversation: &snapshot,
            turns: 1,
            tokens,
            spend: &spend,
            timing: &timing,
            model: outcome.as_ref().ok().map(|o| o.model.as_str()),
            todos: &todos,
            // None, and there can be none: an aside is a question a person types beside a
            // conversation, and a manifest run has neither.
            asides: &[],
            trust: &trust,
            programs: &programs,
            directories: &[],
            manifest: Some(&stored),
            // None, on the same footing as the asides: a manifest run plans its whole sequence
            // in advance and is not resumed, so there is no session for a rewind to go back in.
            rewind: &[],
            // None: a manifest run starts no delegate in a checkout of its own.
            checkouts: &[],
        },
    );
    Some(handle.id().to_string())
}

/// One trust rule as it is written down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRule {
    pub path: String,
    pub integrity: String,
}

/// A recorded rule's path, spelled the way the map is asked about that path today.
///
/// A record keeps the key a write wrote, and a record written before a key was spelled from `/`
/// keeps whatever the host separated with, a drive letter included. Replaying it verbatim leaves
/// the rule keyed under a name nothing asks about, so a file somebody recorded as untrusted comes
/// back decided by the answer given about the project, which is the direction that fails open
/// (TRUST-18).
fn replayed(path: &str) -> String {
    bravebot_core::spelling::to_key(path, bravebot_agent::workspace::BACKSLASH_SEPARATES)
        .into_owned()
}

/// The word for a trusted rule. Anything else reads as untrusted.
const TRUSTED: &str = "trusted";
const UNTRUSTED: &str = "untrusted";

/// One vouched-for command as it is written down.
///
/// The arguments are kept as a list rather than joined into a line, because they are matched
/// exactly and a rendering that had to be re-split would be a parser deciding what the user
/// vouched for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCommand {
    /// The resolved binary the vouch was given for, spelled so that two of them cannot share one
    /// entry: see [`StoredPath`].
    pub program: StoredPath,
    /// The path the program is started by, or absent where that is `program` itself.
    ///
    /// A record written before this field existed has none. The build that wrote it started every
    /// program by the file it resolved to, so absent reads as `program`. Written only where the two
    /// paths differ, so an entry started by its own file is spelled as that build spelled it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_as: Option<StoredPath>,
    #[serde(default)]
    pub args: Vec<String>,
    /// The tree the vouch was given in, relative to the project where it is inside it and absolute
    /// where it is not, or absent in a record written before entries held one.
    ///
    /// Relative because the project is what the entry was granted against, so a checkout that is
    /// moved or renamed keeps its entries. The empty string is the project root, which is what
    /// `strip_prefix` leaves of it.
    ///
    /// Absent reads as the workspace root, which is what such an entry meant when it was written:
    /// a vouch could only be spent at the root then, so restoring one as root-scoped resumes the
    /// session with exactly the grant it recorded rather than a wider one.
    #[serde(default)]
    pub directory: Option<StoredPath>,
}

/// A path as the record spells it.
///
/// A string for a path that has a text spelling, which is every path anybody types and everything
/// any record written before this field could hold; a list of bytes for a path that has none.
/// Untagged, because JSON tells a string from a list itself, so a record from an earlier build reads
/// back as the paths it always named.
///
/// [`Spelling`] is why a rendering will not do: a vouched entry is keyed on
/// the resolved binary, and `to_string_lossy` gives two binaries whose names differ only in bytes
/// that are not valid UTF-8 one spelling. It is also why a string holding a replacement character
/// names no path, which is what makes such an entry, written by an earlier build, restore as nothing
/// rather than as a vouch for whichever binary now renders that way.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StoredPath {
    Text(String),
    Bytes(Vec<u8>),
}

impl StoredPath {
    fn of(path: &Path) -> Self {
        match Spelling::of(path) {
            Spelling::Text(text) => Self::Text(text),
            Spelling::Bytes(bytes) => Self::Bytes(bytes),
        }
    }

    fn to_path(&self) -> Option<PathBuf> {
        match self {
            Self::Text(text) => Spelling::Text(text.clone()),
            Self::Bytes(bytes) => Spelling::Bytes(bytes.clone()),
        }
        .into_path()
    }
}

/// One task as it is written down.
///
/// The status is a word rather than the row's marker, because the marker is a glyph this build
/// chose and the status is what the model said. Rebuilding the row from the status means a
/// resumed list is drawn by the same code that draws a live one, so the two cannot diverge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTask {
    pub content: String,
    pub status: String,
}

impl StoredTask {
    fn of(row: &Row) -> Self {
        Self {
            content: row.content.clone(),
            status: row.status.to_string(),
        }
    }
}

impl Record {
    /// Old records carry this warning only in their checkpoints.
    pub fn server_children_may_run(&self) -> bool {
        self.server_children_may_run
            || self.rewind.iter().any(|point| {
                point
                    .coverage_gaps()
                    .contains(&bravebot_agent::rewind::CoverageGap::LanguageServer)
            })
    }

    /// The trust map this session had, or `None` if it did not record one.
    ///
    /// An integrity this build does not recognise reads as untrusted, the safe direction, as
    /// [`bravebot_agent::conversation::Snapshot`] already does for the context. A hand-edited or
    /// newer-than-this-build record therefore resumes with less trust rather than more.
    ///
    /// Untrusted rules are replayed last, so two recorded spellings of one path resume as
    /// untrusted. Distinct keys are order-independent, and a record carries no decision order to
    /// honour anyway: it is a sorted list, so leaving a collision to it would let the shorter
    /// spelling decide. A record written before every spelling of a path became one rule can hold
    /// both, and the file the session marked untrusted is the one a resume must not read back as
    /// trusted.
    ///
    /// `root` is the directory the resumed session works in, and the rules inside the project come
    /// back under it. The map holds full paths, and a record keeps the ones inside the project
    /// relative, so re-prefixing here is what lets a record survive the checkout being moved or
    /// renamed: the rules still mean the same files. A rule outside the project is recorded in
    /// full and comes back as it was written.
    pub fn trust_map(&self, root: impl AsRef<std::path::Path>) -> Option<TrustStore> {
        let rules = self.trust.as_ref()?;
        Some(restored_rules(root.as_ref(), rules))
    }

    /// The programs this session's user vouched for.
    ///
    /// An empty list where a record predates this being kept, which is the safe direction: every
    /// run asks, rather than a resumed session inheriting a permission nobody recorded.
    ///
    /// `root` is the directory the resumed session works in, and it stands for the tree of an entry
    /// written before entries held one: see [`StoredCommand::directory`].
    pub fn trusted_programs(&self, root: &Path) -> TrustedPrograms {
        restored_programs(&self.programs, root)
    }

    /// The checkouts this session kept, for the workspace of the session resuming it to take back
    /// (CHECKOUT-16). `root` is the directory it resumes in, whose `.git` made them.
    ///
    /// One whose recorded delegate number is not one is left out, and the workspace names the rest
    /// it cannot take back.
    pub fn kept_checkouts(&self, root: &Path) -> Vec<bravebot_agent::workspace::SessionCheckout> {
        self.checkouts
            .iter()
            .filter_map(|checkout| checkout.read(root))
            .collect()
    }

    /// Take back the checkouts this session kept into the workspace of the one resuming it, and
    /// say which could not be taken back (CHECKOUT-16).
    ///
    /// None in a session that keeps no state directory, which could not have made them: what the
    /// record lists is left on disk and out of the list. `handle` holds what was not taken back
    /// and writes it with the record again, so the next resume that can reach it finds it.
    pub fn restore_checkouts(&self, workspace: &Workspace, handle: &mut Handle) -> Option<String> {
        if self.checkouts.is_empty() {
            return None;
        }
        let kept = self.kept_checkouts(workspace.root());
        let mut unplaced: Vec<String> = self
            .checkouts
            .iter()
            .filter(|stored| !kept.iter().any(|one| one.id == stored.id))
            .map(|stored| stored.id.clone())
            .collect();
        match bravebot_agent::home::directory().filter(|_| !bravebot_core::incognito::engaged()) {
            Some(state) => unplaced.extend(workspace.restore_session_checkouts(&state, &kept)),
            None => unplaced.extend(kept.iter().map(|one| one.id.clone())),
        }
        handle.unplaced = self
            .checkouts
            .iter()
            .filter(|stored| unplaced.contains(&stored.id))
            .cloned()
            .collect();
        (!unplaced.is_empty()).then(|| {
            t!(
                session_checkout_not_restored,
                count = unplaced.len(),
                ids = unplaced.join(", ")
            )
        })
    }

    /// Open again the directories this session added, and say which could not be opened.
    ///
    /// The map is only half of what `/add-dir` granted, and it is the half that is no use alone:
    /// restoring the rule without the directory leaves every path under it refused for escaping
    /// the workspace, with nothing on screen to say why. So the reachability is restored beside
    /// the rule that vouches for it.
    ///
    /// A directory that has since been moved or deleted comes back as a line for the user rather
    /// than as silence, because the rule about it does come back and a rule about files nothing
    /// can open is the failure this exists to rule out.
    pub fn reopen_added_directories(&self, workspace: &mut Workspace) -> Vec<String> {
        self.directories
            .iter()
            .filter_map(|directory| match workspace.add_directory(directory) {
                Ok(_) => None,
                Err(error) => Some(t!(
                    session_reopen_failed,
                    directory = directory,
                    problem = error
                )),
            })
            .collect()
    }

    /// The turns a rewind can go back to, oldest first.
    ///
    /// `root` is the directory the resumed session works in, and the paths inside the project
    /// come back under it, as the trust map's rules do: a rewind is about the files this
    /// checkout has, not the ones the machine that wrote the record had.
    pub fn rewind_points(&self, root: impl AsRef<std::path::Path>) -> Vec<RewindPoint> {
        self.rewind
            .iter()
            .cloned()
            .map(|point| point.into_point(root.as_ref()))
            .collect()
    }

    /// The task lists this session kept, shaped for a screen.
    ///
    /// A status this build does not recognise parses as outstanding work, which is
    /// [`Status::parse`]'s own rule: an item nobody can classify is the one reading that cannot
    /// quietly hide something.
    pub fn todo_rows(&self) -> BTreeMap<usize, Vec<Row>> {
        self.todos
            .iter()
            .map(|(turn, tasks)| {
                let items = tasks
                    .iter()
                    .map(|task| Item::new(task.content.clone(), Status::parse(&task.status)))
                    .collect();
                (*turn, todo::rows(&List::new(items)))
            })
            .collect()
    }
}

/// One line of the list, without the conversation behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub branch: Option<String>,
    pub updated: u64,
    /// What the session takes up, record and audit together.
    pub bytes: u64,
    /// Whether this was a manifest run, which can be read but not continued.
    ///
    /// On the summary rather than left to the record, because the picker has to say so on the
    /// row: finding out only after selecting one is finding out too late.
    pub manifest: bool,
}

/// What a session amounts to at the moment it is written down.
///
/// A value rather than a row of arguments, because everything a resume needs restored ends up
/// here and the list was growing one parameter at a time.
#[derive(Debug, Clone, Copy)]
pub struct Standing<'a> {
    pub conversation: &'a Snapshot,
    pub history: Option<&'a [StoredTurn]>,
    pub turns: usize,
    pub tokens: u64,
    /// What each turn cost, by turn number.
    pub spend: &'a BTreeMap<usize, u64>,
    /// Where each turn's wall clock went, by turn number.
    pub timing: &'a BTreeMap<usize, bravebot_agent::timing::Timing>,
    /// The model the server reported answering with, or `None` before a turn reached one.
    pub model: Option<&'a str>,
    pub todos: &'a BTreeMap<usize, Vec<Row>>,
    /// Questions asked beside the work, oldest first.
    pub asides: &'a [Aside],
    pub trust: &'a TrustStore,
    pub programs: &'a TrustedPrograms,
    pub directories: &'a [PathBuf],
    /// What a manifest run produced. `None` for a turn session, which is every session the
    /// interactive interface writes.
    pub manifest: Option<&'a StoredManifest>,
    /// The turns a rewind can go back to, oldest first.
    pub rewind: &'a [RewindPoint],
    /// The checkouts the session keeps (CHECKOUT-15).
    pub checkouts: &'a [bravebot_agent::workspace::SessionCheckout],
}

/// A session worth picking up again, and where to pick it up.
///
/// The id alone is half an answer. `--resume` looks an id up under the working directory it is run
/// in, and `/cd` may have left the record somewhere other than where the process started, so the
/// directory travels with the id rather than being assumed by whoever prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resumable {
    pub id: String,
    /// The working directory the session ended in, which is where its record is.
    pub directory: PathBuf,
}

/// Why [`Handle::branch`] copied nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unbranched {
    /// No record exists yet, so there is nothing to copy.
    NothingWritten,
    /// Records are not written here, as in an incognito session, so a copy would not outlive it.
    Unwritable,
    /// A manifest run, which cannot be continued or forked (SESSION-10).
    Refused,
}

/// Which of the two links a session keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Issue,
    PullRequest,
}

/// A link the person wrote, once it has been read as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url(String);

impl Url {
    /// `text` as a link, or `None` where it is not one `http` or `https` URL with a host. The link
    /// is drawn in the info panel on every frame, so anything but printable ASCII refuses it: a
    /// newline or an escape could act on the terminal there, and a direction override or a
    /// zero-width character could make the row read as another link.
    pub fn read(text: &str) -> Option<Self> {
        let text = text.trim();
        if !text.chars().all(|c| c.is_ascii_graphic()) {
            return None;
        }
        let (scheme, rest) = text.split_once("://")?;
        let known = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        (known && !host.is_empty()).then(|| Self(text.to_string()))
    }
}

/// A live session, holding where to write and what has been written.
#[derive(Debug, Clone)]
pub struct Handle {
    id: String,
    project: PathBuf,
    started: u64,
    branch: Option<String>,
    title: String,
    issue: Option<String>,
    pull_request: Option<String>,
    /// Whether a record for this id is on disk yet.
    ///
    /// An id exists from the first moment, but a session that was opened and abandoned leaves
    /// nothing to pick up again, and offering to resume one is offering something that does not
    /// work.
    wrote: bool,
    /// What the program writing these records is: the version, the commit, and whether the tree had
    /// uncommitted changes.
    ///
    /// Stated once when the session opens rather than at each save, because a session is written
    /// down after every turn and the program running it does not change between two of them.
    /// Supplied rather than read here because this crate cannot see the one that knows: the front
    /// ends depend on it, not the other way round.
    build: String,
    /// Which of the two front ends is doing the writing.
    ///
    /// Stated when the session opens, like the build and for the same reason, and required of
    /// every caller rather than defaulted: a surface that forgot to say would be recorded as the
    /// other one, which is worse than a record that says nothing.
    front: Front,
    server_children_may_run: bool,
    /// Checkouts the record listed that a resume could not take back, written again with every
    /// save so a state directory that is out of reach for one session does not erase them for the
    /// next (CHECKOUT-16).
    unplaced: Vec<StoredCheckout>,
}

impl Handle {
    /// Begin a session for work in `project`, written down as recorded by `front` at `build`.
    ///
    /// Nothing is written yet: a session that is opened and abandoned should not leave a record,
    /// or the list fills with launches nobody meant.
    pub fn begin(project: &Path, front: Front, build: &str) -> Self {
        Self::begin_as(new_id(), project, front, build)
    }

    /// Begin a session under an id that was chosen before it, as a background session's is: the
    /// roster names it by the same id, so the record and the entry are found from one another.
    pub fn begin_as(id: String, project: &Path, front: Front, build: &str) -> Self {
        Self {
            id,
            project: project.to_path_buf(),
            started: now(),
            branch: branch_of(project),
            title: String::new(),
            issue: None,
            pull_request: None,
            wrote: false,
            server_children_may_run: false,
            build: build.to_string(),
            front,
            unplaced: Vec::new(),
        }
    }

    /// Continue the session a record came from, writing back to the same files.
    ///
    /// Stamped with the build and the front end now running rather than the ones in the record:
    /// what the rest of this session writes is written by this program, and what wrote the turns
    /// before it is the caveat the record it came from already carries.
    pub fn resuming(project: &Path, record: &Record, front: Front, build: &str) -> Self {
        Self {
            id: record.id.clone(),
            project: project.to_path_buf(),
            started: record.started,
            branch: branch_of(project),
            title: record.title.clone(),
            issue: record.issue.clone(),
            pull_request: record.pull_request.clone(),
            // The record it came from is the one being written back to.
            wrote: true,
            server_children_may_run: record.server_children_may_run(),
            build: build.to_string(),
            front,
            // Every checkout the record holds is carried through a save until a front end that
            // restores them says which it took back, so one that restores none (the desktop)
            // does not write them away (CHECKOUT-16).
            unplaced: record.checkouts.clone(),
        }
    }

    /// Continue this session in `project`, once the working directory has moved there.
    ///
    /// A record lives under the directory it is about, and the map it carries is written in that
    /// directory's terms: a relative rule means a path under it. Leaving the record where the
    /// session began would put the new directory's map into the old directory's list, so resuming
    /// there would inherit a yes that was given for somewhere else. Moving the record keeps the
    /// two together, which is the whole of what makes an inherited map safe to inherit.
    ///
    /// Nothing already written is moved or removed: what happened before the move happened in the
    /// old directory and is still worth resuming there. The session becomes resumable in its new
    /// home only once it is written there, so it is written here rather than at the end of the next
    /// turn: a session that moved and then slept would otherwise be findable only from the
    /// directory it has left.
    ///
    /// A session with nothing written yet writes nothing, since one that was opened and abandoned
    /// should leave no record anywhere. Its destination moves all the same: the move is not
    /// repeated, so a session that stayed where it was until something was worth writing would
    /// write it under the directory it left, and that record would carry a map spelled against the
    /// directory it is now in.
    pub fn move_to(&mut self, project: &Path, standing: Standing<'_>) {
        let written = self.wrote;
        self.project = project.to_path_buf();
        self.branch = branch_of(project);
        self.wrote = false;

        if !written && standing.turns == 0 {
            return;
        }
        let title = self.title.clone();
        self.save(&title, standing);
    }

    /// Copy this session as `--fork` does and carry on in the copy, which is what `/branch` does.
    ///
    /// The copy is read back from the record on disk, so the caller saves first where the session
    /// holds anything the record does not. A `name` becomes the copy's title in place of the
    /// marked one; an empty one keeps the mark. The original's record and trail are not touched,
    /// and what is returned is how to pick it up again (SESSION-8).
    ///
    /// Refused where nothing was written, because there is no record to copy; where the record
    /// cannot be written, which an incognito session never does; and for a manifest run, which
    /// [`fork`] refuses.
    pub fn branch_off(&mut self, name: &str) -> Result<Resumable, Unbranched> {
        let Some(original) = self.to_resume() else {
            return Err(Unbranched::NothingWritten);
        };
        let Some(directory) = self.directory() else {
            return Err(Unbranched::Unwritable);
        };
        let Some(record) = fork(&self.project, &self.id) else {
            return Err(Unbranched::Refused);
        };
        if !directory.join(format!("{}.json", record.id)).is_file() {
            return Err(Unbranched::Unwritable);
        }
        *self = Self::resuming(&self.project, &record, self.front, &self.build);
        self.rename(name);
        Ok(original)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The id to hand somebody wanting this session back, or `None` if there is nothing to hand.
    ///
    /// A session with no record on disk cannot be resumed, so its id is not worth printing: a
    /// command that would answer "no session by that name" is worse than saying nothing.
    pub fn resumable(&self) -> Option<&str> {
        self.wrote.then_some(self.id.as_str())
    }

    /// The session to hand back as this one ends: its name, and where to ask for it.
    ///
    /// Nothing where [`Handle::resumable`] has nothing, on the same reasoning.
    pub fn to_resume(&self) -> Option<Resumable> {
        self.resumable().map(|id| Resumable {
            id: id.to_string(),
            directory: self.project.clone(),
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The branch checked out where the session runs, as the resume list shows it.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Call the session something the user chose.
    ///
    /// Takes effect at once rather than at the next turn, by rewriting the record where there is
    /// one: a session renamed and then left alone should be findable under its new name, and one
    /// renamed before its first turn has no record to rewrite yet, so the name waits on the handle
    /// and `save` writes it.
    ///
    /// The name is trimmed and cut to the length a derived title gets, since it goes in the same
    /// column of the same list. An empty name is refused, which the caller reports: silently
    /// keeping the old one would look like the rename worked.
    pub fn rename(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        self.title = title_from(name);
        // The rewind points are given up with the old name (SESSION-19), so a record being
        // retitled holds points the session itself no longer has, each describing a session that
        // still had the old name. Carried over, a resume would hand them back to `/undo`, which
        // would rewind to a turn the session it resumed had already given up and rename the
        // session back on the way.
        let title = self.title.clone();
        self.rewrite(|record| {
            record.title = title;
            record.rewind.clear();
        });
        true
    }

    /// The link of this kind the person gave the session, if they gave one.
    pub fn link(&self, kind: Link) -> Option<&str> {
        match kind {
            Link::Issue => self.issue.as_deref(),
            Link::PullRequest => self.pull_request.as_deref(),
        }
    }

    /// Say which issue or pull request the session is for, or with `None` that it is for none.
    ///
    /// Takes effect at once, as [`Handle::rename`] does and for its reason. The rewind points stay:
    /// none of them holds a link, so a rewind leaves the links as they are.
    pub fn set_link(&mut self, kind: Link, url: Option<Url>) {
        let url = url.map(|url| url.0);
        match kind {
            Link::Issue => self.issue = url,
            Link::PullRequest => self.pull_request = url,
        }
        let (issue, pull_request) = (self.issue.clone(), self.pull_request.clone());
        self.rewrite(|record| {
            record.issue = issue;
            record.pull_request = pull_request;
        });
    }

    /// Amend the record on disk, if the session has one yet.
    ///
    /// Read, amended and written rather than rebuilt, because everything else in the record belongs
    /// to the turns that produced it and this knows none of it.
    fn rewrite(&self, amend: impl FnOnce(&mut Record)) {
        let Some(directory) = self.directory() else {
            return;
        };
        let path = directory.join(format!("{}.json", self.id));
        let Some(mut record) = read(&path) else {
            return;
        };
        amend(&mut record);
        record.updated = now();
        record.server_children_may_run = record.server_children_may_run();

        let Ok(body) = serde_json::to_vec_pretty(&record) else {
            return;
        };
        // Beside and renamed, as `save` does, so an interrupted rename leaves the record it had.
        let temporary = directory.join(format!("{}.tmp", self.id));
        if bravebot_agent::home::write_file(&temporary, &body).is_ok() {
            let _ = std::fs::rename(&temporary, path);
        }
    }

    /// Forks discard checkpoints, but cannot stop server descendants that may still write.
    pub fn inherit_rewind_warnings(&mut self, parent: &Self) {
        self.server_children_may_run |= parent.server_children_may_run;
    }

    /// Preserve warnings whose effects can outlive the point being consumed.
    pub fn retain_rewind_coverage(&mut self, coverage: &bravebot_agent::rewind::RewindCoverage) {
        self.server_children_may_run |= coverage
            .gaps()
            .contains(&bravebot_agent::rewind::CoverageGap::LanguageServer);
    }

    /// Write the session down as it now stands.
    ///
    /// Called after each turn rather than at the end, because the end may never come: a session
    /// that was killed, or whose machine slept and never woke, is exactly the one worth
    /// resuming.
    pub fn save(&mut self, first_prompt: &str, standing: Standing<'_>) {
        for point in standing.rewind {
            self.retain_rewind_coverage(&point.coverage);
        }
        if self.title.is_empty() {
            self.title = title_from(first_prompt);
        }

        let Some(directory) = self.directory() else {
            return;
        };

        let record = Record {
            server_children_may_run: self.server_children_may_run,
            id: self.id.clone(),
            directory: self.project.display().to_string(),
            branch: self.branch.clone(),
            issue: self.issue.clone(),
            pull_request: self.pull_request.clone(),
            title: self.title.clone(),
            started: self.started,
            updated: now(),
            turns: standing.turns,
            tokens: standing.tokens,
            model: standing.model.map(str::to_string),
            spend: standing.spend.clone(),
            timing: standing.timing.clone(),
            todos: standing
                .todos
                .iter()
                .map(|(turn, rows)| (*turn, rows.iter().map(StoredTask::of).collect()))
                .collect(),
            trust: Some(stored_rules(standing.trust)),
            programs: stored_programs(standing.programs, &self.project),
            directories: standing
                .directories
                .iter()
                .map(|d| d.display().to_string())
                .collect(),
            build: Some(self.build.clone()),
            front: Some(self.front.recorded().to_string()),
            conversation: standing.conversation.clone(),
            history: standing.history.map(<[StoredTurn]>::to_vec),
            asides: standing.asides.iter().map(StoredAside::of).collect(),
            manifest: standing.manifest.cloned(),
            rewind: standing
                .rewind
                .iter()
                .map(|point| StoredRewind::of(point, &self.project))
                .collect(),
            checkouts: standing
                .checkouts
                .iter()
                .map(StoredCheckout::of)
                .chain(
                    self.unplaced
                        .iter()
                        .filter(|held| !standing.checkouts.iter().any(|one| one.id == held.id))
                        .cloned(),
                )
                .collect(),
        };

        let Ok(body) = serde_json::to_vec_pretty(&record) else {
            return;
        };

        // Written beside and renamed, so a session killed mid-write leaves the last good record
        // rather than half of a new one.
        let temporary = directory.join(format!("{}.tmp", self.id));
        if bravebot_agent::home::write_file(&temporary, &body).is_ok()
            && std::fs::rename(&temporary, directory.join(format!("{}.json", self.id))).is_ok()
        {
            self.wrote = true;
        }
    }

    /// Append what one turn's gates decided.
    ///
    /// One JSON object per line, so the file can be grown a turn at a time and read with
    /// ordinary tools. What goes in it is gate names, labels and paths: the audit says what was
    /// allowed and why, never what the content was.
    pub fn append_audit(&self, turn: usize, events: &[crate::audit::Stamped]) {
        let Some(directory) = self.directory() else {
            return;
        };

        let mut body = String::new();
        for stamped in events {
            // The event's own time, not this moment. A turn is written down once, at the end, so
            // stamping here made every event in it share a second and left the trail unable to
            // say which came first or how long anything took.
            let line = serde_json::json!({
                "at": stamped.at,
                "turn": turn,
                "event": crate::audit::as_json(&stamped.event, stamped.from),
            });
            body.push_str(&line.to_string());
            body.push('\n');
        }

        let path = directory.join(format!("{}.audit.jsonl", self.id));
        let _ = bravebot_agent::home::append_to_file(&path)
            .and_then(|mut file| file.write_all(body.as_bytes()));
    }

    /// Drop what the turns from `from_turn` on decided.
    ///
    /// A rewound turn's gates decided about a turn that is no longer in the conversation, and a
    /// trail that still holds them describes a session nobody can read back. A line the file was
    /// not written by this program is kept rather than dropped: an unreadable trail is somebody
    /// else's to explain, and quietly deleting it would be the wrong answer to it.
    pub fn truncate_audit(&self, from_turn: usize) {
        let Some(directory) = self.directory() else {
            return;
        };
        let path = directory.join(format!("{}.audit.jsonl", self.id));
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return;
        };
        let mut kept = String::new();
        for line in contents.lines() {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(turn) = entry["turn"].as_u64()
                && turn as usize >= from_turn
            {
                continue;
            }
            kept.push_str(line);
            kept.push('\n');
        }
        let _ = bravebot_agent::home::write_file(&path, kept.as_bytes());
    }

    /// Remove what this session wrote, for a rewind that went back past its first turn.
    ///
    /// A record of a session with no turns in it is a session nobody can resume into anything,
    /// and leaving one behind would put an empty row in the list for every rewind. The handle
    /// goes back to being unwritten under the title it held before the turn, so a session nobody
    /// named is named by its next prompt, and one `/rename` named keeps that name.
    pub fn discard_unwritten(&mut self, before: &str) {
        if let Some(directory) = self.directory() {
            let _ = std::fs::remove_file(directory.join(format!("{}.json", self.id)));
            let _ = std::fs::remove_file(directory.join(format!("{}.audit.jsonl", self.id)));
        }
        self.wrote = false;
        self.title = before.to_string();
    }

    /// This session's own directory, resolved the way every writer resolves one.
    fn directory(&self) -> Option<PathBuf> {
        writable_project_directory(&self.project)
    }
}

/// Sessions for this project, newest first.
///
/// A record that will not parse is left out rather than reported: an unreadable file should cost
/// its own line in the list and nothing more.
pub fn list(project: &Path) -> Vec<Summary> {
    let Some(directory) = project_directory(project) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };

    let mut summaries: Vec<Summary> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| {
            let listed = read_listing(&entry.path())?;
            let audit = directory.join(format!("{}.audit.jsonl", listed.id));
            let bytes = size_of(&entry.path()) + size_of(&audit);
            Some(Summary {
                id: listed.id,
                title: listed.title,
                branch: listed.branch,
                updated: listed.updated,
                bytes,
                manifest: listed.manifest.is_some(),
            })
        })
        .collect();

    newest_first(&mut summaries);
    summaries
}

/// Order a list so the most recently written session comes first.
///
/// The picker offers the top entry, so the direction is the behaviour rather than a detail of
/// how the list is built. Named and separate because a reversed comparator is silent: the list
/// still renders, just with the session someone is least likely to want at the top.
fn newest_first(summaries: &mut [Summary]) {
    summaries.sort_by_key(|s| std::cmp::Reverse(s.updated));
}

/// The session to pick up when nobody named one: the most recent one worth continuing.
///
/// A manifest run is passed over rather than refused. It is a session in every other respect, but
/// it holds no conversation, so continuing it is not a thing that exists; the alternative is
/// answering a request to carry on with a complaint about a session the person never asked for.
pub fn most_recent(project: &Path) -> Option<Summary> {
    continuable(list(project))
}

/// The first entry of a newest-first list that can be carried on from.
///
/// Separate from the lookup above so the choice can be tested without a filesystem: which entry
/// of a list is taken is the behaviour, and it is silent when it is wrong.
fn continuable(sessions: Vec<Summary>) -> Option<Summary> {
    sessions.into_iter().find(|session| !session.manifest)
}

/// Read one session back, by the id the list gave.
pub fn load(project: &Path, id: &str) -> Option<Record> {
    let directory = project_directory(project)?;
    read(&directory.join(format!("{id}.json")))
}

/// Why a session was not deleted.
#[derive(Debug)]
pub enum Deletion {
    /// The id is not shaped like a session's name, so it was not looked for.
    Invalid,
    /// No record of that name in that directory's store.
    NotFound,
    /// The record or the trail beside it could not be removed.
    Failed(std::io::Error),
}

/// Whether `id` is a name a session could have been given: one path segment of letters, digits,
/// `-` and `_`, which covers the version 4 UUID of today and the time-and-process-id names of older
/// builds, and nothing that could lead out of the directory it is joined onto.
fn is_a_session_name(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Remove one session from disk: its record and the trail beside it (SESSION-30).
///
/// Keyed on the directory and the id alone. The trail goes first, so a failure part way leaves a
/// record that still opens rather than a trail with nothing to belong to. A link standing where one
/// of the two files should be is removed as a link and never followed, so what it points at is
/// untouched. Forks are copies made whole, which share no file with the session they came from.
pub fn delete(project: &Path, id: &str) -> Result<(), Deletion> {
    if !is_a_session_name(id) {
        return Err(Deletion::Invalid);
    }
    let directory = project_directory(project).ok_or(Deletion::NotFound)?;
    let record = directory.join(format!("{id}.json"));
    if std::fs::symlink_metadata(&record).is_err() {
        return Err(Deletion::NotFound);
    }
    for beside in [
        directory.join(format!("{id}.audit.jsonl")),
        directory.join(format!("{id}.tmp")),
    ] {
        match std::fs::remove_file(&beside) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Deletion::Failed(error)),
        }
    }
    std::fs::remove_file(&record).map_err(Deletion::Failed)
}

/// What a resumed session shows beneath each turn, by turn number.
///
/// Two files behind one type: the plan comes out of the record and the trail out of the audit
/// beside it. The transcript wants them together, since both hang off the same entry.
#[derive(Debug, Default)]
pub struct Recalled {
    pub history: Option<Vec<StoredTurn>>,
    pub turns: Option<usize>,
    pub trails: BTreeMap<usize, Vec<crate::audit::TrailLine>>,
    pub todos: BTreeMap<usize, Vec<Row>>,
    /// Questions asked beside the work, oldest first, for the view rather than the transcript.
    pub asides: Vec<Aside>,
}

/// Everything a resumed transcript needs beyond the conversation itself.
pub fn recall(project: &Path, record: &Record) -> Recalled {
    Recalled {
        history: record.history.clone(),
        turns: Some(record.turns),
        trails: audit_of(project, &record.id),
        todos: record.todo_rows(),
        asides: record
            .asides
            .iter()
            .cloned()
            .map(StoredAside::into_aside)
            .collect(),
    }
}

/// What each turn of a stored session left in the audit, by turn number.
///
/// The record holds the conversation and the audit holds what the gates decided, so a resumed
/// session that reads only the record shows a transcript with the trail missing under every turn
/// that happened before the resume. The data was never lost; it was simply never read back.
///
/// Empty for a session with no audit file, which is the ordinary case for one that never ran a
/// turn. A line that will not parse is skipped rather than reported: a trail is read to answer a
/// question about what happened, and one unreadable line should cost its own line and no more.
pub fn audit_of(project: &Path, id: &str) -> BTreeMap<usize, Vec<crate::audit::TrailLine>> {
    let mut trails: BTreeMap<usize, Vec<crate::audit::TrailLine>> = BTreeMap::new();
    let Some(directory) = project_directory(project) else {
        return trails;
    };
    let Ok(contents) = std::fs::read_to_string(directory.join(format!("{id}.audit.jsonl"))) else {
        return trails;
    };

    for line in contents.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(turn) = entry["turn"].as_u64() else {
            continue;
        };
        if let Some(recorded) = crate::audit::recalled(&entry["event"]) {
            trails.entry(turn as usize).or_default().push(recorded);
        }
    }
    trails
}

/// Where a project's sessions live.
///
/// The reading answer, which is what the list and the resume picker want: an incognito session can
/// still show and reopen the sessions that came before it. What it will not do is add to them.
pub fn project_directory(project: &Path) -> Option<PathBuf> {
    Some(
        crate::store::directory()?
            .join(SESSIONS)
            .join(key_for(project)),
    )
}

/// Where a project's sessions live when one may be written, made and narrowed on the way.
///
/// `None` in an incognito session, `None` on a machine with no home, and `None` when the directory
/// cannot be made, on a full or read-only home. Every writer already treated the first two as "there
/// is nowhere to record this" long before there was a mode that meant it on purpose, and the third
/// is the same answer to the same question: a directory that cannot be made is one nothing can be
/// written into. The mode is asked before the directory is resolved, not after: creating it is
/// itself a write, and an incognito session that left an empty directory behind would have recorded
/// which projects were worked on and when, which is most of what the record was for.
///
/// Narrowing an existing directory belongs here rather than at each writer, because SESSION-16
/// tightens on write and a writer that resolved the location for itself would satisfy the mode on
/// the file it wrote and leave the directory holding it listable by every other account.
fn writable_project_directory(project: &Path) -> Option<PathBuf> {
    let directory = bravebot_agent::home::writable()?
        .join(SESSIONS)
        .join(key_for(project));
    bravebot_agent::home::create_directory(&directory).ok()?;
    Some(directory)
}

/// The single path segment standing for a working directory.
///
/// One definition for every per-directory record under the state directory, which is
/// [`bravebot_agent::home::key_for`]. It is not reversible, which is why the session record holds
/// the real path as well.
pub fn key_for(project: &Path) -> String {
    bravebot_agent::home::key_for(project)
}

/// What to say about a session being picked up somewhere other than where it ran.
///
/// `None` when nothing moved, which is the ordinary case and should cost no line.
///
/// Worth saying at all because the transcript is about to be shown as though the work were still
/// in front of the user, and half of it may no longer be: a session that was editing a feature
/// branch, resumed on main, will be asked to carry on with changes that are not there. The record
/// knew and said nothing, since [`Handle::resuming`] replaces the branch with the current one.
pub fn branch_note(was: Option<&str>, now: Option<&str>) -> Option<String> {
    if was == now {
        return None;
    }
    Some(match (was, now) {
        (Some(was), Some(now)) => t!(session_branch_moved, was = was, now = now),
        (Some(was), None) => t!(session_branch_gone, was = was),
        (None, Some(now)) => t!(session_branch_new, now = now),
        (None, None) => unreachable!("equal cases returned above"),
    })
}

/// The branch checked out in `directory`, where it is a git checkout at all.
///
/// Read out of the files rather than by running git: it is one line, and this is a label on a
/// list entry. A detached head has no branch name, which is reported as none rather than as the
/// commit it happens to be on.
/// What to say when the build that recorded a session is not the one resuming it.
///
/// The same kind of caveat as [`branch_note`], and for the same reason: what the transcript above
/// describes was done by something other than what is about to carry on. Silent for a record
/// with no build written down, which is one from before this was kept and has nothing to compare.
pub fn build_note(was: Option<&str>, now: &str) -> Option<String> {
    let was = was?;
    (was != now).then(|| t!(session_build_differs, was = was, now = now))
}

/// What to say when the front end that recorded a session is not the one resuming it.
///
/// The third caveat of the same kind, beside [`branch_note`] and [`build_note`]: the transcript
/// above was drawn by the other surface, so what a person remembers seeing is not what this one
/// shows, and a detail they are reading as a symptom may be the other surface's rendering.
///
/// Silent for a record with no front end written down, which is one from before this was kept and
/// has nothing to compare. A word this build does not recognise is remarked on rather than passed
/// over: it is a surface this build has never heard of, which is at least as much worth saying as
/// the one it has.
pub fn front_note(was: Option<&str>, now: Front) -> Option<String> {
    let was = was?;
    (was != now.recorded()).then(|| {
        t!(
            session_front_differs,
            was = Front::named(was),
            now = Front::named(now.recorded())
        )
    })
}

pub fn branch_of(directory: &Path) -> Option<String> {
    let git = find_git(directory)?;
    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let reference = head.trim().strip_prefix("ref: ")?;
    let branch = reference.strip_prefix("refs/heads/")?;
    Some(branch.to_string())
}

/// The git directory for a checkout, walking up from `directory`.
fn find_git(directory: &Path) -> Option<PathBuf> {
    for candidate in directory.ancestors() {
        let git = candidate.join(".git");
        if git.is_dir() {
            return Some(git);
        }
        // A worktree or a submodule has a file pointing at the real directory.
        if git.is_file()
            && let Ok(contents) = std::fs::read_to_string(&git)
            && let Some(path) = contents.trim().strip_prefix("gitdir: ")
        {
            return Some(candidate.join(path));
        }
    }
    None
}

/// A title from the first thing the user asked.
///
/// Its first line, shortened. A prompt is what the session was about, and a session named after
/// one is findable in a way that a timestamp is not.
pub fn title_from(prompt: &str) -> String {
    const LONGEST: usize = 60;

    let line = prompt.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let line = line.trim();
    if line.is_empty() {
        return "untitled".to_string();
    }

    let mut title: String = line.chars().take(LONGEST).collect();
    if line.chars().count() > LONGEST {
        title.push('…');
    }
    title
}

/// How long ago, in the words a list uses.
///
/// The phrasing is the agent crate's, so a session last touched thirteen minutes ago and a file
/// replaced thirteen minutes ago are described the same way.
pub fn how_long_ago(then: u64) -> String {
    let seconds = now().saturating_sub(then);
    bravebot_agent::report::how_long_ago(std::time::Duration::from_secs(seconds))
}

/// A size in the units a person reads.
pub fn size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;

    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1}MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes / KB)
    } else {
        format!("{bytes:.0}B")
    }
}

fn read(path: &Path) -> Option<Record> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// The five fields a row of the picker needs, and nothing else.
///
/// Its own shape rather than [`Record`], because a record holds the conversation, what compaction
/// archived out of it, and the turns a rewind can go back to, each of which carries a copy of the
/// conversation and the bytes that turn wrote over. Every one of those would be parsed and
/// allocated to draw one line of a list, once per session in the directory, before the interface
/// has drawn anything at all. As fields nothing here names, they cost the scan over their text.
#[derive(Deserialize)]
struct Listed {
    id: String,
    title: String,
    #[serde(default)]
    branch: Option<String>,
    updated: u64,
    /// Whether the record has one, which is what makes it a manifest run. What is in it is not
    /// read: the row says only that the session cannot be continued.
    #[serde(default)]
    manifest: Option<serde::de::IgnoredAny>,
}

fn read_listing(path: &Path) -> Option<Listed> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A session's name: a version 4 UUID.
///
/// Opaque on purpose. It used to be the time and the process id, which sorted by age and read as
/// two facts about the machine that made it, neither of which is anybody's business once the name
/// is printed on a screen and pasted into a command. Nothing orders sessions by id: the list is
/// sorted on what the record says it was last written.
///
/// Random rather than counted, so two of them cannot collide however many processes are running
/// and whatever the clock does.
pub(crate) fn new_id() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    // The version and variant bits, so what comes out is a well-formed UUID rather than thirty-two
    // hex characters that merely look like one.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut id = String::with_capacity(36);
    for (at, byte) in bytes.iter().enumerate() {
        if matches!(at, 4 | 6 | 8 | 10) {
            id.push('-');
        }
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

/// Write a transcript into the working directory, at `requested_path` or a name of its own.
///
/// Confined the way a workspace write is, and for the same reason: the path is typed on a line
/// that also accepts `/export ../../.ssh/authorized_keys`. `..`, a root and a drive prefix are
/// refused lexically, and then containment is tested against the canonical path of the deepest
/// directory that exists, which is what catches a symlink pointing out of the tree.
///
/// Anything already at the path is refused rather than replaced, a dangling symlink included:
/// following one would write outside the tree past every check above, and a person naming a path
/// that is already taken meant a different path.
pub fn export(
    project: &Path,
    session_id: &str,
    requested_path: Option<&str>,
    content: &str,
) -> std::io::Result<PathBuf> {
    let relative = match requested_path.map(str::trim).filter(|s| !s.is_empty()) {
        Some(custom) => {
            let named = Path::new(custom);
            for component in named.components() {
                match component {
                    std::path::Component::ParentDir => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "path must not contain '..' components",
                        ));
                    }
                    std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "path must be relative to the project directory",
                        ));
                    }
                    std::path::Component::CurDir | std::path::Component::Normal(_) => {}
                }
            }
            named.to_path_buf()
        }
        None => PathBuf::from(format!("bravebot-export-{session_id}.md")),
    };

    let target = project.join(&relative);

    // `exists` follows a symlink and so reports nothing for one whose target is missing, which is
    // exactly the case that would escape.
    if target.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("'{}' already exists", relative.display()),
        ));
    }

    let root = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let inside = target
        .ancestors()
        .skip(1)
        .find_map(|ancestor| ancestor.canonicalize().ok())
        .is_some_and(|ancestor| ancestor.starts_with(&root));
    if !inside {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "path must stay inside the project directory",
        ));
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    bravebot_agent::home::write_file(&target, content.as_bytes())?;
    Ok(target)
}

/// Fork a session, creating a new session record that starts with the same transcript.
///
/// Inherits the title, branch, conversation and other state, but is given a new UUID
/// and starts tracking a new timestamp. Refuses to fork manifest runs (SESSION-10).
pub fn fork(project: &Path, source_id: &str) -> Option<Record> {
    let mut record = load(project, source_id)?;

    // Manifest runs cannot be continued or forked (SESSION-10).
    if record.manifest.is_some() {
        return None;
    }

    let old_id = record.id.clone();
    record.id = new_id();
    record.started = now();
    record.updated = now();
    record.title = format!("{} (fork)", record.title);
    record.server_children_may_run = record.server_children_may_run();
    record.rewind.clear();
    drop_checkouts(&mut record);

    if let Some(directory) = writable_project_directory(project) {
        let path = directory.join(format!("{}.json", record.id));

        if let Ok(body) = serde_json::to_vec_pretty(&record) {
            let _ = bravebot_agent::home::write_file(&path, &body);
        }

        // The prefix is shared, so what its gates decided is the fork's history too. A fork
        // whose trail began at the fork point would report a conversation arriving from nowhere.
        let old_audit_path = directory.join(format!("{}.audit.jsonl", old_id));
        let new_audit_path = directory.join(format!("{}.audit.jsonl", record.id));
        if let Ok(content) = std::fs::read(&old_audit_path) {
            let _ = bravebot_agent::home::write_file(&new_audit_path, &content);
        }
    }

    Some(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_profile::in_isolated_profile;

    /// "the model reached its output limit" leaves somebody guessing a budget nothing shows them.
    /// The figure is what names the setting to raise, and it is this program's own configured
    /// number rather than anything the service said, so repeating it gives nothing away.
    #[test]
    fn a_reply_stopped_at_a_ceiling_says_which_ceiling() {
        use bravebot_agent::{Category, Diagnosis};

        let vague = failure_reason(Diagnosis::of(Category::TooLong), None);
        assert!(
            !vague.contains("8192") && !vague.contains("8,192"),
            "a ceiling nobody measured was named anyway: {vague}"
        );

        // Two different ceilings, because a sentence that hard-coded one would pass with either.
        for ceiling in [8_192_u64, 64_000] {
            let said = failure_reason(Diagnosis::of(Category::TooLong).at_ceiling(ceiling), None);
            assert!(
                said.contains(&ceiling.to_string()),
                "the ceiling that stopped the reply is not in {said}"
            );
            assert!(
                said.contains(bravebot_config::env_var::OUTPUT_BUDGET),
                "the setting that raises it is not in {said}"
            );
        }
    }

    /// A reply stopped part way through a call and one stopped while it was thinking want
    /// different remedies, and the failure line is the only thing left on screen that can tell
    /// them apart. The tool is named where the request offered it, and only then.
    #[test]
    fn a_reply_stopped_at_the_ceiling_says_what_it_was_writing() {
        use bravebot_agent::{Category, Diagnosis};
        use bravebot_aichat::{CutOff, OpenCall};

        let diagnosis = Diagnosis::of(Category::TooLong).at_ceiling(64_000);
        let stopped = |call: Option<Option<&str>>, thought: bool| {
            failure_reason(
                diagnosis,
                Some(&CutOff {
                    ceiling: 64_000,
                    call: call.map(|tool| OpenCall {
                        tool: tool.map(str::to_owned),
                        arguments: 0,
                    }),
                    thought,
                }),
            )
        };

        let in_a_call = stopped(Some(Some("write_file")), false);
        assert!(in_a_call.contains("write_file"), "{in_a_call}");
        assert!(in_a_call.contains("64000"), "{in_a_call}");
        assert!(in_a_call.contains("not made"), "{in_a_call}");

        let unnamed = stopped(Some(None), false);
        assert!(unnamed.contains("a tool call"), "{unnamed}");

        let thinking = stopped(None, true);
        assert!(thinking.contains("thinking"), "{thinking}");

        assert_eq!(
            stopped(None, false),
            failure_reason(diagnosis, None),
            "a reply that wrote nothing at all is reported as it always was"
        );
    }

    /// A turn's ending is recorded in the words the interface uses for it: a stop names its own
    /// turn, and a failure is the composed reason, including what a reply cut off at the ceiling
    /// was writing.
    #[test]
    fn an_ending_is_recorded_in_the_interfaces_words() {
        use bravebot_agent::{Category, Diagnosis, Ending};
        use bravebot_aichat::{CutOff, OpenCall};

        assert!(matches!(
            StoredOutcome::ended(1, Ending::Done, None),
            StoredOutcome::Completed
        ));
        match StoredOutcome::ended(3, Ending::Stopped { attempts: None }, None) {
            StoredOutcome::Cancelled { reason } => assert_eq!(reason, t!(turn_cancelled, turn = 3)),
            other => panic!("a stop was recorded as {other:?}"),
        }
        let diagnosis = Diagnosis::of(Category::TooLong).at_ceiling(64_000);
        let cut_off = CutOff {
            ceiling: 64_000,
            call: Some(OpenCall {
                tool: Some("write_file".to_owned()),
                arguments: 0,
            }),
            thought: false,
        };
        match StoredOutcome::ended(2, Ending::Failed(diagnosis), Some(&cut_off)) {
            StoredOutcome::Failed { reason } => {
                assert_eq!(reason, failure_reason(diagnosis, Some(&cut_off)));
                assert!(reason.contains("write_file"), "{reason}");
            }
            other => panic!("a failure was recorded as {other:?}"),
        }
    }

    /// The name is printed on the way out and pasted into a command, so it has to be the shape a
    /// person recognises as an id and nothing else. It used to be the time and the process id.
    #[test]
    fn a_session_is_named_by_a_uuid() {
        let id = new_id();

        assert_eq!(id.len(), 36, "{id}");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12],
            "{id}"
        );
        assert!(
            id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()),
            "{id} is not hexadecimal"
        );
        assert!(parts[2].starts_with('4'), "not version 4: {id}");
        assert!(
            parts[3].starts_with(['8', '9', 'a', 'b']),
            "not the RFC 4122 variant: {id}"
        );
    }

    /// Two sessions must never be one session. Counted or clocked ids collided when two processes
    /// started in the same second, which is exactly what happens when somebody opens two windows.
    #[test]
    fn no_two_sessions_are_given_the_same_name() {
        let names: std::collections::BTreeSet<String> = (0..64).map(|_| new_id()).collect();
        assert_eq!(names.len(), 64, "an id came up twice");
    }

    #[test]
    fn a_working_directory_becomes_one_readable_segment() {
        let key = key_for(Path::new("/Users/someone/projects/bravebot"));
        assert!(!key.contains('/'));
        assert!(key.contains("projects"), "{key} is not recognisable");
    }

    /// Two checkouts must not share a list, which is the whole point of keying by directory.
    #[test]
    fn two_directories_do_not_share_a_key() {
        assert_ne!(key_for(Path::new("/a/one")), key_for(Path::new("/a/two")));
    }

    /// A path of nothing but separators would otherwise name the sessions directory itself and
    /// scatter records among the project directories.
    #[test]
    fn a_path_with_nothing_in_it_still_names_a_directory() {
        assert_eq!(key_for(Path::new("/")), "root");
    }

    #[test]
    fn a_title_is_the_first_line_of_the_prompt() {
        assert_eq!(
            title_from("make a space invaders game\nwith canvas"),
            "make a space invaders game"
        );
    }

    /// A pasted essay is not a title, and cutting it says so rather than pretending.
    #[test]
    fn a_long_title_is_cut_and_says_it_was() {
        let title = title_from(&"x".repeat(200));
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 61);
    }

    #[test]
    fn a_prompt_with_nothing_in_it_still_has_a_title() {
        assert_eq!(title_from("   \n\n"), "untitled");
    }

    /// The phrasing itself is tested where it lives; what matters here is that a stored time
    /// becomes an age rather than being read as one.
    #[test]
    fn a_stored_time_becomes_an_age() {
        let now = now();
        assert_eq!(how_long_ago(now), "just now");
        assert_eq!(how_long_ago(now - 13 * 60), "13 minutes ago");
    }

    /// Every session written before timing was kept has a record without it, and refusing to parse
    /// one would make an upgrade look like a lost session. It reads as no breakdown, which is not
    /// the same as a session that took no time: the turn count and the token total are still there.
    #[test]
    fn a_record_written_before_timing_was_kept_still_loads() {
        let body = serde_json::to_string(&serde_json::json!({
            "id": "1-2",
            "directory": "/tmp/x",
            "title": "a session",
            "started": 1,
            "updated": 1,
            "turns": 2,
            "tokens": 3_400,
            "conversation": {
                "messages": [],
                "context": "trusted",
                "references": 0,
                "archive": [],
                "measured": 0,
            },
        }))
        .expect("serialises");

        let record: Record = serde_json::from_str(&body).expect("an older record still parses");
        assert!(record.timing.is_empty(), "a breakdown was invented");
        assert_eq!(record.tokens, 3_400, "the figures that were kept were lost");
    }

    /// A clock that has gone backwards since the session was written must not produce an age in
    /// the future or a panic.
    #[test]
    fn a_session_from_the_future_is_not_a_crash() {
        assert_eq!(how_long_ago(now() + 10_000), "just now");
    }

    #[test]
    fn sizes_read_the_way_a_person_says_them() {
        assert_eq!(size(512), "512B");
        assert_eq!(size(182_374), "178.1KB");
        assert_eq!(size(3_774_874), "3.6MB");
    }

    /// A scratch checkout, so the test says what the code reads rather than what this machine
    /// happens to have checked out.
    fn fake_checkout(name: &str, head: &str) -> PathBuf {
        let root = crate::testutil::scratch_dir(&format!("bravebot-sessions-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".git")).expect("create scratch");
        std::fs::write(root.join(".git").join("HEAD"), head).expect("write HEAD");
        root
    }

    #[test]
    fn the_branch_is_read_out_of_the_checkout() {
        let root = fake_checkout("branch", "ref: refs/heads/main\n");
        assert_eq!(branch_of(&root), Some("main".to_string()));

        // And from a directory inside it, since that is where a session usually runs.
        let inside = root.join("crates").join("tui");
        std::fs::create_dir_all(&inside).expect("create");
        assert_eq!(branch_of(&inside), Some("main".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A record from before the list was kept vouches for nothing, which is the safe direction:
    /// every run asks, rather than a resumed session inheriting a permission nobody recorded.
    #[test]
    fn a_record_without_a_program_list_vouches_for_nothing() {
        let record = a_record();
        assert!(record.trusted_programs(Path::new("/work")).is_empty());
    }

    /// What was written down comes back, by resolved path and by tree, so a resumed session stops
    /// asking about exactly the programs its own user vouched for and exactly where they did.
    #[test]
    fn the_programs_a_session_vouched_for_come_back() {
        let mut record = a_record();
        record.programs = vec![
            StoredCommand {
                program: StoredPath::Text("/usr/bin/git".to_string()),
                started_as: None,
                args: vec!["log".to_string()],
                directory: Some(StoredPath::Text("/work".to_string())),
            },
            StoredCommand {
                program: StoredPath::Text("/bin/ls".to_string()),
                started_as: None,
                args: Vec::new(),
                directory: Some(StoredPath::Text("/work/sub".to_string())),
            },
        ];
        let vouched = record.trusted_programs(Path::new("/work"));
        assert!(vouched.contains(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/git"),
            &["log".to_string()],
            Path::new("/work")
        ));
        assert!(vouched.contains(
            Path::new("/bin/ls"),
            Path::new("/bin/ls"),
            &[],
            Path::new("/work/sub")
        ));
        assert!(
            !vouched.contains(
                Path::new("/bin/ls"),
                Path::new("/bin/ls"),
                &[],
                Path::new("/work")
            ),
            "an entry recorded in a subdirectory came back covering the workspace root"
        );
        assert!(
            !vouched.contains(
                Path::new("/usr/bin/git"),
                Path::new("/usr/bin/git"),
                &["push".to_string()],
                Path::new("/work")
            ),
            "a record vouched for a command it never named"
        );
        assert!(
            !vouched.contains(
                Path::new("/opt/homebrew/bin/git"),
                Path::new("/opt/homebrew/bin/git"),
                &["log".to_string()],
                Path::new("/work")
            ),
            "a record vouched for a binary it never named"
        );
    }

    /// A record written before an entry held a tree resumes as the grant it recorded, which could
    /// only ever be spent at the workspace root. Reading it any other way would either widen a
    /// permission nobody gave, or drop one they did.
    #[test]
    fn an_entry_recorded_without_a_tree_comes_back_scoped_to_the_root() {
        let mut record = a_record();
        record.programs = serde_json::from_value(serde_json::json!([
            {"program": "/usr/bin/git", "args": ["log"]}
        ]))
        .expect("a record from before entries held a tree");

        let vouched = record.trusted_programs(Path::new("/work"));
        assert!(vouched.contains(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/git"),
            &["log".to_string()],
            Path::new("/work")
        ));
        assert!(
            !vouched.contains(
                Path::new("/usr/bin/git"),
                Path::new("/usr/bin/git"),
                &["log".to_string()],
                Path::new("/work/sub")
            ),
            "an entry with no recorded tree came back covering one it never named"
        );
    }

    /// The tree a record holds, for a test whose subject is the tree rather than how it is spelled.
    fn written_tree(command: &StoredCommand) -> Option<&str> {
        match command.directory.as_ref()? {
            StoredPath::Text(text) => Some(text),
            StoredPath::Bytes(_) => panic!("a tree with a text spelling was written as bytes"),
        }
    }

    /// A tree inside the project is written down against the project, so what lands in the record
    /// is the tree the checkout holds rather than where this machine happens to keep the checkout.
    /// A tree outside it is written in full, since there is nothing to write it against.
    #[test]
    fn a_tree_inside_the_project_is_written_down_relative() {
        let make = |tree: &str| {
            bravebot_core::programs::Command::new(
                "/usr/bin/make",
                vec!["check".to_string()],
                tree.to_string(),
            )
        };
        let programs =
            TrustedPrograms::from_iter([make("/work"), make("/work/sub"), make("/elsewhere")]);

        let written = stored_programs(&programs, Path::new("/work"));

        let trees: Vec<Option<&str>> = written.iter().map(written_tree).collect();
        assert_eq!(
            trees,
            vec![Some("/elsewhere"), Some(""), Some("sub")],
            "a tree inside the project was not written down against it"
        );
    }

    /// A tree written down relative comes back under the directory the resumed session works in,
    /// so a checkout that was moved or renamed keeps its entries. A tree written down in full comes
    /// back as it was written, which is the tree outside the project and the record an older build
    /// wrote alike.
    #[test]
    fn a_tree_written_down_relative_comes_back_under_the_resumed_root() {
        let mut record = a_record();
        record.programs = vec![
            StoredCommand {
                program: StoredPath::Text("/usr/bin/make".to_string()),
                started_as: None,
                args: vec!["check".to_string()],
                directory: Some(StoredPath::Text("sub".to_string())),
            },
            StoredCommand {
                program: StoredPath::Text("/usr/bin/git".to_string()),
                started_as: None,
                args: vec!["log".to_string()],
                directory: Some(StoredPath::Text(String::new())),
            },
            StoredCommand {
                program: StoredPath::Text("/bin/ls".to_string()),
                started_as: None,
                args: Vec::new(),
                directory: Some(StoredPath::Text("/elsewhere".to_string())),
            },
        ];

        let vouched = record.trusted_programs(Path::new("/moved"));

        let check = ["check".to_string()];
        assert!(
            vouched.contains(
                Path::new("/usr/bin/make"),
                Path::new("/usr/bin/make"),
                &check,
                Path::new("/moved/sub")
            ),
            "a tree written down relative did not come back under the resumed root"
        );
        assert!(
            !vouched.contains(
                Path::new("/usr/bin/make"),
                Path::new("/usr/bin/make"),
                &check,
                Path::new("/work/sub")
            ),
            "a tree written down relative came back under a root nobody resumed"
        );
        assert!(
            vouched.contains(
                Path::new("/usr/bin/git"),
                Path::new("/usr/bin/git"),
                &["log".to_string()],
                Path::new("/moved")
            ),
            "the project root, which is written down as the empty string, did not come back"
        );
        assert!(
            vouched.contains(
                Path::new("/bin/ls"),
                Path::new("/bin/ls"),
                &[],
                Path::new("/elsewhere")
            ),
            "a tree written down in full did not come back as it was written"
        );
    }

    /// A path whose last byte is not valid UTF-8, so no rendering of it can show that byte and two
    /// of them render alike.
    #[cfg(unix)]
    fn unrenderable(prefix: &str, last: u8) -> std::path::PathBuf {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.push(last);
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&bytes))
    }

    /// The path a command was started by comes back with it (RUN-8). A command started by its own
    /// file is written with no such field, and a record with none reads back as started by its own
    /// file.
    #[test]
    fn the_path_a_command_was_started_by_comes_back_with_it() {
        let root = Path::new("/work");
        let interpreter = Path::new("/usr/local/bin/python3.12");
        let link = Path::new("/work/venv/bin/python");
        let version = ["-V".to_string()];
        let written = stored_programs(
            &TrustedPrograms::from_iter([
                bravebot_core::programs::Command::new(interpreter, version.to_vec(), root)
                    .started_as(link),
                bravebot_core::programs::Command::new("/usr/bin/git", vec!["log".into()], root),
            ]),
            root,
        );

        let json = serde_json::to_string(&written).expect("a record is written as JSON");
        assert_eq!(
            json.matches("started_as").count(),
            1,
            "a command started by its own file was written differently from an earlier build: {json}"
        );
        let read: Vec<StoredCommand> = serde_json::from_str(&json).expect("and read back");
        let vouched = restored_programs(&read, root);
        assert!(
            vouched.contains(interpreter, link, &version, root),
            "the command started through a link did not come back"
        );
        assert!(
            !vouched.contains(interpreter, interpreter, &version, root),
            "a record vouched for the interpreter started as itself"
        );

        let earlier: Vec<StoredCommand> =
            serde_json::from_str(r#"[{"program":"/usr/bin/git","args":["log"],"directory":""}]"#)
                .expect("a record from an earlier build reads");
        let vouched = restored_programs(&earlier, root);
        assert!(vouched.contains(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/git"),
            &["log".to_string()],
            root
        ));
    }

    /// A binary and a tree are written as bytes wherever they have no text spelling, so a resumed
    /// session vouches for the file its user vouched for and no other. `to_string_lossy` maps every
    /// byte it cannot read onto one replacement character, so a record holding a rendering came back
    /// covering every binary whose path renders that way.
    #[cfg(unix)]
    #[test]
    fn a_binary_no_rendering_can_show_comes_back_as_itself() {
        let root = Path::new("/work");
        let program = |last: u8| unrenderable("/usr/bin/make-", last);
        let tree = |last: u8| unrenderable("/work/sub-", last);
        let written = stored_programs(
            &TrustedPrograms::from_iter([bravebot_core::programs::Command::new(
                program(0xff),
                vec!["check".to_string()],
                tree(0xff),
            )]),
            root,
        );

        let json = serde_json::to_string(&written).expect("a record is written as JSON");
        let read: Vec<StoredCommand> = serde_json::from_str(&json).expect("and read back");
        let vouched = restored_programs(&read, root);

        let check = ["check".to_string()];
        assert!(
            vouched.contains(&program(0xff), &program(0xff), &check, &tree(0xff)),
            "the entry that was written down did not come back"
        );
        assert!(
            !vouched.contains(&program(0xfe), &program(0xfe), &check, &tree(0xff)),
            "a record vouched for a binary whose path only renders the same way"
        );
        assert!(
            !vouched.contains(&program(0xff), &program(0xff), &check, &tree(0xfe)),
            "a record vouched in a tree whose path only renders the same way"
        );
    }

    /// An entry naming a rendering rather than a path vouches for nothing, which is what a record
    /// written by a build that keyed on `to_string_lossy` holds. The replacement character in it is
    /// equally consistent with every byte it could have stood for, so restoring it would vouch for a
    /// file nobody was shown. That entry is dropped and not the list, because the rest of the
    /// answers are the same user's.
    #[test]
    fn an_entry_whose_recorded_binary_is_a_rendering_vouches_for_nothing() {
        let mut record = a_record();
        record.programs = vec![
            StoredCommand {
                program: StoredPath::Text("/usr/bin/make-\u{fffd}".to_string()),
                started_as: None,
                args: vec!["check".to_string()],
                directory: Some(StoredPath::Text(String::new())),
            },
            StoredCommand {
                program: StoredPath::Text("/usr/bin/git".to_string()),
                started_as: None,
                args: vec!["log".to_string()],
                directory: Some(StoredPath::Text(String::new())),
            },
        ];

        let vouched = record.trusted_programs(Path::new("/work"));
        assert!(
            !vouched.contains(
                Path::new("/usr/bin/make-\u{fffd}"),
                Path::new("/usr/bin/make-\u{fffd}"),
                &["check".to_string()],
                Path::new("/work")
            ),
            "an entry naming a rendering of a path came back vouching for one"
        );
        assert_eq!(
            vouched.len(),
            1,
            "one entry nothing can read took the rest of the list with it"
        );
        assert!(vouched.contains(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/git"),
            &["log".to_string()],
            Path::new("/work")
        ));
    }

    /// A record from before the map was kept must be asked about, not read as a map that trusts
    /// nothing. The two look the same in the end and are answered differently: nothing recorded
    /// is a question, and an empty map is an answer.
    #[test]
    fn a_record_that_predates_the_map_has_none_rather_than_an_empty_one() {
        let older = serde_json::json!({
            "id": "1-2",
            "directory": "/tmp/x",
            "title": "older",
            "started": 1,
            "updated": 1,
            "conversation": {"messages": [], "context": "trusted"},
        });
        let record: Record = serde_json::from_value(older).expect("an older record still loads");
        assert!(record.trust_map(&record.directory).is_none());
    }

    /// Whatever a record says that this build does not recognise, the answer is untrusted. A
    /// hand edit or a newer build's word lands in the safe direction, as everything else does.
    #[test]
    fn an_unrecognised_integrity_in_a_record_reads_as_untrusted() {
        for word in ["", "TRUSTED", "trusted-ish", "yes"] {
            let mut record = a_record();
            record.trust = Some(vec![StoredRule {
                path: ".".to_string(),
                integrity: word.to_string(),
            }]);
            let map = record
                .trust_map(&record.directory)
                .expect("a map was recorded");
            assert!(
                !map.is_trusted("src/main.rs"),
                "{word:?} was read as trusted"
            );
        }
    }

    /// A record keeps the name a rule was written under, not the key the map holds it by, and the
    /// two differ for every rule inside the project. So a checkout that was moved or renamed since
    /// resumes with its rules about the same files: they are read under the directory being
    /// resumed into. Recording the key instead would fail quietly, every rule naming a path that
    /// is not there any more and the session behaving as though nobody had vouched for anything.
    #[test]
    fn a_record_resumes_its_rules_under_the_directory_it_is_read_in() {
        let mut record = a_record();
        record.trust = Some(vec![
            StoredRule {
                path: String::new(),
                integrity: "trusted".to_string(),
            },
            StoredRule {
                path: "src/fetched.json".to_string(),
                integrity: "untrusted".to_string(),
            },
            StoredRule {
                path: "/Users/me/notes".to_string(),
                integrity: "trusted".to_string(),
            },
        ]);

        let map = record
            .trust_map("/tmp/moved-since")
            .expect("a map was recorded");
        assert!(
            map.is_trusted("src/main.rs"),
            "the yes given for the project was lost by the project moving"
        );
        assert!(
            !map.is_trusted("src/fetched.json"),
            "a no given inside the project was lost by the project moving"
        );
        assert!(
            map.is_trusted("/Users/me/notes/todo.md"),
            "a rule recorded in full was re-read as a path inside the project"
        );
        // And the rules are about the directory resumed into rather than the one recorded.
        assert_eq!(map.integrity_of("/tmp/x/src/main.rs"), None);
    }

    #[test]
    fn undecided_children_survive_session_and_checkpoint_storage() {
        let mut current = TrustStore::new("/work");
        current.distrust("vendor");
        current.trust("vendor/ours");
        let met = current.meet(&TrustStore::new("/work"));
        let mut record = a_record();
        record.trust = Some(stored_rules(&met));
        let encoded = serde_json::to_value(&record).unwrap();
        let record: Record = serde_json::from_value(encoded).unwrap();
        let loaded = record.trust_map("/moved").unwrap();
        assert_eq!(loaded.integrity_of("vendor/ours/file"), None);
        assert_eq!(
            loaded.integrity_of("vendor/other"),
            Some(Integrity::Untrusted)
        );
        let mut stored = a_stored_point(vec![]);
        stored.trust = Some(stored_rules(&met));
        let stored: StoredRewind =
            serde_json::from_value(serde_json::to_value(stored).unwrap()).unwrap();
        let checkpoint = stored.into_point(Path::new("/moved")).snapshot.trust;
        assert_eq!(checkpoint, loaded);
        assert_eq!(
            checkpoint.meet(&loaded).integrity_of("vendor/ours/file"),
            None
        );
    }

    /// The rule the whole map turns on has to survive being written down: a path a write marked
    /// untrusted, inside a tree the user vouched for, stays untrusted when the session resumes.
    #[test]
    fn a_distrusted_path_inside_a_trusted_tree_survives_the_record() {
        let mut record = a_record();
        record.trust = Some(vec![
            StoredRule {
                path: String::new(),
                integrity: "trusted".to_string(),
            },
            StoredRule {
                path: "src/fetched.json".to_string(),
                integrity: "untrusted".to_string(),
            },
        ]);

        let map = record
            .trust_map(&record.directory)
            .expect("a map was recorded");
        assert!(map.is_trusted("src/main.rs"));
        assert!(!map.is_trusted("src/fetched.json"));
    }

    /// A record written before every spelling of a path became one rule can hold two spellings of
    /// one file, and the two now collapse to a single key. The untrusted answer is the one the
    /// session actually gave about that file, so it has to be the one that survives; leaving the
    /// collision to the record's sort order would let `src/./x` be overwritten by `src/x` and
    /// hand the resumed turn a poisoned file as trusted content.
    #[test]
    fn two_recorded_spellings_of_one_path_resume_as_untrusted() {
        let mut record = a_record();
        record.trust = Some(vec![
            StoredRule {
                path: "src/./fetched.json".to_string(),
                integrity: "untrusted".to_string(),
            },
            StoredRule {
                path: "src/fetched.json".to_string(),
                integrity: "trusted".to_string(),
            },
        ]);

        let map = record
            .trust_map(&record.directory)
            .expect("a map was recorded");
        assert!(
            !map.is_trusted("src/fetched.json"),
            "a resume upgraded a file the session had marked untrusted"
        );
    }

    /// A rewind keeps bytes in the record only where the map vouches for the file they came out of,
    /// and the rule about a directory opened by name on a drive letter is held under a key spelled
    /// from `/` (TRUST-18). Asked only in the host's own spellings, such a file is read under the
    /// project, where the answer about the project vouches for it, and bytes the planner was never
    /// allowed to see go into a record a resumed session puts back.
    #[test]
    fn a_file_in_a_distrusted_directory_on_a_drive_letter_is_not_vouched_for() {
        let key = |name: &str| bravebot_core::spelling::to_key(name, true).into_owned();
        let mut trust = TrustStore::new(key(r"\\?\C:\work"));
        trust.trust(".");
        trust.distrust(&key(r"\\?\C:\fetched"));

        assert!(
            !vouched_for(&trust, Path::new(r"C:\fetched\page.html"), true),
            "the answer about the project vouched for a file in a directory nobody vouched for"
        );
        assert!(
            vouched_for(&trust, Path::new("src/main.rs"), true),
            "the answer about the project did not vouch for a file in it"
        );
    }

    /// A resumed session asks about writes, whatever the session that wrote the record was doing when
    /// it ended. The trust map and the vouched commands come back because the person resuming is the
    /// person who granted them; a mode is a standing answer somebody gave while watching one piece of
    /// work, and a session that quietly opened tomorrow with writes going through unasked would be
    /// acting on a decision nobody made today.
    ///
    /// Written against a record carrying a mode, which is what a hand edit or a newer build would
    /// produce: the field is ignored rather than read, so the answer is the same either way.
    #[test]
    fn a_resumed_session_asks_about_writes_whatever_the_record_says() {
        let with_a_mode = serde_json::json!({
            "id": "1-2",
            "directory": "/tmp/x",
            "title": "a session",
            "started": 1,
            "updated": 1,
            "permission_mode": "bypass",
            "conversation": {"messages": [], "context": "trusted"},
        });
        let record: Record = serde_json::from_value(with_a_mode).expect("the record loads");
        // Nothing on a record answers the question, so nothing can restore an answer to it. The
        // trust map and the programs are the two grants that do come back, and they are separate.
        assert!(record.trust_map(&record.directory).is_none());
        assert!(
            record
                .trusted_programs(Path::new(&record.directory))
                .is_empty()
        );
    }

    /// The record is what a resume reads, so a mode that a record kept would be one a resume could
    /// restore. A record that was handed one, as a newer build or a hand edit would, writes back
    /// without it, and one built fresh never had a place for it.
    #[test]
    fn a_record_does_not_keep_a_permission_mode() {
        let with_a_mode = serde_json::json!({
            "id": "1-2",
            "directory": "/tmp/x",
            "title": "a session",
            "started": 1,
            "updated": 1,
            "permission_mode": "bypass",
            "conversation": {"messages": [], "context": "trusted"},
        });
        let loaded: Record = serde_json::from_value(with_a_mode).expect("the record loads");

        for (which, record) in [("loaded", loaded), ("built", a_record())] {
            let written = serde_json::to_string(&record).expect("the record is written");
            assert!(
                !written.contains("permission_mode") && !written.contains("bypass"),
                "the {which} record kept a mode: {written}"
            );
        }
    }

    /// The picker offers the top entry, so a reversed comparator would silently hand someone
    /// the session they last touched a month ago. Nothing else in the suite pins the direction.
    #[test]
    fn a_list_puts_the_most_recently_written_session_first() {
        let mut summaries = vec![at(10), at(30), at(20)];
        newest_first(&mut summaries);

        assert_eq!(
            summaries.iter().map(|s| s.updated).collect::<Vec<_>>(),
            vec![30, 20, 10]
        );
    }

    /// `--continue` names no session, so the list it is handed decides which one somebody gets.
    /// Taking any entry but the first would hand them work they finished last week, and the
    /// mistake is invisible until they read the transcript they were given.
    #[test]
    fn continuing_takes_the_most_recent_session() {
        let taken = continuable(vec![at(30), at(20), at(10)]).expect("a session to continue");

        assert_eq!(taken.updated, 30);
    }

    /// A manifest run has no conversation behind it, so there is nothing to carry on from. It is
    /// passed over rather than refused: a run planned in one directory would otherwise block
    /// every later `--continue` there, and the session underneath it is the one being asked for.
    #[test]
    fn continuing_passes_over_a_manifest_run() {
        let mut planned = at(30);
        planned.manifest = true;

        let taken = continuable(vec![planned, at(20)]).expect("a session to continue");

        assert_eq!(
            taken.updated, 20,
            "a manifest run was offered as continuable"
        );
    }

    /// Both cases where there is nothing to continue: nothing has run here, and nothing that ran
    /// here can be continued. Neither is an older session to fall back to.
    #[test]
    fn a_list_with_nothing_continuable_in_it_offers_nothing() {
        assert_eq!(continuable(Vec::new()), None);

        let mut planned = at(30);
        planned.manifest = true;
        assert_eq!(continuable(vec![planned]), None);
    }

    /// One row of a list, distinguished by when it was written.
    fn at(updated: u64) -> Summary {
        Summary {
            id: format!("s-{updated}"),
            title: "a session".to_string(),
            branch: None,
            updated,
            bytes: 0,
            manifest: false,
        }
    }

    /// A transcript is read after the fact, and the first question about a strange one is whether
    /// the code that produced it is the code in front of you. Inferring that from the transcript's
    /// own symptoms is guesswork at the moment guesswork is worth least.
    ///
    /// The stamp is the caller's, since this crate cannot see the program that knows what it was
    /// built from. That makes losing it a possibility rather than an impossibility, so the record is
    /// read back here rather than the stamp being trusted to arrive.
    ///
    /// A resumed session is written by the program resuming it, whatever wrote the turns already in
    /// the record. Carrying the recorded stamp forward instead is the tempting thing, because
    /// everything else about a resumed handle does come from the record, and it would leave the
    /// caveat below permanently unable to fire.
    #[test]
    fn a_record_says_which_build_wrote_it() {
        const LATER: &str = "0.0.0-test+1111111";

        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-session-build-stamp");

        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut handle);

        let record = load(&root, handle.id()).expect("the record was not written");
        assert_eq!(
            record.build.as_deref(),
            Some(A_BUILD),
            "the record does not say which build wrote it"
        );

        let mut resumed = Handle::resuming(&root, &record, Front::Terminal, LATER);
        save_a_turn_session(&mut resumed);
        let again = load(&root, resumed.id()).expect("the record was not written back");
        assert_eq!(
            again.build.as_deref(),
            Some(LATER),
            "the record names the build that wrote the turns before the resume"
        );
    }

    /// A link is set between turns as often as during one, and a session left alone after it
    /// should resume with it, so it is written at once rather than at the next turn.
    #[test]
    fn a_link_is_written_at_once_and_a_resume_and_a_fork_keep_it() {
        const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
        const PULL: &str = "https://github.com/brave/bravebot/pull/1270";

        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-session-links");

        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut handle);
        handle.set_link(Link::Issue, Url::read(ISSUE));
        handle.set_link(Link::PullRequest, Url::read(PULL));

        let record = load(&root, handle.id()).expect("the record was not written");
        assert_eq!(record.issue.as_deref(), Some(ISSUE));
        assert_eq!(record.pull_request.as_deref(), Some(PULL));

        let resumed = Handle::resuming(&root, &record, Front::Terminal, A_BUILD);
        assert_eq!(resumed.link(Link::Issue), Some(ISSUE));
        assert_eq!(resumed.link(Link::PullRequest), Some(PULL));

        let forked = fork(&root, handle.id()).expect("the session forks");
        assert_eq!(forked.issue.as_deref(), Some(ISSUE));
        assert_eq!(forked.pull_request.as_deref(), Some(PULL));

        save_a_turn_session(&mut handle);
        let saved = load(&root, handle.id()).expect("the record is still there");
        assert_eq!(
            saved.issue.as_deref(),
            Some(ISSUE),
            "the next turn's save dropped the issue"
        );
        assert_eq!(saved.pull_request.as_deref(), Some(PULL));

        handle.set_link(Link::Issue, None);
        let cleared = load(&root, handle.id()).expect("the record is still there");
        assert_eq!(cleared.issue, None, "clearing the issue left it on disk");
        assert_eq!(cleared.pull_request.as_deref(), Some(PULL));
    }

    /// Every record written before the links were kept has neither field, and it must still
    /// resume, with no link rather than with an error.
    #[test]
    fn a_record_from_before_the_links_reads_as_having_none() {
        let mut written = serde_json::to_value(a_record()).expect("serialises");
        let fields = written.as_object_mut().expect("an object");
        fields.remove("issue").expect("the issue is written");
        fields
            .remove("pull_request")
            .expect("the pull request is written");

        let record: Record = serde_json::from_value(written).expect("an older record reads");
        assert_eq!(record.issue, None);
        assert_eq!(record.pull_request, None);
    }

    /// The panel draws a link on every frame, so a value that could end its row, start an escape
    /// sequence or reverse what follows is refused rather than drawn, and so is anything a browser
    /// would not open.
    #[test]
    fn only_one_web_address_on_one_line_is_a_link() {
        for link in [
            "https://github.com/brave/bravebot/issues/1267",
            "http://localhost:8080/pr/1",
            "HTTPS://example.com/a",
            "  https://example.com/padded  ",
        ] {
            assert!(Url::read(link).is_some(), "{link:?} was refused");
        }
        for refused in [
            "",
            "https://",
            "github.com/brave/bravebot/issues/1267",
            "ftp://example.com/issue",
            "javascript://alert(1)",
            "file:///etc/passwd",
            "https://example.com/a\nhttps://example.com/b",
            "https://example.com/\u{1b}[2J",
            "https://example.com/\u{7}",
            "https://example.com/two words",
            "https://example.com/\u{202e}1/seussi",
            "https://example.com/\u{200b}",
            "\u{feff}https://example.com/",
            "https:///issues/1",
            "http://?q",
            "https://#a",
        ] {
            assert_eq!(Url::read(refused), None, "{refused:?} was taken as a link");
        }
    }

    /// Resuming on different code is a caveat on the transcript above it, exactly as resuming on
    /// a different branch is.
    #[test]
    fn a_session_recorded_by_another_build_says_so() {
        assert_eq!(build_note(Some("0.1.0 (aaaaaaa)"), "0.1.0 (aaaaaaa)"), None);
        let note = build_note(Some("0.1.0 (aaaaaaa)"), "0.1.0 (bbbbbbb)")
            .expect("a different build is worth saying");
        assert!(
            note.contains("aaaaaaa") && note.contains("bbbbbbb"),
            "{note}"
        );
        // Nothing recorded is nothing to compare, rather than something to remark on.
        assert_eq!(build_note(None, "0.1.0 (bbbbbbb)"), None);
    }

    /// Two programs write into one store, so a record is evidence about whichever of them wrote
    /// it, and nothing in what a session holds says which. Writing the surface on the way out is
    /// what makes the word there at all; writing the one now running rather than the one the
    /// record arrived with is what keeps it about the turns it stands beside, which is the same
    /// reason the build stamp is taken from the program resuming.
    #[test]
    fn a_record_says_which_front_end_wrote_it() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-session-front-stamp");

        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut handle);

        let record = load(&root, handle.id()).expect("the record was not written");
        assert_eq!(
            record.front.as_deref(),
            Some("terminal"),
            "the record does not say which front end wrote it"
        );

        let mut resumed = Handle::resuming(&root, &record, Front::Desktop, A_BUILD);
        save_a_turn_session(&mut resumed);
        let again = load(&root, resumed.id()).expect("the record was not written back");
        assert_eq!(
            again.front.as_deref(),
            Some("desktop"),
            "the record names the front end that wrote the turns before the resume"
        );
    }

    /// Resuming in the other surface is a caveat on the transcript above it, exactly as resuming
    /// on other code or another branch is: what a person remembers seeing was drawn by a program
    /// that is not the one about to draw the rest.
    #[test]
    fn a_session_written_in_the_other_front_end_says_so() {
        assert_eq!(front_note(Some("terminal"), Front::Terminal), None);
        assert_eq!(front_note(Some("desktop"), Front::Desktop), None);

        let note = front_note(Some("desktop"), Front::Terminal)
            .expect("the other front end is worth saying");
        assert!(
            note.contains("desktop app") && note.contains("terminal"),
            "the note names neither surface: {note}"
        );

        // Nothing recorded is nothing to compare, rather than something to remark on.
        assert_eq!(front_note(None, Front::Terminal), None);

        // A surface added after this build is still not this one, and a word it does not know is
        // shown as it was written rather than swallowed.
        let unknown = front_note(Some("hologram"), Front::Terminal)
            .expect("a front end this build has never heard of is worth saying");
        assert!(unknown.contains("hologram"), "{unknown}");
    }

    /// A point whose contents a rewind cannot produce must not read as a file that was never
    /// there: the two states differ by whether the rewind deletes the person's work.
    #[test]
    fn a_kept_file_this_build_cannot_read_will_not_go_back_rather_than_being_deleted() {
        use bravebot_agent::workspace::Before;

        let point = a_stored_point(vec![
            StoredBackup {
                captured_trust: None,
                path: "notes.md".to_string(),
                before: "some word from a later build".to_string(),
                bytes: None,
            },
            StoredBackup {
                captured_trust: None,
                path: "draft.md".to_string(),
                before: BYTES.to_string(),
                bytes: Some("not base64 at all !!".to_string()),
            },
            StoredBackup {
                captured_trust: None,
                path: "made.md".to_string(),
                before: NOTHING.to_string(),
                bytes: None,
            },
        ])
        .into_point(Path::new("/work"));

        assert_eq!(
            point.backups[0].was,
            Before::NotKept,
            "a word this build does not know was read as a file that was never there"
        );
        assert_eq!(
            point.backups[1].was,
            Before::NotKept,
            "contents that would not decode were read as a file that was never there"
        );
        assert_eq!(
            point.backups[2].was,
            Before::Nothing,
            "a file the turn created is no longer removed by a rewind"
        );
    }

    /// Missing evidence cannot mean that an old transcript can restore grants over current files.
    #[test]
    fn unknown_coverage_keeps_undo_and_warns_after_a_marker_losing_round_trip() {
        let mut record = a_record();
        record.trust = Some(vec![StoredRule {
            path: "output".into(),
            integrity: UNTRUSTED.into(),
        }]);
        record.rewind = vec![a_stored_point(vec![]), a_stored_point(vec![])];
        assert_eq!(record.rewind_points("/work").len(), 2);
        record.rewind[1].coverage.as_mut().unwrap().version = 99;
        assert_eq!(record.rewind_points("/work").len(), 2);
        assert!(!record.rewind_points("/work")[1].coverage.is_complete());
        record.rewind[1].coverage.as_mut().unwrap().version = 1;
        let mut encoded = serde_json::to_value(&record).unwrap();
        for point in encoded["rewind"].as_array_mut().unwrap() {
            point.as_object_mut().unwrap().remove("coverage");
        }
        for unknown in [
            serde_json::json!({"version": 2, "different": []}),
            serde_json::json!("future format"),
            serde_json::json!({"version": 1}),
        ] {
            let mut future = encoded.clone();
            future["rewind"][1]["coverage"] = unknown;
            let loaded: Record = serde_json::from_value(future).unwrap();
            assert_eq!(loaded.rewind_points("/work").len(), 2);
            assert!(
                loaded
                    .rewind_points("/work")
                    .iter()
                    .all(|p| !p.coverage.is_complete())
            );
            assert_eq!(loaded.title, record.title);
        }
        let loaded: Record = serde_json::from_value(encoded).unwrap();
        assert_eq!(loaded.rewind_points("/work").len(), 2);
        assert!(
            loaded
                .rewind_points("/work")
                .iter()
                .all(|p| !p.coverage.is_complete())
        );
        assert_eq!(loaded.trust.as_ref().unwrap()[0].integrity, UNTRUSTED);
        assert_eq!(loaded.title, record.title);
        let roundtrip: Record =
            serde_json::from_slice(&serde_json::to_vec(&loaded).unwrap()).unwrap();
        assert_eq!(roundtrip.rewind_points("/work").len(), 2);
        assert!(
            roundtrip
                .rewind_points("/work")
                .iter()
                .all(|p| !p.coverage.is_complete())
        );
    }

    /// Old or incomplete metadata must preserve required paths without claiming complete coverage.
    #[test]
    fn complete_coverage_requires_explicit_version_two_gaps() {
        use bravebot_agent::rewind::CoverageGap;
        use bravebot_agent::workspace::Before;
        for (coverage, expected) in [
            (
                serde_json::json!({"version": 1, "paths": ["missing.txt"]}),
                vec![CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 1, "paths": ["missing.txt"], "gaps": []}),
                vec![CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 1, "paths": ["missing.txt"], "gaps": ["language-server"]}),
                vec![CoverageGap::LanguageServer, CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"]}),
                vec![CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"], "gaps": null}),
                vec![CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"], "gaps": []}),
                vec![],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"], "gaps": ["command"]}),
                vec![CoverageGap::Command],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"], "gaps": ["future-effect"]}),
                vec![CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"], "gaps": ["language-server", "future-effect", 42]}),
                vec![CoverageGap::LanguageServer, CoverageGap::Unknown],
            ),
            (
                serde_json::json!({"version": 2, "paths": ["missing.txt"], "gaps": {}}),
                vec![CoverageGap::Unknown],
            ),
        ] {
            let mut encoded = serde_json::to_value(a_stored_point(vec![])).unwrap();
            encoded["coverage"] = coverage;
            let stored: StoredRewind = serde_json::from_value(encoded).unwrap();
            let roundtrip: StoredRewind =
                serde_json::from_slice(&serde_json::to_vec(&stored).unwrap()).unwrap();
            for point in [stored, roundtrip] {
                let point = point.into_point(Path::new("/work"));
                assert_eq!(point.backups.len(), 1);
                assert_eq!(point.backups[0].path, Path::new("/work/missing.txt"));
                assert_eq!(point.backups[0].was, Before::NotKept);
                assert_eq!(
                    point.coverage.gaps().into_iter().collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }

    /// A coverage marker proves nothing about a backup's capture label, and vice versa.
    #[test]
    fn backup_provenance_is_independent_of_coverage() {
        for marker in [true, false] {
            for (captured, expected) in [
                (Some(TRUSTED), Integrity::Trusted),
                (None, Integrity::Untrusted),
                (Some("future-label"), Integrity::Untrusted),
            ] {
                let mut stored = a_stored_point(vec![StoredBackup {
                    path: "file".into(),
                    before: BYTES.into(),
                    bytes: Some("b3JpZ2luYWw=".into()),
                    captured_trust: captured.map(str::to_string),
                }]);
                if !marker {
                    stored.coverage = None;
                }
                let encoded = serde_json::to_vec(&stored).unwrap();
                let read: StoredRewind = serde_json::from_slice(&encoded).unwrap();
                let point = read.into_point(Path::new("/work"));
                assert_eq!(point.coverage.is_complete(), marker);
                assert_eq!(point.backups[0].captured_trust, expected);
                assert_eq!(
                    point.backups[0].was,
                    bravebot_agent::workspace::Before::Bytes(b"original".to_vec())
                );
            }
        }
    }

    /// A missing payload is not evidence that a path never changed.
    #[test]
    fn required_paths_missing_from_backup_entries_are_unavailable_restorations() {
        let mut point = a_stored_point(vec![]);
        point
            .coverage
            .as_mut()
            .unwrap()
            .paths
            .push("missing.txt".into());
        let restored = point.into_point(Path::new("/work"));
        assert_eq!(restored.backups.len(), 1);
        assert_eq!(restored.backups[0].path, Path::new("/work/missing.txt"));
        assert_eq!(
            restored.backups[0].was,
            bravebot_agent::workspace::Before::NotKept
        );
    }

    /// One point as a record holds it, with the paths a test wants to read back.
    fn a_stored_point(wrote_over: Vec<StoredBackup>) -> StoredRewind {
        StoredRewind {
            coverage: Some(StoredCoverage {
                version: 2,
                gaps: Some(Default::default()),
                paths: wrote_over
                    .iter()
                    .map(|backup| backup.path.clone())
                    .collect(),
            }),
            conversation: bravebot_agent::Conversation::new().snapshot(),
            turns: 1,
            tokens: 0,
            spend: BTreeMap::new(),
            timing: BTreeMap::new(),
            trust: None,
            programs: Vec::new(),
            title: "a session".to_string(),
            wrote: true,
            prompt: "write the notes".to_string(),
            wrote_over,
        }
    }

    fn a_record() -> Record {
        Record {
            server_children_may_run: false,
            history: None,
            id: "1-2".to_string(),
            directory: "/tmp/x".to_string(),
            branch: None,
            issue: None,
            pull_request: None,
            title: "a session".to_string(),
            started: 1,
            updated: 1,
            turns: 0,
            tokens: 0,
            model: None,
            spend: BTreeMap::new(),
            timing: BTreeMap::new(),
            todos: BTreeMap::new(),
            trust: None,
            programs: Vec::new(),
            directories: Vec::new(),
            build: None,
            front: None,
            asides: Vec::new(),
            conversation: Snapshot {
                messages: Vec::new(),
                context: "trusted".to_string(),
                references: 0,
                archive: Vec::new(),
                measured: 0,
                asked_to_write: false,
                holds: "public".to_string(),
            },
            manifest: None,
            rewind: Vec::new(),
            checkouts: Vec::new(),
        }
    }

    /// The ordinary case, which must cost no line: a session picked up where it was left.
    #[test]
    fn resuming_on_the_same_branch_is_not_worth_saying() {
        assert_eq!(branch_note(Some("main"), Some("main")), None);
        assert_eq!(branch_note(None, None), None);
    }

    /// A session that was editing a feature branch, resumed on main, is about to be asked to
    /// carry on with changes that are not there. Both names go in the line, since which one is
    /// wanted is the user's decision and they need to see both to make it.
    #[test]
    fn resuming_on_another_branch_says_which_one_it_ran_on() {
        let note = branch_note(Some("feature-x"), Some("main")).expect("a note");
        assert!(note.contains("feature-x"), "{note}");
        assert!(note.contains("main"), "{note}");
    }

    /// A detached head has no name to print, and the move is still worth reporting: the branch
    /// the work was on is not the thing checked out.
    #[test]
    fn moving_on_or_off_a_branch_is_still_a_move() {
        let note = branch_note(Some("main"), None).expect("a note");
        assert!(note.contains("main"), "{note}");
        assert!(note.contains("not on a branch"), "{note}");

        let note = branch_note(None, Some("main")).expect("a note");
        assert!(note.contains("main"), "{note}");
    }

    /// A detached head is on no branch, and reporting the commit it happens to be on would put a
    /// forty-character hex string where a name goes.
    #[test]
    fn a_detached_head_has_no_branch_name() {
        let root = fake_checkout("detached", "9fceb02d0ae598e95dc970b74767f19372d61af8\n");
        assert_eq!(branch_of(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Not every directory is a checkout, and that is not a failure to report.
    #[test]
    fn a_directory_that_is_not_a_checkout_has_no_branch() {
        // The one scratch directory that cannot live under `target/`: this asserts a
        // directory is *not* inside a checkout, and `target/` is inside this one.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let root = std::env::temp_dir().join("bravebot-sessions-not-a-checkout");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create scratch");
        assert_eq!(branch_of(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exporting_a_transcript_is_confined_to_the_project_root() {
        let root = crate::testutil::scratch_dir("bravebot-export-test-confined");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");

        let exported = export(&root, "test-id", None, "# Hello").expect("export");
        assert!(exported.starts_with(&root));
        assert!(exported.exists());
        let content = std::fs::read_to_string(&exported).expect("read");
        assert_eq!(content, "# Hello");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exporting_refuses_traversal_components() {
        let root = crate::testutil::scratch_dir("bravebot-export-test-traversal");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");

        let err = export(&root, "test-id", Some("../escape.md"), "# Evil");
        assert!(err.is_err());

        #[cfg(unix)]
        {
            let err_abs = export(&root, "test-id", Some("/tmp/escape.md"), "# Evil");
            assert!(err_abs.is_err());
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exporting_refuses_to_overwrite_an_existing_file() {
        let root = crate::testutil::scratch_dir("bravebot-export-test-overwrite");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");

        let target = root.join("transcript.md");
        std::fs::write(&target, "existing").expect("write");

        let err = export(&root, "test-id", Some("transcript.md"), "# New");
        assert!(err.is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "existing");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exporting_creates_intermediate_directories() {
        let root = crate::testutil::scratch_dir("bravebot-export-test-subdirs");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");

        let exported = export(
            &root,
            "test-id",
            Some("nested/deep/transcript.md"),
            "# Content",
        )
        .expect("export nested");
        assert!(exported.exists());
        assert_eq!(std::fs::read_to_string(&exported).unwrap(), "# Content");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A transcript is the conversation, so an export at the process umask would drop a
    /// world-readable copy of everything the session read into the working directory, undoing for
    /// the copy what the record's own mode does for the original.
    #[cfg(unix)]
    #[test]
    fn an_exported_transcript_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let root = crate::testutil::scratch_dir("bravebot-export-test-mode");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");

        let exported = export(&root, "test-id", None, "# Hello").expect("export");

        let mode = std::fs::metadata(&exported)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the exported transcript is at {mode:o}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// An empty project in the profile [`in_isolated_profile`] made, which is removed with the
    /// records written about it whether or not the test passes.
    fn an_empty_project(name: &str) -> PathBuf {
        let root = crate::test_profile::project(name);
        std::fs::create_dir_all(&root).expect("create");
        root
    }

    /// A stamp shaped like the one a front end passes in: a version and the commit behind it.
    const A_BUILD: &str = "0.0.0-test+0000000";

    /// Write a plain turn session down, so a run recorded beside it has something to be beside.
    ///
    /// Its own function because [`Standing`] borrows everything it carries, so the empty maps a
    /// conversation-less fixture needs have to outlive the call rather than the expression.
    fn save_a_turn_session(handle: &mut Handle) {
        let snapshot = bravebot_agent::Conversation::new().snapshot();
        handle.save(
            "what do the specs say",
            Standing {
                history: None,
                conversation: &snapshot,
                turns: 1,
                tokens: 0,
                spend: &BTreeMap::new(),
                timing: &BTreeMap::new(),
                model: None,
                todos: &BTreeMap::new(),
                asides: &[],
                trust: &TrustStore::new("/work"),
                programs: &TrustedPrograms::default(),
                directories: &[],
                manifest: None,
                rewind: &[],
                checkouts: &[],
            },
        );
    }

    /// A failed run, with everything it produced. The success path cannot be built here, since an
    /// [`bravebot_agent::Outcome`] carries a released reply that only the agent may set, so the
    /// failure is what these tests use: it is also the run somebody most needs to read.
    fn a_failed_run() -> Result<bravebot_agent::Outcome, bravebot_agent::TurnError> {
        Err(bravebot_agent::TurnError::Manifest {
            attempt: Box::new(bravebot_agent::manifest::Attempt {
                shape: Some("read the specs, then write a summary".to_string()),
                proposed: Some("{\"steps\": []}".to_string()),
                plan: Some("1. [read] read docs/specs/manifest.md".to_string()),
                steps: vec!["1. [read] read docs/specs/manifest.md: 4kB".to_string()],
            }),
            cause: Box::new(bravebot_agent::TurnError::Precommit(
                "step 2 had nothing to write".to_string(),
            )),
        })
    }

    /// MANIFEST-11. A session that starts a run stays a conversation and the run becomes a record
    /// of its own, so the presence of a manifest in a record is still what makes it a manifest
    /// run. Written the other way round, the session's own record would be both at once and the
    /// picker would have to ask which half of it Enter was about.
    #[test]
    fn a_manifest_run_is_recorded_apart_from_the_session() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-session-manifest-run");

        let mut session = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut session);

        let run = record_manifest_run(
            &root,
            "summarise the specs",
            &a_failed_run(),
            None,
            Front::Terminal,
            A_BUILD,
        )
        .expect("the run was not written down");

        assert_ne!(run, session.id(), "the run took the session's own record");

        let listed = list(&root);
        assert_eq!(listed.len(), 2, "one of the two records is missing");
        let manifests: Vec<&Summary> = listed.iter().filter(|row| row.manifest).collect();
        assert_eq!(manifests.len(), 1, "exactly one row is a manifest run");
        assert_eq!(manifests[0].id, run);

        let written = load(&root, &run).expect("the run's record does not load");
        let stored = written.manifest.expect("the run left no manifest");
        assert_eq!(
            stored.failure.as_deref(),
            Some("step 2 had nothing to write")
        );
        assert!(
            stored.describe().contains("read docs/specs/manifest.md"),
            "the plan is not in the record: {}",
            stored.describe()
        );
    }

    /// The other half of the same clause, and the reason for splitting the records at all: a
    /// conversation with a manifest run in it must not become unresumable.
    #[test]
    fn a_session_that_started_a_run_can_still_be_resumed() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-session-manifest-resumable");

        let mut session = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut session);
        record_manifest_run(
            &root,
            "summarise the specs",
            &a_failed_run(),
            None,
            Front::Terminal,
            A_BUILD,
        )
        .expect("written");

        let record = load(&root, session.id()).expect("the session does not load");
        assert!(
            record.manifest.is_none(),
            "the session's own record reads as a manifest run"
        );
        let row = list(&root)
            .into_iter()
            .find(|row| row.id == session.id())
            .expect("the session is not in the list");
        assert!(
            !row.manifest,
            "the picker would refuse Enter on the session"
        );
    }

    /// A run the person stopped has nothing in it to read, so it leaves nothing, exactly as it
    /// does from the command line. A record for every interrupted run would fill the picker with
    /// rows whose whole content is that somebody changed their mind.
    #[test]
    fn a_cancelled_run_leaves_no_record() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-session-manifest-cancelled");

        let cancelled = Err(bravebot_agent::TurnError::Cancelled { attempts: None });
        assert!(
            record_manifest_run(
                &root,
                "summarise the specs",
                &cancelled,
                None,
                Front::Terminal,
                A_BUILD
            )
            .is_none()
        );
        assert!(list(&root).is_empty(), "a stopped run was written down");
    }

    /// Missing timing remains absent; measured zero and measured failed spend survive reload.
    #[test]
    fn failed_manifest_records_distinguish_unknown_timing_from_measured_zero() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("manifest-measured-zero");
        for retained in [
            None,
            Some(bravebot_agent::Spent::default()),
            Some(bravebot_agent::Spent {
                tokens: 46,
                timing: bravebot_agent::timing::Timing {
                    wall_ms: 101,
                    inference_ms: 37,
                    tools_ms: 19,
                    stalled_ms: 11,
                },
                ..Default::default()
            }),
        ] {
            let id = record_manifest_run(
                &root,
                "summarise",
                &a_failed_run(),
                retained,
                Front::Terminal,
                A_BUILD,
            )
            .unwrap();
            let record = load(&root, &id).unwrap();
            assert_eq!(record.tokens, retained.map_or(0, |s| s.tokens));
            assert_eq!(record.spend[&1], record.tokens);
            assert_eq!(record.timing.get(&1).copied(), retained.map(|s| s.timing));
            assert!(record.model.is_none());
        }
    }

    #[test]
    fn forking_a_manifest_session_is_refused() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-fork-manifest");

        let record = Record {
            id: "manifest-sess".to_string(),
            directory: root.display().to_string(),
            title: "manifest run".to_string(),
            turns: 1,
            tokens: 10,
            manifest: Some(StoredManifest {
                shape: None,
                proposed: None,
                plan: None,
                steps: vec!["one".to_string()],
                failure: None,
            }),
            ..a_record()
        };

        let dir = project_directory(&root).expect("dir");
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("manifest-sess.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();

        assert!(fork(&root, "manifest-sess").is_none());
    }

    /// SESSION-31: the copy has its own id and the marked title, the handle moves onto it, and the
    /// original is left byte for byte as it was, including after the copy saves a turn of its own.
    #[test]
    fn branching_moves_onto_a_marked_copy_and_leaves_the_original_untouched() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-branch-copy");
        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut handle);
        let original_id = handle.id().to_string();
        let original_path = project_directory(&root)
            .expect("dir")
            .join(format!("{original_id}.json"));
        let before = std::fs::read(&original_path).expect("the original record");

        let left = handle
            .branch_off("")
            .expect("a session with a record branches");

        assert_eq!(
            left.id, original_id,
            "the id handed back is not the original's"
        );
        assert_eq!(left.directory, root);
        assert_ne!(handle.id(), original_id, "the copy kept the original's id");
        assert_eq!(handle.title(), "what do the specs say (fork)");
        let copy = load(&root, handle.id()).expect("the copy's record");
        assert_eq!(copy.title, "what do the specs say (fork)");
        assert_eq!(copy.turns, 1, "the copy did not bring the turns along");

        save_a_turn_session(&mut handle);
        assert_eq!(
            std::fs::read(&original_path).expect("the original record"),
            before,
            "the original's record changed"
        );
    }

    /// SESSION-31: a name given to `/branch` is the copy's title, in place of the mark.
    #[test]
    fn a_named_branch_takes_the_name_as_its_title() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-branch-named");
        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut handle);

        handle.branch_off("try the other parser").expect("branches");

        assert_eq!(handle.title(), "try the other parser");
        assert_eq!(
            load(&root, handle.id()).expect("the copy").title,
            "try the other parser"
        );
    }

    /// SESSION-31: nothing is copied before the session has a record, and the handle stays where
    /// it was.
    #[test]
    fn branching_before_anything_is_written_refuses_and_stays_put() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-branch-unwritten");
        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        let id = handle.id().to_string();

        assert_eq!(handle.branch_off(""), Err(Unbranched::NothingWritten));
        assert_eq!(handle.id(), id);
    }

    /// SESSION-31: where the session directory cannot be written, as in an incognito session, the
    /// command is refused, the handle keeps its id, and no copy is written.
    #[test]
    fn branching_where_records_cannot_be_written_refuses_and_writes_no_copy() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-branch-unwritable");
        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut handle);
        let id = handle.id().to_string();
        let directory = project_directory(&root).expect("dir");
        std::fs::remove_dir_all(&directory).expect("remove the records");
        std::fs::write(&directory, b"not a directory").expect("block the directory");

        assert_eq!(handle.branch_off(""), Err(Unbranched::Unwritable));

        assert_eq!(
            handle.id(),
            id,
            "the handle moved although nothing was copied"
        );
        assert_eq!(
            std::fs::read(&directory).expect("the blocker is untouched"),
            b"not a directory",
            "a copy was written"
        );
        std::fs::remove_file(&directory).expect("unblock the directory");
    }

    /// SESSION-31: a manifest run is refused as `--fork` refuses it, and writes no copy.
    #[test]
    fn branching_a_manifest_run_is_refused() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-branch-manifest");
        let record = Record {
            id: "branch-manifest".to_string(),
            directory: root.display().to_string(),
            turns: 1,
            manifest: Some(StoredManifest {
                shape: None,
                proposed: None,
                plan: None,
                steps: vec!["one".to_string()],
                failure: None,
            }),
            ..a_record()
        };
        let dir = project_directory(&root).expect("dir");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            dir.join("branch-manifest.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        let mut handle = Handle::resuming(&root, &record, Front::Terminal, A_BUILD);

        assert_eq!(handle.branch_off(""), Err(Unbranched::Refused));
        assert_eq!(handle.id(), "branch-manifest");
        assert_eq!(list(&root).len(), 1, "a copy of a manifest run was written");
    }

    #[test]
    fn truncating_an_audit_log_removes_events_from_undone_turns() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-audit-truncate");

        let handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        let stamped = crate::audit::Stamped {
            at: 1,
            from: None,
            event: bravebot_core::event::Event::GatePassed {
                gate: "file_read",
                detail: "secret.txt".to_string(),
            },
        };

        handle.append_audit(1, std::slice::from_ref(&stamped));
        handle.append_audit(2, &[stamped]);

        let audit_before = audit_of(&root, handle.id());
        assert_eq!(audit_before.len(), 2);

        handle.truncate_audit(2);

        let audit_after = audit_of(&root, handle.id());
        assert_eq!(audit_after.len(), 1);
        assert!(audit_after.contains_key(&1));
        assert!(!audit_after.contains_key(&2));
    }

    /// A rewind past a session's only turn leaves a conversation nobody can resume into anything,
    /// so the record goes rather than standing in the list as a row with nothing behind it.
    #[test]
    fn discarding_a_record_leaves_nothing_to_resume() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-discard-unwritten");

        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        let empty = bravebot_agent::Conversation::new().snapshot();
        handle.save(
            "delete the tests",
            Standing {
                history: None,
                conversation: &empty,
                turns: 1,
                tokens: 0,
                spend: &BTreeMap::new(),
                timing: &BTreeMap::new(),
                model: None,
                todos: &BTreeMap::new(),
                asides: &[],
                trust: &TrustStore::new("/work"),
                programs: &TrustedPrograms::default(),
                directories: &[],
                manifest: None,
                rewind: &[],
                checkouts: &[],
            },
        );
        handle.append_audit(
            1,
            &[crate::audit::Stamped {
                at: 1,
                from: None,
                event: bravebot_core::event::Event::GatePassed {
                    gate: "file_read",
                    detail: "secret.txt".to_string(),
                },
            }],
        );
        assert_eq!(list(&root).len(), 1, "the record was not written");

        handle.discard_unwritten("");

        assert!(list(&root).is_empty(), "the record is still in the list");
        assert!(audit_of(&root, handle.id()).is_empty());
        assert!(handle.resumable().is_none());
        assert!(
            handle.title().is_empty(),
            "the undone turn still names the session"
        );
    }

    /// A name somebody chose outlives the turn that was rewound: it was not the turn's to give, so
    /// dropping it would make the next prompt rename a session that had already been named.
    #[test]
    fn discarding_keeps_a_name_chosen_before_the_turn() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-discard-renamed");

        let mut handle = Handle::begin(&root, Front::Terminal, A_BUILD);
        assert!(handle.rename("release audit"), "the name was refused");

        handle.discard_unwritten("release audit");

        assert_eq!(handle.title(), "release audit");
    }

    /// Two sessions in one project, with a trail each, so that a delete has a neighbour to get
    /// wrong. Returns their ids.
    fn two_sessions_with_trails(root: &Path) -> (String, String) {
        let mut ids = Vec::new();
        for prompt in ["first question", "second question"] {
            let mut handle = Handle::begin(root, Front::Terminal, A_BUILD);
            handle.save(
                prompt,
                Standing {
                    history: None,
                    conversation: &bravebot_agent::Conversation::new().snapshot(),
                    turns: 1,
                    tokens: 0,
                    spend: &BTreeMap::new(),
                    timing: &BTreeMap::new(),
                    model: None,
                    todos: &BTreeMap::new(),
                    asides: &[],
                    trust: &TrustStore::new("/work"),
                    programs: &TrustedPrograms::default(),
                    directories: &[],
                    manifest: None,
                    rewind: &[],
                    checkouts: &[],
                },
            );
            handle.append_audit(
                1,
                &[crate::audit::Stamped {
                    at: 1,
                    from: None,
                    event: bravebot_core::event::Event::GatePassed {
                        gate: "file_read",
                        detail: "notes.txt".to_string(),
                    },
                }],
            );
            ids.push(handle.id().to_string());
        }
        (ids.remove(0), ids.remove(0))
    }

    /// Deleting names one session. A delete that matched on the directory alone, or on a prefix of
    /// the id, would take its neighbour with it, and the list would look right until somebody
    /// opened the one they expected to find.
    #[test]
    fn deleting_a_session_removes_its_record_and_trail_and_only_those() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-delete-one");
        let (gone, kept) = two_sessions_with_trails(&root);
        assert_eq!(list(&root).len(), 2);

        delete(&root, &gone).expect("the session was not deleted");

        let left: Vec<String> = list(&root).into_iter().map(|s| s.id).collect();
        assert_eq!(left, vec![kept.clone()], "the wrong session was removed");
        assert!(load(&root, &gone).is_none());
        assert!(
            audit_of(&root, &gone).is_empty(),
            "the trail outlived its record"
        );
        assert!(
            !project_directory(&root)
                .unwrap()
                .join(format!("{gone}.audit.jsonl"))
                .exists(),
            "the trail file is still on disk"
        );
        assert!(load(&root, &kept).is_some());
        assert!(
            !audit_of(&root, &kept).is_empty(),
            "the other session lost its trail"
        );
    }

    /// Two projects can hold a session each, and a delete is made in one of them.
    #[test]
    fn deleting_in_one_project_leaves_another_projects_session_alone() {
        if !in_isolated_profile() {
            return;
        }
        let here = an_empty_project("bravebot-delete-here");
        let there = an_empty_project("bravebot-delete-there");
        let mut in_here = Handle::begin(&here, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut in_here);
        let mut in_there = Handle::begin(&there, Front::Terminal, A_BUILD);
        save_a_turn_session(&mut in_there);

        assert!(matches!(
            delete(&there, in_here.id()),
            Err(Deletion::NotFound)
        ));

        assert!(load(&here, in_here.id()).is_some());
        assert!(load(&there, in_there.id()).is_some());
    }

    /// A manifest run is a record like any other and is deleted like one.
    #[test]
    fn a_manifest_run_can_be_deleted() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-delete-run");
        let id = record_manifest_run(
            &root,
            "summarise the specs",
            &a_failed_run(),
            None,
            Front::Terminal,
            A_BUILD,
        )
        .expect("the run was not recorded");
        assert_eq!(list(&root).len(), 1);

        delete(&root, &id).expect("the run was not deleted");

        assert!(list(&root).is_empty());
    }

    /// A name that is not one of ours is refused before any path is built from it, and a name
    /// that is ours but names nothing is a refusal too, not a success.
    #[test]
    fn deleting_refuses_a_name_that_could_leave_the_directory_and_one_that_names_nothing() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-delete-names");
        let (first, _second) = two_sessions_with_trails(&root);
        let store = project_directory(&root).unwrap();
        std::fs::write(store.parent().unwrap().join("outside.json"), "{}").expect("write");

        for name in ["", "..", "../outside", "a/b", "a\\b", "x.json", "."] {
            assert!(
                matches!(delete(&root, name), Err(Deletion::Invalid)),
                "{name:?} was not refused as a name"
            );
        }
        assert!(store.parent().unwrap().join("outside.json").exists());
        assert!(matches!(
            delete(&root, "00000000-0000-4000-8000-000000000000"),
            Err(Deletion::NotFound)
        ));
        assert_eq!(list(&root).len(), 2);
        assert!(load(&root, &first).is_some());
    }

    /// A link where the trail should be is removed as a link. A delete that opened it or
    /// removed its target would reach a file the state directory does not own.
    #[test]
    #[cfg(unix)]
    fn deleting_does_not_follow_a_link_out_of_the_state_directory() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-delete-link");
        let (id, _other) = two_sessions_with_trails(&root);
        let store = project_directory(&root).unwrap();
        let outside = crate::testutil::scratch_dir("bravebot-delete-link-outside");
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(outside.parent().unwrap()).expect("create");
        std::fs::write(&outside, "not ours").expect("write");
        let trail = store.join(format!("{id}.audit.jsonl"));
        std::fs::remove_file(&trail).expect("remove the real trail");
        std::os::unix::fs::symlink(&outside, &trail).expect("symlink");

        delete(&root, &id).expect("the session was not deleted");

        assert_eq!(
            std::fs::read_to_string(&outside).ok().as_deref(),
            Some("not ours"),
            "the file the link pointed at was removed or changed"
        );
        assert!(
            std::fs::symlink_metadata(&trail).is_err(),
            "the link is still in the store"
        );
        assert!(load(&root, &id).is_none());
        let _ = std::fs::remove_file(&outside);
    }

    /// A fork is written whole, so deleting the session it came from leaves it openable with the
    /// history it was cut from.
    #[test]
    fn deleting_a_session_leaves_its_fork_openable_with_its_trail() {
        if !in_isolated_profile() {
            return;
        }
        let root = an_empty_project("bravebot-delete-parent-of-fork");
        let (parent, _other) = two_sessions_with_trails(&root);
        let child = fork(&root, &parent).expect("the fork was not made");

        delete(&root, &parent).expect("the parent was not deleted");

        assert!(load(&root, &parent).is_none());
        let reopened = load(&root, &child.id).expect("the fork cannot be opened");
        assert_eq!(reopened.id, child.id);
        assert!(
            !audit_of(&root, &child.id).is_empty(),
            "the fork lost the trail it was given"
        );
    }

    /// The lexical check on `..` says nothing about where a directory inside the tree actually
    /// leads, and a symlink is how a path with no `..` in it lands outside the project.
    #[test]
    #[cfg(unix)]
    fn exporting_refuses_a_path_through_a_symlinked_directory() {
        let root = crate::testutil::scratch_dir("bravebot-export-test-symlink-dir");
        let outside = crate::testutil::scratch_dir("bravebot-export-test-symlink-dir-outside");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&root).expect("create");
        std::fs::create_dir_all(&outside).expect("create outside");
        std::os::unix::fs::symlink(&outside, root.join("escape")).expect("symlink");

        let refused = export(&root, "test-id", Some("escape/transcript.md"), "# Evil");

        assert!(refused.is_err(), "a symlinked directory is not the project");
        assert!(!outside.join("transcript.md").exists());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// A symlink whose target does not exist reports nothing to `exists`, so a check written in
    /// those terms both misses that the path is taken and follows it out of the tree on the write.
    #[test]
    #[cfg(unix)]
    fn exporting_refuses_a_path_that_is_a_dangling_symlink() {
        let root = crate::testutil::scratch_dir("bravebot-export-test-symlink-file");
        let outside = crate::testutil::scratch_dir("bravebot-export-test-symlink-file-outside");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(&root).expect("create");
        std::os::unix::fs::symlink(&outside, root.join("transcript.md")).expect("symlink");

        let refused = export(&root, "test-id", Some("transcript.md"), "# Evil");

        assert!(refused.is_err(), "a path already taken is not written over");
        assert!(!outside.exists(), "nothing was written through the symlink");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }
}
