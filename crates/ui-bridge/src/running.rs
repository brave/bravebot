//! One session's live state, and the turn that may be running in it.
//!
//! The turn engine is synchronous: it blocks on the model, and it blocks on a person when
//! it wants to write. So a turn runs on its own thread and the dispatch thread stays free
//! to answer, to cancel, and to serve other sessions.
//!
//! What the two threads share is deliberately small. The worker takes the session's state
//! for the duration of the turn and hands it back by releasing the lock; the dispatch
//! thread holds only what it needs to answer a question or stop the work.

use crate::turn::Reply;
use bravebot_agent::Conversation;
use bravebot_agent::answers::Answers;
use bravebot_agent::conversation::Snapshot;
use bravebot_core::cancel::Cancel;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::todo::Row;
use bravebot_core::trust::TrustStore;
use bravebot_session::sessions::Handle;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Everything a session carries between turns.
///
/// Held behind a mutex the worker takes for the length of a turn. The dispatch thread does
/// not contend for it, because a session with a turn in flight refuses another one anyway.
pub struct State {
    /// The exchange so far, which is what resuming restores.
    pub conversation: Conversation,
    /// What this session's user has agreed to, kept past the prompt that asked.
    ///
    /// The map is carried forward turn to turn rather than re-derived: a turn that writes
    /// untrusted data into a trusted path records that path as untrusted, and losing that would
    /// let the next turn read the same data back as trusted. The programs are a vouch, a
    /// standing answer about a command, read back off the record when a session resumes, so
    /// re-asking about one already answered, or forgetting one, would both be wrong. The prompts
    /// drawn and the files agreed to be shown are live only and in no record. Opening, forking,
    /// undoing and the end of a turn each take it apart with no `..` in [`Answers`].
    ///
    /// The rules are the permission rules this session opened under (PERM-12): empty until the
    /// session is opened, and for a session no settings file wrote a rule for. Read once and
    /// kept, so a settings file edited while the session is open changes nothing about it. Not
    /// saved: a reopened session reads the files again.
    ///
    /// The language servers are the ones this session has started (LSP-8): `None` until a turn
    /// builds the set. The session holds the set so that an approved server answers later turns.
    /// A set owned by a turn would stop at the end of it, and the next turn would ask about the
    /// same language and index the same tree again. Each turn takes the set and puts it back. The
    /// set is not saved to the record: a reopened or forked session starts with none and asks
    /// before starting a server. Dropping this state stops the servers, which happens when the
    /// session closes or the process ends.
    pub answers: Answers<crate::rules::SettingsRules>,
    /// Which directories beyond the project this session has open, canonical.
    ///
    /// This front-end opens none: there is no `/add-dir` in the protocol, so a session begun
    /// here is a session over one directory. It is still carried, because a session begun in
    /// the terminal may have opened others and this is the half of that grant a trust rule
    /// cannot express — an absolute path is refused unless its directory is open, whatever the
    /// map says. Held so a turn taken here restores the reach beside the rules it inherited,
    /// and so saving writes the grant back rather than dropping it on the session's behalf.
    pub directories: Vec<PathBuf>,
    /// Bypass was on at some point while this session ran, so the map and the directories above
    /// may hold what the mode answered, and the record says so (MODE-4).
    pub answered_by_mode: bool,
    /// The checkouts the session's delegates were given and it keeps, which each turn's workspace
    /// is given because it is dropped when the turn ends (CHECKOUT-16). `None` until the first
    /// turn, or until a record that lists checkouts is opened.
    pub kept: Option<bravebot_agent::workspace::KeptBetweenTurns>,
    /// How the session is written down.
    ///
    /// `None` for a fresh session until its first turn creates it, so a window somebody
    /// opened and abandoned leaves nothing behind. A resumed one has this from the start:
    /// it already has a record, and the point of resuming is to write back to it.
    pub handle: Option<Handle>,
    pub turns: usize,
    /// Where each turn began and ended, and what came of it (SESSION-23).
    ///
    /// `None` for a session whose record has none: a record from before turn boundaries were kept
    /// is read as the legacy shape, and a history started halfway through it would claim
    /// boundaries for turns nobody recorded. `Some` is carried forward from the record on resume
    /// and extended by every turn taken here, so a save writes back the history it was given.
    pub history: Option<Vec<bravebot_session::sessions::StoredTurn>>,
    pub tokens: u64,
    /// How many of `tokens` the model read and how many it wrote, for the turns that reported it.
    pub split: bravebot_session::sessions::Split,
    /// What each turn cost, by turn number.
    ///
    /// The total says what the session cost; this says which turn cost it, which is the question
    /// when one turn spent most of it. Written down beside the total so a record this front-end
    /// wrote reads the same in the terminal, where the breakdown is what the transcript shows.
    pub spend: BTreeMap<usize, u64>,
    /// The agent's per-turn timing, preserved when a terminal session resumes here.
    pub timing: BTreeMap<usize, bravebot_agent::timing::Timing>,
    /// The model the server reported answering with, as of the last turn.
    ///
    /// What answered rather than what was asked for: an endpoint may serve something other than
    /// the name it was given. `None` until a turn has reached a server.
    pub model: Option<String>,
    pub todos: BTreeMap<usize, Vec<Row>>,
    /// Preserve terminal side conversations when continuing a session in the UI.
    pub asides: Vec<bravebot_session::sessions::Aside>,
    /// Imported terminal checkpoints stay available with a desktop coverage warning after execution.
    pub rewind: bravebot_session::rewind::RewindStack,
    /// The first thing the user asked, which is what a list calls the session.
    pub first_prompt: Option<String>,
    /// How many manifest runs this open session has started. Numbers the runs in events.
    ///
    /// Not saved: a run is written as its own record (MANIFEST-11), and the session's record
    /// holds nothing about it.
    pub runs: usize,
    /// The MCP servers this session started, once its first turn has started them (SERVERS-9).
    pub mcp: Mcp,
    /// The directory of its own outside the project, made as the session opened (TRUST-14).
    ///
    /// Held here so it lasts as long as the session's state does: the directory is removed when
    /// this is dropped, which happens once the session is closed and no turn still holds the state.
    pub scratch: Scratch,
    /// The last question number this session has put to a person, shared by every turn and run in
    /// it. A number is never reused, so an answer meant for one question cannot match a later one
    /// that happens to carry the same number (RPCVIEW-6).
    pub question_ids: Arc<AtomicU64>,
    /// What the person answered to the planner's questions in this session, shared by every turn
    /// in it so a question the planner puts again is not put to them again (ASK-8). Not saved: a
    /// session opened from a record, and a fork, start with none, as `--resume` does in the
    /// terminal.
    pub question_answers: Arc<Mutex<bravebot_core::ask::AnswerMemo>>,
    /// The session moments (HOOK-10): `None` until the session's first turn begins it, which reads
    /// the hooks once, and the same ones fire when the session is closed. Held with the rest of the
    /// state so that the end waits for the turn that is running, which holds the state for as long as
    /// it runs. Not saved: a reopened session and a fork begin their own.
    pub moments: Option<bravebot_agent::hooks::SessionMoments>,
    /// The agents this session has started and where each stands. The reporter of every turn and
    /// run reports into it, so the list lasts across them, and the window is sent the whole of it
    /// each time it changes (VIEW-35). Not saved: a reopened session and a fork start with none.
    pub roster: Arc<Mutex<bravebot_agent::roster::Roster>>,
    /// What this session carries into its requests to the ChatGPT account: the identifier they carry
    /// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
    ///
    /// Begun with the state, so a session opened, one resumed from a record and a fork each have an
    /// identifier of their own, and a second session in the process never shares the first's. Held
    /// here so that every turn and run of the session is built with the same one. Not saved: the
    /// record holds nothing about which requests the account tied together.
    pub account: bravebot_agent::chatgpt::AccountSession,
}

