//! Running a turn, and telling a front-end what it is doing.
//!
//! Three implementations of the seams the turn engine takes its interface as. Two of them
//! only talk, and one of them asks — and the asymmetry between those decides how each
//! behaves when something goes wrong.
//!
//! [`BridgeReporter`] and [`BridgeSink`] **announce**. There is no reply to wait for and
//! nothing to refuse, so every failure is a dropped line.
//!
//! [`BridgeConfirmer`] **asks**, and blocks until it is answered. Every failure resolves
//! to refusal: a channel that cannot carry the question cannot carry consent either. That
//! is the single most important property in this file, and the tests in `tests/refusal.rs`
//! exist to keep it true.

use crate::emit::Emitter;
use crate::protocol::Event;
use crate::wire;
use bravebot_agent::confirm::{
    Confirmer, Decision, FetchRequest, ManifestRequest, ServerRequest, OutputRequest, RunDecision, RunRequest, VetRequest, VouchRequest, WriteRequest,
};
use bravebot_core::ask::{Answer, Asking};
use bravebot_agent::report::{Activity, Landing, Phase, Reporter, Shown};
use bravebot_core::event::Sink;
use bravebot_core::todo::Row;
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

/// Which kind of approval or question this is.
///
/// Recorded alongside the id so an answer has to be an answer to the question that was
/// actually asked. Without it, a front-end that replied to a write while a run was
/// outstanding would have its approval applied to the run — the ids match, and nothing
/// else would notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Write,
    Run,
    Output,
    Vouch,
    Vet,
    Ask,
}

/// The question waiting on a person right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Question {
    pub id: u64,
    pub kind: Kind,
}

/// What is waiting, if anything.
///
/// Shared between the worker that asked and the dispatch thread that will be told the
/// answer. `None` means nothing is outstanding, so a reply that names a request cannot be
/// applied — which is what makes an approval single-use rather than replayable.
pub type Pending = Arc<Mutex<Option<Question>>>;

/// What a front-end answered.
///
/// One variant per question rather than a bare [`Decision`] for every kind, so a reply
/// carries which question it is answering and the kernel's own types come back intact —
/// [`RunDecision`] in particular, whose `remember` is a second answer that a `Decision`
/// has nowhere to put.
///
/// Not `Copy`, because [`Reply::Ask`] carries one answer per question.
#[derive(Debug, Clone)]
pub enum Reply {
    Write(Decision),
    Run(RunDecision),
    Output(Decision),
    Vouch(Decision),
    Vet(Decision),
    /// One answer per question, in the order they were asked. Empty means nobody could be
    /// asked — see [`Confirmer::ask_user`].
    Ask(Vec<Answer>),
}

impl Reply {
    pub fn kind(&self) -> Kind {
        match self {
            Reply::Write(_) => Kind::Write,
            Reply::Run(_) => Kind::Run,
            Reply::Output(_) => Kind::Output,
            Reply::Vouch(_) => Kind::Vouch,
            Reply::Vet(_) => Kind::Vet,
            Reply::Ask(_) => Kind::Ask,
        }
    }
}

// ---------------------------------------------------------------- announcing

/// Tells a front-end what a turn is doing.
pub struct BridgeReporter {
    emitter: Emitter,
    session: String,
    /// The last token count actually sent. See [`Reporter::output_tokens`].
    last_tokens: Option<u64>,
}

impl BridgeReporter {
    pub fn new(emitter: Emitter, session: impl Into<String>) -> Self {
        Self {
            emitter,
            session: session.into(),
            last_tokens: None,
        }
    }

    fn say(&self, name: &'static str, data: serde_json::Value) {
        self.emitter.send(Event::new(name, &self.session, data));
    }
}

impl Reporter for BridgeReporter {
    fn todos(&mut self, rows: Vec<Row>) {
        let rows: Vec<_> = rows.iter().map(wire::row).collect();
        self.say("todos", json!({ "rows": rows }));
    }

    /// How much the model has written, when it changes and not before.
    ///
    /// The engine reports this on a timer rather than on a change, so a slow round
    /// repeats one figure many times. A terminal redrawing a counter does not care; a
    /// front-end across a pipe is woken for every one of them. In the first live turn
    /// this was 130 of 168 events, and 63 of those said nothing new.
    ///
    /// Coalescing loses nothing: the figure is cumulative, so the newest supersedes every
    /// earlier one, and a front-end that never saw the repeats shows the same number.
    fn output_tokens(&mut self, written: u64) {
        if self.last_tokens == Some(written) {
            return;
        }
        self.last_tokens = Some(written);
        self.say("tokens", json!({ "written": written }));
    }

