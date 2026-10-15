//! The transcript's rows and the delegate and job rows they carry.

use super::*;

/// Who produced a transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    /// The user's prompt. Trusted input.
    User,
    /// The assistant's reply. Untrusted model output, shown but never acted on.
    Assistant,
    /// A note from the program itself: an error, a refusal, a status.
    System,
    /// A failure reason shown in the transcript and included in exports.
    Failure,
    /// A deliberate stop shown in the transcript and included in exports.
    Stopped,
    /// A tool call the turn made. Shown as it happens, and kept afterwards.
    Tool,
    /// A command the user typed in shell mode. Trusted input, like their prompts.
    Shell,
    /// What such a command printed.
    ///
    /// Drawn plainly rather than styled: it is a terminal's output and the user is reading it as
    /// one, so markdown would be a misreading and a marker on every line would be noise.
    Output,
    /// A delegate the turn started, with its own work drawn underneath it.
    Delegate,
}

/// One delegate's work, drawn where the call that started it would have been.
///
/// A delegate is a second planner with a run of its own, and several are going at once, so its
/// lines are kept together rather than interleaved with the turn's. The block where the call
/// happened draws the last few of them; the rest are here for the mode that opens over it.
///
/// Nothing here reaches a model. The turn is told the report and nothing else, which is the point
/// of delegating; these are the same lines going to a screen instead.
#[derive(Debug, Clone)]
pub struct Delegate {
    /// Which one it is, as the driver numbered it.
    pub id: bravebot_agent::report::DelegateId,
    /// Which definition it is: the driver's own word where nothing was defined, and otherwise
    /// the name a vouched file gave the kind of delegate this is.
    pub kind: String,
    /// What it was asked to do, as the planner wrote it.
    pub task: String,
    /// What it has done, oldest first, back as far as is kept.
    pub lines: Vec<Entry>,
    /// How many it has done in all, which is more than are kept once it has run long enough.
    pub calls: usize,
    /// What the turn was told when it finished. `None` while it is still working, which is what
    /// tells a delegate that is running from one that answered.
    pub note: Option<String>,
    /// What it handed back, where it handed back anything. A delegate that could not finish
    /// reported nothing.
    ///
    /// Held apart from the note because the two answer different questions: the note is the
    /// driver's sentence about how the run ended, and this is the delegate's own about what it
    /// found. Which shape it takes was settled by the gate that decided what the planner got.
    pub reported: Option<Reported>,
    /// Whether it ended by failing, so the line saying so can be coloured as such.
    pub failed: bool,
    /// The token that asks this delegate to stop, and no other and not the turn (DELEGATE-25).
    pub stop: bravebot_core::cancel::DelegateStop,
}

/// A command this session ran, and what it printed.
///
/// Kept whether or not the planner was allowed to read it. The person owns the directory and is
/// entitled to read what their agent ran; what must not happen is those bytes reaching a model's
/// context, and a screen is not a context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The command, as the plan the person endorsed showed it.
    pub command: String,
    /// What it printed, as far back as is kept.
    pub lines: Vec<String>,
    /// How many lines there were altogether, so the view can say what it left out.
    pub total: usize,
    /// Whether the planner was allowed to read it.
    pub read_by_the_planner: bool,
    /// How the command ended, as the driver said it from the exit codes and the clock.
    pub outcome: bravebot_agent::report::Outcome,
    /// The background job this row is, where it is one (RUN-26).
    ///
    /// One row for the whole of a job's life, made when it starts, so it is the row a person finds
    /// a running build at rather than one that appears the first time the planner looks.
    pub job: Option<JobView>,
}

impl Output {
    /// Whether the planner read what the row holds, or `None` for a job that has printed nothing
    /// yet, where there is nothing it could have read or been kept from.
    pub fn read(&self) -> Option<bool> {
        (self.job.is_none() || self.total > 0).then_some(self.read_by_the_planner)
    }
}

/// A background job, as the driver reported it (RUN-26).
///
/// Every field is the driver's or this end's own: a name the driver minted, the line the person
/// endorsed, a clock reading taken here, and an outcome said from exit codes. None of it comes from
/// a byte the job printed, so the hint line, `/status` and the row may decide from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobView {
    /// The name the driver gave it, which is the one the planner reads it by.
    pub name: String,
    /// The delegate whose job it is, where it is not the turn's own: each delegate mints names of
    /// its own, so `job:1` can be two jobs at once.
    pub delegate: Option<bravebot_agent::report::DelegateId>,
    /// When this end was told it started, so how long it has run is this end's clock and not a
    /// figure the worker sent.
    pub since: Instant,
    /// How long the run had been waited for when the person moved it, where they did.
    pub moved_after: Option<Duration>,
    pub state: JobState,
    /// The token `/jobs stop` sets, which the turn reads at its next step (RUN-27).
    pub stop: bravebot_core::cancel::JobStop,
}

