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
    CallDecision, Confirmer, Decision, ExposureRequest, FetchRequest, ManifestRequest,
    McpCallRequest, MoveRequest, OutputRequest, RunDecision, RunRequest, ServerRequest,
    ToolListRequest, VetRequest, VouchRequest, WriteDecision, WriteRequest,
};
use bravebot_agent::report::{Activity, Landing, Phase, Reporter, Shown};
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::delegate::DelegateId;
use bravebot_core::event::Sink;
use bravebot_core::todo::Row;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Whether to fetch one URL, which is consent to talk to its host and to nothing it sends back.
    Fetch,
    /// Whether to start a language server for the session.
    Server,
    /// Whether to run a frozen plan. Asked once per manifest run, before its first step.
    Manifest,
    /// Whether the planner may be given a vouched file the scan found a credential in.
    Exposure,
    /// Whether to use an MCP server a project requests, asked before it is started (SERVERS-4).
    McpServer,
    /// Whether to offer an MCP server's tools to the model (SERVERS-8).
    McpTools,
    /// Whether to make one call to an MCP server's tool (SERVERS-7).
    McpCall,
    /// Whether a remote MCP server moved where its reply pointed (SERVERS-11).
    McpMove,
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
    Fetch(Decision),
    Server(Decision),
    Manifest(Decision),
    Exposure(Decision),
    /// Three answers: no, yes, and yes for every server this project requests from now on.
    McpServer(bravebot_agent::servers::Answer),
    McpTools(Decision),
    /// Three answers: no, yes, and yes without asking again about this tool in this project.
    McpCall(CallDecision),
    McpMove(Decision),
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
            Reply::Fetch(_) => Kind::Fetch,
            Reply::Server(_) => Kind::Server,
            Reply::Manifest(_) => Kind::Manifest,
            Reply::Exposure(_) => Kind::Exposure,
            Reply::McpServer(_) => Kind::McpServer,
            Reply::McpTools(_) => Kind::McpTools,
            Reply::McpCall(_) => Kind::McpCall,
            Reply::McpMove(_) => Kind::McpMove,
            Reply::Ask(_) => Kind::Ask,
        }
    }

    /// The yes or no this reply carries, where a yes or a no is all it is.
    ///
    /// `None` for the ones that carry more: a run's answer is a decision and whether to remember
    /// it, an MCP server's and an MCP call's are each one of three, and a series of questions has
    /// an answer per question. Reading any of them as a bare decision would drop the part that
    /// makes it the answer it is, so none has one here.
    pub fn decision(&self) -> Option<Decision> {
        match self {
            Reply::Write(decision)
            | Reply::Output(decision)
            | Reply::Vouch(decision)
            | Reply::Vet(decision)
            | Reply::Fetch(decision)
            | Reply::Server(decision)
            | Reply::Manifest(decision)
            | Reply::Exposure(decision)
            | Reply::McpTools(decision)
            | Reply::McpMove(decision) => Some(*decision),
            Reply::Run(_) | Reply::McpServer(_) | Reply::McpCall(_) | Reply::Ask(_) => None,
        }
    }
}