    fn phase(&mut self, phase: Phase) {
        self.say("phase", json!({ "phase": wire::phase(phase) }));
    }

    /// What the model said between tool calls.
    ///
    /// Empty where it went straight from one call to the next, which the engine still
    /// reports. There is nothing to draw for it, and an interface that made a bubble per
    /// empty narration would show a row of blank messages.
    fn narration(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        self.say("narration", json!({ "text": text }));
    }

    fn quarantined(&mut self, shown: Shown) {
        self.say("quarantined", wire::shown(&shown));
    }

    fn landed(&mut self, landing: Landing) {
        self.say("landed", json!({ "landing": wire::landing(landing) }));
    }

    fn tool_started(&mut self, activity: Activity) {
        self.say("tool.started", wire::activity(&activity));
    }

    fn tool_finished(&mut self, activity: Activity) {
        self.say("tool.finished", wire::activity(&activity));
    }
}

/// Collects the audit trail, and streams it as it arrives.
///
/// Both, deliberately. The collected copy is what gets written beside the record at the
/// end of the turn, in the agent's own format, so `bravebot --resume` reads a complete trail.
/// The stream is for the interface, which should not have to wait for a turn to end
/// before it can show what the gates decided.
///
/// The two can disagree if a turn dies before its trail is written. The interface reloads
/// on `turn.done` for that reason; until then what it holds is provisional.
pub struct BridgeSink {
    emitter: Emitter,
    session: String,
    turn: usize,
    trail: bravebot_session::audit::Trail,
}

impl BridgeSink {
    pub fn new(emitter: Emitter, session: impl Into<String>, turn: usize) -> Self {
        Self {
            emitter,
            session: session.into(),
            turn,
            trail: bravebot_session::audit::Trail::new(),
        }
    }

    /// The trail as the agent writes it down.
    pub fn trail(&self) -> &bravebot_session::audit::Trail {
        &self.trail
    }
}

impl Sink for BridgeSink {
    fn emit(&mut self, event: bravebot_core::event::Event) {
        // Projected with the agent's own function rather than a second spelling of it:
        // two renderings of one trail would drift the moment either changed.
        let data = json!({
            "turn": self.turn,
            "event": bravebot_session::audit::as_json(&event, None),
        });
        self.emitter.send(Event::new("audit", &self.session, data));
        self.trail.emit(event);
    }
}

// ---------------------------------------------------------------- asking

/// Carries a write to whoever is watching, and waits.
///
/// Everything about this type is arranged so that the answer is either an explicit
/// approval from a person or a refusal. There is no third outcome and no timeout: a write
/// waits for a human for as long as that takes, which is what it should do.
pub struct BridgeConfirmer {
    emitter: Emitter,
    session: String,
    pending: Pending,
    answers: Receiver<Reply>,
    next: u64,
    cancel: bravebot_core::cancel::Cancel,
}

impl BridgeConfirmer {
    pub fn new(
        emitter: Emitter,
        session: impl Into<String>,
        pending: Pending,
        answers: Receiver<Reply>,
        cancel: bravebot_core::cancel::Cancel,
    ) -> Self {
        Self {
            emitter,
            session: session.into(),
            pending,
            answers,
            next: 0,
            cancel,
        }
    }

    /// Put one question to whoever is watching, and block until it is answered.
    ///
    /// The whole of the asking is here, once, because every question has
    /// the same failure modes and each of them must resolve to refusal. Writing that several
    /// times would be four chances to get it wrong in a way no test distinguishes.
    ///
    /// `None` means nobody answered — a poisoned lock, a departed front-end, a closed
    /// session, a shutting-down process, or a reply to a different question. Every caller
    /// turns that into its own flavour of no.
    fn ask(&mut self, kind: Kind, event: &'static str, data: impl FnOnce(u64) -> Value) -> Option<Reply> {
        if self.cancel.is_cancelled() { return None; }
        self.next += 1;
        let id = self.next;

        // Registered before the question goes out, so an answer that arrives immediately
        // has something to match against. The other order has a race the front-end wins.
        let Ok(mut pending) = self.pending.lock() else {
            // Another thread panicked while holding this. Nothing can be registered, so
            // nothing can be answered, so nothing is approved.
            return None;
        };
        *pending = Some(Question { id, kind });
        drop(pending);

        self.emitter
            .send(Event::new(event, &self.session, data(id)));

        // Blocks until the dispatch thread sends an answer, or until the sending end is
        // dropped — which is what a departed front-end, a closed session, or a shutting
        // down process looks like from here. All of them are refusals.
        let reply = loop {
            if self.cancel.is_cancelled() { break None; }
            match self.answers.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(reply) => break (!self.cancel.is_cancelled()).then_some(reply),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break None,
            }
        };