/// Where a background job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    /// The person asked to stop it and the turn has not reached its next step to do so.
    Stopping,
    /// It exited, or the planner or the person stopped it, and somebody was told how.
    Ended,
    /// Still running when its turn ended, and stopped with the turn.
    EndedWithTurn,
}

impl JobView {
    /// Whether its program may still be going, which a job being stopped still is.
    pub fn is_running(&self) -> bool {
        matches!(self.state, JobState::Running | JobState::Stopping)
    }

    /// Where it is, in the words the row's view and `/status` say it in.
    ///
    /// From the state and this end's clock. An ended job is left to the outcome beside it, which
    /// says how it ended from its exit codes. A moved line counts from when it started, not from
    /// the move, as the planner is told.
    pub fn standing(&self, outcome: &bravebot_agent::report::Outcome) -> String {
        match self.state {
            JobState::Running => t!(
                job_running,
                ran_for = crate::loops::spell(
                    self.since.elapsed() + self.moved_after.unwrap_or_default()
                )
            )
            .to_string(),
            JobState::Stopping => t!(job_stopping).to_string(),
            JobState::Ended => outcome.summary(),
            JobState::EndedWithTurn => t!(job_ended_with_turn).to_string(),
        }
    }

    /// The name, with the delegate's number where it is a delegate's job.
    pub fn label(&self) -> String {
        match self.delegate {
            Some(delegate) => t!(
                job_of_delegate,
                name = self.name.clone(),
                number = delegate.to_string()
            )
            .to_string(),
            None => self.name.clone(),
        }
    }
}

/// How many of a job's lines its row keeps, the same as the driver keeps of one look.
pub(super) const JOB_ROW_LINES: usize = 2000;