impl Kind {
    /// The refusal of a question of this kind, in the shape its own answer takes.
    ///
    /// What is sent when nobody is going to answer: a session closing, or the process ending. It
    /// is the kind's own variant because the worker discards a reply of any other kind as an
    /// answer to a different question, and would then wait on a channel nothing else writes to.
    ///
    /// Written as a match with no wildcard, so a kind added above does not build until somebody
    /// has said what refusing it looks like.
    pub fn refusal(self) -> Reply {
        match self {
            Kind::Write => Reply::Write(Decision::Reject),
            Kind::Run => Reply::Run(RunDecision::reject()),
            Kind::Output => Reply::Output(Decision::Reject),
            Kind::Vouch => Reply::Vouch(Decision::Reject),
            Kind::Vet => Reply::Vet(Decision::Reject),
            Kind::Fetch => Reply::Fetch(Decision::Reject),
            Kind::Server => Reply::Server(Decision::Reject),
            Kind::Manifest => Reply::Manifest(Decision::Reject),
            Kind::Exposure => Reply::Exposure(Decision::Reject),
            Kind::McpServer => Reply::McpServer(bravebot_agent::servers::Answer::No),
            Kind::McpTools => Reply::McpTools(Decision::Reject),
            Kind::McpCall => Reply::McpCall(CallDecision::reject()),
            Kind::McpMove => Reply::McpMove(Decision::Reject),
            // No answers at all, which is how this question says nobody was asked.
            Kind::Ask => Reply::Ask(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------- announcing

/// Tells a front-end what a turn is doing.
pub struct BridgeReporter {
    spent: Option<bravebot_agent::Spent>,
    emitter: Emitter,
    session: String,
    /// The last token count actually sent. See [`Reporter::output_tokens`].
    last_tokens: Option<u64>,
    /// What the turn said about itself as it ran. See [`BridgeReporter::notices`].
    notices: Vec<String>,
    /// The delegate the next report belongs to, or `None` for the turn's own.
    reporting_for: Option<DelegateId>,
    /// Where the submitted prompt entered the recounted conversation (SESSION-23).
    prompt_at: Option<usize>,
}

impl BridgeReporter {
    pub fn new(emitter: Emitter, session: impl Into<String>) -> Self {
        Self {
            emitter,
            spent: None,
            session: session.into(),
            last_tokens: None,
            notices: Vec::new(),
            reporting_for: None,
            prompt_at: None,
        }
    }

    /// The latest cumulative measurements, retained for manifest storage on failure.
    pub fn spent(&self) -> Option<bravebot_agent::Spent> {
        self.spent
    }

    /// Where the turn's prompt landed in the recounted conversation, if it got that far.
    pub fn prompt_at(&self) -> Option<usize> {
        self.prompt_at
    }

    /// What the turn said as it ran, in the order it said it.
    ///
    /// What a turn that failed has instead of an outcome. A hook that could not be started is said
    /// here as it happens (HOOK-7), and a turn that then fails for its own reasons produces nothing
    /// to carry it, so the event reporting the failure carries these instead.
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    fn say(&self, name: &'static str, data: serde_json::Value) {
        self.emitter.send(Event::new(name, &self.session, data));
    }
}

impl Reporter for BridgeReporter {
    fn spent(&mut self, spent: bravebot_agent::Spent) {
        self.spent = Some(spent);
    }

    fn prompt_recorded(&mut self, at: usize) {
        self.prompt_at = Some(at);
    }

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

    fn reporting_for(&mut self, delegate: Option<DelegateId>) {
        self.reporting_for = delegate;
    }

    /// The call the model is writing, as the word its row will start with, or `null` once
    /// there is none.
    ///
    /// A delegate's is dropped: one model writes at a time, and a call named beside the turn's
    /// working line would read as the planner's.
    fn composing(&mut self, call: Option<&'static str>) {
        if self.reporting_for.is_some() {
            return;
        }
        self.say("composing", json!({ "call": call }));
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

    /// Kept for whichever event ends the turn to carry, rather than sent as its own.
    ///
    /// An event of this reporter's names the session and not the turn, and a notice belongs to the
    /// turn it was said in: the window files these under a turn number, so one arriving loose would
    /// have nowhere to go. `turn.done` and `turn.error` both have that number.
    fn notice(&mut self, text: String) {
        self.notices.push(text);
    }

    /// A check running, which is news here for the same reason a phase is: a whole model call
    /// inside the tool call already reported, and no front-end can time it from the outside.
    ///
    /// The count, or which kind of file, and nothing else. What the check reads and what it
    /// decides go to a person on the prompt, and this event crosses a pipe to a program.
    fn check_started(&mut self, checking: bravebot_core::vetting::Checking) {
        use bravebot_core::vetting::Checking;
        let shape = match checking {
            Checking::Lines(lines) => json!({ "lines": lines }),
            Checking::Picture => json!({ "file": "picture" }),
            Checking::Pdf => json!({ "file": "pdf" }),
        };
        self.say("check.started", shape);
    }

    /// Sent however the check ended, including the failure nobody could read a verdict out of: a
    /// front-end told only that one began has no way back to a screen that is not checking.
    fn check_finished(&mut self) {
        self.say("check.finished", json!({}));
    }

    /// A hook running, news here for the reason a check is: it holds the turn open and no
    /// front-end can see it from the outside. The moment's word and the program as the person's
    /// own file spells it, and nothing the hook printed.
    fn hook_started(&mut self, moment: &'static str, program: String) {
        self.say(
            "hook.started",
            json!({ "moment": moment, "program": program }),
        );
    }

    /// Sent however the hook ended, a stopped one included.
    fn hook_finished(&mut self) {
        self.say("hook.finished", json!({}));
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
    /// The key the number below is sent under: `turn`, or `run` for a manifest run.
    counted: &'static str,
    turn: usize,
    trail: bravebot_session::audit::Trail,
}

impl BridgeSink {
    pub fn new(emitter: Emitter, session: impl Into<String>, turn: usize) -> Self {
        Self {
            emitter,
            session: session.into(),
            counted: "turn",
            turn,
            trail: bravebot_session::audit::Trail::new(),
        }
    }

    /// A sink for a manifest run, whose events carry `run` and no `turn`.
    ///
    /// A run is not one of the session's turns, so its events must not be filed under a turn
    /// number. A front end that groups audit events by turn leaves these out.
    pub fn for_run(emitter: Emitter, session: impl Into<String>, run: usize) -> Self {
        Self {
            counted: "run",
            ..Self::new(emitter, session, run)
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
            self.counted: self.turn,
            "event": bravebot_session::audit::as_json(&event, self.trail.recording()),
        });
        self.emitter.send(Event::new("audit", &self.session, data));
        self.trail.emit(event);
    }

    /// Passed to the trail, which both readers above take the answer from: the record written at
    /// the end of the turn and the event sent out as it happens name the same run (TRACE-4).
    fn recording_for(&mut self, delegate: Option<DelegateId>) {
        self.trail.recording_for(delegate);
    }
}

// ---------------------------------------------------------------- asking

/// The next number in a session's sequence of question numbers, or nothing once it is used up.
// A compare-and-swap loop: `fetch_update` is deprecated for `try_update` in current Rust, which
// the minimum supported version does not have.
fn next_question(ids: &AtomicU64) -> Option<u64> {
    let mut last = ids.load(Ordering::SeqCst);
    loop {
        let next = last.checked_add(1)?;
        match ids.compare_exchange_weak(last, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return Some(next),
            Err(seen) => last = seen,
        }
    }
}

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
    ids: Arc<AtomicU64>,
    cancel: bravebot_core::cancel::Cancel,
    /// Whether a plan was put to the person and not approved. See [`Self::declined_a_plan`].
    declined_a_plan: bool,
}

impl BridgeConfirmer {
    pub fn new(
        emitter: Emitter,
        session: impl Into<String>,
        pending: Pending,
        answers: Receiver<Reply>,
        ids: Arc<AtomicU64>,
        cancel: bravebot_core::cancel::Cancel,
    ) -> Self {
        Self {
            emitter,
            session: session.into(),
            pending,
            answers,
            ids,
            cancel,
            declined_a_plan: false,
        }
    }

    /// Whether a plan was put to the person and came back without a yes.
    ///
    /// A run that ends this way fails with the agent's sentence about an unapproved plan. A
    /// front end is sent this flag so that it does not have to read that sentence to tell a
    /// declined plan from a failed one.
    pub fn declined_a_plan(&self) -> bool {
        self.declined_a_plan
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
    fn ask(
        &mut self,
        kind: Kind,
        event: &'static str,
        data: impl FnOnce(u64) -> Value,
    ) -> Option<Reply> {
        if self.cancel.is_cancelled() {
            return None;
        }
        // From the session's own counter, so a later turn does not reuse a number. A counter with
        // nothing left refuses the question, like every other way of not asking.
        let id = next_question(&self.ids)?;

        // Registered before the question goes out, so an answer that arrives immediately
        // has something to match against. The other order has a race the front-end wins.
        let Ok(mut pending) = self.pending.lock() else {
            // Another thread panicked while holding this. Nothing can be registered, so
            // nothing can be answered, so nothing is approved.
            return None;
        };
        *pending = Some(Question { id, kind });
        self.emitter
            .send(Event::new(event, &self.session, data(id)));
        // Answering cannot consume a question before its display is published.
        drop(pending);

        // Blocks until the dispatch thread sends an answer, or until the sending end is
        // dropped — which is what a departed front-end, a closed session, or a shutting
        // down process looks like from here. All of them are refusals.
        let reply = loop {
            if self.cancel.is_cancelled() {
                break None;
            }
            match self
                .answers
                .recv_timeout(std::time::Duration::from_millis(50))
            {
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

        self.emitter.view_answered(&self.session, id);

        // Belt and braces. `Running::answer` already refuses a reply whose kind does not
        // match the outstanding question, so this should be unreachable — and it is
        // checked anyway, because the cost of being wrong is an approval landing on a
        // question nobody was shown.
        reply.filter(|reply| reply.kind() == kind)
    }

    /// Put a question whose answer is a yes or a no, and read anything but a yes as a no.
    ///
    /// The whole of what a new approval of that shape needs from this type. Nobody answering, an
    /// answer to a different question and an answer that carries no decision all arrive at the
    /// refusal, so a kind added through here cannot be approved by a reply that was not about it.
    ///
    /// The kind is compared here as well as in [`Self::ask`], for the reason every other question
    /// in this file matches on its own variant: [`Reply::decision`] reads a yes out of any reply
    /// that is one, so without this the check in `ask` would be the only thing between a yes
    /// about a write and a request leaving the machine.
    fn yes_or_no(
        &mut self,
        kind: Kind,
        event: &'static str,
        data: impl FnOnce(u64) -> Value,
    ) -> Decision {
        self.ask(kind, event, data)
            .filter(|reply| reply.kind() == kind)
            .and_then(|reply| reply.decision())
            .unwrap_or(Decision::Reject)
    }
}

impl Confirmer for BridgeConfirmer {
    /// Ask whether to fetch one URL.
    ///
    /// The host goes out beside the URL, taken from it by the agent's parser, because the host is
    /// what a yes agrees to talk to and a URL can be written to read as another one. A yes is
    /// consent to that one request: what comes back stays quarantined whatever is answered, and
    /// nothing is remembered, so the next fetch asks again (FETCH-1, FETCH-3).
    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        self.yes_or_no(Kind::Fetch, "fetch.request", |id| {
            wire::fetch_request(id, request)
        })
    }

    /// Ask whether the planner may read a vouched file the scan found a credential in (CRED-15).
    ///
    /// The request names the file and each finding: a kind, a location and a masked preview. It
    /// carries no part of a value and no text of the file (CRED-19).
    ///
    /// An approval discloses the file to the model. It covers this file for the session and
    /// writes no rule. A refusal keeps the file's text from the planner, which is told why.
    fn confirm_exposing_read(&mut self, request: &ExposureRequest) -> Decision {
        self.yes_or_no(Kind::Exposure, "exposure.request", |id| {
            wire::exposure_request(id, request)
        })
    }

    /// Ask whether to offer an MCP server's tools to the model (SERVERS-8).
    ///
    /// The request carries each tool as the client drew it, the server's descriptions among them.
    /// A yes offers the list and is recorded beside the server's approval, so the same list is
    /// offered unasked in a later session. A no offers none of its tools for the rest of this one.
    fn confirm_tool_list(&mut self, request: &ToolListRequest) -> Decision {
        self.yes_or_no(Kind::McpTools, "mcp-tools.request", |id| {
            wire::mcp_tools_request(id, request)
        })
    }

    /// Ask whether to make one call to an MCP server's tool (SERVERS-7).
    ///
    /// Answer 2, stop asking about this tool in this project, is kept only where the request says
    /// it can be recorded. A front end that sent it anyway gets answer 1, which is the narrower.
    fn confirm_mcp_call(&mut self, request: &McpCallRequest) -> CallDecision {
        match self.ask(Kind::McpCall, "mcp-call.request", |id| {
            wire::mcp_call_request(id, request)
        }) {
            Some(Reply::McpCall(decision)) if decision.stand && !request.may_stand => {
                CallDecision::approve()
            }
            Some(Reply::McpCall(decision)) => decision,
            _ => CallDecision::reject(),
        }
    }

    /// Ask whether a remote MCP server moved where its reply pointed (SERVERS-11).
    ///
    /// Nothing was sent there. A yes declares the server at the new address, and every later
    /// request to it goes there.
    fn confirm_move(&mut self, request: &MoveRequest) -> Decision {
        self.yes_or_no(Kind::McpMove, "mcp-move.request", |id| {
            wire::mcp_move_request(id, request)
        })
    }

    /// Ask whether to start a language server.
    ///
    /// The request names the language, the resolved binary, the tree it would index, and whether
    /// starting it runs build tooling. Build tooling runs code from the dependency tree with the
    /// person's own access (LSP-5).
    ///
    /// An approval starts one process for the session and is not remembered after it. It does
    /// not change how the server's answers are labelled.
    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        self.yes_or_no(Kind::Server, "server.request", |id| {
            wire::server_request(id, request)
        })
    }

    /// Ask whether to run a frozen plan (MANIFEST-10).
    ///
    /// The request carries the task and every step. An approval covers this plan only and is not
    /// remembered. It does not approve the plan's writes: each write is still asked about when
    /// its step is reached.
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        let decision = self.yes_or_no(Kind::Manifest, "manifest.request", |id| {
            wire::manifest_request(id, request)
        });
        self.declined_a_plan = decision == Decision::Reject;
        decision
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        match self.ask(Kind::Vet, "vet.request", |id| {
            wire::vet_request(id, request)
        }) {
            Some(Reply::Vet(decision)) => decision,
            _ => Decision::Reject,
        }
    }

    // The UI queues messages for the next turn; the protocol has no mid-turn input.
    // In particular, polling must neither block nor consume an approval reply.
    fn interjection(&mut self) -> Option<String> {
        None
    }

    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        match self.ask(Kind::Write, "confirm.request", |id| {
            wire::write_request(id, request)
        }) {
            Some(Reply::Write(decision)) => decision.into(),
            _ => WriteDecision::reject(),
        }
    }

    /// Ask whether to run a pipeline.
    ///
    /// The refusal is the interesting path, and it refuses **without** remembering:
    /// a single unanswered "no" costs one command, where a remembered one would vouch for
    /// a program on the strength of a question nobody saw. `RunDecision::reject()` is that
    /// pair, and it is what every failure here resolves to.
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        match self.ask(Kind::Run, "run.request", |id| {
            wire::run_request(id, request)
        }) {
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

/// The questions put while a session's MCP servers start, through the same channel a turn's
/// questions take.
///
/// A window is always there to answer, so nobody being present is never the reason a server is
/// left out. A turn stopped while one of these is waiting answers it no, as every other question
/// here is answered.
impl bravebot_agent::servers::Asker for BridgeConfirmer {
    fn anybody_there(&self) -> bool {
        true
    }

    /// Ask whether to use a server a project requests (SERVERS-4).
    fn ask_to_start(
        &mut self,
        question: &bravebot_agent::servers::Question<'_>,
    ) -> bravebot_agent::servers::Answer {
        match self.ask(Kind::McpServer, "mcp-server.request", |id| {
            wire::mcp_server_request(id, question)
        }) {
            Some(Reply::McpServer(answer)) => answer,
            _ => bravebot_agent::servers::Answer::No,
        }
    }

    /// Ask whether a server whose handshake was redirected moved there (SERVERS-11). The same
    /// question, and the same card, as [`Confirmer::confirm_move`].
    fn ask_to_move(&mut self, request: &MoveRequest) -> bool {
        self.confirm_move(request) == Decision::Approve
    }
}
