//! The schema of every tool and the choice of which tools a turn is offered. Every string here is
//! read by the model, so none is looked up in a catalog (`tests/audience.rs`).
//!
//! None of it takes a `Labelled` value, a `Policy`, a `SlotStore` or a `Sink`: the tool bodies that
//! read, label or declassify content stay in `tools.rs`.

use super::{MAX_FANOUT, WHY, offer_runs};
use crate::confine::LOOPBACK_CONNECT_RULE;
use bravebot_aichat::protocol::Tool;
use bravebot_core::todo::Status;
use serde_json::json;

/// The statuses the schema advertises, taken from the kernel so the two cannot drift.
const TODO_STATUSES: [&str; 5] = Status::NAMES;

/// What a turn may say about when it runs again.
///
/// Seven states rather than a flag, because "nobody is pacing this turn", "somebody else is",
/// "this session's own loop, goal or watch takes the next look" and "nothing will ask this turn
/// anything again" want different answers. A turn nobody is looping, on a caller that will put its
/// line again, has to arrange its own later look or answer a request to report a change from one
/// read and stop. A tick of a loop the person timed has that look coming already, so a tool for
/// asking for one would have it schedule a second, and the wait would be dropped: the interval
/// decides when that loop runs, not the turn. A line the person typed while the session's loop,
/// goal or watch is live has the wait dropped too, because the session does one of those at a
/// time, but something of the session's own does start another turn, so it is not told that
/// nothing will. A turn nothing will ask again has the same wait dropped for the opposite reason,
/// and is told no later look is coming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheduling {
    /// Not a tick of anything, on a caller that will ask again. It may arrange the one later look
    /// a watch needs, which starts a loop repeating the line the person typed.
    ArrangingALook,
    /// A tick of a loop nobody gave an interval for, setting the pace of the next tick.
    PacingALoop,
    /// A tick of a loop the person gave an interval for. There is nothing here to decide.
    TheirInterval,
    /// A line the person typed while this session's loop runs. The loop's next tick is the next
    /// look, and a wait this turn asked for would be dropped, so the tool is withheld.
    UnderALoop,
    /// A line the person typed while this session works towards a goal. The goal sends the work
    /// back on its own, and a wait this turn asked for would be dropped, so the tool is withheld.
    UnderAGoal,
    /// A line the person typed while this session holds a standing watch. The watch starts a turn
    /// when its file changes, and a wait this turn asked for would be dropped, so the tool is
    /// withheld.
    UnderAWatch,
    /// Nothing will ask this turn's line again, whatever wait it names: a surface that runs one
    /// turn and exits, or a line this program wrote rather than the person.
    ///
    /// There is no later look to arrange, so the wait is discarded wherever one is asked for. A
    /// confirmation saying a later look is arranged and needs nothing from the person would be
    /// describing a watch that does not exist, which is why the tool is withheld here rather than
    /// answered.
    NoLaterLook,
}

impl Scheduling {
    /// Whether the tool is offered to the planner, and dispatched where it is called anyway.
    ///
    /// Withheld from a tick the person timed, which has nothing to decide, from a line typed under
    /// the session's own loop, goal or watch, whose wait the session would drop, and from a turn
    /// nothing will ask again, which has nothing a wait could become. A call from any of them is
    /// answered the way any other name nobody offered is.
    pub fn offered(self) -> bool {
        matches!(self, Scheduling::ArrangingALook | Scheduling::PacingALoop)
    }
}

/// Whether this turn is offered `run`.
///
/// Read by the tool table and by dispatch, because both say what to do where `read_git` will not
/// open a repository. git answers nearly everything this reader declines and `run` is how git is
/// asked, so a turn holding one is sent there. A delegate that may read files and not run programs is
/// offered `read_git` and no `run`, and sending that one there costs a round on a name that is not on
/// its list and then a refusal it cannot act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    /// `run` is on this turn's list, so what is said may name it.
    Offered,
    /// It is not, so what is said names what can be done without it instead.
    Withheld,
}

impl Running {
    /// What a list of tools offers, read off the list rather than from the capability that gates
    /// `run`.
    ///
    /// A definition's own `tools:` line takes names off a list whose capabilities reach them, so
    /// the capability is the wider answer of the two and what a description may name is what is
    /// beside it on the list.
    pub fn on(offered: &[Tool]) -> Self {
        if offer_runs(offered) {
            Self::Offered
        } else {
            Self::Withheld
        }
    }

    /// Whether what is said may name `run`.
    pub fn offered(self) -> bool {
        matches!(self, Running::Offered)
    }
}

/// Which of the tools that planner-facing text names a turn's list holds.
///
/// Built once where the turn settles its list ([`Beside::on`]) and carried on `Tools` and into
/// the turn-level code that words a result, so a sentence names one of these tools only where the
/// list holds it and otherwise says what can be done without it, or nothing (TOOL-6). A fact about
/// the offered list and about nothing a tool printed. [`Beside::every`] is the list that holds
/// every one, which is what the text was written for, so a turn offered every tool reads the same
/// bytes as before.
///
/// `run` is also [`Running`], which the refusals read; the two always agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beside {
    held: u16,
}

/// A tool that planner-facing text names only where the turn's list holds it. [`Beside`] holds a
/// fact for each of these and for no other tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Run,
    ReadOutput,
    ReadFile,
    ReadGit,
    JobOutput,
    WriteFile,
    EditFile,
    SpawnProcessor,
    VetContent,
    RequestPath,
    Search,
    SpawnAgent,
    Lsp,
    ApplyCheckout,
}

impl Named {
    const ALL: [Named; 14] = [
        Named::Run,
        Named::ReadOutput,
        Named::ReadFile,
        Named::ReadGit,
        Named::JobOutput,
        Named::WriteFile,
        Named::EditFile,
        Named::SpawnProcessor,
        Named::VetContent,
        Named::RequestPath,
        Named::Search,
        Named::SpawnAgent,
        Named::Lsp,
        Named::ApplyCheckout,
    ];

    /// The name the tool is offered under. `run` is read as [`Running::on`] reads it, so it has
    /// no entry here.
    fn offered_as(self) -> Option<&'static str> {
        match self {
            Named::Run => None,
            Named::ReadOutput => Some("read_output"),
            Named::ReadFile => Some("read_file"),
            Named::ReadGit => Some("read_git"),
            Named::JobOutput => Some("job_output"),
            Named::WriteFile => Some("write_file"),
            Named::EditFile => Some("edit_file"),
            Named::SpawnProcessor => Some("spawn_processor"),
            Named::VetContent => Some("vet_content"),
            Named::RequestPath => Some("request_path"),
            Named::Search => Some("search"),
            Named::SpawnAgent => Some("spawn_agent"),
            Named::Lsp => Some("lsp"),
            Named::ApplyCheckout => Some("apply_checkout"),
        }
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

impl Beside {
    /// What a list of tools holds, read off the list. `run` is read as [`Running::on`] reads it.
    pub fn on(offered: &[Tool]) -> Self {
        let running = Running::on(offered).offered();
        let held = Named::ALL
            .into_iter()
            .filter(|named| match named.offered_as() {
                None => running,
                Some(name) => offered.iter().any(|tool| tool.function.name == name),
            })
            .fold(0, |held, named| held | named.bit());
        Self { held }
    }

    /// A list that holds every one of them.
    pub fn every() -> Self {
        Self {
            held: Named::ALL
                .into_iter()
                .fold(0, |held, named| held | named.bit()),
        }
    }

    /// Whether the list holds this tool.
    pub fn holds(self, named: Named) -> bool {
        self.held & named.bit() != 0
    }

    /// This list without the tool, as a list that lacks it is read.
    #[must_use]
    pub fn without(self, named: Named) -> Self {
        Self {
            held: self.held & !named.bit(),
        }
    }
}

/// The tools the model may call.
///
/// `scheduling` says what this turn may say about when it runs again, which decides whether
/// `schedule_next` is offered at all and, where it is, which of two jobs its description describes.
/// `arming` says whether the session this turn belongs to keeps standing watches, which decides
/// whether `watch_file` is offered. `deadlines` is how long a command may run, which `run`'s
/// description quotes: a planner told 600 seconds where a person has made room for 1200 spends its
/// deadline on the figure it was told about and never asks for the run they paid for. `running` says
/// whether this list holds `run`, which `read_git`'s description names where a repository is one it
/// will not open.
pub fn available(
    scheduling: Scheduling,
    arming: crate::watch::Arming,
    deadlines: crate::exec::Deadlines,
    running: Running,
) -> Vec<Tool> {
    let mut tools = table(scheduling, arming, deadlines, running);
    for tool in &mut tools {
        ask_why(tool);
    }
    tools
}

/// What the planner is told of the refs and the stash its checkouts share, in the description of
/// `isolation` and in the answer to a spawn that made one (CHECKOUT-7).
pub(super) fn checkouts_share_refs(running: Running) -> String {
    let fetch = if running.offered() {
        "run a fetch they need once yourself before starting them"
    } else {
        "have one of them run a fetch they need, not each"
    };
    format!(
        "Checkouts share refs, tags and the stash with your working directory and each other, so \
         {fetch}, and do not have a delegate in one use git stash."
    )
}

/// The description of `spawn_processor`, naming only the tools `beside` says are on the list.
///
/// The table writes it for a list that holds every tool, and [`state_what_is_beside`] writes it
/// again for the one a turn settled on, since a delegate is never offered `vet_content` and a
/// reader holds no `write_file`. A sentence that sends a run to a tool it has not got costs it a
/// round on a refusal it cannot act on.
fn spawn_processor_description(beside: &dyn Fn(&str) -> bool) -> String {
    let changing = if beside("write_file") {
        "To change a file you cannot see, say what must be true of it afterwards and write the \
         returned reference with write_file as contents_ref. Nobody reads it first, so ask for the \
         whole document and nothing else."
    } else {
        "You are not shown its output."
    };

    // What it says about a summary, a translation or other text made from content the planner was
    // not shown, which a processor writes (SPAWN-6). The route to the program that sends it names
    // `run`, so it is said only where the list holds one. A list without `run` is told who writes
    // such text and what to call it, and is not sent to a program it cannot start. Writing the
    // answer to a file needs `write_file`, so a list without it is sent to `stdin_ref` alone.
    let derived_text = if beside("run") {
        let answer_reaches_it = if beside("write_file") {
            "Name a fetched page or run output in reads directly. To send the answer, give its \
             reference to run as stdin_ref, or write it with write_file to a new file in the \
             scratch directory and pass that path."
        } else {
            "Name a fetched page or run output in reads directly. To send the answer, give its \
             reference to run as stdin_ref."
        };
        format!(
            "Summaries, translations, rewrites, explanations and lists of points made from \
             content you were not shown are written by this tool, not by you; a script you write \
             can only cut the original down. Ask for the form, e.g. a plain-text summary of each \
             of six stories, two sentences each, with each link kept. {answer_reaches_it} \
             Recipient and subject are yours; only the body \
             is the processor's. Say what was sent to whom, and call text a summary only if a \
             processor wrote it."
        )
    } else {
        "Summaries, translations, rewrites, explanations and lists of points made from content \
         you were not shown are written by this tool, not by you; ask for the form, e.g. a \
         plain-text summary of each of six stories, two sentences each, with each link kept. \
         Call text a summary only if a processor wrote it."
            .to_string()
    };

    format!(
        "Transform quarantined content you were not shown: a model with no tools reads only the \
         references you name, follows your instruction and returns a new quarantined reference. \
         {changing} It decides the change and may leave a file alone. {derived_text}"
    )
}

/// What `spawn_agent` says about the tools a delegate is never offered, naming each of `vet_content`,
/// `ask_user`, `fetch_url` and `download_url` only where `beside` says it is on the list. It starts
/// with a space, to sit after the sentence before it, and is empty where the list holds none of
/// them, as a delegate's own list does.
///
/// The planner writes the task, so it is told here that work needing one of these is its own to do
/// before it delegates or instead. The same tools are in
/// [`NEVER_DELEGATED`](bravebot_core::delegate::NEVER_DELEGATED); the others there have no
/// planner-facing reason to be said when a task is written.
fn spawn_agent_lacks(beside: &dyn Fn(&str) -> bool) -> String {
    let held: Vec<&str> = ["vet_content", "ask_user", "fetch_url", "download_url"]
        .into_iter()
        .filter(|name| beside(name))
        .collect();
    let (named, needs) = match held.as_slice() {
        [] => return String::new(),
        [only] => ((*only).to_string(), "it"),
        [first @ .., last] => (format!("{} or {last}", first.join(", ")), "one of them"),
    };
    let whole = if beside("vet_content") {
        " A reference that has to be read whole is read by you with vet_content."
    } else {
        ""
    };
    format!(" A delegate cannot call {named}; do work needing {needs} yourself.{whole}")
}

/// What `spawn_agent` says about output the planner may not read: a delegate is quarantined as the
/// planner is (DELEGATE-1), so delegating a run whose output the planner may not read does not make
/// it readable. It names `spawn_processor` only where `beside` says it is on the list, and points to
/// vouching for the command, which names no tool. It starts with a space, to sit after the sentence
/// before it.
///
/// Without it the description's build example reads as a way round the quarantine, and a planner
/// holding a reference to a run's output started a delegate to run the command again, which got the
/// same reference and could not read it either.
fn spawn_agent_quarantined(beside: &dyn Fn(&str) -> bool) -> String {
    let instead = if beside("spawn_processor") {
        " Ask spawn_processor a bounded question about the reference instead, or have a person \
         vouch for the command."
    } else {
        " Have a person vouch for the command instead."
    };
    format!(
        " A delegate is quarantined as you are: run output you may not read stays unreadable to \
         it.{instead}"
    )
}

/// The description of `read_output`, naming only the tools `beside` says are on the list. See
/// [`spawn_processor_description`].
fn read_output_description(beside: &dyn Fn(&str) -> bool) -> String {
    let anything_else = if beside("vet_content") {
        "ask vet_content for other references. "
    } else {
        ""
    };
    let filter = if beside("run") {
        "; for a large one, run tail or grep with it as stdin_ref and read: true"
    } else {
        ""
    };
    format!(
        "Show what a run printed: give its reference. Run output is quarantined by default, so \
         ask for results you need, errors included, and do not claim success unseen. Not for a \
         quarantined file; {anything_else}Output cut for length may be read without asking, paged \
         by byte offset or searched with pattern. A pattern on output you may not read is \
         refused. Output you may not read comes back whole and stays in every later request, \
         about a token per four bytes{filter}."
    )
}

/// The sentence, with its leading space, that says what to do with the reference a quarantined
/// result comes back as, naming only the tools `beside` says are on the list. Empty where neither
/// `spawn_processor` nor `write_file` is.
fn quarantined_reference_moves(beside: &dyn Fn(&str) -> bool) -> &'static str {
    match (beside("spawn_processor"), beside("write_file")) {
        (true, true) => {
            " Give it to spawn_processor with a question, or to write_file as contents_ref."
        }
        (true, false) => " Give it to spawn_processor with a question.",
        (false, true) => " Give it to write_file as contents_ref to save it.",
        (false, false) => "",
    }
}