        // Consumed either way. An id that is no longer pending cannot be answered again,
        // so an approval cannot be replayed against a second question.
        if let Ok(mut pending) = self.pending.lock() {
            *pending = None;
        }

        // Belt and braces. `Running::answer` already refuses a reply whose kind does not
        // match the outstanding question, so this should be unreachable — and it is
        // checked anyway, because the cost of being wrong is an approval landing on a
        // question nobody was shown.
        reply.filter(|reply| reply.kind() == kind)
    }
}

impl Confirmer for BridgeConfirmer {
    // These upstream capabilities have no approval UI yet. Never grant authority
    // for a request the person could not review.
    fn confirm_fetch(&mut self, _request: &FetchRequest) -> Decision {
        Decision::Reject
    }

    fn confirm_server(&mut self, _request: &ServerRequest) -> Decision {
        Decision::Reject
    }

    fn confirm_manifest(&mut self, _request: &ManifestRequest) -> Decision {
        Decision::Reject
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        match self.ask(Kind::Vet, "vet.request", |id| wire::vet_request(id, request)) {
            Some(Reply::Vet(decision)) => decision,
            _ => Decision::Reject,
        }
    }

    // The UI queues messages for the next turn; the protocol has no mid-turn input.
    // In particular, polling must neither block nor consume an approval reply.
    fn interjection(&mut self) -> Option<String> {
        None
    }

    fn confirm_write(&mut self, request: &WriteRequest) -> Decision {
        match self.ask(Kind::Write, "confirm.request", |id| {
            wire::write_request(id, request)
        }) {
            Some(Reply::Write(decision)) => decision,
            _ => Decision::Reject,
        }
    }

    /// Ask whether to run a pipeline.
    ///
    /// The refusal is the interesting path, and it refuses **without** remembering:
    /// a single unanswered "no" costs one command, where a remembered one would vouch for
    /// a program on the strength of a question nobody saw. `RunDecision::reject()` is that
    /// pair, and it is what every failure here resolves to.
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        match self.ask(Kind::Run, "run.request", |id| wire::run_request(id, request)) {
            Some(Reply::Run(decision)) => decision,
            _ => RunDecision::reject(),
        }
    }

    /// Ask whether the planner may read what a command printed.
    ///
    /// The one question here whose answer rests on bytes rather than on a prediction, so
    /// the bytes go on the wire — see [`wire::output_request`] for why that is the point of
    /// the question rather than a leak, and what it means for a front-end.
    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        match self.ask(Kind::Output, "output.request", |id| {
            wire::output_request(id, request)
        }) {
            Some(Reply::Output(decision)) => decision,
            _ => Decision::Reject,
        }
    }

    /// Ask whether to vouch for a quarantined file the model wants to read.
    ///
    /// A yes writes a standing rule into the trust map, so it outlives the turn that asked
    /// — which is exactly why an unanswered one must not be read as one.
    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        match self.ask(Kind::Vouch, "vouch.request", |id| {
            wire::vouch_request(id, request)
        }) {
            Some(Reply::Vouch(decision)) => decision,
            _ => Decision::Reject,
        }
    }

    /// Put a series of questions to the person.
    ///
    /// The only one of the five that is not a yes or a no, and the only one where the empty
    /// reply is the right way to say nothing: the contract asks for **no answers at all**
    /// when nobody could be asked, rather than a decline for each question. The kernel reads
    /// a missing answer as a decline anyway, and saying nothing is the one reply that cannot
    /// be wrong about how many questions there were.
    ///
    /// So a decline the person actually made and a question that never reached them arrive
    /// at the same place by different routes, and only one of them claims a person chose it.
    ///
    /// Answers are read against the prompts they answer, which is what stops a front-end
    /// returning an index for a choice that does not exist.
    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        match self.ask(Kind::Ask, "ask.request", |id| wire::ask_request(id, asking)) {
            Some(Reply::Ask(answers)) => wire::fitted(answers, asking),
            _ => Vec::new(),
        }
    }
}