/// The session's own directory outside the project, or why it has none.
pub enum Scratch {
    /// Made, and removed when this is dropped.
    Held(bravebot_agent::SessionScratch),
    /// Could not be made. The session runs without one and says so, with the reason the system
    /// gave.
    Unavailable(String),
}

impl Scratch {
    /// Make one, as the terminal does as a session opens.
    pub fn open() -> Self {
        match bravebot_agent::SessionScratch::create() {
            Ok(held) => Self::Held(held),
            Err(problem) => Self::Unavailable(problem.to_string()),
        }
    }

    /// The directory as the session made it, where the session has one.
    pub fn directory(&self) -> Option<bravebot_agent::ScratchDirectory> {
        match self {
            Self::Held(held) => held.directory(),
            Self::Unavailable(_) => None,
        }
    }

    /// What the session tells the window about it: the path, or that there is none and why.
    pub fn report(&self) -> serde_json::Value {
        match self {
            Self::Held(held) => serde_json::json!({
                "directory": held.path().display().to_string(),
                "unavailable": null,
            }),
            Self::Unavailable(problem) => serde_json::json!({
                "directory": null,
                "unavailable": problem,
            }),
        }
    }
}

/// Where a session stands with the MCP servers its project requests.
///
/// Started once, by the first turn that runs to the end of starting them, and held until the
/// session closes, since dropping one stops its server. Not saved to the record, so a reopened or
/// forked session starts its own.
#[derive(Default)]
pub enum Mcp {
    /// No turn has started them yet.
    #[default]
    Unstarted,
    /// Started, where any was, with the lines saying why each other request was not.
    Started(Option<bravebot_agent::mcp::Session>),
}