/// The sentence, with its leading space, that names `vet_content` as the way to read `what` a
/// quarantined result holds. Empty where `beside` says `vet_content` is not on the list.
fn read_it_yourself(beside: &dyn Fn(&str) -> bool, what: &str) -> String {
    if beside("vet_content") {
        format!(" To read the {what} yourself, ask vet_content.")
    } else {
        String::new()
    }
}

/// The description of `fetch_url`, naming only the tools `beside` says are on the list. See
/// [`spawn_processor_description`].
fn fetch_url_description(beside: &dyn Fn(&str) -> bool) -> String {
    let moves = quarantined_reference_moves(beside);
    let reading = read_it_yourself(beside, "page");
    format!(
        "Fetch an http or https URL, but not a github.com pull request or issue where the \
         prompt says the GitHub CLI is on PATH. The result is a quarantined reference you cannot \
         read.{moves}{reading} The user is asked before anything leaves the machine."
    )
}

/// The description of `job_output`, naming only the tools `beside` says are on the list. See
/// [`spawn_processor_description`].
fn job_output_description(beside: &dyn Fn(&str) -> bool) -> String {
    let passed_to: Vec<&str> = ["read_output", "spawn_processor", "write_file"]
        .into_iter()
        .filter(|name| beside(name))
        .collect();
    let reference = match passed_to.as_slice() {
        [] => "a reference".to_string(),
        [one] => format!("a reference to pass to {one}"),
        [init @ .., last] => format!("a reference to pass to {} or {last}", init.join(", ")),
    };
    format!(
        "See what a background job printed since your last look, and whether it ended. Each \
         look is a slice; the result gives its line range, and earlier slices are in the \
         references it lists. Output is quarantined as a run's is: text where the user vouched \
         for every command in the line, otherwise {reference}. wait_seconds waits for new \
         output, the end or a typed line; until_exit waits for the end. A wait stays inside \
         this turn; the job's start says whether it outlives the turn. The same call without \
         wait_seconds, or on an ended job, is held at the third in a row."
    )
}

/// What the description of `run` says to do with output that comes back as a reference, led by a
/// colon, naming only the tools `beside` says are on the list. Nothing where none is.
///
/// The rest of the description depends on the deadlines and on what the turn may say about a later
/// look, so the table joins this into it and [`state_what_is_beside`] replaces it there.
fn run_reference_moves(beside: &dyn Fn(&str) -> bool) -> String {
    let moves: Vec<&str> = [
        beside("read_output").then_some("read it with read_output"),
        beside("spawn_processor").then_some("give it to spawn_processor"),
        beside("write_file").then_some("save it with write_file"),
    ]
    .into_iter()
    .flatten()
    .collect();
    match moves.as_slice() {
        [] => String::new(),
        [one] => format!(": {one}"),
        [init @ .., last] => format!(": {}, or {last}", init.join(", ")),
    }
}

/// What the description of `run` says about text made from output that comes back as a reference:
/// a processor writes it, and its description says how that text reaches a program (SPAWN-6). The
/// sentence, with its leading full stop, where `beside` says `spawn_processor` is on the list, else
/// nothing. The table joins it in after [`run_reference_moves`] and [`state_what_is_beside`]
/// replaces it.
fn run_text_made_from_it(beside: &dyn Fn(&str) -> bool) -> String {
    if beside("spawn_processor") {
        ". Text made from it, such as a summary, is written by spawn_processor, whose \
         description says how it reaches a program"
            .to_string()
    } else {
        String::new()
    }
}

/// The description of `run`'s `background` argument, naming `job_output` only where `beside` says
/// it is on the list.
fn run_background_description(beside: &dyn Fn(&str) -> bool) -> String {
    let look = if beside("job_output") {
        " job_output shows its output."
    } else {
        ""
    };
    format!(
        "Leave it running and return a job name; you are told when it ends during this \
         turn.{look} One pipeline, no redirection; killed when the turn ends."
    )
}

/// What the description of `run` says about watching something: a file by its change token and a
/// program by `job_output`, naming `read_file` and `job_output` only where `beside` says they are on
/// the list. A list without `read_file` is not told how a file is compared, and one without
/// `job_output` is told only that it is told when the job ends.
///
/// The table joins this into the description and [`state_what_is_beside`] replaces it there.
fn run_watching(beside: &dyn Fn(&str) -> bool) -> String {
    let file = if beside("read_file") {
        "comparing read_file's change token between looks"
    } else {
        "comparing one look with the next"
    };
    let program = if beside("job_output") {
        "running it with background: true and calling job_output with wait_seconds"
    } else {
        "running it with background: true"
    };
    format!(
        "What is watched decides how. A file is watched by {file}, a program by {program}. Neither reaches past this turn: a \
         background job is killed when the turn ends unless its start says the session keeps it."
    )
}

/// The description of `run`'s `read` argument, naming `read_output` only where `beside` says it is
/// on the list. A turn without it is not released output by `read`, so it is told the argument does
/// nothing there and where the output goes.
fn run_read_description(beside: &dyn Fn(&str) -> bool) -> String {
    if beside("read_output") {
        "Return the output as text here where read_output would release it without asking; \
         otherwise, or if too long, a reference."
            .to_string()
    } else {
        "Changes nothing in this turn; output you may not read stays a reference.".to_string()
    }
}

/// The description of `job_output`'s `read` argument, naming `read_output` only where `beside`
/// says it is on the list, for the reason [`run_read_description`] does.
fn job_output_read_description(beside: &dyn Fn(&str) -> bool) -> String {
    if beside("read_output") {
        "Return the output as text here where read_output would release it without asking; \
         otherwise, or if too long, a reference. The report of the job's end follows the latest \
         look's read."
            .to_string()
    } else {
        "Changes nothing in this turn; output you may not read stays a reference.".to_string()
    }
}

/// The paragraph of the `read_file` description on pictures and PDFs, naming `spawn_processor`
/// only where `beside` says it is on the list.
fn read_file_pictures(beside: &dyn Fn(&str) -> bool) -> String {
    let and_then = if beside("spawn_processor") {
        "; give it to spawn_processor with a question"
    } else {
        ""
    };
    format!(
        "A picture or PDF (.png, .jpg, .gif, .webp, .pdf) comes back as a reference you cannot \
         look at, whoever vouched for it{and_then}."
    )
}

/// The paragraph of the `read_file` description on notebooks, naming `edit_file` only where
/// `beside` says it is on the list.
fn read_file_notebook(beside: &dyn Fn(&str) -> bool) -> String {
    let cells = if beside("edit_file") {
        "; edit_file edits a cell by number"
    } else {
        ""
    };
    format!(
        "A .ipynb notebook comes back as its cells' source numbered from 1{cells}; outputs come \
         only through output_of."
    )
}

/// The `output_of` argument of `read_file`, naming `spawn_processor` only where `beside` says it is
/// on the list.
fn read_file_output_of(beside: &dyn Fn(&str) -> bool) -> String {
    let reference = if beside("spawn_processor") {
        "a reference for spawn_processor"
    } else {
        "a reference"
    };
    format!(
        "A notebook code cell whose saved outputs to return, as {reference}; offset and limit \
         are ignored."
    )
}

/// The description of `write_file`'s `contents_ref` argument, naming `spawn_processor` only where
/// `beside` says it is on the list. Where it is not, no answer of a processor is made in the turn,
/// so the argument does not say where one may be written. The driver refuses such a write
/// whatever the description says (PROC-6).
fn write_file_contents_ref_description(beside: &dyn Fn(&str) -> bool) -> String {
    let lands = if beside("spawn_processor") {
        " A spawn_processor answer may go only over the file that call was about, or, where it \
         read no file, to a new file in the scratch directory."
    } else {
        ""
    };
    format!(
        "A reference whose quarantined content becomes the file, such as a fetched page.{lands} \
         A picture or PDF is written as the file it encodes, to a path with its extension."
    )
}

