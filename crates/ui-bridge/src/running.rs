//! One session's live state, and the turn that may be running in it.
//!
//! The turn engine is synchronous: it blocks on the model, and it blocks on a person when
//! it wants to write. So a turn runs on its own thread and the dispatch thread stays free
//! to answer, to cancel, and to serve other sessions.
//!
//! What the two threads share is deliberately small. The worker takes the session's state
//! for the duration of the turn and hands it back by releasing the lock; the dispatch
//! thread holds only what it needs to answer a question or stop the work.

use bravebot_agent::Conversation;
use bravebot_agent::conversation::Snapshot;
use crate::turn::{Kind, Reply};
use bravebot_agent::confirm::{Decision, RunDecision};
use bravebot_core::cancel::Cancel;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::todo::Row;
use bravebot_core::trust::TrustStore;
use bravebot_session::sessions::Handle;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

/// Everything a session carries between turns.
///
/// Held behind a mutex the worker takes for the length of a turn. The dispatch thread does
/// not contend for it, because a session with a turn in flight refuses another one anyway.
pub struct State {
    /// The exchange so far, which is what resuming restores.
    pub conversation: Conversation,
    /// Which paths this session's user vouched for, and what its writes recorded since.
    ///
    /// Carried forward turn to turn rather than re-derived: a turn that writes untrusted
    /// data into a trusted path records that path as untrusted, and losing that would let
    /// the next turn read the same data back as trusted.
    pub trust: TrustStore,
    /// Which programs this session's user vouched for.
    ///
    /// Carried forward turn to turn for the same reason as [`State::trust`], and read back
    /// off the record when a session resumes: a vouch is a standing answer about a command,
    /// so re-asking about one already answered — or forgetting one — would both be wrong.
    pub programs: TrustedPrograms,
    /// Which directories beyond the project this session has open, canonical.
    ///
    /// This front-end opens none: there is no `/add-dir` in the protocol, so a session begun
    /// here is a session over one directory. It is still carried, because a session begun in
    /// the terminal may have opened others and this is the half of that grant a trust rule
    /// cannot express — an absolute path is refused unless its directory is open, whatever the
    /// map says. Held so a turn taken here restores the reach beside the rules it inherited,
    /// and so saving writes the grant back rather than dropping it on the session's behalf.
    pub directories: Vec<PathBuf>,
    /// How the session is written down.
    ///
    /// `None` for a fresh session until its first turn creates it, so a window somebody
    /// opened and abandoned leaves nothing behind. A resumed one has this from the start:
    /// it already has a record, and the point of resuming is to write back to it.
    pub handle: Option<Handle>,
    pub turns: usize,
    pub tokens: u64,
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
    /// Keep terminal rewind checkpoints intact when the UI saves a resumed session.
    pub rewind: Vec<bravebot_session::sessions::RewindPoint>,
    /// The first thing the user asked, which is what a list calls the session.
    pub first_prompt: Option<String>,
}

impl State {
    pub fn fresh(trust: TrustStore) -> Self {
        Self {
            conversation: Conversation::new(),
            trust,
            programs: TrustedPrograms::new(),
            directories: Vec::new(),
            handle: None,
            turns: 0,
            tokens: 0,
            spend: BTreeMap::new(),
            timing: BTreeMap::new(),
            model: None,
            todos: BTreeMap::new(),
            asides: Vec::new(),
            rewind: Vec::new(),
            first_prompt: None,
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
    pub fn resumed(project: &Path, record: &bravebot_session::sessions::Record, trust: TrustStore) -> Self {
        Self {
            conversation: Conversation::restored(record.conversation.clone()),
            trust,
            programs: record.trusted_programs(project),
            directories: record.directories.iter().map(PathBuf::from).collect(),
            handle: Some(Handle::resuming(project, record, crate::agent_build())),
            turns: record.turns,
            tokens: record.tokens,
            spend: record.spend.clone(),
            timing: record.timing.clone(),
            model: record.model.clone(),
            todos: record.todo_rows(),
            asides: bravebot_session::sessions::recall(project, record).asides,
            rewind: record.rewind_points(project),
            first_prompt: Some(record.title.clone()),
        }
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
    #[allow(clippy::too_many_arguments)]
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
            conversation: Conversation::restored(before),
            trust,
            programs,
            directories,
            handle: Some(handle),
            turns,
            tokens: 0,
            spend: BTreeMap::new(),
            timing: BTreeMap::new(),
            model: None,
            todos,
            asides: Vec::new(),
            rewind: Vec::new(),
            first_prompt,
        }
    }
}

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
    pub turn: usize,
    /// Set by the worker on its way out.
    ///
    /// The dispatch thread needs to know a turn has ended without joining on it, and it
    /// must not learn this by probing the answer channel: sending anything down that to
    /// see whether it is still connected would deliver a real decision to a real write.
    pub finished: Arc<AtomicBool>,
}

impl Running {
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
        if let Ok(mut pending) = self.pending.lock()
            && let Some(question) = pending.take()
        {
            // Refused in the shape of the question that was asked, so the worker's own
            // match arm accepts it. A `Write` sent at a waiting run would be discarded as
            // a mismatch and the turn would block until the channel dropped instead.
            let _ = self.answers.send(match question.kind {
                Kind::Write => Reply::Write(Decision::Reject),
                Kind::Run => Reply::Run(RunDecision::reject()),
                Kind::Output => Reply::Output(Decision::Reject),
                Kind::Vouch => Reply::Vouch(Decision::Reject),
                Kind::Vet => Reply::Vet(Decision::Reject),
                // No answers at all, which is how this question says nobody was asked.
                Kind::Ask => Reply::Ask(Vec::new()),
            });
        }
    }
}