impl State {
    pub fn fresh(trust: TrustStore) -> Self {
        Self::blank(
            Conversation::new(),
            Answers::opening(trust, TrustedPrograms::new(), Default::default()),
        )
    }

    /// A state holding the exchange and the answers it opens with, and nothing else.
    ///
    /// The fields no record holds (`kept`, `runs`, `mcp`, `scratch`, `question_ids`,
    /// `question_answers`, `moments`, `roster`, `account` and `answered_by_mode`) are written here
    /// and nowhere else, so a session opened, one resumed and a fork start them alike. The scratch
    /// directory and the account session are made once per call, so each state has its own.
    fn blank(conversation: Conversation, answers: Answers<crate::rules::SettingsRules>) -> Self {
        Self {
            conversation,
            answers,
            directories: Vec::new(),
            answered_by_mode: false,
            kept: None,
            handle: None,
            turns: 0,
            history: Some(Vec::new()),
            tokens: 0,
            split: Default::default(),
            spend: BTreeMap::new(),
            timing: BTreeMap::new(),
            model: None,
            todos: BTreeMap::new(),
            asides: Vec::new(),
            rewind: Default::default(),
            first_prompt: None,
            runs: 0,
            mcp: Mcp::Unstarted,
            scratch: Scratch::open(),
            question_ids: Arc::new(AtomicU64::new(0)),
            question_answers: Arc::default(),
            moments: None,
            roster: Arc::default(),
            account: bravebot_agent::chatgpt::AccountSession::begin(),
        }
    }