/// The description of `edit_file`, naming `write_file` only where `beside` says it is on the list.
fn edit_file_description(beside: &dyn Fn(&str) -> bool) -> String {
    let diff = if beside("write_file") {
        " Prefer it to write_file for part of a file."
    } else {
        ""
    };
    format!(
        "Replace an exact passage in a workspace file.{diff} In a directory the user vouched for \
         it applies without a prompt; elsewhere the user approves the diff unless a permission \
         rule or mode answers. edits applies several passages as one diff. In a .ipynb notebook, \
         cell and new_text replace a cell's source, or use cell_action."
    )
}

/// What `repo_map` says about reading the lines a map points at, naming `read_file` only where
/// `beside` says it is on the list.
fn repo_map_bodies(beside: &dyn Fn(&str) -> bool) -> String {
    if beside("read_file") {
        "never bodies: read_file reads the lines it points at.".to_string()
    } else {
        "never bodies.".to_string()
    }
}

/// What `lsp` says about a definition in a dependency, naming `read_file` only where `beside` says
/// it is on the list.
fn lsp_dependency(beside: &dyn Fn(&str) -> bool) -> String {
    if beside("read_file") {
        "the line says so, and read_file will not open it.".to_string()
    } else {
        "the line says so.".to_string()
    }
}

/// What `vet_content` says about the alternative to reading, naming `spawn_processor` only where
/// `beside` says it is on the list.
fn vet_content_instead(beside: &dyn Fn(&str) -> bool) -> String {
    if beside("spawn_processor") {
        "Say plainly in your reply why you need to read it rather than pass it to \
         spawn_processor."
            .to_string()
    } else {
        "Say plainly in your reply why you need to read it.".to_string()
    }
}

/// Write the descriptions that name other tools again for the list a turn settled on, so a run is
/// not sent to a tool it is not offered.
///
/// Called once, on the turn's list after its narrowing and its additions. It replaces the whole
/// description of `spawn_processor`, `read_output`, `fetch_url` and `job_output`, so it runs before
/// anything is appended to them, as [`state_read_output_answer`] appends to `read_output` on each
/// request's copy. `run` is rewritten in place at the sentence about its output and at its
/// `background` argument, since the rest of its text was written from the deadlines and from what
/// the turn may say about a later look, and what [`state_confinement`] and
/// [`state_withheld_variables`] appended to it stays. A tool the list does not hold is skipped, and
/// a list that holds every tool gets the descriptions the table wrote.
pub fn state_what_is_beside(tools: &mut [Tool]) {
    let names: Vec<String> = tools
        .iter()
        .map(|tool| tool.function.name.clone())
        .collect();
    let beside = |name: &str| names.iter().any(|offered| offered == name);
    for tool in tools.iter_mut() {
        match tool.function.name.as_str() {
            "spawn_processor" => tool.function.description = spawn_processor_description(&beside),
            "read_output" => tool.function.description = read_output_description(&beside),
            "fetch_url" => state_fetch_url_beside(tool, &beside),
            "web_search" => {
                tool.function.description = web_search_description(&beside);
                remove_read_without_vet_content(tool, &beside);
            }
            "search" | "list_files" => remove_read_without_vet_content(tool, &beside),
            "job_output" => {
                tool.function.description = job_output_description(&beside);
                if let Some(read) = tool
                    .function
                    .parameters
                    .pointer_mut("/properties/read/description")
                {
                    *read = json!(job_output_read_description(&beside));
                }
            }
            "spawn_agent" => {
                for (full, narrowed) in [
                    (spawn_agent_lacks(&|_| true), spawn_agent_lacks(&beside)),
                    (
                        spawn_agent_quarantined(&|_| true),
                        spawn_agent_quarantined(&beside),
                    ),
                ] {
                    tool.function.description =
                        tool.function.description.replacen(&full, &narrowed, 1);
                }
            }
            "run" => state_run_beside(tool, &beside),
            "read_file" => {
                for (full, narrowed) in [
                    (read_file_pictures(&|_| true), read_file_pictures(&beside)),
                    (read_file_notebook(&|_| true), read_file_notebook(&beside)),
                ] {
                    tool.function.description =
                        tool.function.description.replacen(&full, &narrowed, 1);
                }
                if let Some(output_of) = tool
                    .function
                    .parameters
                    .pointer_mut("/properties/output_of/description")
                {
                    *output_of = json!(read_file_output_of(&beside));
                }
            }
            "write_file" => {
                if let Some(contents_ref) = tool
                    .function
                    .parameters
                    .pointer_mut("/properties/contents_ref/description")
                {
                    *contents_ref = json!(write_file_contents_ref_description(&beside));
                }
            }
            "edit_file" => tool.function.description = edit_file_description(&beside),
            "repo_map" => {
                tool.function.description = tool.function.description.replacen(
                    &repo_map_bodies(&|_| true),
                    &repo_map_bodies(&beside),
                    1,
                );
            }
            "lsp" => {
                tool.function.description = tool.function.description.replacen(
                    &lsp_dependency(&|_| true),
                    &lsp_dependency(&beside),
                    1,
                );
            }
            "vet_content" => {
                tool.function.description = tool.function.description.replacen(
                    &vet_content_instead(&|_| true),
                    &vet_content_instead(&beside),
                    1,
                );
            }
            _ => {}
        }
    }
}

/// [`state_what_is_beside`] for `fetch_url`.
fn state_fetch_url_beside(tool: &mut Tool, beside: &dyn Fn(&str) -> bool) {
    tool.function.description = fetch_url_description(beside);
    remove_read_without_vet_content(tool, beside);
}

/// What `read` says on `search`: the release `vet_content` makes, asked for in the result of the
/// walk. Planner-facing, so it is English in the code and not in a catalog (localization.md). It
/// says nothing of who answers or whether a check is made (VET-5).
const READ_A_WALK: &str = "Release a quarantined result here as vet_content would; kept back \
                           or too long, it is still a reference. Defaults to false.";

/// What `read` says on `list_files`: [`READ_A_WALK`] for a listing, which comes back kept back as
/// one reference or as one per entry depending on who answers vet_content (LIST-2).
const READ_A_LISTING: &str = "Release a quarantined listing here as vet_content would; kept \
                              back or too long, it is still references. Defaults to false.";

/// `read` asks for the release `vet_content` makes, so a list without it has nothing for the
/// argument to ask.
fn remove_read_without_vet_content(tool: &mut Tool, beside: &dyn Fn(&str) -> bool) {
    if !beside("vet_content")
        && let Some(properties) = tool
            .function
            .parameters
            .pointer_mut("/properties")
            .and_then(|properties| properties.as_object_mut())
    {
        properties.remove("read");
    }
}

/// [`state_what_is_beside`] for `run`.
fn state_run_beside(tool: &mut Tool, beside: &dyn Fn(&str) -> bool) {
    tool.function.description = tool.function.description.replacen(
        &(run_reference_moves(&|_| true) + &run_text_made_from_it(&|_| true)),
        &(run_reference_moves(beside) + &run_text_made_from_it(beside)),
        1,
    );
    tool.function.description =
        tool.function
            .description
            .replacen(&run_watching(&|_| true), &run_watching(beside), 1);
    if let Some(background) = tool
        .function
        .parameters
        .pointer_mut("/properties/background/description")
    {
        *background = json!(run_background_description(beside));
    }
    if let Some(read) = tool
        .function
        .parameters
        .pointer_mut("/properties/read/description")
    {
        *read = json!(run_read_description(beside));
    }
}