/// Something the delegate view can open.
///
/// One list rather than two, because a person pressing the key is asking to see work that is not
/// in the transcript, and which kind of work it was is a property of the row rather than a reason
/// for a second key.
#[derive(Debug, Clone, Copy)]
pub enum Watched<'a> {
    /// A question asked beside the work, and its answer.
    Aside(&'a Aside),
    /// A delegate's own work.
    Delegate(&'a Delegate),
    /// What a command printed.
    Output(&'a Output),
}

/// How many of a delegate's own lines the block where it started draws.
///
/// Enough to see that something is happening and roughly what, which is all the block is for.
/// Somebody wanting more than that opens the delegate itself.
pub const DELEGATE_SHOWN: usize = 3;

/// How many of a delegate's own lines are kept for the screen.
///
/// A bound rather than the whole of it: a delegate that runs long enough produces an unbounded
/// number of lines, and these are held in memory for a person who may never look. Far enough back
/// to cover the work somebody opens a delegate to ask about.
pub(super) const DELEGATE_KEPT: usize = 400;

impl Delegate {
    /// Whether this one is still working.
    pub fn is_running(&self) -> bool {
        self.note.is_none()
    }

    /// Whether the person has asked it to stop and it has not yet finished doing so.
    pub fn is_stopping(&self) -> bool {
        self.is_running() && self.stop.is_requested()
    }

    /// The last few of its calls, which is what the block where it started draws.
    ///
    /// Its calls rather than the last of its lines, because the two are not the same sequence: a
    /// preview released for the person to read is held among them and carries no call, so the
    /// last three lines of a delegate that has made three calls can hold one of them. The rows
    /// the block draws are calls, and so is the number it counts them against.
    pub fn latest(&self) -> Vec<&Entry> {
        let mut latest: Vec<&Entry> = self
            .lines
            .iter()
            .rev()
            .filter(|entry| entry.activity.is_some())
            .take(DELEGATE_SHOWN)
            .collect();
        latest.reverse();
        latest
    }

    /// Hold one more of its lines, dropping the oldest where there are already enough.
    ///
    /// The one way a line enters a delegate, so the bound holds however the line arrived. A
    /// preview released for the person to read is not a call, but it is a line held in the same
    /// memory, and a delegate releasing one per call passes the bound a preview at a time without
    /// this.
    pub(super) fn hold(&mut self, entry: Entry) {
        self.lines.push(entry);
        if self.lines.len() > DELEGATE_KEPT {
            self.lines.remove(0);
        }
    }

    /// Hold one more of its lines, and count it against the calls it has made.
    pub(super) fn keep(&mut self, entry: Entry) {
        self.calls += 1;
        self.hold(entry);
    }
}

/// One entry in the transcript.
#[derive(Debug, Clone)]
pub struct Entry {
    pub speaker: Speaker,
    pub text: String,
    /// The audit trail recorded while producing this entry, shown when the trail is visible.
    ///
    /// Already in the words it is drawn in, because an entry replayed from a stored session has
    /// no events behind it: what the audit file holds is a record of what a gate decided, not the
    /// decision. See [`bravebot_session::audit::TrailLine`].
    pub trail: Vec<TrailLine>,
    /// The task list as it stood when this entry was made, if the turn kept one.
    ///
    /// Held on the entry rather than in one place so the scrollback shows what each turn did.
    /// A live list belongs to the turn in flight and goes here when that turn ends.
    pub todos: Vec<bravebot_core::todo::Row>,
    /// The call this entry describes, for a [`Speaker::Tool`] entry.
    ///
    /// Carries the note and the hunks separately from `text` so the interface can style them
    /// without parsing anything back out of a formatted line.
    pub activity: Option<Activity>,
    /// Where this call's result went: into the model's context, into a slot, or nowhere.
    ///
    /// The line says what was read; this says who can read it, which is the part a person
    /// cannot work out from the outside and the part the whole design turns on.
    pub landing: Option<Landing>,
    /// Quarantined content this call produced, for the person watching.
    ///
    /// Kept apart from `text` because it is drawn apart: it is the one thing on the screen the
    /// model was not allowed to read, and it is marked as such by the renderer rather than by
    /// anything in the bytes, which could say whatever they liked.
    pub shown: Option<Shown>,
    /// A few lines of what this call handed the planner, drawn plainly because the planner read them.
    pub returned: Option<Returned>,
    /// Whether the person expanded this call past its glimpse or its twelve lines of change.
    pub expanded: bool,
    /// The delegate this entry stands for, for a [`Speaker::Delegate`] entry.
    ///
    /// Its own lines live here rather than in the transcript around it. Several delegates work at
    /// once, so lines interleaved with the turn's could not be read in either direction: whose
    /// each one was would be a guess from the words, and the words are prose a model wrote.
    pub delegate: Option<Delegate>,
    /// The definition a person addressed the turn this reply ends, for a [`Speaker::Assistant`]
    /// entry.
    ///
    /// The name the driver matched against the set it resolved, never anything read out of the
    /// reply: which definition answered is a fact the driver holds, and the reply is model output
    /// that could claim to be any of them (ADDRESS-12).
    pub answered_as: Option<String>,
    /// Why the planner made the call, for a [`Speaker::Tool`] entry read back off disk.
    ///
    /// A live call carries its reason on its [`Activity`]; a recalled one has no activity, and
    /// the reason is still what the person watching was shown beside it the first time round.
    pub why: String,
}

impl Entry {
    /// An entry of one speaker with every other field empty.
    ///
    /// The one place the full field list is written, so each constructor sets only what it adds.
    fn blank(speaker: Speaker, text: String) -> Self {
        Self {
            speaker,
            text,
            trail: Vec::new(),
            todos: Vec::new(),
            landing: None,
            shown: None,
            returned: None,
            expanded: false,
            activity: None,
            delegate: None,
            answered_as: None,
            why: String::new(),
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::blank(Speaker::User, text.into())
    }

    pub fn assistant(text: impl Into<String>, trail: Vec<TrailLine>) -> Self {
        Self {
            trail,
            ..Self::blank(Speaker::Assistant, text.into())
        }
    }

    /// Why a turn failed, in words composed from what was known about the failure.
    pub fn failure(text: impl Into<String>) -> Self {
        Self {
            speaker: Speaker::Failure,
            ..Self::system(text)
        }
    }

    pub fn stopped(text: impl Into<String>) -> Self {
        Self {
            speaker: Speaker::Stopped,
            ..Self::system(text)
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::blank(Speaker::System, text.into())
    }

    /// The block a delegate's work is drawn under.
    pub fn delegate(delegate: Delegate) -> Self {
        Self {
            delegate: Some(delegate),
            ..Self::blank(Speaker::Delegate, String::new())
        }
    }

    /// A command the user ran in shell mode, echoed as they typed it.
    pub fn shell(line: impl Into<String>) -> Self {
        Self::blank(Speaker::Shell, line.into())
    }

    /// What such a command printed.
    pub fn output(text: impl Into<String>) -> Self {
        Self::blank(Speaker::Output, text.into())
    }

    /// One tool call, as it stands.
    ///
    /// Made while the call is still running and replaced when it finishes, which is what puts
    /// a slow call on the screen while it is slow rather than only once it is over.
    pub fn tool(activity: Activity) -> Self {
        let line = activity.line();
        Self {
            activity: Some(activity),
            ..Self::blank(Speaker::Tool, line)
        }
    }

    /// One call read back out of a stored session.
    ///
    /// No [`Activity`], because a stored session records that the call happened and not what came
    /// of it. Giving it one would mean choosing an outcome, and every choice available is a
    /// claim the record does not support: `running` says it never finished, and `done` says it
    /// succeeded. The line alone is what is known, and the interface draws it as such.
    pub fn recalled_tool(line: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            why: why.into(),
            ..Self::blank(Speaker::Tool, line.into())
        }
    }

    /// Attach the task list the turn finished with.
    pub fn with_todos(mut self, todos: Vec<bravebot_core::todo::Row>) -> Self {
        self.todos = todos;
        self
    }

    /// Say which definition answered, where a person addressed one.
    pub fn answered_as(mut self, name: Option<String>) -> Self {
        self.answered_as = name;
        self
    }
}

/// The entries of a transcript, with a stamp that changes whenever they may have.
///
/// A frame's cost has to follow what changed, not the length of the conversation, and what a frame
/// can ask is whether the entries are the ones it laid out last time. Every route to a mutable
/// entry goes through [`DerefMut`], which takes a new stamp, so no edit of the transcript can be
/// missed by a reader of the stamp and no call site has to remember to announce one. A borrow that
/// changes nothing costs a redundant layout, never a stale one.
#[derive(Debug)]
pub struct Entries {
    entries: Vec<Entry>,
    stamp: u64,
}

/// Stamps are drawn from one counter for the whole process, so two transcripts can never hold the
/// same stamp and a layout kept for one is never taken for the other's.
fn next_stamp() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Entries {
    /// The stamp the entries hold right now. Equal stamps mean the entries are untouched.
    pub fn stamp(&self) -> u64 {
        self.stamp
    }
}

impl Default for Entries {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl From<Vec<Entry>> for Entries {
    fn from(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            stamp: next_stamp(),
        }
    }
}

impl std::ops::Deref for Entries {
    type Target = Vec<Entry>;

    fn deref(&self) -> &Vec<Entry> {
        &self.entries
    }
}

impl std::ops::DerefMut for Entries {
    fn deref_mut(&mut self) -> &mut Vec<Entry> {
        self.stamp = next_stamp();
        &mut self.entries
    }
}

impl<'a> IntoIterator for &'a Entries {
    type Item = &'a Entry;
    type IntoIter = std::slice::Iter<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<'a> IntoIterator for &'a mut Entries {
    type Item = &'a mut Entry;
    type IntoIter = std::slice::IterMut<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.stamp = next_stamp();
        self.entries.iter_mut()
    }
}

/// One computed value kept between frames, replaced when the question it answers changes.
///
/// Behind a lock because a frame is drawn from a shared borrow of the session, and the thing
/// being kept is a product of drawing it. An [`Arc`] is handed out so a reader holds the value
/// without holding the lock while it works.
#[derive(Debug)]
pub struct Kept<T>(std::sync::Mutex<Option<std::sync::Arc<T>>>);

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self(std::sync::Mutex::new(None))
    }
}

impl<T> Kept<T> {
    /// The value kept, if `current` says it still answers the question, and otherwise one built
    /// afresh and kept in its place.
    pub fn get(
        &self,
        current: impl FnOnce(&T) -> bool,
        build: impl FnOnce() -> T,
    ) -> std::sync::Arc<T> {
        let mut held = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = held.as_ref().filter(|value| current(value)) {
            return value.clone();
        }
        let built = std::sync::Arc::new(build());
        *held = Some(built.clone());
        built
    }
}