    /// The state a stored session resumes with.
    ///
    /// The handle is built here rather than left for `save` to mint, because the two ways
    /// of making one are not interchangeable: `Handle::begin` takes a new id and would
    /// write the continued session to a second record, leaving the one the user opened
    /// frozen at the point they opened it. `Handle::resuming` keeps the record's id, which
    /// is what makes a turn taken here land in the session it was taken in — and what
    /// `bravebot --resume` needs in order to pick the same session back up. The terminal does
    /// the same at `crates/tui/src/app.rs`.
    ///
    /// `Err` is a record another process holds, which the window is not given (SESSION-38).
    pub fn resumed(
        project: &Path,
        record: &bravebot_session::sessions::Record,
        trust: TrustStore,
    ) -> Result<Self, bravebot_session::sessions::Held> {
        let mut handle = Handle::resuming(project, record, crate::FRONT, crate::agent_build())?;
        // The desktop addresses no definition, so the turns it writes are not the definition's and
        // the record must not go on saying they were.
        handle.set_agent(None);
        Ok(Self {
            // None where the record's sitting had bypass on: `Record::trust_map` took the rules
            // back, and a directory with no rule would be reachable and vouched for by nothing.
            directories: record
                .restorable_directories()
                .iter()
                .map(PathBuf::from)
                .collect(),
            handle: Some(handle),
            turns: record.turns,
            history: record.history.clone(),
            tokens: record.tokens,
            split: record.split,
            spend: record.spend.clone(),
            timing: record.timing.clone(),
            model: record.model.clone(),
            todos: record.todo_rows(),
            asides: bravebot_session::sessions::recall(project, record).asides,
            rewind: bravebot_session::rewind::RewindStack::of(record.rewind_points(project)),
            first_prompt: Some(record.title.clone()),
            ..Self::blank(
                Conversation::restored(record.conversation.clone()),
                Answers::opening(trust, record.trusted_programs(project), Default::default()),
            )
        })
    }

    /// The state a fork begins with: part of another session's conversation, and an id of its
    /// own.
    ///
    /// The handle is `Handle::begin` where [`State::resumed`] deliberately uses
    /// `Handle::resuming`, and that one line is the whole difference between a fork and a
    /// resume. Resuming keeps the record's id so a turn lands back in the session it was taken
    /// in; forking takes a new one so the session it came from is left exactly as it was. Get
    /// this backwards and the parent is silently rewritten with a shorter history — which is
    /// what `tests/interop.rs` pins from both directions.
    ///
    /// The handle is made by the caller rather than here, because making one that is safe to
    /// use is a question about every *other* session — see `Bridge::begin_unique`. It is made at
    /// all, rather than left for `save` to mint, so the caller can say what the fork's durable
    /// id is at once. It still writes nothing: like a session started fresh, a fork opened and
    /// abandoned leaves no record.
    ///
    /// `directories` comes from the parent for the reason `trust` does: the child inherits the
    /// rules, and a rule about a directory nothing can open is the half-grant upstream keeps both
    /// halves of. The parent is left as it was either way — opening a directory is reach, not a
    /// change to it.
    ///
    /// `turns` and `todos` come from the parent, cut to the same place the conversation was, so
    /// the turn numbering the transcript shows carries on rather than restarting under a history
    /// that already has some. `tokens` starts at nothing: the figure answers "what has this
    /// session cost me", and this one has not run yet.
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is parent session state the fork inherits, with one caller in bridge.rs and one test"
    )]
    pub fn forked(
        handle: Handle,
        before: Snapshot,
        trust: TrustStore,
        programs: TrustedPrograms,
        directories: Vec<PathBuf>,
        turns: usize,
        todos: BTreeMap<usize, Vec<Row>>,
        first_prompt: Option<String>,
    ) -> Self {
        Self {
            directories,
            handle: Some(handle),
            turns,
            // The parent's boundaries are offsets into a conversation this one has cut, so none
            // are carried: the fork reads as the legacy shape, as it did before.
            history: None,
            tokens: 0,
            split: Default::default(),
            spend: BTreeMap::new(),
            timing: BTreeMap::new(),
            model: None,
            todos,
            asides: Vec::new(),
            rewind: Default::default(),
            first_prompt,
            ..Self::blank(
                Conversation::restored(before),
                Answers::opening(trust, programs, Default::default()),
            )
        }
    }
}