fn table(
    scheduling: Scheduling,
    arming: crate::watch::Arming,
    deadlines: crate::exec::Deadlines,
    running: Running,
) -> Vec<Tool> {
    // Which of the two answers a request about one file gets. Both exist wherever watches do, and
    // a description that named neither as the better one would leave the planner picking the one
    // it read first, which is the read's own paragraph and therefore always the loop.
    let watching = if arming.offered() {
        let mut said = " For one file, watch_file outlives this turn.".to_string();
        // The second half only where there is a schedule_next to keep for something. A turn
        // offered no way to arrange a later look has nothing to save one for, and saying
        // otherwise sends it to a name that is not on its list.
        if scheduling.offered() {
            said.push_str(" Keep schedule_next for anything else.");
        }
        said
    } else {
        String::new()
    };
    // What a turn asked to report a change does about the next look, in the two descriptions that
    // route such a request: the read's change token for a file, the run's own output for a
    // program. Neither technique reaches past this turn by itself, so both have to say where the
    // later look comes from, and that differs by turn. A planner told to arrange one on a surface
    // offering no way to arrange it spends a round on a name that is not there, and then has a
    // refusal to explain to somebody who asked to be told when something changes.
    let (look_again_after_a_read, look_again_after_a_run) = match scheduling {
        Scheduling::ArrangingALook => (
            "Call schedule_next at the end of this turn: you are asked again after the wait with the user's line unchanged, so a request to be told when a file changes is answered by reading it now and scheduling the look that would catch a change. Do not tell somebody to arrange it.",
            "Take the first look now and call schedule_next at the end of the turn, which has you asked again after the wait with the user's line unchanged.",
        ),
        Scheduling::PacingALoop | Scheduling::TheirInterval => (
            "In a loop the next tick is the next look; there is nothing to arrange.",
            "In a loop, report this tick's look; the next tick looks again.",
        ),
        // A line the person typed while the session's own loop, goal or watch is live. Something
        // of the session's starts another turn, so the turn is not told that nothing will, and
        // the session drops any wait this turn would ask for, so it is not told to arrange one.
        // Each says what that next turn looks at, because it is not necessarily what this
        // request asked about. The loop and the watch say not to wait in the foreground for the
        // look they take; the goal does not, because GOAL-15 has a turn under a goal wait inside
        // the turn for a condition outside the session.
        Scheduling::UnderALoop => (
            "This session's loop sends its line again without this turn, and this turn does not set the wait. Where asked to watch something or be told when it changes, report what this look found and say that the loop's next turn looks again at what the loop's line asks about; where the loop's line does not ask about what was asked here, say plainly that no later look at it is coming, and do not wait in this turn for the look the loop's next turn takes.",
            "This session's loop sends its line again without this turn, and this turn does not set the wait. Where asked to watch something or be told when it changes, take this look now, report what it found and say that the loop's next turn looks again at what the loop's line asks about; where the loop's line does not ask about what was asked here, say plainly that no later look at it is coming, and do not wait in this turn for the look the loop's next turn takes.",
        ),
        Scheduling::UnderAGoal => (
            "This session's goal sends the work back for another turn while its condition is unmet, and this turn does not set the wait. Where asked to watch something or be told when it changes, report what this look found and say that the goal's next turn, while its condition is unmet, looks again only at what the condition asks about; where the condition does not ask about what was asked here, say plainly that no later look at it is coming. What the goal's instructions say about waiting for the condition inside a turn still holds.",
            "This session's goal sends the work back for another turn while its condition is unmet, and this turn does not set the wait. Where asked to watch something or be told when it changes, take this look now, report what it found and say that the goal's next turn, while its condition is unmet, looks again only at what the condition asks about; where the condition does not ask about what was asked here, say plainly that no later look at it is coming. What the goal's instructions say about waiting for the condition inside a turn still holds.",
        ),
        // The turn is not told which files the session's watches cover, and a session may hold
        // several, so neither wording asks it to compare. Where it may arm a watch, arming one of
        // the file asked about is the later look, and a path already watched arms nothing new, so
        // it is not told that no later look at that file is coming. Where it may not, it is told
        // only what the watches report on, and names no tool it is not offered (ARM-6).
        Scheduling::UnderAWatch if arming.offered() => (
            "This session's standing watches start a turn only when a file they cover changes, and this turn does not set the wait. They report on the files they cover and on nothing else. Where asked to watch something or be told when it changes, report what this look found; where what was asked about is a file, arm a standing watch of it with watch_file, which reports its change and arms nothing new where a watch already covers that file, and do not wait in this turn for that file to change; where it is not a file, or no watch of it could be armed, say plainly that no later look at it is coming.",
            "This session's standing watches start a turn only when a file they cover changes, and this turn does not set the wait. They report on the files they cover and on nothing else. Where asked to watch something or be told when it changes, take this look now and report what it found; where what was asked about is a file, arm a standing watch of it with watch_file, which reports its change and arms nothing new where a watch already covers that file, and do not wait in this turn for that file to change; where it is not a file, or no watch of it could be armed, say plainly that no later look at it is coming.",
        ),
        Scheduling::UnderAWatch => (
            "This session's standing watches start a turn only when a file they cover changes, and this turn does not set the wait. They report on the files they cover and on nothing else. Where asked to watch something or be told when it changes, report what this look found, say that this session's standing watches report a change only to the files they cover and that no later look at anything else is coming, and do not wait in this turn for a change one of them reports.",
            "This session's standing watches start a turn only when a file they cover changes, and this turn does not set the wait. They report on the files they cover and on nothing else. Where asked to watch something or be told when it changes, take this look now, report what it found, say that this session's standing watches report a change only to the files they cover and that no later look at anything else is coming, and do not wait in this turn for a change one of them reports.",
        ),
        Scheduling::NoLaterLook => (
            "Nothing asks about this line again. Where asked to watch something or be told when it changes, report this look and say plainly no later look is coming.",
            "Nothing asks about this line again. Where asked to watch something or be told when it changes, say plainly no later look is coming.",
        ),
    };
    // The two figures `run` quotes, in seconds, wherever its description names them. Written out
    // here because a description is the only place the planner can learn either: a call that names
    // nothing gets the first, and a call may raise its own deadline as far as the second.
    let (deadline_default, deadline_ceiling) =
        (deadlines.default.as_secs(), deadlines.ceiling.as_secs());
    // What read_git says to do where it will not answer: a repository or a working tree it may not
    // read, and a request none of its queries covers. git answers both and `run` is how git is
    // asked, so a turn holding one is sent there. A turn without one has no second way to that
    // history at all, and naming `run` to it costs what naming `schedule_next` on a surface
    // offering none costs: a round spent on a name that is not there, and then a question to report
    // back on unanswered.
    let (git_elsewhere, git_beyond) = if running.offered() {
        (
            "elsewhere use run",
            "For --follow, blame or anything else use run.",
        )
    } else {
        (
            "elsewhere no tool on your list reads that history",
            "--follow, blame and the rest cannot be read here: report what you could not read.",
        )
    };
    // The tools ask_user sends the planner to for a fact about this machine. `run` is named only
    // where this turn holds one; a turn without it is sent to the others alone.
    let looking_tools = if running.offered() {
        "list_files, search, read_file or run"
    } else {
        "list_files, search or read_file"
    };
    // The paragraphs of the read_file description that name another tool. The same for a turn with
    // `run` and one without; `state_what_is_beside` rewrites them for the list a turn settled on.
    let pictures = read_file_pictures(&|_| true);
    let notebook = read_file_notebook(&|_| true);
    let mut tools = vec![
        Tool::function(
            "read_file",
            format!(
                "Read a workspace file's lines, a page at a time, by path or a listing's \
             path_ref. Each read returns a change token: the same token on a later read means \
             nobody wrote the file in between, a different one means somebody did, even with \
             the same bytes, and a change that keeps the modification time moves nothing; \
             for a file you may not see, compare its size. Answer whether something changes by \
             reading it now, keeping the token and comparing it with the next look's, taking the \
             first in this turn, and name the looks, never a time \
             of day, which you have no clock for. {look_again_after_a_read} Where asked to watch \
             something or be told when it changes, with no later look scheduled, say so. Any \
             other request gets no sentence about a \
             later look.{watching} {pictures} {notebook}"
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative."
                    },
                    "output_of": {
                        "type": "integer",
                        "description": read_file_output_of(&|_| true)
                    },
                    "path_ref": {
                        "type": "string",
                        "description": "A listing's reference, instead of path."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "1-based line."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Most lines; omit to read the whole file."
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "list_files",
            format!(
                "List workspace files under a directory, recursively unless depth is given. Not \
                 entered or reported at any depth unless named as directory: {}, \
                 .claude/worktrees. Symbolic links are counted, not entered, and the result says \
                 how many; list the directory one points to by its own name. A listing stops at a \
                 time limit and says so: a directory, a depth or a pattern that starts with a \
                 directory (src/**) narrows it. Where names are quarantined, each file is a \
                 reference usable as path_ref.",
                bravebot_filetype::ignored_directories().join(", ")
            ),
            json!({
                "type": "object",
                "properties": {
                    "directory": {
                        "type": "string",
                        "description": "Workspace-relative; \".\" for the root."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Case-sensitive glob with *, ?, ** and braces, e.g. \
                                        \"src/**/*.{rs,toml}\"; {r,R}eadme* matches both cases."
                    },
                    "depth": {
                        "type": "integer",
                        "description": format!(
                            "Levels to descend; 1 is the directory itself. Each directory \
                             gives at most {per} files and {dirs} subdirectories.",
                            per = crate::workspace::MAX_FILES_PER_DIRECTORY,
                            dirs = crate::workspace::MAX_SUBDIRECTORIES_PER_DIRECTORY
                        ),
                        "minimum": 1
                    },
                    "read": {
                        "type": "boolean",
                        "description": READ_A_LISTING
                    }
                },
                "required": ["directory"]
            }),
        ),
        Tool::function(
            "write_file",
            "Write a UTF-8 workspace file, by path or a listing's path_ref, from contents or \
             contents_ref. It replaces an existing file whole, so read or list the path first if \
             one may exist. In a directory the user vouched for it applies without a prompt; \
             elsewhere the user approves it unless a permission rule or mode answers. A path_ref \
             write is always shown.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative."
                    },
                    "path_ref": {
                        "type": "string",
                        "description": "A listing's reference, instead of path."
                    },
                    "contents": {
                        "type": "string",
                        "description": "The whole text."
                    },
                    "contents_ref": {
                        "type": "string",
                        "description": write_file_contents_ref_description(&|_| true)
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "edit_file",
            edit_file_description(&|_| true),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative."
                    },
                    "path_ref": {
                        "type": "string",
                        "description": "A reference to a trusted file."
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Exact text, occurring once unless replace_all."
                    },
                    "new_text": {
                        "type": "string"
                    },
                    "replace_all": {
                        "type": "boolean"
                    },
                    "cell": {
                        "type": "integer",
                        "description": "Notebook cell from 1; with insert_after, 0 is first."
                    },
                    "cell_action": {
                        "type": "string",
                        "enum": ["replace", "insert_after", "delete"]
                    },
                    "cell_type": {
                        "type": "string",
                        "enum": ["code", "markdown"]
                    },
                    "edits": {
                        "type": "array",
                        "description": "Passages applied in order, all or none.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": {"type": "string"},
                                "new_text": {"type": "string"}
                            },
                            "required": ["old_text", "new_text"]
                        }
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "apply_checkout",
            "Copy files a delegate wrote in a checkout back to the working directory: all it \
             recorded by name, or paths. The user approves each, shown the difference. Files \
             written through a reference or by a program are not copied.",
            json!({
                "type": "object",
                "properties": {
                    "checkout": {
                        "type": "string",
                        "description": "As the report named it, e.g. \"c1\"."
                    },
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"}
                    }
                },
                "required": ["checkout"]
            }),
        ),
        Tool::function(
            "todo_write",
            "Replace the task list for multi-step work with the whole current list, finished \
             tasks included.",
            json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {
                                    "type": "string"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": TODO_STATUSES,
                                    "description": "One in_progress at a time; waiting for a \
                                                    task still open whose next step is not \
                                                    yours; cancelled for one you dropped."
                                }
                            },
                            "required": ["content", "status"]
                        }
                    }
                },
                "required": ["todos"]
            }),
        ),
        Tool::function(
            "search",
            "Find regular expressions in workspace files; returns path:line:text. Put every \
             spelling in one call. Symbolic links are counted, not entered; search the directory \
             one points to by its own name.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "description": "Regex or list of them: ., *, +, ?, |, (), (?:), [], \
                                        \\d, \\w, \\s, ^, $, \\b, (?i). No {n} or lookaround.",
                        "anyOf": [
                            {"type": "string"},
                            {"type": "array", "items": {"type": "string"}}
                        ]
                    },
                    "directory": {
                        "type": "string",
                        "description": "Directory or file. Defaults to \".\"."
                    },
                    "include": {
                        "type": "string",
                        "description": "Case-sensitive glob with *, ?, ** and braces, e.g. \
                                        \"**/*.{cc,h}\"; {r,R}eadme* matches both cases."
                    },
                    "case_sensitive": {
                        "type": "boolean"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "1-based match, as a capped result gives it."
                    },
                    "context": {
                        "type": "integer",
                        "description": "Lines around each match, at most 10."
                    },
                    "output": {
                        "type": "string",
                        "enum": ["lines", "files", "count"]
                    },
                    "read": {
                        "type": "boolean",
                        "description": READ_A_WALK
                    }
                },
                "required": ["pattern"]
            }),
        ),
        Tool::function(
            "read_git",
            format!(
                "Read git history from .git without starting git: log, show (a commit, or \
             <rev>:<path>), diff, status (--short), tags, or search (git grep -n). Only where all \
             of .git is trusted (and the working tree, for status) and .git is a directory: the \
             .git file of a linked worktree or a submodule is declined except at the root of \
             your checkout; {git_elsewhere}. git configuration is not \
             applied and no remote URL is returned. {git_beyond}"
            ),
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "enum": ["log", "show", "diff", "status", "tags", "search"]
                    },
                    "repository": {
                        "type": "string",
                        "description": "Holds .git. Defaults to \".\"."
                    },
                    "revision": {
                        "type": "string",
                        "description": "git syntax. log: one, A..B or A...B. diff: A..B, \
                                        \"A B\" or A...B. tags: those it reaches. Defaults to \
                                        HEAD."
                    },
                    "path": {
                        "type": "string",
                        "description": "Limits the query to it."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "search's regex."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Defaults to 20, at most 200."
                    },
                    "skip": {
                        "type": "integer"
                    },
                    "messages": {
                        "type": "boolean",
                        "description": "log: whole messages."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "show of <revision>:<path> only: the first line, from 1. A \
                                        show cut short gives the next offset."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "show of <revision>:<path> only: the most lines, \
                                        default and most 2000."
                    },
                    "since": {
                        "type": "string",
                        "description": "log: YYYY-MM-DD UTC."
                    },
                    "until": {
                        "type": "string",
                        "description": "log: YYYY-MM-DD UTC."
                    }
                },
                "required": ["query"]
            }),
        ),
        Tool::function(
            "repo_map",
            "Map the declarations in a directory's source files, with lines, most-referenced \
             first: Rust, Python, JS/TS, Go, Java, Kotlin, C, C++. Signatures only, never \
             bodies: read_file reads the lines it points at. Files nobody vouched for are \
             counted, not named.",
            json!({
                "type": "object",
                "properties": {
                    "directory": {
                        "type": "string"
                    },
                    "budget": {
                        "type": "integer",
                        "description": "Tokens, 100 to 8000, default 1000."
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "lsp",
            "Ask a language server about the symbol at a position you were shown. Where a \
             definition is in a dependency the line says so, and read_file will not open it. \
             hover, and diagnostics' messages, come back as references you cannot read; \
             diagnostics takes only a path and gives error lines and counts. No errors is not a \
             passing build.",
            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": [
                            "goToDefinition",
                            "findReferences",
                            "hover",
                            "documentSymbol",
                            "workspaceSymbol",
                            "goToImplementation",
                            "incomingCalls",
                            "outgoingCalls",
                            "diagnostics"
                        ]
                    },
                    "path": {
                        "type": "string",
                        "description": "Required except for workspaceSymbol."
                    },
                    "line": {
                        "type": "integer",
                        "minimum": 1
                    },
                    "character": {
                        "type": "integer",
                        "description": "Column of the name.",
                        "minimum": 1
                    },
                    "query": {
                        "type": "string",
                        "description": "workspaceSymbol's name."
                    }
                },
                "required": ["operation"]
            }),
        ),
        Tool::function(
            "spawn_processor",
            spawn_processor_description(&|name| name != "run" || running.offered()),
            json!({
                "type": "object",
                "properties": {
                    "reads": {
                        "type": "array",
                        "description": "Every reference bearing on the task.",
                        "items": {"type": "string"}
                    },
                    "about": {
                        "type": "string",
                        "description": "The reference in reads the answer replaces, and the \
                                        only file it may be written to. Required with several."
                    },
                    "choose_upto": {
                        "type": "integer",
                        "minimum": 2,
                        "maximum": 99,
                        "description": "Choose one numbered entry instead (0 for none), \
                                        returned as a reference, for a tool's _ref argument. \
                                        Not with about."
                    },
                    "instruction": {
                        "type": "string",
                        "description": "What to produce; for a file, the whole document and \
                                        nothing else, no remarks about what it did. For a \
                                        summary, translation or explanation, name the form and \
                                        length. Give the symptom, not an unverified remedy. It \
                                        knows only this and the references."
                    }
                },
                "required": ["reads", "instruction"]
            }),
        ),
        Tool::function(
            "load_skill",
            "Load a skill you were listed or search_skills found, before its kind of work, and \
             follow it.",
            json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string"
                    }
                },
                "required": ["name"]
            }),
        ),
        Tool::function(
            "search_skills",
            "Find unlisted skills whose name or description holds every word of the query.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string"
                    }
                },
                "required": ["query"]
            }),
        ),
        // ASK-12. The text does not depend on the mode. The sentences it adds say that a question
        // costs the user's attention and how to word a question for somebody who has not seen
        // what the planner read, and the `question` and `detail` arguments say the latter in a
        // shorter form. They name no mode and no place a question goes, because the planner is
        // told of no mode but plan mode (MODE-3, MODE-8) and nothing about what decides (AUTO-7).
        // The sentence that the questions are put to the user one at a time is not part of ASK-12.
        Tool::function(
            "ask_user",
            format!(
                "Ask the user up to four questions, put one at a time, and wait. Only for what you \
             cannot find out yourself; not for a fact about this machine: look with \
             {looking_tools} first; a quarantined result does not \
             stop you asking after. Do \
             not ask a question whose answer the request states. Do not ask whether to do what \
             the user asked, or whether to do it automatically, because they asked for it. What \
             is left to ask is a choice that the request leaves open and that differs in \
             consequence, such as which of two accounts to send from. Each question you ask \
             costs the user's attention. Ask everything the plan turns on in one call rather \
             than a question per turn. Offer concrete \
             options where you can; the user may also answer in their own words or skip a \
             question, and a skipped question is an answer to work with rather than a reason to \
             ask again. Write each question and option so that somebody who has not seen what \
             you read can answer it, in plain words saying what they would see or get, with no \
             issue numbers, clause IDs or internal names; the user sees only the question, its \
             options and the text of your reply before the call, not files you wrote or results \
             you read, so state there what each option does and the facts the choice rests on."
            ),
            json!({
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "header": {
                                    "type": "string",
                                    "description": "Two or three words."
                                },
                                "question": {
                                    "type": "string",
                                    "description": "The question, in one sentence, in plain \
                                                    words that somebody who has not seen what \
                                                    you read can answer, saying what they \
                                                    would see or get, with no issue numbers, \
                                                    clause IDs or internal names."
                                },
                                "options": {
                                    "type": "array",
                                    "description": "The choices, not in the question text.",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label": {
                                                "type": "string"
                                            },
                                            "detail": {
                                                "type": "string",
                                                "description": "Optional line saying what \
                                                                choosing it means, in plain \
                                                                words for somebody who has \
                                                                not seen what you read: what \
                                                                they would see or get, with \
                                                                no issue numbers, clause IDs \
                                                                or internal names."
                                            }
                                        },
                                        "required": ["label"]
                                    }
                                },
                                "multiple": {
                                    "type": "boolean",
                                    "description": "Allow several answers."
                                }
                            },
                            "required": ["header", "question"]
                        }
                    }
                },
                "required": ["questions"]
            }),
        ),
        Tool::function(
            "run",
            // Joined rather than formatted: this description states shell syntax the planner may
            // not use, `${...}` and `{a,b}` among it, and a format string would have to escape
            // every brace in it to say so.
            "Run one command line. Pipes, &&, ||, ;, ( ), quotes, redirection, globs and {a,b} \
             work, but there is no shell: $(...), backticks, $VAR, ${...}, $?, $$, $@, $*, $#, \
             $-, $! and $'...' are refused, so write values out (`NAME=$VAR cmd` and \
             `NAME=\"$(cmd)\" cmd` pass only where everything runs unasked). Single quotes \
             are literal, double quotes are not; a `$` before no name, \
             brace, parenthesis or quote is literal. Quote a pattern meant for the program: an \
             unquoted one is replaced by the files it matches (`find . -name '*.md'`), but one \
             after an option's `=` is passed as written (`--include=*.md`). A \
             `cd` step is refused since it moves nothing for later steps; the `directory` \
             argument runs a line elsewhere. A `ulimit`, `umask`, `unset`, `alias`, `unalias`, \
             `hash` or `wait` step is refused since it changes nothing for later steps; for a \
             limit or mask, write the whole line as `sh -c '... ulimit ...'`, approved as one \
             line. Narrowing inside the line is cheapest: `head -n`, `grep -l`, `grep -c`, `sed \
             -n 40,80p`; use whichever tool returns less. \
             Output is text where the user vouched for every command in the line, else a \
             reference"
                .to_string()
                + &run_reference_moves(&|_| true)
                + &run_text_made_from_it(&|_| true)
                + ". Before asking whether a browser is running, or whether to quit or relaunch \
                   it, probe its debugging port. One started with --remote-debugging-port=9222 \
                   answers `curl http://127.0.0.1:9222/json/version`. Ask about quitting or \
                   relaunching a browser only after that probe is refused. Use background: true \
                   for servers and builds that may outlast the deadline ("
                + &format!(
                    "{deadline_default} seconds by default; set deadline_seconds to allow up to \
                     {deadline_ceiling}"
                )
                + "). "
                + &run_watching(&|_| true)
                + " "
                + look_again_after_a_run
                + " Name the looks you compared, never a time of day, which you have no clock \
                   for. Where asked to watch something or be told when it changes, with no \
                   further look scheduled, say so. Any other \
                   request gets no sentence about a later look.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "One line; programs are found on PATH or relative to \
                                        directory."
                    },
                    "stdin_ref": {
                        "type": "string",
                        "description": "A reference fed to standard input without passing \
                                        through you; a program sending a processor's answer \
                                        as its body takes it here. Not with '<' or background."
                    },
                    "scopes": {
                        "type": "array",
                        "items": { "type": "string", "enum": bravebot_sandbox::scope::Requested::MENU },
                        "description": format!(
                            "Added to every stage, for a script that runs `gh`, `aws`, `git \
                             commit` or a build. A stage with a NAME=value in front of its \
                             program gets none, and a line where every stage has one is refused. \
                             macOS `loopback` lets a line listen on a local port. \
                             {LOOPBACK_CONNECT_RULE} On Linux and Windows `loopback` changes \
                             nothing. `ssh-key` re-picks the key a refused `git push` offers."
                        )
                    },
                    "unconfined": {
                        "type": "boolean",
                        "description": "No sandbox for this line, only after the same line \
                                        failed because the sandbox refused access, not for \
                                        another failure; say what in why. The user is asked \
                                        every time; a session that does not accept it \
                                        refuses the call. Not with scopes."
                    },
                    "directory": {
                        "type": "string",
                        "description": "Kept for later calls. Naming one asks the user."
                    },
                    "deadline_seconds": {
                        "type": "integer",
                        "description": format!(
                            "Default {deadline_default}, from {} to {deadline_ceiling}. One past \
                             the prompt cache lifetime rewrites the cache; prefer background: \
                             true.",
                            crate::exec::FLOOR.as_secs()
                        )
                    },
                    "background": {
                        "type": "boolean",
                        "description": run_background_description(&|_| true)
                    },
                    "read": {
                        "type": "boolean",
                        "description": run_read_description(&|_| true)
                    }
                },
                "required": ["command"]
            }),
        ),
        Tool::function(
            "job_output",
            job_output_description(&|_| true),
            json!({
                "type": "object",
                "properties": {
                    "job": {
                        "type": "string"
                    },
                    "kill": {
                        "type": "boolean",
                        "description": "Stop the job after reading."
                    },
                    "wait_seconds": {
                        "type": "integer",
                        "description": format!("From {} to {}, under the prompt cache lifetime; outside is refused. A longer wait makes the next request write the whole conversation to the cache again.",
                            crate::exec::WAIT_FLOOR.as_secs(),
                            deadlines.wait_ceiling.as_secs().max(crate::exec::WAIT_FLOOR.as_secs())
                        )
                    },
                    "until_exit": {
                        "type": "boolean",
                        "description": "Needs wait_seconds; not with kill."
                    },
                    "read": {
                        "type": "boolean",
                        "description": job_output_read_description(&|_| true)
                    }
                },
                "required": ["job"]
            }),
        ),
        Tool::function(
            "read_output",
            read_output_description(&|_| true),
            json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Byte to start from, as the sample or last page names \
                                        it."
                    },
                    "pattern": {
                        "anyOf": [
                            {"type": "string"},
                            {"type": "array", "items": {"type": "string"}}
                        ],
                        "description": "Regex, as search takes; returns matching lines by \
                                        starting byte. Not with offset."
                    }
                },
                "required": ["ref"]
            }),
        ),
        Tool::function(
            "vet_content",
            "Ask to be shown content you hold a reference to, when working blind is not \
             enough. Say plainly in your \
             reply why you need to read it rather than pass it to spawn_processor. \
             It releases this content once and vouches for nothing else.",
            json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string"
                    },
                    "expects": {
                        "type": "string",
                        "description": "In your own words, what it holds and why you want it."
                    }
                },
                "required": ["ref", "expects"]
            }),
        ),
        Tool::function(
            "spawn_agent",
            // No numbers for the depth or the ceiling, which delegates read here too: a model
            // told how many it has left spends them.
            format!(
                "Hand a separable sub-task to an agent with its own context and get one report; \
                 not for one read. A delegate whose run output you may read reports what \
                 failed.{quarantined} It cannot ask the user, so the task must hold everything. \
                 Its writes, runs and MCP calls still need the user's approval.{lacks}",
                lacks = spawn_agent_lacks(&|_| true),
                quarantined = spawn_agent_quarantined(&|_| true)
            ),
            json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "reader reads and runs processors; checker also runs \
                                        programs; worker also writes.",
                        "enum": ["reader", "checker", "worker"]
                    },
                    "task": {
                        "type": "string",
                        "description": "All it will know: it sees nothing of yours. Name \
                                        paths, commands and what to report."
                    },
                    "each": {
                        "type": "array",
                        "description": format!(
                            "One delegate per entry, told the task and its entry; at most \
                             {MAX_FANOUT} delegates per call; a further call starts more."
                        ),
                        "items": {"type": "string"}
                    },
                    "isolation": {
                        "type": "string",
                        "description": format!(
                            "A new checkout of the last commit per delegate, with your changes \
                             copied in for checkout_with_changes. Not for a reader. {}",
                            checkouts_share_refs(running)
                        ),
                        "enum": ["checkout", "checkout_with_changes"]
                    },
                    "mcp_servers": {
                        "type": "array",
                        "description": "MCP servers a worker keeps; omitted keeps all.",
                        "items": {"type": "string"}
                    },
                    "budget": {
                        "type": "string",
                        "description": "Most tokens per delegate, e.g. \"500k\" or \"2m\". Each \
                                        request counts the whole conversation, so 2m is about \
                                        thirty requests, not 2m tokens of reading."
                    },
                    "wake_after": {
                        "type": "integer",
                        "minimum": 1,
                        "description": wake_after_description(true)
                    }
                },
                "required": ["kind", "task"]
            }),
        ),
        Tool::function(
            "fetch_url",
            fetch_url_description(&|_| true),
            json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string"
                    },
                    "read": {
                        "type": "boolean",
                        "description": "Release the page here as vet_content would; kept back or over \
                                        16 KiB, it is still a reference."
                    }
                },
                "required": ["url"]
            }),
        ),
        Tool::function(
            "download_url",
            "Save an http or https URL to a workspace file, bytes unread, for binaries fetch_url \
             would corrupt. The file is quarantined. The user is asked about the host and the \
             destination first.",
            json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string"
                    },
                    "path": {
                        "type": "string"
                    }
                },
                "required": ["url", "path"]
            }),
        ),
    ];

    if running.offered() {
        tools.push(Tool::function(
            "request_path",
            "Ask the user to let programs started by run reach one path outside the workspace \
             for this session, after a run could not. A yes does not trust the directory. Never \
             a root, home or above, a credential location or a pattern; refused where the \
             sandbox is off or the workspace untrusted. The file tools are not widened, except \
             with permission prompts off, where they reach outside paths with no request, and a \
             request is granted unasked and also opens and trusts the directory for them.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "An existing absolute or ~/ path."
                    },
                    "write": {
                        "type": "boolean"
                    }
                },
                "required": ["path"]
            }),
        ));
    }

    if arming.offered() {
        tools.push(Tool::function(
            "watch_file",
            "Be told when one file changes, with no turn of yours running to notice it. This \
             arms a standing watch: the file is looked at every few seconds from now on, and the \
             first look that finds its size or modification time moved, or finds it removed or \
             newly there, begins a new turn naming this watch and this path. The watch outlives \
             this turn and every turn after it.\n\n\
             Nothing is read, now or when it fires. A fire says the path looks written to, no \
             longer exists, or now exists, and says nothing else about the file, so read it then \
             if you need what is in it. Two things follow that are worth saying to the user: a \
             write that restores the same bytes fires the watch, and a change that leaves the \
             modification time alone does not.\n\n\
             Use this where somebody asked to be told when a file changes. Use schedule_next \
             instead where what is being waited on is not one file, or where they asked for \
             their own line to be run again.\n\n\
             One file, which may not exist yet, never a directory and never a pattern, and the \
             path cannot be changed once the watch is armed. A watch lives at most a week, at \
             most 8 are live at once, and the user sees every live one: they end one with \
             /watch stop <n>, and ctrl-c ends them all.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path of the one file to watch, e.g. \
                                        src/main.rs. It may have nothing at it yet, in which \
                                        case the watch reports it appearing."
                    }
                },
                "required": ["path"]
            }),
        ));
    }

    if !scheduling.offered() {
        return tools;
    }
    let arranging = scheduling == Scheduling::ArrangingALook;
    tools.push(Tool::function(
        "schedule_next",
        if !arranging {
            "Say when this loop should run again. The user started a loop with no interval, so \
                 each turn sets the pace for the next one. Call this once, at the end of the turn, \
                 after the work is done. You are choosing only the moment: the prompt is the \
                 user's own line and it is sent again unchanged, so there is nothing here to say \
                 what the next turn asks. Pick the wait from what you are actually waiting on, not \
                 from a round number: something that takes ten minutes to change is not worth \
                 looking at in one. Once there is nothing left to watch, call it with stop true \
                 instead of a wait, which ends the loop now. A turn that calls neither is woken \
                 once more after twenty minutes before the loop ends."
        } else {
            "Arrange the next look at something, when one turn cannot answer what was asked. \
                 A request to be told when something changes is the case this exists for: take the \
                 first look now, answer from it, and call this at the end of the turn to be asked \
                 again after the wait. You are choosing only the moment. The user's own line is \
                 what gets sent again, unchanged, so there is nothing here to say what the next \
                 turn asks, and that turn is asked in the same way when to look after it. Pick the \
                 wait from what you are waiting on rather than from a round number. Do not call it \
                 where this turn has already answered the question: looking again at something \
                 settled is a loop somebody has to notice and stop."
        },
        {
            let mut parameters = json!({
            "type": "object",
            "properties": {
                "delay_seconds": {
                    "type": "integer",
                    "description": "How long to wait before the next run. Held to between \
                                    1 and 3600 seconds, so anything outside that becomes \
                                    the nearest of the two. The wait starts when this turn \
                                    ends, so a whole turn separates two looks however short \
                                    it is."
                },
                "reason": {
                    "type": "string",
                    "description": "What you are waiting on, in a few words, e.g. \"watching \
                                    the release build\". The user reads this; it is not sent \
                                    anywhere and nothing acts on it."
                },
                "noop": {
                    "type": "boolean",
                    "description": "True when this run found nothing to do and changed \
                                    nothing, false when something happened worth keeping: an \
                                    edit, a message, a finding. Runs of quiet ticks are \
                                    counted, so the user can see the loop is healthy without \
                                    reading every one."
                }
            },
            "required": ["delay_seconds", "noop"]
            });
            // Only a tick of a self-paced loop can end the loop it belongs to, and it ends it
            // without a wait, so the wait stops being required beside it (SCHED-4).
            if !arranging {
                parameters["properties"]["stop"] = json!({
                    "type": "boolean",
                    "description": "True when the work this loop watches is finished and there \
                                    is nothing left to look at: the loop ends now, with no \
                                    further tick. delay_seconds is not needed with it. It can \
                                    only end this loop, never start or change one."
                });
                parameters["required"] = json!(["noop"]);
            }
            parameters
        },
    ));
    tools
}

/// Ask for the reason a call is made, on every tool alike.
///
/// Required, because a model asked for a reason in prose mostly sends calls without one, and a
/// person watching sees every call and otherwise nothing of what it was for. Content rather than
/// routing (TOOL-5): no tool reads it, so it is added here once rather than written into each
/// schema.
fn ask_why(tool: &mut Tool) {
    let parameters = &mut tool.function.parameters;
    parameters["properties"][WHY] = json!({
        "type": "string",
        "description": "Why you make this call."
    });
    // A tool with no required argument of its own gets a list of one. Indexing the missing key to
    // write through it would store `null` there, which Bedrock refuses as a schema for the whole
    // request.
    if let Some(required) = parameters
        .as_object_mut()
        .map(|schema| schema.entry("required").or_insert_with(|| json!([])))
        .and_then(|required| required.as_array_mut())
    {
        required.push(json!(WHY));
    }
}

/// The tools a delegate of one kind is offered.
///
/// Filtered from the one table rather than written again, so a schema improved for the planner is
/// improved for a delegate on the same line. What a capability reaches is the rule, because a tool
/// offered to a run whose gates refuse it on every call is a tool the model has to be told to
/// ignore.
///
/// Five are left out by name, each for its own reason:
///
/// - `ask_user`, because a delegate's task came from a planner rather than from the person, so a
///   question about it asks somebody to arbitrate something they never set up.
/// - `todo_write`, because the list on the screen belongs to the turn the person is watching, and
///   a delegate writing to it would replace what they were reading with the steps of a sub-task
///   they did not ask about.
/// - `schedule_next`, because what it schedules is another turn of the session, sending the line
///   the person typed. A delegate was given a task by a planner and ends when it answers, so a
///   wait it asked for would either be dropped or would put the session back to work on something
///   nobody at the keyboard is waiting for.
/// - `vet_content`, because what crosses back from a delegate is its own set of rules and a
///   delegate has nobody watching it. The prompt it would draw belongs to the person who set the
///   sub-task going, about content they never asked to see, in the middle of work they are not
///   reading.
/// - `fetch_url`, because every kind holds the capability for reaching the network so the driver
///   can make its model call, and that is the whole of what it buys. A delegate pointing a
///   request at a host of its own would be egress nobody approved for this sub-task, and the
///   person shown the host would be answering for a task they never set. The capability being
///   held is what makes this one a name and not a capability check: no gate would refuse it.
///
/// The planner writes a delegate's task, so the description of `spawn_agent` tells it that a
/// delegate cannot call `vet_content`, `ask_user`, `fetch_url` or `download_url`
/// ([`spawn_agent_lacks`]).
///
/// `spawn_agent` is offered by where the delegate sits rather than by what it is: `delegating` is
/// the kinds it may start, and `None` is a delegate already [`MAX_DEPTH`] below the turn. The
/// kernel refuses one that asks there anyway, so this only spares the model a tool that cannot
/// work.
///
/// Two terms rather than one, and they are visible as two: the capability is the gate, and the
/// definition's list is a confinement inside it that can only ever remove a name. `None` is a
/// definition that named no tools, which is the kind's own set and is what every delegate had
/// before definitions existed.
///
/// [`MAX_DEPTH`]: bravebot_core::delegate::MAX_DEPTH
pub fn for_delegate(
    capabilities: &bravebot_core::capability::CapabilitySet,
    confined_to: Option<&[String]>,
    delegating: Option<&bravebot_core::delegate::Definitions>,
    deadlines: crate::exec::Deadlines,
) -> Vec<Tool> {
    use bravebot_core::delegate::{NEVER_DELEGATED, gating_capability};

    let offers = |name: &str| {
        if NEVER_DELEGATED.contains(&name) || !bravebot_core::tool_set::allows(name) {
            return false;
        }
        if name == "spawn_agent" && delegating.is_none() {
            return false;
        }
        if !gating_capability(name).is_some_and(|needs| capabilities.contains(&needs)) {
            return false;
        }
        // Applied after the capability rather than instead of it. A definition subtracts from
        // what its kind reaches and never adds, so a name here that the gate above dropped is a
        // name this delegate loaded without.
        confined_to.is_none_or(|named| named.iter().any(|tool| tool == name))
    };

    // Asked before the table is written as well as of each name in it, because a description that
    // says to use `run` is wrong on a list that does not hold one, and the question is the same
    // question either way round.
    let running = if offers("run") {
        Running::Offered
    } else {
        Running::Withheld
    };
    // NoLaterLook rather than ArrangingALook: a delegate is offered no `schedule_next` and nothing
    // asks it again, so the read and run descriptions say that instead of sending it to a tool its
    // list does not hold.
    let mut tools: Vec<Tool> = available(
        Scheduling::NoLaterLook,
        crate::watch::Arming::Unavailable,
        deadlines,
        running,
    )
    .into_iter()
    .filter(|tool| offers(tool.function.name.as_str()))
    .collect();
    if let Some(delegates) = delegating {
        // The kinds this delegate's own capabilities reach, so a checker is not offered a worker
        // it could only start narrowed to what it holds itself (DELEGATE-32).
        offer_kinds(&mut tools, &delegates.offered_to(capabilities));
        // A typed line never ends a delegate's wait (DELEGATE-17), so its `wake_after` does not
        // say one does.
        if let Some(wake_after) = tools
            .iter_mut()
            .find(|tool| tool.function.name == "spawn_agent")
            .and_then(|spawn| spawn.function.parameters.get_mut("properties"))
            .and_then(|properties| properties.get_mut("wake_after"))
        {
            wake_after["description"] = json!(wake_after_description(false));
        }
    }
    tools
}