/// What a person typed while a turn ran, for the turn to read at its next round boundary
/// (INPUT-10). The buffer is the one in `bravebot_agent`, which the terminal also uses.
pub use bravebot_agent::interjections::Interjections;

/// A turn in flight, from the dispatch thread's point of view.
///
/// It cannot see the work. It can stop it, and it can answer the one question the work is
/// allowed to ask.
pub struct Running {
    /// Fresh for every turn. Reusing one could cancel a turn before it started.
    pub cancel: Cancel,
    /// Where an approval goes. Dropping this end is what turns a departed front-end into
    /// a refusal, so it must not outlive the session.
    pub answers: Sender<Reply>,
    /// Which question is waiting, if any.
    pub pending: crate::turn::Pending,
    /// What a cancel names to stop this turn or run and nothing else (RPCVIEW-6). Never reused,
    /// unlike a turn number, which repeats after `session.rewind` and which a manifest run does
    /// not have.
    pub target: u64,
    /// Set by the worker on its way out.
    ///
    /// The dispatch thread needs to know a turn has ended without joining on it, and it
    /// must not learn this by probing the answer channel: sending anything down that to
    /// see whether it is still connected would deliver a real decision to a real write.
    pub finished: Arc<AtomicBool>,
    /// Lines typed into this turn, taken by the worker at a round boundary. A manifest run has
    /// none, because it plans before anything is read and never asks between rounds (MANIFEST-9).
    pub interjections: Option<Interjections>,
}

/// The next number for a turn or run to be stopped by. It counts across sessions, which is
/// harmless and keeps one source of numbers that nothing resets.
pub fn next_target() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::SeqCst) + 1
}

/// A [`Running`] together with the halves of it the worker thread keeps.
pub struct Started {
    /// What the session holds while the work runs.
    pub running: Running,
    /// The worker's end of the stop.
    pub cancel: Cancel,
    /// The worker's view of the question slot.
    pub pending: crate::turn::Pending,
    /// Where the worker receives answers.
    pub answers: Receiver<Reply>,
    /// The flag the worker sets on its way out.
    pub finished: Arc<AtomicBool>,
    /// What a cancel names to stop this work, the same number [`Running::target`] holds.
    pub target: u64,
    /// The worker's end of [`Running::interjections`].
    pub interjections: Interjections,
}

impl Running {
    /// A turn or run beginning: a fresh stop, answer channel, question slot, finished flag and
    /// target. Nothing is reused from earlier work, since a reused stop could cancel work before
    /// it started and a reused channel could deliver yesterday's answer to today's question.
    pub fn start() -> Started {
        let cancel = Cancel::new();
        let (answers_tx, answers_rx) = mpsc::channel();
        let pending: crate::turn::Pending = Arc::new(Mutex::new(None));
        let finished = Arc::new(AtomicBool::new(false));
        let target = next_target();
        let interjections = Interjections::default();
        let running = Running {
            cancel: cancel.clone(),
            answers: answers_tx,
            pending: Arc::clone(&pending),
            target,
            finished: Arc::clone(&finished),
            interjections: Some(interjections.clone()),
        };
        Started {
            running,
            cancel,
            pending,
            answers: answers_rx,
            finished,
            target,
            interjections,
        }
    }

    /// Hand the turn a line the person typed while it runs, and say whether it will be read.
    ///
    /// `false` for work that has ended, work that is being stopped, and work that never asks
    /// between rounds. The caller then has a turn of its own to make of the line, which is what
    /// INPUT-10 says becomes of one the turn does not take. A `true` is not a promise: a turn that
    /// answers without another round ends with the line unread, and the `turn.interjected` event is
    /// what says it was.
    pub fn interject(&self, said: String) -> bool {
        let reading = !self.is_finished() && !self.cancel.is_cancelled();
        self.interjections
            .as_ref()
            .is_some_and(|waiting| waiting.offer(reading, said).is_ok())
    }