/// What `spawn_agent`'s `wake_after` is told (DELEGATE-17). `typed_lines_wake` is false for a
/// delegate's list, whose wait no typed line ends.
fn wake_after_description(typed_lines_wake: bool) -> String {
    let also = match typed_lines_wake {
        true => " A typed line or another call's count also wakes you.",
        false => " Another call's count still wakes you.",
    };
    format!("Delegates that must end before you are asked again; default 1.{also}")
}

/// The tools a turn offers its planner, with the kinds of delegate this turn resolved.
///
/// The schema is otherwise the table's own, and this replaces one field of one tool: which names
/// `spawn_agent` accepts, and what each of them is for. Built here rather than threaded through
/// [`available`] because every other tool is the same whatever a person has written down.
///
/// `running` is the planner's own `Offered` on an ordinary turn, which holds the whole table. A turn
/// addressed to a definition is narrowed by a list read after this is built, so its caller asks
/// again with `Withheld` where that narrowing took `run` away.
pub fn for_planner(
    scheduling: Scheduling,
    arming: crate::watch::Arming,
    delegates: &bravebot_core::delegate::Definitions,
    deadlines: crate::exec::Deadlines,
    running: Running,
) -> Vec<Tool> {
    let mut tools = available(scheduling, arming, deadlines, running);
    offer_kinds(&mut tools, delegates);
    tools
}

/// Add the `advisor` tool to a planner's list, for a session that named an advisor model.
///
/// Built here rather than in the table because no session is offered it by default: it exists only
/// where somebody chose a model to consult, and a description naming a tool that is not on the
/// list sends the planner to a name that is not there.
pub fn offer_advisor(tools: &mut Vec<Tool>) {
    let mut advisor = Tool::function(
        "advisor",
        "Ask a stronger model for guidance. The advisor is given this whole conversation, \
         exactly as you have it, and the question you write; it has no tools and cannot look \
         at anything you have not. It returns advice as text, which is yours to weigh and act \
         on. Use it before committing to an approach you are unsure of, when you are stuck, or \
         before calling the work done. It costs a request to a larger model, and a turn may ask \
         it only a few times, so put the whole question in one call rather than several.",
        json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "What you want the advisor's view on, written so that it \
                                    can be answered from the conversation alone."
                }
            },
            "required": ["question"]
        }),
    );
    ask_why(&mut advisor);
    tools.push(advisor);
}

/// Add `propose_plan` to a planner's list, for a turn that begins in plan mode.
///
/// Built here rather than in the table because only plan mode has a plan to approve, and a
/// description naming a tool that is not on the list sends the planner to a name that is not
/// there. A delegate is built from the table and so is never offered it.
pub fn offer_propose_plan(tools: &mut Vec<Tool>) {
    let mut propose = Tool::function(
        "propose_plan",
        "Present the finished plan to the user and ask them to approve it. Call it once the plan \
         is settled, instead of ending your reply with the plan. The user chooses whether to \
         carry it out with edits accepted, to carry it out asking before each edit, or to keep \
         planning; the answer comes back as the result, and on approval plan mode has ended for \
         the rest of the turn so you can begin the work. If they keep planning, revise from what \
         they said. It is refused where nobody can be asked.",
        json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "The whole plan, written so the user can approve it as it \
                                    stands: the steps, the files each touches, and why."
                }
            },
            "required": ["plan"]
        }),
    );
    ask_why(&mut propose);
    tools.push(propose);
}

/// Add `remember` to a planner's list (RECALL-5).
///
/// Built here rather than in the table because a delegate is never offered it, and a description
/// naming a tool that is not on the list sends the planner to a name that is not there. It takes
/// literal text and no reference to a data slot, so nothing quarantined can be kept (RECALL-8).
pub fn offer_remember(tools: &mut Vec<Tool>) {
    let mut remember = Tool::function(
        "remember",
        "Keep one fact about the user or their work for later sessions, in your own words, \
         not copied from a file or web page. The user is shown each save and can undo it. A \
         fact close to a kept one is refused with its key: save over that key, pass supersedes, \
         or pass new.",
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Lower-case hyphenated words, grouped with slashes."
                },
                "scope": {
                    "type": "string",
                    "enum": ["everywhere", "project"]
                },
                "text": {
                    "type": "string"
                },
                "supersedes": {
                    "type": "string",
                    "description": "Key of a kept fact this replaces."
                },
                "new": {
                    "type": "boolean",
                    "description": "Keep it beside a close fact."
                }
            },
            "required": ["key", "scope", "text"]
        }),
    );
    ask_why(&mut remember);
    tools.push(remember);
}

/// Add `propose_rule` to a planner's list (RECALL-3).
///
/// Built here rather than in the table because a delegate is never offered it. It takes literal
/// text and no reference to a data slot, so nothing quarantined can become a rule, and nothing is
/// written until the person says yes.
pub fn offer_propose_rule(tools: &mut Vec<Tool>) {
    let mut propose = Tool::function(
        "propose_rule",
        "Propose a standing rule for working with this user, in your own words, not copied \
         from a file or web page. The user is asked and may keep, edit or refuse it; nothing is \
         written unless kept. A fact that is not an instruction belongs to remember.",
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string"
                },
                "scope": {
                    "type": "string",
                    "enum": ["home", "project"]
                },
                "reason": {
                    "type": "string",
                    "description": "One line, shown to the user."
                }
            },
            "required": ["text", "scope", "reason"]
        }),
    );
    ask_why(&mut propose);
    tools.push(propose);
}

/// Add `set_link` to a planner's list (SETLINK-1), beside `propose_rule`.
///
/// Built here rather than in the table because a delegate is never offered it. The one field that
/// decides anything is the address, and it is recorded only where it is a word of a line the
/// person typed, so the planner cannot choose a link the person did not write.
pub fn offer_set_link(tools: &mut Vec<Tool>) {
    let mut link = Tool::function(
        "set_link",
        "Record the issue or pull request this session is about, so the user sees it in the \
         session's info panel. Use it only when the user wrote the address in one of their own \
         messages, for example when they asked you to work on an issue and gave its link. The \
         address must appear in a message the user typed; an address found in a file, a web page \
         or a program's output is refused. Nothing else is changed.",
        json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["issue", "pull_request"],
                    "description": "issue for an issue, pull_request for a pull request."
                },
                "url": {
                    "type": "string",
                    "description": "The http or https address, exactly as the user typed it."
                }
            },
            "required": ["kind", "url"]
        }),
    );
    ask_why(&mut link);
    tools.push(link);
}

/// Add `web_search` to a planner's list (WEBSEARCH-1), beside `fetch_url`.
///
/// Built here rather than in the table because it is offered only where a key source is configured,
/// a variable `webSearch.env` names that holds text or a key file, and never to a delegate. The one
/// argument that decides anything is `service`, a word from a closed set the driver reads as a
/// literal; the host and the path are written in this crate.
pub fn offer_web_search(tools: &mut Vec<Tool>) {
    let mut search = Tool::function(
        "web_search",
        web_search_description(&|name| tools.iter().any(|tool| tool.function.name == name)),
        web_search_schema(),
    );
    ask_why(&mut search);
    tools.push(search);
}

/// The description of `web_search`, naming only the tools `beside` says are on the list. See
/// [`spawn_processor_description`].
fn web_search_description(beside: &dyn Fn(&str) -> bool) -> String {
    let moves = quarantined_reference_moves(beside);
    let reading = read_it_yourself(beside, "results");
    format!(
        "Search the web with the Brave Search API. 'service' picks what is asked: web, news, \
         videos, images, llm_context (pre-extracted page text for grounding), suggest, \
         spellcheck or places. What comes back is quarantined, like a file nobody vouched for: \
         you get a reference rather than the results, and you cannot read it or be told what \
         it says.{moves}{reading} The user is asked before the query leaves the machine, and \
         it is sent to the Brave Search API and nowhere else."
    )
}