    /// Answer the question that is waiting, if this is an answer to that question.
    ///
    /// Both halves have to match. The id makes an approval single-use, and the kind makes
    /// it an approval of the thing that was actually shown: a front-end answering a write
    /// while a run is outstanding would otherwise have its yes applied to the run, since
    /// the ids agree and nothing else is looking.
    ///
    /// Returns whether the answer was applied. A `false` is not a retryable failure: the
    /// question is unknown, already answered, or of another kind entirely.
    pub fn answer(&self, request: u64, reply: Reply) -> bool {
        self.answer_with(request, reply, || {})
    }

    /// Publish the consumed question before waking the worker that can ask the next one.
    pub fn answer_with(&self, request: u64, reply: Reply, consumed: impl FnOnce()) -> bool {
        let Ok(mut pending) = self.pending.lock() else {
            return false;
        };
        let Some(question) = *pending else {
            return false;
        };
        if question.id != request || question.kind != reply.kind() {
            return false;
        }
        // Cleared here as well as in the confirmer, so a second reply racing the first
        // finds nothing to answer whichever of them gets the lock first.
        *pending = None;
        drop(pending);
        consumed();
        self.answers.send(reply).is_ok()
    }

    /// Whether the work has ended.
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    /// Refuse whatever is waiting, because nobody is going to answer it.
    ///
    /// Called when a session closes or the process ends. Dropping the sender would have
    /// the same effect, since the confirmer treats a closed channel as a refusal, but
    /// saying it outright does not depend on when a drop happens to run.
    pub fn refuse_pending(&self) {
        self.refuse_pending_with(|_| {});
    }

    /// As [`Self::refuse_pending`], and publish the refused question's number before waking the
    /// worker, as an answer does ([`Self::answer_with`]).
    pub fn refuse_pending_with(&self, refused: impl FnOnce(u64)) {
        let Ok(mut pending) = self.pending.lock() else {
            return;
        };
        let Some(question) = pending.take() else {
            return;
        };
        // The lock is held through the refusal, so the worker cannot leave its wait and register
        // another question that this refusal would then answer. Publishing under it takes the
        // update lock in the order `ask` does.
        refused(question.id);
        // Refused in the shape of the question that was asked, so the worker's own
        // match arm accepts it. A `Write` sent at a waiting run would be discarded as
        // a mismatch and the turn would block until the channel dropped instead.
        let _ = self.answers.send(question.kind.refusal());
    }
}

/// What the tests of the desktop's account sessions read: the identifier of the session each turn
/// and each manifest run was built in, in the order they began.
///
/// Recorded by [`crate::bridge`]'s worker and [`crate::manifest`]'s walk at the point they hand the
/// task they built to the agent, so a task built outside the session says so here. The agent sends
/// the identifier to a host that is a constant, which no test of this crate can stand a server in
/// for. A test that reads it runs in a process of its own (`in_isolated_profile`), so what it reads
/// is what that test did.
#[cfg(test)]
pub(crate) mod built_in {
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());

    /// Note that a `kind` of work, `"turn"` or `"manifest"`, was built in the session `identifier`.
    pub(crate) fn record(kind: &'static str, identifier: &str) {
        if let Ok(mut seen) = SEEN.lock() {
            seen.push((kind, identifier.to_string()));
        }
    }

    /// Everything recorded so far.
    pub(crate) fn seen() -> Vec<(&'static str, String)> {
        SEEN.lock().map(|seen| seen.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CHATGPT-10: two sessions open in one process are two sessions to the account, so neither
    /// takes the other's share of the service's cache. The regression it rejects is an identifier
    /// for the process, which is what every session carried before it had one of its own.
    #[test]
    fn two_sessions_open_in_one_process_carry_identifiers_of_their_own() {
        let first = State::fresh(TrustStore::new("/work"));
        let second = State::fresh(TrustStore::new("/work"));

        assert_ne!(first.account.identifier(), second.account.identifier());
    }
}