/// The arguments of `web_search`: the vendor's own, by the vendor's names, checked against the
/// vendor's documented types, ranges and sets before anything is sent.
fn web_search_schema() -> serde_json::Value {
    let words = |set: &[&str]| json!({"type": "string", "enum": set});
    let number = |least: u64, most: u64, what: &str| json!({"type": "integer", "minimum": least, "maximum": most, "description": what});
    let flag = |what: &str| json!({"type": "boolean", "description": what});
    json!({
        "type": "object",
        "properties": {
            "service": {
                "type": "string",
                "enum": crate::web_search::Service::NAMES,
                "description": "Which search to make. llm_context returns page text prepared \
                                for a model. places lists points of interest in an area, and is \
                                the one service that takes no q."
            },
            "q": {
                "type": "string",
                "description": "The query. At most 600 characters and 75 words for web and \
                                llm_context, 400 and 50 for the others. Not taken by places \
                                where only an area is wanted."
            },
            "country": {"type": "string", "description": "Two-letter country the results come \
                from, such as US or GB, or ALL. Not every country is accepted."},
            "search_lang": {"type": "string", "description": "Language of the results, such as \
                en, fr, de or pt-br. On suggest and spellcheck this is a hint."},
            "ui_lang": {"type": "string", "description": "Preferred language of the response, \
                as a locale such as en-US. Not taken by images, llm_context, suggest or \
                spellcheck."},
            "count": number(1, 200, "How many results. The most is 20 for web and suggest, 50 \
                for news, videos and llm_context, 100 for places, 200 for images. Not taken by \
                spellcheck."),
            "offset": number(0, 9, "Zero-based page, in units of count. Taken by web, news and \
                videos."),
            "safesearch": words(&["off", "moderate", "strict"]),
            "spellcheck": flag("Whether the query is spell-checked first. Defaults to true."),
            "freshness": {"type": "string", "description": "pd for the last day, pw for the \
                last week, pm for the last month, py for the last year, or a range written \
                YYYY-MM-DDtoYYYY-MM-DD."},
            "text_decorations": flag("Whether display strings carry highlight markers. Web \
                only."),
            "result_filter": {"type": "string", "description": "Web only. Comma-separated result \
                types to keep, from discussions, faq, infobox, news, query, summarizer, videos, \
                web, locations."},
            "units": words(&["metric", "imperial"]),
            "extra_snippets": flag("Up to five more excerpts per result. Web and news."),
            "operators": flag("Whether search operators in the query are applied."),
            "operators_fallback": flag("Try relaxed operator matching first, and fall back to \
                the original if nothing matches."),
            "include_fetch_metadata": flag("Include fetch metadata. Web, news and videos."),
            "maximum_number_of_urls": number(1, 50, "llm_context only: the most distinct URLs \
                in the context."),
            "maximum_number_of_tokens": number(1024, 32768, "llm_context only: approximate most \
                tokens in the context. Defaults to 8192."),
            "context_threshold_mode": words(&["disabled", "strict", "lenient", "balanced"]),
            "maximum_number_of_tokens_per_url": number(512, 8192, "llm_context only: most \
                tokens taken from one URL."),
            "latitude": {"type": "number", "minimum": -90, "maximum": 90, "description": "places \
                only. Give with longitude, instead of location."},
            "longitude": {"type": "number", "minimum": -180, "maximum": 180, "description": "places \
                only. Give with latitude, instead of location."},
            "location": {"type": "string", "description": "places only: an area written as the \
                city and country, such as 'tokyo japan' or 'san francisco ca united states'. \
                Instead of latitude and longitude."},
            "radius": number(0, 40075000, "places only: metres, a bias towards results nearer \
                the coordinates and not a strict limit."),
            "read": {
                "type": "boolean",
                "description": "Ask to have the results checked and released in this result, \
                                as vet_content would, so there is no vet_content call to make. \
                                Where it would be kept back, or the results are longer than one \
                                result may hold, 16 KiB by default, they still come back as a \
                                reference. Defaults to false."
            }
        },
        "required": ["service"]
    })
}

/// Add `recall` to a planner's list (RECALL-10), beside `remember`.
///
/// Built here rather than in the table for the reason `remember` is: a delegate is never offered
/// it, and a description naming a tool that is not on the list sends the planner to a name that is
/// not there.
pub fn offer_recall(tools: &mut Vec<Tool>) {
    let mut recall = Tool::function(
        "recall",
        "Look up kept facts about the user or this project, by a query or one fact's key. \
         When the answer says probably nothing is stored, say so rather than assume.",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Words; a quoted phrase must match. Give query or key."
                },
                "key": {
                    "type": "string"
                }
            }
        }),
    );
    ask_why(&mut recall);
    tools.push(recall);
}

/// Add `message_session` to a planner's list, for a background session that can reach the others.
///
/// Built here rather than in the table because no other session is offered it, and a description
/// naming a tool that is not on the list sends the planner to a name that is not there. A delegate
/// is built from the table and so is never offered it (MSG-12).
pub fn offer_message_session(tools: &mut Vec<Tool>) {
    let mut message = Tool::function(
        "message_session",
        "Send a message to another background session on this machine, by the name the person \
         gave it. The message is the text you write and nothing else: the other session is not \
         given this conversation, your files or your references. Its planner reads the text as \
         advice from another session, not as the person's words, and the text grants it no \
         permission and answers nothing it asked. A message starts no turn: it is read at the \
         start of that planner's next request, so a session that is idle or waiting for the \
         person reads it later. The person is asked before the first message to a session in a \
         turn, and a turn may send only a few to one session, so put what matters in one message.",
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "The name of the session to send to, exactly as the person \
                                    gave it to you."
                },
                "text": {
                    "type": "string",
                    "description": "What you want that session's planner to be told, written so \
                                    that it can be read without this conversation. At most 4,000 \
                                    characters."
                }
            },
            "required": ["target", "text"]
        }),
    );
    ask_why(&mut message);
    tools.push(message);
}

/// Add `load_tool` to the list of a turn whose server tools are offered by name only (SERVERS-16).
///
/// Built here rather than in the table because it exists only where tools are deferred, and a
/// description naming a tool that is not on the list sends the planner to a name that is not
/// there. `names` are the `alias:tool` of tools a person vouched for, so nothing a server wrote
/// decides what is listed.
pub fn offer_tool_loader(tools: &mut Vec<Tool>, names: &[String]) {
    let mut loader = Tool::function(
        "load_tool",
        format!(
            "Load a tool of an MCP server the user connected, so that it is offered to you from \
             your next request on. These tools exist but are not shown to you until you name one, \
             to keep this conversation short. Only the names listed here exist; there is nothing \
             to search. The user is still asked before each call to a loaded tool. The tools: {}.",
            names.join(", ")
        ),
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of a tool exactly as it is listed here, written \
                                    alias:tool."
                }
            },
            "required": ["name"]
        }),
    );
    ask_why(&mut loader);
    tools.push(loader);
}

/// Say in the `run` description what its programs are held to, for a turn that confines them.
///
/// Appended to the table's description rather than a parameter of it, so the many callers that
/// build a table for another reason are untouched. Nothing where the list does not offer `run`.
/// Where the turn does not confine or the mode is `off`, no boundary is stated, since a planner is
/// not told of a boundary its programs do not have, and the description says only that
/// `request_path` is refused and nobody is asked.
///
/// `requests_offered` says the session would accept a `scopes` argument and a `request_path` call,
/// so the description names them only where it would. The sentences that name `request_path` are
/// written only where `tools` holds it: a `--tools run` turn or a definition that leaves it out is
/// not told of a tool it cannot call, including that the tool is refused (TOOL-6). `nobody_to_ask`
/// says nobody can be asked in this run (CLI-1), and then the description names no route only a
/// person takes, as the sentence after a failed step does not (SANDBOX-19).
pub fn state_confinement(
    tools: &mut [Tool],
    confine_runs: bool,
    sandbox: bravebot_sandbox::SandboxMode,
    requests_offered: bool,
    nobody_to_ask: bool,
) {
    let request_path_listed = tools
        .iter()
        .any(|tool| tool.function.name == "request_path");
    let Some(stated) = crate::confine::stated_in_the_description_for(
        sandbox,
        crate::confine::RunStatement {
            confine_runs,
            requests_offered,
            request_path_listed,
            askable: crate::confine::Askable::nobody_when(nobody_to_ask),
        },
    ) else {
        return;
    };
    if let Some(run) = tools.iter_mut().find(|tool| tool.function.name == "run") {
        run.function.description.push(' ');
        run.function.description.push_str(&stated);
    }
}

/// Say in the `run` description which variables its programs are not handed (RUN-12).
///
/// Appended to the table's description for the reason [`state_confinement`] is. Nothing where the
/// list does not offer `run`, and nothing where the filtering is switched off: a planner is not told
/// of a removal that does not happen.
///
/// Only the names fixed in the source are listed, from [`bravebot_config::scrub::builtin_credentials`].
/// A name a settings file wrote is removed from the program and never said here: an entry in a
/// provider's `env` is a free-form string that may be a key pasted into the wrong field, and
/// `run.scrubEnv` is read from the person's own file alone. The statement says such variables
/// exist, which is what lets a planner read an unset report from one of them correctly.
///
/// A fact about this machine and what follows from it, with no instruction. Without the fact a
/// program reporting one of these unset reads as a gap in the person's setup, and a planner reports
/// it as one.
pub fn state_withheld_variables(tools: &mut [Tool]) {
    state_withheld(tools, &bravebot_config::scrub::builtin_credentials());
}

/// [`state_withheld_variables`], for the names it is handed.
pub(super) fn state_withheld(tools: &mut [Tool], names: &[&str]) {
    if names.is_empty() {
        return;
    }
    if let Some(run) = tools.iter_mut().find(|tool| tool.function.name == "run") {
        run.function.description.push(' ');
        run.function
            .description
            .push_str(&withheld_variables_sentence(names));
    }
}

/// What [`state_withheld`] appends.
fn withheld_variables_sentence(names: &[&str]) -> String {
    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    format!(
        "The agent's own credentials ({}), and unlisted variables the person's settings name as \
         a gateway token, search key or to be withheld, are removed from every program this tool \
         starts, so a program reporting one unset says nothing about whether the person set it.",
        listed.join(", ")
    )
}

/// Say in the `read_output` description who answers a read of what a run printed, for the session
/// as it stands when the request is built (RUN-14).
///
/// `answer` is the sentence for that case. It is appended to the table's description for the
/// reason [`state_confinement`] is, and nothing where the list does not offer `read_output`, so a
/// turn left without it is not pointed at a tool it has not got. Called on each request's copy of
/// the list and not on the list itself, so a mode the person changes in the middle of a turn is
/// what the next request says, and a statement is never appended twice.
pub fn state_read_output_answer(tools: &mut [Tool], answer: &str) {
    if let Some(read_output) = tools
        .iter_mut()
        .find(|tool| tool.function.name == "read_output")
    {
        read_output.function.description.push(' ');
        read_output.function.description.push_str(answer);
    }
}

/// Say in the `vet_content` description who answers and what follows from it, for the session as
/// it stands when the request is built (VET-5).
///
/// `answer` is the sentence for that case. Like [`state_read_output_answer`] it is called on each
/// request's copy of the list, appends nothing where the list does not offer `vet_content`, and so
/// cannot state a mode twice or point a turn at a tool it has not got.
pub fn state_vet_content_answer(tools: &mut [Tool], answer: &str) {
    if let Some(vet_content) = tools
        .iter_mut()
        .find(|tool| tool.function.name == "vet_content")
    {
        vet_content.function.description.push(' ');
        vet_content.function.description.push_str(answer);
    }
}

/// Replace which names `spawn_agent` accepts, and what each is for, with the kinds this turn
/// resolved. Nothing where the list does not offer the tool.
fn offer_kinds(tools: &mut [Tool], delegates: &bravebot_core::delegate::Definitions) {
    let Some(spawn) = tools
        .iter_mut()
        .find(|tool| tool.function.name == "spawn_agent")
    else {
        return;
    };
    let Some(kind) = spawn
        .function
        .parameters
        .get_mut("properties")
        .and_then(|properties| properties.get_mut("kind"))
    else {
        return;
    };

    // Name and description together, because a name on its own says nothing about when to pick
    // it. Both are the definition's own words, from a file that passed the trusted-content gate
    // or from this program: a name nobody vouched for never entered the set. A checkout is said
    // beside them, since it decides which tree the delegate's report is about.
    let described = delegates
        .iter()
        .map(|definition| {
            let apart = match definition.asks_for_checkout() {
                true => {
                    " Its delegates work in a checkout of the last commit, which holds no change \
                     not yet committed unless the call sets isolation to \
                     \"checkout_with_changes\"."
                }
                false => "",
            };
            // The kind's own figure, so a task can be sized to it. Not the definition's
            // `rounds:` and not the kind's ceiling, which the person who wrote the file owns.
            format!(
                "\n- {}: {} (limit: {} rounds; a round is one reply with its tool calls){apart}",
                definition.name(),
                definition.description(),
                definition.kind().rounds()
            )
        })
        .collect::<String>();
    kind["description"] = json!(format!("Which kind, narrowest first.{described}"));
    kind["enum"] = json!(delegates.names());
}
