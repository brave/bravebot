//! The tool set offered to the model (the schema is in `offered`), and dispatch for the calls it
//! makes.
//!
//! Only non-destructive tools are exposed. A model-proposed path may become routing for
//! a confined read (see `Policy::promote_confined_read`), which is what makes iteration
//! possible; nothing here can change the workspace, so a wrong choice costs a step
//! rather than causing harm.
//!
//! Writing is present but gated differently. A write destination chosen by the model is
//! routing derived from whatever it just read, so it is never promoted on its own: the
//! user is shown the change and must approve, and that approval mints a single-use
//! endorsement bound to the exact path. A refusal, or a context where nobody can be asked,
//! means no write.
//!
//! `edit_file` exists because that approval has to be readable. A whole-file body cannot be
//! reviewed on a terminal, so an edit names a passage instead and the user approves a diff.
//! Locating the passage is an ordinary confined read; only the write that follows needs the
//! endorsement. Where the passage is ambiguous the edit is refused rather than guessed,
//! since a guess would mutate bytes that were never shown to anyone.
//!
//! `spawn_processor` is how work happens in a file the planner may not read. It hands
//! quarantined content to a model that holds nothing: no tools, no memory, no second round,
//! and one quarantined output. What comes back is a reference like any other, and
//! `write_file`'s `contents_ref` is what puts it in a file. Between them, the planner can
//! change a file it never saw and the driver can write bytes it never opened.

use crate::confirm::{Confirmer, Decision, Intent, Remark, WriteDecision, WriteRequest};
use crate::diff::Diff;
use crate::processor::{self, Chat};
use crate::report::{Activity, CallId, Reporter};
use bravebot_aichat::protocol::{Tool, ToolCall, Usage};
use bravebot_core::ask::{self, Choice, Question, Series};
use bravebot_core::credentials::Scanned;
use bravebot_core::delegate::CheckoutRefusal;
use bravebot_core::event::{RecordingSink, Sink};
use bravebot_core::label::Label;
use bravebot_core::policy::{Containment, Destination, Policy, RunRequestCause};
use bravebot_core::slot::{SlotId, SlotStore};
use bravebot_core::todo::{self, Item, List, Status};
use bravebot_core::value::Labelled;
use bravebot_core::vetting::{Endorsed, Verdict};
use serde_json::{Value, json};

use crate::lsp::LanguageServers;
use crate::workspace::{Listing, Page, Paging, Workspace};

mod link;
mod message;
mod recall;
mod web_search;
use link::set_link;
use recall::{propose_rule, recall, remember};
use web_search::web_search;
mod offered;

use offered::checkouts_share_refs;
#[cfg(test)]
use offered::state_withheld;
pub use offered::{
    Beside, Named, Running, Scheduling, available, for_delegate, for_planner, offer_advisor,
    offer_message_session, offer_propose_plan, offer_propose_rule, offer_recall, offer_remember,
    offer_set_link, offer_tool_loader, offer_web_search, state_confinement,
    state_read_output_answer, state_vet_content_answer, state_what_is_beside,
    state_withheld_variables,
};

/// The argument every tool takes saying why the call is being made.
pub const WHY: &str = "why";

/// A read a tool decided not to perform yet.
///
/// The planner asked for a file whose contents it may not see, so there is nothing to show it
/// and no reason to have the bytes in hand. What travels back is the path and the size, and the
/// slot the turn reserves from them holds the file until something needs it.
///
/// The path is the promoted one, still labelled, because the kernel checks it as routing again
/// when it reserves the slot.
#[derive(Debug, Clone)]
pub struct Deferral {
    pub path: Labelled<String>,
    /// What the planner is told the reference is of. The path itself where the planner named
    /// it, since it is its own words coming back.
    pub origin: String,
    pub bytes: usize,
}

/// A listing the planner may not read, reserved one reference per entry.
///
/// The names stay wrapped: this crate carries them from the lister to the kernel and never
/// looks. `count` is how many entries there are, files and the directories a bounded walk stopped
/// at together, and it is released to the planner and the person alike, because how much a
/// directory holds is shape rather than content.
#[derive(Debug)]
pub struct Entries {
    /// What to call each one to the planner, naming the directory and never the file.
    pub origin: String,
    pub paths: Labelled<Vec<String>>,
    pub count: usize,
    /// How many of the last `paths` are directories a bounded walk stopped at rather than files
    /// in it. Shape like `count`, and released for the same reason, though the planner reads it
    /// one reference at a time rather than as a number: handed a listing of files alone it sees a
    /// tree with no branches.
    pub directories: usize,
    /// How many files and how many subdirectories were left out of each directory a per-directory
    /// cap cut (LIST-5), in the order of the last of `paths`, which are those directories. Counts
    /// of directory entries, released with `count`, so the turn can say beside the references how
    /// much each one holds that is not listed.
    pub left_out: Vec<(usize, usize)>,
    /// Whether the listing reached the cap over the whole walk, as against leaving files or
    /// subdirectories out by the per-directory caps alone. Shape like `count`, from the same
    /// release.
    pub at_the_cap: bool,
}

/// Content a result carries beside its own text, under a label of its own.
///
/// A result is shown or quarantined whole by the label on its text. Where one answer holds both a
/// part the driver wrote and a part a file or a server chose, the second travels here: the turn
/// puts it through the same gate as any other result, so the planner reads the first and is handed
/// a reference to the second.
#[derive(Debug)]
pub struct Apart {
    /// What the planner is told the reference is of. A file's name only where the planner
    /// proposed it itself, never one that a server or a file supplied.
    pub origin: String,
    pub text: Labelled<String>,
    /// The driver's sentence that introduces whatever the planner is handed in its place.
    pub introduced_by: &'static str,
}

/// The number a processor chose from a list, still labelled, and the limit the planner wrote.
///
/// Carried from the tool to the turn and stored there. Nothing in the driver reads the number.
#[derive(Debug)]
pub struct ChosenNumber {
    /// The number, labelled as the processor's answer was.
    pub number: Labelled<u8>,
    /// The largest number the planner said the processor could choose.
    pub upto: u8,
}

impl Output {
    /// Whether a model request completed for this call: it waited on the model and was not
    /// stopped while it did (SESSION-40).
    pub(crate) fn requested(&self) -> bool {
        self.inference_interval.is_some() && self.cancelled.is_none() && !self.unanswered
    }
}

/// What a dispatched call produced, ready to send back as a tool message.
#[derive(Debug)]
pub struct Output {
    /// Structured cancellation from a processor, never reconstructed from tool-result text.
    pub cancelled: Option<crate::outcome::Cancellation>,
    pub call_id: Option<String>,
    /// The id dispatch gave this call on its start and finish lines, so the line saying where the
    /// result went can name the row it belongs to.
    pub reported_as: Option<CallId>,
    pub tool: String,
    /// The rendered result, still labelled.
    ///
    /// Deliberately not a plain `String`: whether the planner may see this is the kernel's
    /// decision, made from the label in `Policy::present`. A tool that could hand back bare
    /// text would be a tool that decided for itself, and untrusted workspace content would
    /// reach the planner's context by whichever tool forgot.
    pub text: Labelled<String>,
    /// Where the content came from, for the reference the planner is shown instead.
    pub origin: String,
    /// A file this call reserved rather than read. `text` is empty where this is set: there is
    /// nothing to present, and the turn reserves the slot instead.
    pub deferred: Option<Deferral>,
    /// The entries of a listing this call reserved, one reference each.
    pub entries: Option<Entries>,
    /// Whether a cap or a directory that could not be opened cut the result short, so the turn can
    /// say so beside the reference it hands over. Text inside a quarantined result reaches nobody
    /// who could act on it. A listing's clock is `timed_out` and not part of this.
    pub incomplete: bool,
    /// Whether a listing's walk stopped at the time budget, beside the reference for the reason
    /// `incomplete` is. Independent of `incomplete`: a listing both cut by a cap and stopped by the
    /// clock has both set. A fact about the walk and never a name (LIST-4).
    pub timed_out: bool,
    /// How many symbolic links the walk behind this result met and did not enter. A number and
    /// never a name, beside the reference for the reason `incomplete` is: a notice written into a
    /// quarantined body reaches nobody who could act on it.
    pub links: usize,
    /// The driver's sentence for a listing whose `pattern` selected no file, written beside the
    /// reference for the reason `incomplete` is. Fixed words, and the advice about unsupported
    /// syntax decided from the pattern the planner typed: no name out of the tree is in it.
    pub unmatched: Option<String>,
    /// Where a further call resumes, or that this page fell past the end. Beside the reference for
    /// the same reason as `incomplete`.
    pub paging: Option<Paging>,
    /// What a run printed, whole, where the cap cut down what the planner is shown.
    ///
    /// Quarantined by the turn loop, which is what mints slots, and the reference goes back beside
    /// the sample. The cap is on what enters the conversation, not on what the command printed, so
    /// the middle stays reachable: a planner that needs it hands the reference to a processor or
    /// writes it to a file rather than running the command again.
    pub whole: Option<Labelled<String>>,
    /// Content that travels beside `text` under a label of its own.
    pub apart: Option<Apart>,
    /// What the person typed beside their answer to the question this call asked, with the
    /// driver's sentence that introduces it ([PROMPT-12]). Trusted and public: the person's own
    /// words, so the turn puts it after the result as it would a prompt they typed.
    ///
    /// [PROMPT-12]: ../../../docs/specs/prompting.md
    pub told: Option<Labelled<String>>,
    /// What a processor's answer is for, where it produced one: the call, whose reads and document
    /// decide where the answer may be written.
    pub answers_for: Option<bravebot_core::processor::ProcessorSpec>,
    /// What an isolated processor said about what it did. For the person watching only.
    pub said: Option<Labelled<String>>,
    /// The number a processor chose, where the call asked it to choose (PROC-16). Set by the tool
    /// from the planner's own argument, whatever number the answer was read as, and stored by the
    /// turn as a reference of its own kind.
    pub chosen: Option<ChosenNumber>,
    /// The model a processor was answered by, where it is not the one the person named for processors
    /// (PROC-13). The turn words it for the person: this module is read by the planner.
    pub substituted: Option<String>,
    /// The content alone, where `text` has the driver's own notes after it, so that a glimpse of
    /// a file is of the file and counts the file's lines.
    pub glimpsed: Option<Labelled<String>>,
    /// The window of a file this result shows, where it is a read that put lines in the result.
    ///
    /// Recorded by the turn loop only once the planner was shown the result (READ-8): a read that
    /// was quarantined, refused or answered with a notice shows no window.
    pub window: Option<crate::conversation::ReadWindow>,
    /// How many tasks the planner's task list leaves open, where this is the list, accepted, so
    /// that a compaction can carry it across the cut (COMPACT-16) and the turn can ask about it
    /// when it answers (TURN-12). `None` for a `todo_write` that was refused.
    ///
    /// Counted inside the kernel and labelled as the list is, so the turn holds a number it may
    /// read only through [`Policy::read_trusted_content`].
    pub task_list: Option<Labelled<usize>>,
    /// How many of the open tasks in `task_list` are waiting, counted and labelled as it is, and
    /// read only through [`Policy::read_trusted_content`]. `Some` exactly where `task_list` is.
    pub waiting_tasks: Option<Labelled<usize>>,
    /// Whether the text is workspace content rather than the driver's own words about the call.
    pub content: bool,
    /// Whether this call left a file on disk different from how it found it.
    ///
    /// Read by the turn loop, which says at the end whether what changed was ever built. The
    /// outcome rather than the request: a write the person declined changed nothing, so a turn
    /// driven by the call the planner asked for would report a diff that does not exist.
    pub changed_a_file: bool,
    /// Whether this call wrote a file that landed in the session's own directory, decided on the
    /// resolved destination. Never true together with `changed_a_file`, and read only for the
    /// count a delegate's finished note gives apart from the project's writes (GROUND-2).
    pub wrote_in_the_session_directory: bool,
    /// Whether this call ran a program.
    ///
    /// Read beside `changed_a_file` and the outcome for the same reason: a run the person
    /// declined leaves the change as unbuilt as it was before.
    pub ran_a_program: bool,
    /// Whether the program this call ran ended badly: a stage exited with a status other than 0,
    /// or the line was stopped. False where no program ran (OPEN-1).
    pub command_failed: bool,
    /// Whether a program was started, whatever came of the call.
    ///
    /// Wider than `ran_a_program`, which says a program ran: a line that failed after an earlier
    /// stage may have spawned is marked here and not there, since the failure does not say whether
    /// anything started. Read for the checkout, which is kept wherever one was started
    /// (CHECKOUT-15).
    pub started_a_program: bool,
    /// Whether this call was a `run` whose compiled plan is not established as leaving every file
    /// as it found it (TURN-3).
    ///
    /// The request rather than the outcome, as a write tool's name is: a line the person declined
    /// was still the planner trying to deliver. Not a count of files changed, so it never feeds
    /// `changed_a_file` or the work record. False for a line refused before it was compiled.
    pub run_may_change_files: bool,
    /// The directory each git stage of the command worked in, so a result sealed from the planner
    /// can name the tool that reads history without a prompt where that tool opens the
    /// repository. Where the line ran, moved by the stage's leading `-C` options.
    pub ran_git_in: Vec<std::path::PathBuf>,
    /// What the call spent at the model, where it called one.
    ///
    /// Zero for every tool but a processor, an advisor, a delegate and a check over content nobody
    /// vouched for. A turn that reported only its own rounds would understate what it cost by
    /// however much those wrote.
    pub usage: Usage,
    /// Whether [`Output::usage`] is what a check over content nobody vouched for cost, or what a
    /// processor cost.
    ///
    /// Set by the tool that made the check or ran the processor, whether or not it was made, so it
    /// is never decided from what the check or the processor said. The turn counts such usage in
    /// what it spent and leaves it out of what a delegate's budget counts (DELEGATE-29).
    pub usage_is_unbudgeted: bool,
    /// When the call waited on the model, where it called one.
    ///
    /// Travels beside [`Output::usage`] and for the same reason. A processor is a request like any
    /// other, and the turn was waiting on the endpoint for it: left out, the seconds would be
    /// charged to tool execution, and a turn that did most of its work in processors would read as
    /// one that ran a very slow subprocess.
    ///
    /// The boundary rather than the duration, because a parent clips a delegate's requests to its
    /// own waits and cannot do that from a length. The duration is [`crate::timing::Interval::duration`].
    pub inference_interval: Option<crate::timing::Interval>,
    /// Set where the call waited on a check that could not be made, so no request completed.
    pub unanswered: bool,
    /// The command whose output this is and how it ended, where a run produced it.
    ///
    /// Recorded on the slot by the turn loop, since only a slot minted from a command may be
    /// offered to the user for reading.
    pub printed_by: Option<crate::report::Command>,
    /// Which lines of a job's whole output this result holds and which references earlier looks at
    /// it became, where it is a look at a background job. Counts and driver-minted names only.
    pub look: Option<Look>,
    /// Whether the line ran unasked because a record already covers it.
    ///
    /// Read where a quarantined result says what would lift the quarantine: the advice about
    /// vouching is advice about a prompt, and no prompt will return here for this line until
    /// somebody deletes the entry.
    pub covered_by_record: bool,
    /// Whether the planner asked to read what the line printed, or what `fetch_url` or a server's
    /// tool returned, in this result.
    ///
    /// Acted on by the turn loop, after the slot is minted, and only where nobody would be asked
    /// before `read_output` (for a line) or `vet_content` (for the others) handed it over.
    pub read_asked: bool,
    /// The media type, where what this produced is a picture.
    ///
    /// Recorded on the slot by the turn loop, and what makes a picture reach a processor as a part
    /// rather than as a body. Its presence is also what forces a reference: a picture is never put
    /// in the planner's context, whatever the label on the file it came from says.
    pub picture: Option<String>,
    /// How many bytes the file `picture` names holds, as the length of what was read. The URI the
    /// slot holds is longer, and the person is told this one.
    pub picture_bytes: usize,
    /// The pictures a server's tool result carried beside its text, in the order the server sent
    /// them, each to be quarantined as a picture of its own (MCP-6). Empty for every other tool.
    pub pictures: Vec<bravebot_mcp::Picture>,
    /// How many parts of a server's tool result its transport left out, which the planner is told as
    /// a number and nothing else (MCP-6).
    pub left_out: usize,
    /// Where `read_file` read a file under another spelling of the name it was given (READ-11).
    /// The turn words it beside the result from the two names, both the planner's own.
    pub respelled: Option<Respelled>,
    /// A picture `vet_content` promoted, which the turn attaches to the planner's next request in a
    /// message of its own after the round's results (VET-4). Never carried in `text`.
    pub attached: Option<bravebot_core::vetting::Attached>,
    /// When the planner asked for the next tick of a self-paced loop.
    ///
    /// Travels back to whoever started the loop, which is the only thing that knows there is one.
    /// A turn that says it twice is taken at its last word, since that is the one it ended on.
    pub wakeup: Option<crate::turn::Wakeup>,
    /// The path the planner asked to have a standing watch armed on, where it asked.
    ///
    /// Travels back for the reason a wakeup does: a watch outlives the turn, so the thing that
    /// holds it is the session rather than anything here. Already past the gate a read of the
    /// path goes through, so what arrives is a path this session may look at.
    pub watch: Option<crate::watch::Armed>,
    /// The issue or pull request link the planner asked to have recorded, where it asked.
    ///
    /// Travels back for the reason a watch does: the session record is held by whatever owns the
    /// session. Already checked to be an address the person typed.
    pub link: Option<crate::links::LinkSet>,
    /// The issues and pull requests the planner's own arguments named in this call (PANEL-16).
    ///
    /// Travels back for the reason a link does: the list is held by whatever owns the session.
    pub references: Vec<crate::links::Reference>,
    /// The kind of item the command this call ran created, where it ran, exited cleanly and its
    /// words were `gh pr create`, `gh issue create` or `pr-fix.py open` (PANEL-18). Read from the
    /// planner's own command and never from what the program printed.
    pub created: Option<crate::links::LinkKind>,
    /// The delegates the kernel approved, for the turn to start.
    ///
    /// Started by the turn because a delegate outlives the call that asked for one: the call
    /// answers at once and the delegate goes on working, so what starts it has to be the thing
    /// still there when it finishes.
    ///
    /// A list because one call may fan a task out over several. Each is gated, numbered and
    /// seeded on its own, so what the turn starts is the same thing whether the planner asked
    /// for them one call at a time or all at once.
    pub delegate: Vec<(crate::report::DelegateId, crate::delegate::Seeded)>,
    /// How many of `delegate` have to end before the planner is asked again: the call's own
    /// `wake_after`, or 1 where it named none (DELEGATE-17).
    pub wake_after: usize,
    /// The skill the planner loaded and how its file asks the rounds after it to run.
    ///
    /// Travels back for the reason a wakeup does: what it changes is the next request rather than
    /// this result, and the model and the effort a round is asked at are the turn's to hold.
    ///
    /// Absent where nothing was loaded and where what was loaded named neither.
    pub loaded: Option<(String, crate::skills::RunsAs)>,
    /// The name of the skill this call loaded, as the catalogue spells it.
    ///
    /// Unlike `loaded` it is set for every skill, so the turn can tag the result and a compaction
    /// can send the skill again (COMPACT-17). The catalogue's own string, never the call's.
    pub skill: Option<String>,
    /// The tag for the result of a `message_session` call whose message was delivered (MSG-11).
    ///
    /// Built from the roster's name and id and the label the policy layer gave the words, never from
    /// anything the receiver answered.
    pub message_sent: Option<crate::conversation::Composed>,
    /// The wire name of the MCP server tool the planner asked to have offered from the next round
    /// on (SERVERS-16). Carried back because the offer is the turn's to hold.
    pub tool_loaded: Option<String>,
}

/// The directory `run` lines land in when they name none, carried from the last line that ran.
///
/// Two spellings of one place. `landing` is where the workspace resolved the directory, which the
/// line runs in and every check is made against. `told` is the directory as the planner typed it,
/// which is what a sentence addressed to the planner spells: where a link in a tree nobody vouched
/// for led is a name whoever wrote the tree chose (LABEL-3, TOOL-4). `told` is never compared with
/// `landing` or with anything else.
#[derive(Debug, Clone)]
pub struct RunDirectory {
    pub landing: std::path::PathBuf,
    pub told: String,
}

impl RunDirectory {
    /// The workspace root, which a sentence about a line run there does not name.
    pub fn at(root: &std::path::Path) -> Self {
        Self {
            landing: root.to_path_buf(),
            told: String::new(),
        }
    }
}

/// Everything a tool works with that is not the policy.
///
/// Three things, and the second two are new because of the processor: the quarantine, so a
/// reference the planner names resolves to something, and the model, so an isolated processor
/// has somewhere to run. Bundled rather than passed one by one because dispatch would otherwise
/// take seven arguments to give two tools what they need.
pub struct Tools<'a> {
    pub workspace: &'a Workspace,
    /// How much of what a program printed may enter the conversation, from `run.maxOutput` or
    /// [`OUTPUT_CAP`] where nothing named one.
    ///
    /// The figure in force rather than what a file said, because a cap nobody named is the
    /// built-in one and a tool runs under that. Resolved by the caller that read the settings, for
    /// the reason the search caps are resolved there: this crate is built by every test in the
    /// tree, and one that read a settings file would answer differently on a machine whose owner
    /// had configured it.
    pub output_cap: usize,
    /// The most one `download_url` call may write to a file, from `download.maxBytes` or
    /// [`DOWNLOAD_CAP`] where nothing named one.
    ///
    /// Resolved by the caller that read the settings, for the reason `output_cap` is.
    pub download_cap: usize,
    /// How long a command may run, and the most a call may ask for, from `run.defaultSeconds` and
    /// `run.maxSeconds` or the built-in figures where nothing named them.
    ///
    /// The figures in force rather than what a file said, and resolved by the caller that read the
    /// settings, for the reason `output_cap` is: this crate is built by every test in the tree, and
    /// one that read a settings file would answer differently on a machine whose owner had
    /// configured it.
    pub deadlines: crate::exec::Deadlines,
    /// The skills this turn found, which the planner selects from by name.
    pub skills: &'a crate::skills::Catalogue,
    /// Where quarantined content lives, by the names the planner was given for it.
    pub slots: &'a mut SlotStore,
    /// The windows of files this conversation has shown the planner, which a repeat read is
    /// compared against.
    pub reads: &'a mut crate::conversation::ShownReads,
    /// The model an isolated processor runs on.
    pub chat: Chat<'a>,
    /// The turn's stop token, so a slow program does not have to be waited out.
    pub cancel: &'a bravebot_core::cancel::Cancel,
    /// Whether a line the person typed is waiting for the turn to take (INPUT-10).
    ///
    /// Read by a `job_output` wait, which ends when it reports a line, so that the line reaches the
    /// planner at the round's boundary and not when the wait's bound runs out (RUN-17). It is a fact
    /// about the queue and never about the words. A delegate gets [`Arrival::never`], since it takes
    /// no lines.
    pub arrival: crate::interjections::Arrival,
    /// What this turn may say about when it runs again.
    ///
    /// Read by dispatch as well as by the tool table, so a call to a tool this turn was not
    /// offered is answered the way any other unknown name is rather than quietly working.
    pub scheduling: Scheduling,
    /// Whether this turn may arm a standing watch, and why not where it may not.
    ///
    /// Read by dispatch as well as by the tool table, for the reason `scheduling` is: a call to a
    /// tool this turn was not offered is answered the way any other unknown name is rather than
    /// quietly working.
    pub arming: crate::watch::Arming,
    /// Whether this turn's list holds `run`.
    ///
    /// Read by dispatch as well as by the tool table, because the two have to agree: `read_git`'s
    /// description says what to do where it will not open a repository, and the refusal it is
    /// refused with says the same thing again. A turn told in the description that nothing else
    /// reads the history and then told by the refusal to use `run` has been given both answers.
    pub running: Running,
    /// Which of the tools that result text names this turn's list holds, built once where the turn
    /// settles its list. A sentence a tool writes names `read_output`, `read_file`, `job_output`
    /// and the like only where this says the list holds them (TOOL-6). Agrees with `running`.
    pub beside: Beside,
    /// How many watches this turn has already armed, which is what the count bound is read
    /// against.
    ///
    /// Held by the turn rather than counted here, for the reason the delegate count is: the bound
    /// is on the session and a call is one round of it, so a turn arming its ninth watch has to
    /// be refused by something that saw the other eight.
    pub armed: &'a mut usize,
    /// The user's own directory, where their standing instructions and skills live.
    ///
    /// Carried so a delegate discovers the same ones this turn did. Without it a delegate would
    /// find only what the workspace holds, and a user whose conventions live in their home
    /// directory would have them apply to the turn and not to the work it handed on.
    pub home: Option<&'a std::path::Path>,
    /// The user's profile directory, which is what a leading `~` in a command line stands for.
    ///
    /// The directory `home` sits inside, and carried separately because everything else here
    /// wants the state directory: a `~` names a file of the user's own, so resolving one against
    /// `home` would put every home-relative path the planner writes inside `~/.bravebot`
    /// (CMDLINE-4).
    pub profile: Option<&'a std::path::Path>,
    /// The directory the platform keeps this user's disposable files in, usually
    /// [`crate::home::cache`].
    ///
    /// Where the copy of a picture goes that a person opens before answering `vet_content`'s
    /// prompt about it. `None` refuses that prompt rather than writing the copy somewhere a
    /// confined program can reach.
    pub cache: Option<&'a std::path::Path>,
    /// Whether this turn is itself a delegate's, and so may not ask a person, write the task
    /// list on their screen, or reach a host.
    ///
    /// Read by dispatch as well as by the tool table, for the reason `self_paced` is: a delegate
    /// is offered none of those three, and a call it makes anyway has to be answered the way any
    /// other unknown name is rather than quietly working. Two refusals rather than one, because a
    /// rule resting on the tool list alone rests on the model reading it, and a model naming a
    /// tool it was never offered is ordinary.
    pub delegated: bool,
    /// The only tools this turn was offered, where an addressed definition narrowed them, and
    /// `None` where the table itself is the list.
    ///
    /// A name outside it is answered the way any other unknown name is, for the reason
    /// [`Tools::delegated`] gives: a narrowing that rested on the offer alone would rest on the
    /// model reading it.
    pub confined_to: Option<&'a [String]>,
    /// The schemas this turn's request offered, which a built-in call's argument names are checked
    /// against before it runs (TOOL-3), or `None` where no request offered a list.
    pub offered: Option<&'a [Tool]>,
    /// The language servers this session has started, or `None` where the host offers none.
    ///
    /// Held across calls rather than per call because indexing is the whole cost of a server, and
    /// paying it per question would make this slower than the search it replaces. `None` for a
    /// host that cannot confine a subprocess: LSP-5 is MCP-3 applied here, so no confinement means
    /// no process, and the tool answers by saying so.
    pub servers: Option<&'a mut LanguageServers>,
    /// The tools of the MCP servers this run holds a grant for whose lists somebody vouched for,
    /// and the session they are called through. `None` for a run that holds no server's grant
    /// and for a session that reached no server.
    pub mcp: Option<&'a crate::mcp::Offer>,
    /// The pipelines this turn left running, by the reference each was given.
    ///
    /// Held by the turn so they end with it: a background job outliving the turn that started one
    /// would be an effect nobody is watching and nobody can stop.
    pub jobs: &'a mut Jobs,
    /// The standing answer this turn runs under, for the one mode that refuses rather than answers.
    ///
    /// Read by dispatch, for the reason `self_paced` and `delegated` are: a mode that refuses
    /// writes has to refuse them here, because the confirmer that carries the same mode is
    /// consulted only where something wanted to prompt. A path the trust map already covers, and a
    /// path a rule in the settings file allows, raise no prompt at all, so a refusal that waited
    /// for one would let exactly those writes through.
    pub permission_mode: crate::LiveMode,
    /// Whether the lines handed to this turn come from a channel the person vouched for with a
    /// flag they typed (CLI-36). Set from the task by the driver, and read only to say in the trail
    /// what footing a typed answer to `ask_user` had.
    pub vouched_channel: bool,
    /// Whether a check that finds nothing may promote a slot without anybody being asked.
    ///
    /// `false` for every turn nobody turned it on for, which is the default and what a turn has
    /// always done. The three routes into it and the rule that resolves them are
    /// [`bravebot_core::vetting::auto`]; by the time it reaches here it is one answer, settled
    /// before the session opened and unchanged for its life.
    ///
    /// Read by `vet_content`, by `read_output` and by `whole_of_it`, which reads it only to word
    /// a refusal, and by nothing else: the two prompts that promote one slot's bytes once, which `docs/specs/vetting.md`'s CHECK-12 divides from the
    /// third. That third one writes a trust rule about a whole path, which is a standing decision
    /// rather than bytes in front of a reader, and not something a word from a model may answer
    /// with nobody asked.
    pub auto_vetting: bool,
    /// The current working directory for `run` commands in this turn.
    ///
    /// Initialized to the workspace root and updated when `run` specifies a `directory`.
    pub run_directory: &'a mut RunDirectory,
    /// How many lines ran with a sandbox in the foreground of this turn and did not exit zero,
    /// which a request for a line with no sandbox is accepted after and not before (SANDBOX-29).
    pub confined_failures: &'a mut ConfinedFailures,
    /// The slots a check has already said `unsafe` about in this turn, so `read_output` and
    /// `vet_content` do not check them again (OUTPUT-3).
    pub objections: &'a mut Objections,
    /// The session whose run prompts may have their answers remembered past it.
    ///
    /// `None` for a turn with nobody to put a prompt to, which reads no record and writes none:
    /// what a record answers is a prompt, and where no prompt can be drawn it would be saying
    /// instead which effects may happen with nobody to see them. The identifier is what the reading
    /// back uses to tell this session's own answers from an earlier session's.
    pub remembering: Option<&'a str>,
    /// The tokens this run has left under its own budget, where it carries one.
    ///
    /// Read when it starts a delegate, so a child is never given more than its parent has left
    /// (DELEGATE-29). Two counts the driver holds: the budget the spec fixed and the tokens the
    /// run has reported so far, the delegates it has collected included.
    pub budget_left: Option<u64>,
    /// The advisor this turn may consult, and what it has been asked so far, or `None` where the
    /// session named none.
    ///
    /// Read by dispatch as well as by the tool list, for the reason `arming` is: a call to a tool
    /// this turn was not offered is answered the way any other unknown name is rather than
    /// quietly working.
    pub advising: Option<crate::advisor::Advising<'a>>,
    /// What the auto mode's classifier is given of the session, or `None` where it is not to be
    /// consulted: another mode, a delegate, or a session the person has typed nothing to (AUTO-3).
    ///
    /// Built from the lines the person typed, selected by the tag recorded when they arrived.
    /// The mode is read again where a question is put, so a mode chosen after this was built
    /// decides the question and not this field.
    pub classifying: Option<crate::classifier::Classifying<'a>>,
    /// The lines the person typed to this session, oldest first, where this call needs them, and
    /// empty otherwise. Read by `set_link` to hold an address to what the person wrote (SETLINK-1).
    pub typed: &'a [String],
    /// What this turn sends messages to other background sessions with, and what it has sent, or
    /// `None` where the session cannot send (MSG-12).
    ///
    /// Read by dispatch as well as by the tool list, for the reason `advising` is: a call to a tool
    /// this turn was not offered is answered the way any other unknown name is rather than quietly
    /// working.
    pub messaging: Option<crate::messages::Messaging<'a>>,
    /// Whether the front end running this turn keeps the links `set_link` records (SETLINK-2).
    ///
    /// Read by dispatch as well as by the tool list, for the reason `messaging` is: a call to a tool
    /// this turn was not offered is answered as an unknown name rather than recorded and dropped.
    pub keeping_links: bool,
    /// The repository a bare issue or pull request number in a command belongs to, from the
    /// person's own `/pr` or `/issue` link, where the session has one (PANEL-16).
    pub default_repository: Option<(&'a str, &'a str)>,
    /// The issues and pull requests the planner's own words named in this call, which the call
    /// hands back in [`Output::references`] (PANEL-16). Empty where the turn keeps no links.
    pub found: Vec<crate::links::Reference>,
    /// Whether a program `run` starts is held to the profile its plan accounts for.
    ///
    /// `false` for a turn nobody turned it on for, which starts programs with the access the
    /// user's own shell has. Where it is `true` the platform has to confine every step or the step
    /// is not started; a platform with no base (Windows) starts them as it always has
    /// ([SANDBOX-1]).
    ///
    /// [SANDBOX-1]: ../../../docs/specs/sandboxing.md
    pub confine_runs: bool,
    /// How much of the machine a confined program reads, or that none is confined (SANDBOX-22).
    ///
    /// Read only where `confine_runs` is true. A delegate inherits the spawning turn's, so the
    /// mode is no looser one level down.
    pub sandbox: bravebot_sandbox::SandboxMode,
    /// Whether the front end can put a line that starts with no sandbox to a person
    /// ([`Task::asks_about_unconfined`](crate::turn::Task)).
    pub asks_about_unconfined: bool,
}

/// A host list a unit test sets for the runs on its thread, read in place of the settled one.
///
/// The settled list is set once per process, so without this every test in a binary would hold
/// the same list and none could set one without the others seeing it.
#[cfg(all(test, unix))]
pub(crate) mod scripted_hosts {
    use bravebot_config::sandbox_network::Hosts;
    use std::cell::RefCell;

    thread_local! {
        static HELD: RefCell<Option<Hosts>> = const { RefCell::new(None) };
    }

    /// Hold the runs on this thread to `hosts` until the returned guard is dropped.
    pub(crate) fn hold(hosts: Hosts) -> Held {
        HELD.with(|held| *held.borrow_mut() = Some(hosts));
        Held
    }

    pub(super) fn held() -> Option<Hosts> {
        HELD.with(|held| held.borrow().clone())
    }

    /// Clears the list when dropped, so a test that fails part way leaves nothing for the next
    /// test on this thread.
    pub(crate) struct Held;

    impl Drop for Held {
        fn drop(&mut self) {
            HELD.with(|held| *held.borrow_mut() = None);
        }
    }
}

/// The program a unit test has the platform refuse to confine for the runs on its thread, which no
/// machine's real mechanism does on demand.
#[cfg(test)]
pub(crate) mod scripted_unconfinable {
    use std::cell::RefCell;

    thread_local! {
        static HELD: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Refuse to confine `program` on this thread until the returned guard is dropped.
    pub(crate) fn hold(program: &str) -> Held {
        HELD.with(|held| *held.borrow_mut() = Some(program.to_string()));
        Held
    }

    pub(super) fn held() -> Option<String> {
        HELD.with(|held| held.borrow().clone())
    }

    /// Clears the program when dropped, so a test that fails part way leaves nothing for the next
    /// test on this thread.
    pub(crate) struct Held;

    impl Drop for Held {
        fn drop(&mut self) {
            HELD.with(|held| *held.borrow_mut() = None);
        }
    }
}

/// The filesystem lists a unit test sets for the runs on its thread, read in place of the settled
/// ones, for the reason [`scripted_hosts`] is.
#[cfg(all(test, unix))]
pub(crate) mod scripted_filesystem {
    use bravebot_sandbox::rules::Lists;
    use std::cell::RefCell;

    thread_local! {
        static HELD: RefCell<Option<Lists>> = const { RefCell::new(None) };
    }

    /// Hold the runs on this thread to `lists` until the returned guard is dropped.
    pub(crate) fn hold(lists: Lists) -> Held {
        HELD.with(|held| *held.borrow_mut() = Some(lists));
        Held
    }

    pub(super) fn held() -> Option<Lists> {
        HELD.with(|held| held.borrow().clone())
    }

    /// Clears the lists when dropped, so a test that fails part way leaves nothing for the next
    /// test on this thread.
    pub(crate) struct Held;

    impl Drop for Held {
        fn drop(&mut self) {
            HELD.with(|held| *held.borrow_mut() = None);
        }
    }
}

impl<'a> Tools<'a> {
    /// The facts this turn's calls reach outside the workspace under (MODE-4).
    pub(crate) fn reach_facts(&self) -> ReachFacts<'_> {
        ReachFacts {
            permission_mode: self.permission_mode.clone(),
            profile: self.profile,
        }
    }

    /// Whether this call's programs are held to a profile at all: the turn confines them and the
    /// sandbox mode is not `off`.
    fn confines_programs(&self) -> bool {
        self.confine_runs && self.sandbox != bravebot_sandbox::SandboxMode::Off
    }

    /// What this call's programs are confined to, or `None` where they are not.
    ///
    /// `workspace_trusted` is [`workspace_is_trusted`], which the tools cannot ask themselves since
    /// the trust map is the policy's. It decides whether bypass gives the stages what a
    /// `request_path` for writing would be granted: only where a request is accepted
    /// ([`requests_are_accepted`]), so a session that refuses every request is given no row for one.
    /// A delegate is given them as the lead session is, less the directories its checkout holds it
    /// out of (SANDBOX-28).
    fn confinement(&self, workspace_trusted: bool) -> Option<crate::confine::Confinement> {
        if !self.confines_programs() {
            return None;
        }
        // Held to the directories the session recorded, so a name a stage has relinked since is
        // given no row (SANDBOX-18). The paths are the same names, for the callers that read them.
        let recorded = self.workspace.session_directories();
        let roots = std::iter::once(self.workspace.root().to_path_buf())
            .chain(self.workspace.added_directories())
            .collect();
        let hosts = bravebot_config::sandbox_network::settled().map(|settled| &settled.hosts);
        #[cfg(all(test, unix))]
        let scripted = scripted_hosts::held();
        #[cfg(all(test, unix))]
        let hosts = scripted.as_ref().or(hosts);
        let lists = bravebot_config::sandbox_filesystem();
        #[cfg(all(test, unix))]
        let lists = scripted_filesystem::held().unwrap_or(lists);
        let confinement =
            crate::confine::Confinement::here(roots, self.workspace.scratch_made(), self.profile)?
                .with_session_directories(&recorded)
                .with_left_directories(&self.workspace.left_directories())
                .with_network(bravebot_config::run_network())
                .with_hosts(hosts)
                .with_mode(self.sandbox)
                .with_temporary_held_back(bravebot_config::holds_back_temporary_directory())
                .with_filesystem(&lists)
                .with_path_reach(&self.workspace.path_reach())
                .with_unasked_writes(
                    self.permission_mode.get() == crate::PermissionMode::Bypass
                        && requests_are_accepted(
                            self.confine_runs,
                            self.sandbox,
                            workspace_trusted,
                        ),
                )
                .with_withheld_writes(self.workspace.checkout_exclusions())
                .with_named_settings(self.workspace.named_settings_paths())
                .with_host_grants(&self.workspace.granted_hosts())
                .with_identities(bravebot_config::git_identities::settled())
                .with_state_directory(self.home.map(std::path::Path::to_path_buf));
        #[cfg(test)]
        let confinement = match scripted_unconfinable::held() {
            Some(program) => confinement.failing_for(&program),
            None => confinement,
        };
        // Read only where a person is there to see the row it adds: a session with nobody to put
        // a prompt to reads no record, for the reason a remembered line is not read there.
        let grants = match (self.home, self.remembering) {
            (Some(home), Some(session)) => {
                crate::reach::Store::new(home).read(Some(session), self.workspace.root())
            }
            _ => Vec::new(),
        };
        // Read wherever there is a record, as `git.identities` is: a choice adds no row, since it
        // applies only to a key ssh could offer, which the remote scope reads already, and it only
        // narrows which of them ssh offers first.
        let chosen_keys = self
            .home
            .map(|home| crate::reach::Store::new(home).chosen_keys())
            .unwrap_or_default();
        Some(
            confinement
                .with_grants(grants)
                .with_chosen_keys(chosen_keys),
        )
    }

    /// Where this turn's credential findings are written, and under whose name.
    ///
    /// Both halves are already here for other reasons, and putting them together in one place is
    /// what stops a second scan site pairing the state directory with the wrong session.
    ///
    /// The answer outlives the borrow taken to ask for it, because both halves are the turn's and
    /// neither is read from this structure again. That is what lets a call take this and a slot
    /// store in the same expression.
    fn recording(&self) -> crate::findings::Recording<'a> {
        crate::findings::Recording {
            home: self.home,
            session: self.remembering,
        }
    }

    /// Note the issues and pull requests a command line the planner wrote names. Called with the
    /// line [`Policy::read_planner_argument`] returned, so a context that has met untrusted content
    /// never reaches it (PANEL-16). What it notes is for the list the person reads and is not read
    /// by anything that decides what the call does.
    fn note_command(&mut self, line: &str) {
        if self.keeping_links {
            self.found.extend(crate::links::references_in_command(
                line,
                self.default_repository,
            ));
        }
    }

    /// Note the issues and pull requests an address the planner wrote names, as
    /// [`Tools::note_command`] does for a command line.
    fn note_address(&mut self, url: &str) {
        if self.keeping_links {
            self.found.extend(crate::links::references_in_text(url));
        }
    }
}

/// How many lines have run with a sandbox in the foreground of a turn and not exited zero, or were
/// refused because their program could not be confined, less the requests for no sandbox accepted
/// since (SANDBOX-29). A line in the background is counted only for a program that could not be
/// confined, never for its exit code.
///
/// Counted from exit codes, which are structure and not what a program printed, so nothing a
/// program wrote reaches it. A request for a line with no sandbox is accepted only while it is
/// above zero, and accepting one spends one, so each failure earns one request.
#[derive(Debug, Default)]
pub struct ConfinedFailures(usize);

impl ConfinedFailures {
    fn note(&mut self) {
        self.0 = self.0.saturating_add(1);
    }

    fn has_one(&self) -> bool {
        self.0 > 0
    }

    fn spend(&mut self) -> bool {
        match self.0 {
            0 => false,
            held => {
                self.0 = held - 1;
                true
            }
        }
    }
}

/// The slots a check said `unsafe` about in this turn, which `read_output` and `vet_content` do
/// not check again (OUTPUT-3).
///
/// Holds the slot and nothing else: the word is `unsafe` by being here, the reason the check wrote
/// is never kept, and a check that could not be made or was stopped leaves nothing, because those
/// say nothing about the content and the next ask has to check. Filled by the check a `run`,
/// `fetch_url` or a server's tool makes for `read: true` and read by `read_output` and
/// `vet_content` before either would make one of its own. Held by the turn, so it ends with it.
#[derive(Debug, Default)]
pub struct Objections(std::collections::BTreeSet<SlotId>);

impl Objections {
    fn note(&mut self, slot: &SlotId) {
        self.0.insert(slot.clone());
    }

    /// Whether a check in this turn already said `unsafe` about `slot`, in which case the trail
    /// says the word was reused and the caller makes no check of its own.
    fn reuse<S: Sink>(&self, policy: &mut Policy<'_, S>, slot: &SlotId) -> bool {
        let held = self.0.contains(slot);
        if held {
            policy.record_reused_verdict(slot, Verdict::Unsafe);
        }
        held
    }
}

/// The background pipelines a turn has started.
///
/// Dropping this kills whatever is still running, so the turn ending is the end of them.
///
/// A job's name is not a reference and holds no content. It is a label the driver minted, which
/// makes it trusted and public and so usable as routing: the planner names one to say which
/// pipeline it means, exactly as it names a program. What a job *printed* is content, and that
/// comes back through the ordinary quarantine like anything else a program printed.
#[derive(Debug, Default)]
pub struct Jobs {
    running: std::collections::BTreeMap<String, Job>,
    /// How many have been started, so each gets a name of its own.
    ///
    /// Never reused within a turn, so a name cannot come to mean a second pipeline after the
    /// planner has been told what it means. Never reused within a session either where the jobs
    /// are kept between turns, since the set is then the session's.
    started: usize,
    /// Whether these belong to a session and survive the turn that started them (RUN-15).
    ///
    /// Said to the planner in the account of each start, because what a job does when the turn ends
    /// is the one thing it cannot work out for itself.
    kept: bool,
}

/// The background jobs of an interactive session, kept between its turns (RUN-15).
///
/// A turn borrows them for as long as it runs and leaves them running when it ends. Dropping the
/// last handle kills whatever is still going, so the session ending, being cleared or being
/// replaced by another is the end of them, and a one-shot run or an incognito session, which never
/// make one, keep the rule that the turn owns its jobs.
#[derive(Debug, Clone, Default)]
pub struct SessionJobs(std::sync::Arc<std::sync::Mutex<Jobs>>);

impl SessionJobs {
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(Jobs {
            kept: true,
            ..Jobs::default()
        })))
    }

    /// Hold the jobs for the length of a turn.
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Jobs> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Carry out every stop the person has asked for, between turns, and name each job it reached.
    ///
    /// No turn is there to read the token at its next step, and a job nobody can stop between turns
    /// would be the effect nobody can stop that RUN-15 exists to prevent. Each is killed and keeps
    /// what it printed, and how it ended is told the planner at the next turn's first round as any
    /// finish is, as the person's stop. Reads a token and a clock, nothing a job printed.
    pub fn stop_requested(&self) -> Vec<(String, std::time::Duration)> {
        let mut jobs = self.lock();
        let mut stopped = Vec::new();
        for (name, job) in jobs.running.iter_mut() {
            if !job.reported && job.stopped_by_the_person.is_none() && job.stop_asked() {
                stopped.push((name.clone(), job.stop_for_the_person()));
            }
        }
        stopped
    }
}

/// A job's line as two people are told it: the person, who is shown the plan they approved, and the
/// planner, who is told the line with its operands as it wrote them.
struct JobLines {
    shown: String,
    told: String,
}

impl JobLines {
    fn new(shown: &str, told: &str) -> Self {
        Self {
            shown: shown.to_string(),
            told: told.to_string(),
        }
    }
}

/// One background pipeline, and what it was started as.
#[derive(Debug)]
struct Job {
    file_authority: bravebot_core::file_authority::FileAuthority,
    file_revision: u64,
    running: crate::exec::Background,
    /// The line as the person approved it, for the account given afterwards.
    line: String,
    /// The line as the planner wrote it, which is what the planner is told it started.
    said: String,
    /// The initial output label. A later file revision can lower its integrity, never raise it.
    label: bravebot_core::label::Label,
    /// How much of each pipe has already been handed over, so a later look is a slice that starts
    /// where the previous one stopped, which can be partway through a line.
    ///
    /// Counts of bytes, never a comparison of them: nothing here reads what was printed. Per pipe
    /// rather than one offset into the composed text, because output arriving on one stream moves
    /// where the other sits in that composition.
    seen: crate::exec::Seen,
    /// How many lines of this job's output have been handed over, across every look and the
    /// account of its finish.
    ///
    /// A count of lines and never a line. Moved in [`Job::take`] and nowhere else, so the route
    /// that hands the output over cannot do it without the count following. It is what lets each
    /// result say which lines of the whole it holds (a reference states the size of its own slot
    /// only).
    ///
    /// Each line of the job's output is counted once, by the look that holds the first byte of
    /// it. A look cuts at the bytes that had arrived, which can be in the middle of a line, so
    /// counting the lines in each look's text would count that line in the look that began it and
    /// again in the look that finished it.
    handed: usize,
    /// The references the turn minted for earlier looks at this job, in order.
    ///
    /// Names the driver minted and nothing read out of the output. Recorded by the turn once it
    /// has minted the slot ([`Jobs::record_look`]), and listed in the next result for this job.
    looks: Vec<String>,
    /// How much of each pipe the screen has been shown the unread lines of (WATCH-24).
    ///
    /// Byte counts, like `seen`, and only a way to tell that more has arrived since the screen was
    /// last given the unread lines. It never moves `seen`: the planner's next look still returns
    /// everything it has not read.
    drawn: crate::exec::Seen,
    /// Whether somebody has already been given the account of how this one ended.
    ///
    /// Set by whichever route got there first, so the finish is news exactly once: the turn's own
    /// look between rounds, or a `job_output` call whose answer already said it had ended. Without
    /// it a planner that waited for a build would be told a second time, with the output gone,
    /// since the bytes go to whoever was handed them.
    reported: bool,
    /// Whether a record of lines remembered past the session, and not an answer, stopped the
    /// asking for this line (RUN-19). Decides whether the account of its finish may advise
    /// vouching for the command (RUN-14).
    covered_by_record: bool,
    /// Whether the latest `job_output` look at it asked, with `read`, for its output in the result.
    /// The planner's own boolean, carried to the account of its end, which no call carries
    /// (RUN-22).
    read_asked: bool,
    /// The token the person sets to stop this job, read here at the turn's next step.
    stop: bravebot_core::cancel::JobStop,
    /// How long it had run when the person's stop was carried out, so a look after the account
    /// says the person stopped it and not how a killed program exited.
    stopped_by_the_person: Option<std::time::Duration>,
    /// How long it had run when a stage of it was found stopped by a signal and the job was ended
    /// for that, and the signal's name where it is one that stops a process (RUN-17). Kept so a
    /// look after the account says what stopped it and not how a killed program exited.
    stopped_by_a_signal: Option<(std::time::Duration, Option<&'static str>)>,
    /// What its programs were confined to, where they were, told with a failure so the planner
    /// reads a refusal as the sandbox's and not the file's.
    confinement: Option<String>,
    /// The workspace tree as the line found it, compared once when the job's end is learned
    /// (GROUND-1). `None` once compared, so a job is counted as a write at most once, by whichever
    /// route reports its end first.
    tree: Option<TreeBefore>,
}

/// The workspace tree as a background line found it, with what the second mark needs: the root
/// it was taken under and the stores it left out.
///
/// Names, lengths and modification times, as for a foreground line. No file is opened.
#[derive(Debug)]
struct TreeBefore {
    mark: crate::workspace::TreeMark,
    root: std::path::PathBuf,
    stores: Vec<std::path::PathBuf>,
}

impl TreeBefore {
    /// Mark the workspace's tree now, leaving out its stores.
    fn take(workspace: &Workspace) -> Self {
        Self {
            mark: crate::workspace::TreeMark::take(workspace.root(), &|path| {
                workspace.in_the_stores(path)
            }),
            root: workspace.root().to_path_buf(),
            stores: workspace.stores().to_vec(),
        }
    }

    /// Whether the tree is different now from how this mark found it.
    fn differs_now(&self) -> bool {
        let stores = &self.stores;
        self.mark
            .differs_from(&crate::workspace::TreeMark::take(&self.root, &|path| {
                crate::workspace::within_stores(stores, path)
            }))
    }
}

impl Job {
    /// Whether the job left the workspace tree different from how its line found it, asked once
    /// at the point its end is reported: the mark is dropped here, so a second report counts
    /// nothing (GROUND-1). A comparison of metadata the driver took itself, and nothing the job
    /// printed.
    fn wrote(&mut self) -> bool {
        self.tree.take().is_some_and(|tree| tree.differs_now())
    }

    /// Take what the job has printed since the last look, and say which lines of everything it has
    /// printed that is.
    ///
    /// The one place output leaves a job for the planner's side, used by `job_output` and by the
    /// account of a finish alike, so both advance the same count (LABEL-4: counting lines is not
    /// reading them). The screen's own look ([`Jobs::unread_output`]) copies the cursor and does
    /// not come through here.
    ///
    /// A line is counted once, by the look that holds its first byte. A look cuts at the bytes
    /// that had arrived, which can be in the middle of a line, and the next look then begins with
    /// the rest of that line and adds nothing to the count for it. A last line with no newline
    /// counts as soon as its first byte arrives. The count is of line starts in each pipe's own
    /// bytes, so the `standard error:` label and the newline that joins the two streams are not
    /// lines of the job.
    fn take(&mut self) -> (String, Look) {
        let (text, counted) = self.running.take(&mut self.seen);
        let look = Look {
            first: self.handed + 1,
            lines: counted.lines,
            earlier: self.looks.clone(),
        };
        self.handed += counted.lines;
        (text, look)
    }

    /// Whether the person asked to stop it and it is still going, so the stop is theirs to report.
    ///
    /// Its steps having exited first means the stop reached nothing, and how it ended is said from
    /// its exit codes as it would have been.
    fn stop_asked(&mut self) -> bool {
        self.stop.is_requested() && !self.running.steps_exited()
    }

    /// Carry out the person's stop, and say how long the job had run.
    ///
    /// Killed before its output is taken, and given the pipes' grace a finish gets, so what the
    /// account hands over is all it printed and nothing of it is left for a look nobody makes.
    fn stop_for_the_person(&mut self) -> std::time::Duration {
        let ran_for = self.running.ran_for();
        self.running.kill();
        self.running.ended();
        self.stopped_by_the_person = Some(ran_for);
        ran_for
    }

    /// Whether a stage of the job is stopped by a signal, ending the job at the first look that
    /// finds it so (RUN-17).
    ///
    /// A stopped process runs nothing and exits nothing, so a job left alone is reported as running
    /// for as long as anybody asks. The state is the kernel's account of a child the driver
    /// started, and nothing the job printed is read. A job the person already stopped, or whose
    /// steps have all exited, is not asked: their exit codes are the account. Ended as the person's
    /// stop is, before the output is taken, so what is handed over is all it printed.
    fn stopped_by_a_signal(&mut self) -> bool {
        if self.stopped_by_a_signal.is_some() {
            return true;
        }
        if self.stopped_by_the_person.is_some() || self.running.steps_exited() {
            return false;
        }
        let Some(signal) = self.running.stopped_by_signal() else {
            return false;
        };
        let ran_for = self.running.ran_for();
        self.running.kill();
        self.running.ended();
        self.stopped_by_a_signal = Some((ran_for, signal.name()));
        true
    }

    /// The outcome of a job a stage of which was found stopped by a signal.
    ///
    /// Only asked once [`Job::stopped_by_a_signal`] has said so, and falls back to the job having
    /// run no time at all rather than failing where it was not.
    fn signal_outcome(&self) -> crate::report::Outcome {
        let (ran_for, signal) = self.stopped_by_a_signal.unwrap_or_default();
        crate::report::Outcome::StoppedBySignal { ran_for, signal }
    }

    /// The label this job's output may carry now, rather than the one fixed before it started.
    ///
    /// A proof about the files a line was endorsed against says nothing about a tree something has
    /// decided about since, and the delivery is where that shows. Both routes out of here ask this,
    /// so they ask it in one place: a second copy of the question is a second chance for one of them
    /// to stop asking it.
    fn label_now(&self) -> Label {
        if self.file_authority.is_current(self.file_revision) {
            return self.label;
        }
        Label::new(
            bravebot_core::label::Integrity::Untrusted,
            self.label.confidentiality,
        )
    }
}

/// Which lines of a job's whole output one look covers, and the looks before it that were kept
/// apart as references.
///
/// Counts and names the driver minted, so the sentences built from it (the origin of the result and
/// [`crate::turn`]'s note beside the reference) carry nothing the job printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    /// The number of the first line this look covers, counting from 1 across the job's whole output.
    pub first: usize,
    /// How many lines begin in this look, numbered from `first`. Carried into a sentence whose
    /// words are the same for every value.
    pub lines: usize,
    /// The references of the earlier looks at the same job, oldest first.
    pub earlier: Vec<String>,
}

impl Look {
    /// The origin a reference to this look's output carries: what the job printed over this look's
    /// span, and not everything it has printed, which a reference to a later look would otherwise
    /// read as.
    pub fn origin(&self, said: &str) -> String {
        format!("what `{said}` printed {}", Self::SINCE)
    }

    /// Where a look's span starts, in words. The same words for every look, so no line count
    /// chooses them.
    pub const SINCE: &'static str =
        "since the previous look, or since the job started when there was none";

    /// The sentence beside a reference to this look: how many lines of everything `job` has
    /// printed begin in it and the number the first of them has, and the references of the earlier
    /// looks at the same job.
    ///
    /// The words are the same for every look and carry the two numbers and the references. A line
    /// is counted by the look that holds its first byte, which the sentence says outright and
    /// does not repeat for a look that happens to cut one. Which look holds nothing is decided
    /// from the byte count of its reference, before this is built.
    ///
    /// Built from counts and from names the driver minted (`job` is the name the planner typed and
    /// the references are slot names), so it carries nothing the job printed.
    pub fn note(&self, job: &str) -> String {
        let earlier = self.earlier_looks(
            job,
            "the lines before these are in them, and none of them is repeated here",
        );
        format!(
            " It holds the part of the output of {job} that arrived {}. Lines of the job's whole \
             output that begin in it: {}, numbered from {}. A line a look cuts is counted in the \
             look that holds its first byte, and the next look begins with the rest of it.{earlier}",
            Self::SINCE,
            self.lines,
            self.first,
        )
    }

    /// The sentence a look with no new bytes carries in place of [`Look::note`]: it has no
    /// reference, but what the job printed before is still in the references of the earlier looks.
    /// Empty where there are none.
    pub fn note_for_nothing_new(&self, job: &str) -> String {
        self.earlier_looks(job, "everything it printed before is in them")
    }

    /// The earlier looks at `job`, by the references the driver minted, followed by what they hold.
    /// Empty where there are none, which is every look at a job whose output was shown as text.
    fn earlier_looks(&self, job: &str, holding: &str) -> String {
        match self.earlier.is_empty() {
            true => String::new(),
            false => format!(
                " Earlier looks at {job} are {}; {holding}.",
                self.earlier.join(", ")
            ),
        }
    }
}

/// What a running job has printed that the planner has not looked at, for the screen alone.
pub(crate) struct Unread {
    pub name: String,
    pub text: Labelled<String>,
}

/// A background job's finish, as the turn is told about it (CMDLINE-14).
///
/// The name and the outcome are the driver's own: a name this module minted, and a verdict read off
/// exit codes and a clock. Nothing here was read out of a byte the pipeline printed. What it
/// *printed* is content, and it carries the label the kernel fixed before anything started.
pub struct Ended {
    /// The name the planner was given for it.
    pub name: String,
    /// The line as the person approved it.
    pub line: String,
    /// The line as the planner wrote it, which is the only spelling the planner is told.
    pub said: String,
    /// How it ended.
    pub outcome: crate::report::Outcome,
    /// What it printed that nobody has been handed yet, cut to the cap where it may be read.
    ///
    /// `None` where no byte arrived since the previous look, which is the job whose exit code
    /// is the whole of its account: a reference to an empty slot spends a name the planner is
    /// reading the numbering of and says nothing. A count of bytes and never a look at one.
    pub printed: Option<Labelled<String>>,
    /// The whole of it, where the cap cut the sample down.
    ///
    /// Beside the sample for the reason a run's is: the cap bounds what a conversation holds and
    /// not what the program printed, so the middle has to exist somewhere a later call can reach.
    pub whole: Option<Labelled<String>>,
    /// Which lines of the job's whole output `printed` holds, taken through the same count a
    /// `job_output` look is, and the references of the looks before it.
    pub look: Look,
    /// Whether a record of remembered lines stopped the asking for this line (RUN-19), in which
    /// case the account of a quarantined output leaves out the advice about vouching (RUN-14).
    pub covered_by_record: bool,
    /// Whether the job's latest `job_output` look asked to read its output in the result, so the
    /// account is released as that look would have been (RUN-22).
    pub read_asked: bool,
    /// Whether the job left the workspace tree different from how its line found it (GROUND-1).
    /// Measured from names, lengths and times when this end was learned, never from what it
    /// printed.
    pub changed_a_file: bool,
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many are held, running or finished.
    pub fn len(&self) -> usize {
        self.running.len()
    }

    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    /// Whether these belong to a session and outlive each turn (RUN-15).
    pub fn is_kept(&self) -> bool {
        self.kept
    }

    /// How a look at a job is named on the line a person reads: the name the planner typed and
    /// the command that job runs, so a wait says what it is waiting for instead of `job:1`.
    ///
    /// The name is read through the gate every planner argument is read through, as `job_output`
    /// reads it, because finding the job is a comparison against the planner's words. The command
    /// is the line the person approved, which a screen already drew when the job started, and it
    /// goes back to a screen drawn as a `run` command is. `None` where the name is no job of
    /// this turn's or the line declares a credential, which leaves the line as the name alone.
    pub(crate) fn drawn_with_its_command<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        arguments: &Value,
    ) -> Option<String> {
        let named = argument(arguments, "job")?;
        let name = policy
            .read_planner_argument("job_output", "job", &named)
            .ok()?;
        let job = self.running.get(&name)?;
        let command = command_as_drawn(&job.line);
        (!command.is_empty()).then(|| format!("{name}  {command}"))
    }

    /// The jobs still running that the planner has not been told the end of, with how long each
    /// has run.
    ///
    /// Names the driver minted and a count read off a clock, so what is said of them is the
    /// driver's own words. A job whose steps have exited is left out: its finish is the news, and
    /// the turn reports it as one.
    pub fn running_now(&mut self) -> Vec<(String, std::time::Duration)> {
        self.running
            .iter_mut()
            .filter_map(|(name, job)| {
                (!job.reported && !job.running.steps_exited())
                    .then(|| (name.clone(), job.running.ran_for()))
            })
            .collect()
    }

    /// Whether `call` is a bounded wait on a job that is still running (TURN-9).
    ///
    /// True only for a `job_output` call whose arguments are JSON, whose `wait_seconds` is one
    /// the tool accepts, which neither kills the job nor carries an `until_exit` or a `read` the
    /// tool would refuse before it waits, and whose `job` names a job of this set that has not
    /// been reported and whose steps have not exited. Every part is the planner's own tool name
    /// and arguments, or the driver's own observation that the process is running, which is a
    /// process fact like an exit status.
    /// What the job printed is never read. The name is read through the gate every planner
    /// argument is read through, so a context that gate denies leaves the call counted.
    pub(crate) fn is_waited_on_while_running<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        call: &ToolCall,
        deadlines: crate::exec::Deadlines,
    ) -> bool {
        if strip_namespace(&call.function.name) != "job_output" {
            return false;
        }
        let Ok(arguments) = call.arguments() else {
            return false;
        };
        let kill = arguments
            .get("kill")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if kill {
            return false;
        }
        if read_asked_of(&arguments).is_err() {
            return false;
        }
        let Ok(Some(wait)) = wait_from(&arguments, deadlines) else {
            return false;
        };
        if until_exit_from(&arguments, Some(wait), kill).is_err() {
            return false;
        }
        let Some(named) = argument(&arguments, "job") else {
            return false;
        };
        let Ok(name) = policy.read_planner_argument("job_output", "job", &named) else {
            return false;
        };
        self.running
            .get_mut(&name)
            .is_some_and(|job| !job.reported && !job.running.steps_exited())
    }

    /// Take a background pipeline and hand back the name the planner will call it by, with the
    /// token that stops it.
    fn keep(
        &mut self,
        running: crate::exec::Background,
        lines: JobLines,
        label: bravebot_core::label::Label,
        file_authority: bravebot_core::file_authority::FileAuthority,
        file_revision: u64,
        covered_by_record: bool,
    ) -> (String, bravebot_core::cancel::JobStop) {
        self.started += 1;
        let name = format!("job:{}", self.started);
        let stop = bravebot_core::cancel::JobStop::new();
        self.running.insert(
            name.clone(),
            Job {
                file_authority,
                file_revision,
                running,
                line: lines.shown,
                said: lines.told,
                label,
                seen: crate::exec::Seen::default(),
                handed: 0,
                looks: Vec::new(),
                drawn: crate::exec::Seen::default(),
                reported: false,
                covered_by_record,
                read_asked: false,
                stop: stop.clone(),
                stopped_by_the_person: None,
                stopped_by_a_signal: None,
                confinement: None,
                tree: None,
            },
        );
        (name, stop)
    }

    /// Start `line` as a job whose output carries `label`, for a test that holds jobs without a
    /// turn around them, and hand back its name and the token that stops it.
    #[cfg(test)]
    pub(crate) fn start_for_a_test(
        &mut self,
        line: &str,
        label: Label,
    ) -> (String, bravebot_core::cancel::JobStop) {
        use bravebot_core::TrustStore;
        use bravebot_core::file_authority::FileAuthority;

        let root = std::env::current_dir().unwrap();
        let plan = crate::cmdline::compile(line, &root, None, &mut |_, _| Ok(())).unwrap();
        let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
            panic!("one pipeline");
        };
        let running = crate::exec::start_steps_within(
            steps,
            &root,
            None,
            None,
            &bravebot_core::cancel::Cancel::new(),
            crate::exec::LIMIT,
        )
        .unwrap();
        self.keep(
            running,
            JobLines::new(
                &plan.display(),
                &crate::cmdline::display_as_written(&plan, None),
            ),
            label,
            FileAuthority::new(TrustStore::new(&root)),
            0,
            false,
        )
    }

    /// Remember that the look just taken at the job named became `reference`, so the next result
    /// for that job can name it. Called by the turn after it mints the slot, for a look that
    /// produced one: an empty look has none to name.
    pub(crate) fn record_look(&mut self, name: &str, reference: String) {
        if let Some(job) = self.running.get_mut(name) {
            job.looks.push(reference);
        }
    }

    /// Record what the job named was confined to, for the account of a failed finish.
    fn confined_as(&mut self, name: &str, confinement: Option<String>) {
        if let Some(job) = self.running.get_mut(name) {
            job.confinement = confinement;
        }
    }

    /// Keep the tree as the line of the job named found it, for the comparison when its end is
    /// reported (GROUND-1).
    fn tree_found(&mut self, name: &str, tree: TreeBefore) {
        if let Some(job) = self.running.get_mut(name) {
            job.tree = Some(tree);
        }
    }

    /// Every job that has ended and whose finish nobody has been told about yet (CMDLINE-14).
    ///
    /// Polled rather than waited on, so a turn asking between rounds is never held by a program
    /// behaving as intended: a job still running answers immediately. The exit is what makes this
    /// news, so the planner is told about a build that finished whether or not it thought to ask,
    /// and a job whose account has already been given is passed over rather than reported twice.
    ///
    /// The output is taken as it is handed over, which is what stops it being handed over again:
    /// `seen` moves, so a `job_output` call after this reports what arrived after this and not the
    /// whole log a second time.
    /// `cap` bounds what a finished job's report may spend, as it bounds a foreground run's
    /// output. Taken as an argument because a job outlives the round that started it and this is
    /// not the round's own call: the turn holds the figure it read from the settings and hands it
    /// over here.
    ///
    /// A job the person asked to stop is killed here, at the turn's next step after they asked
    /// (RUN-27), and reported with the rest.
    pub fn ended(&mut self, cap: usize) -> Vec<Ended> {
        let mut finished = Vec::new();
        for (name, job) in self.running.iter_mut() {
            if job.reported {
                continue;
            }
            let outcome = if let Some(ran_for) = job.stopped_by_the_person {
                // Carried out between turns, where no round was there to read the token.
                crate::report::Outcome::StoppedByTheUser(ran_for)
            } else if job.stop_asked() {
                crate::report::Outcome::StoppedByTheUser(job.stop_for_the_person())
            } else if job.stopped_by_a_signal() {
                job.signal_outcome()
            } else if job.running.ended() {
                how_it_ended(
                    job.running.codes(),
                    job.running.signals(),
                    &job.running.closed(),
                    job.confinement.as_deref(),
                )
            } else {
                continue;
            };
            job.reported = true;
            let changed_a_file = job.wrote();
            let (printed, look) = job.take();
            job.label = job.label_now();
            // Capped only where the planner may read it, exactly as a run's output is: what it may
            // not read is quarantined whole, and there is nothing of it in the conversation to
            // bound.
            let sample = if job.label.is_trusted() {
                bounded(&printed, cap)
            } else {
                None
            };
            let (printed, whole) = match sample {
                Some(sample) => (sample, Some(Labelled::new(printed, job.label))),
                None => (printed, None),
            };
            finished.push(Ended {
                name: name.clone(),
                line: job.line.clone(),
                said: job.said.clone(),
                outcome,
                printed: (!printed.is_empty()).then(|| Labelled::new(printed, job.label)),
                whole,
                look,
                covered_by_record: job.covered_by_record,
                read_asked: job.read_asked,
                changed_a_file,
            });
        }
        finished
    }

    /// What each running job has printed since the planner last looked, where more has arrived
    /// since the screen was last given it (WATCH-24).
    ///
    /// Read without moving the planner's cursor, so a `job_output` call afterwards returns every
    /// line. Whether more has arrived is a comparison of byte counts; the text goes to the caller
    /// to be released for display and is not read here.
    pub(crate) fn unread_output(&mut self) -> Vec<Unread> {
        let mut unread = Vec::new();
        for (name, job) in self.running.iter_mut() {
            if job.reported || !job.running.has_more(&job.drawn) {
                continue;
            }
            let mut cursor = job.seen.clone();
            let text = job.running.since(&mut cursor);
            job.drawn = cursor;
            if text.is_empty() {
                continue;
            }
            unread.push(Unread {
                name: name.clone(),
                text: Labelled::new(text, job.label_now()),
            });
        }
        unread
    }

    /// Tell the person what the turn ending does to each job nobody has had the finish of, then
    /// stop every one (RUN-26).
    ///
    /// Before the kill rather than after it, so the account says the job was running when the turn
    /// ended and not how a killed program exited. One that exited since the turn last looked is
    /// reported as what it did, from its exit codes.
    ///
    /// Asked whether its steps exited and not whether its pipes drained: nothing here reads what it
    /// printed, and waiting out the drain would hold the end of the turn for each one.
    ///
    /// One the person asked to stop, with no round left to read the request, is reported as their
    /// stop and recorded in the trail as one (RUN-27), so their stop is not told back to them as
    /// the turn's.
    ///
    /// Hands back how many of the jobs it reported left the workspace tree different from how
    /// their lines found it, compared once they have been stopped, since a killed job has still
    /// left what it wrote (GROUND-1).
    pub fn stop_all<S: Sink, R: Reporter>(
        &mut self,
        policy: &mut Policy<'_, S>,
        reporter: &mut R,
    ) -> usize {
        for (name, job) in self.running.iter_mut() {
            if job.reported {
                continue;
            }
            job.reported = true;
            let name = name.clone();
            let event = if job.running.steps_exited() {
                crate::report::JobEvent::Ended {
                    name,
                    outcome: how_it_ended(
                        job.running.codes(),
                        job.running.signals(),
                        &job.running.closed(),
                        job.confinement.as_deref(),
                    ),
                }
            } else if job.stop.is_requested() {
                let ran_for = job.running.ran_for();
                policy.record_job_stop(&name, ran_for);
                crate::report::JobEvent::Ended {
                    name,
                    outcome: crate::report::Outcome::StoppedByTheUser(ran_for),
                }
            } else {
                crate::report::JobEvent::Dropped { name }
            };
            reporter.job(event);
        }
        // After every job is classified, since a job that exits on the request would otherwise be
        // reported by how it exited. Stopped together, so jobs that ignore the request share one
        // grace period; each is then dropped with nothing left to wait for.
        let mut jobs: Vec<&mut crate::exec::Background> = self
            .running
            .values_mut()
            .map(|job| &mut job.running)
            .collect();
        crate::exec::Background::stop_together(&mut jobs);
        // A job reported before this call has had its mark taken already, so only the ones
        // reported here are compared.
        let wrote = self
            .running
            .values_mut()
            .map(Job::wrote)
            .filter(|wrote| *wrote)
            .count();
        self.running.clear();
        wrote
    }
}

/// What the planner is told happens to a job when the turn ends.
///
/// The driver's own words either way: which of the two holds is a fact about how the session was
/// started and says nothing about what any program printed.
fn what_becomes_of_a_job(kept: bool) -> &'static str {
    if kept {
        "It keeps running after this turn ends, until it exits, you or the user stops it, or the \
         session ends, and you are told at the start of each later turn which jobs are still \
         running."
    } else {
        "It is killed when this turn ends."
    }
}

/// What follows "without having to ask" in the note that starts a job: how to look at it before it
/// ends, where `job_output` is on the list, and the end of the sentence where it is not.
///
/// The driver's own words, chosen by the list the turn was offered and by nothing the job printed.
fn how_to_look_at_a_job(job: &str, beside: Beside) -> String {
    if beside.holds(Named::JobOutput) {
        format!(
            "; call job_output with \"{job}\" before then to see what it has printed. Each look \
             returns what arrived since the previous look, and the output before it stays in the \
             references of the earlier looks."
        )
    } else {
        ".".to_string()
    }
}

/// What a refusal of `vet_content` offers in place of the bytes, naming `spawn_processor` only
/// where the list holds it: "pass ref:N to spawn_processor, " or nothing.
///
/// The driver's own words, chosen by the list the turn was offered.
fn pass_to_a_processor(beside: Beside, slot: &impl std::fmt::Display) -> String {
    if beside.holds(Named::SpawnProcessor) {
        format!("pass {slot} to spawn_processor, ")
    } else {
        String::new()
    }
}

/// What `spawn_processor` says to a planner that gave it nothing to read: where a new file comes
/// from, naming `write_file` only where the list holds it, and nothing where it does not.
fn write_it_yourself(beside: Beside) -> &'static str {
    if beside.holds(Named::WriteFile) {
        " To make a file from nothing, write it yourself with write_file."
    } else {
        ""
    }
}

/// What the notice of a checkout a `spawn_agent` call made says about `lsp`, naming it only where
/// the collecting turn's own list holds it (TOOL-6): a checkout's delegate is never offered `lsp`
/// (CHECKOUT-setup strips it unconditionally), but that is a fact about the delegate, and naming
/// the tool here is a fact read off the collecting turn's own list instead.
///
/// `plural` is whether the sentence speaks of more than one checkout: "is not offered" for one,
/// "or is offered" for several, since the plural sentence is already phrased as a negation
/// ("None ... or is offered lsp").
fn lsp_clause(beside: Beside, plural: bool) -> &'static str {
    if !beside.holds(Named::Lsp) {
        ""
    } else if plural {
        " or is offered lsp"
    } else {
        " and is not offered lsp"
    }
}

/// What a checkout refusal names as the reader that would have opened the repository: `read_git`
/// itself where this turn's list holds it, and a name this turn can act on otherwise (TOOL-6). The
/// turn asking `spawn_agent` or `apply_checkout` for a checkout is not the turn that would have
/// called `read_git`, and may not be offered it.
fn checkout_reader(beside: Beside) -> &'static str {
    if beside.holds(Named::ReadGit) {
        "read_git"
    } else {
        "the repository reader"
    }
}

/// How a job that has ended finished, read off the exit code and the signal of each of its steps.
///
/// Structure and nothing else: no byte of what the pipeline printed reaches this. Failed rather
/// than succeeded where a step did not exit zero, because a planner that waited for a build and was
/// told it exited 0 reports a red build as green, and where the output is quarantined that sentence
/// is the only account of it the planner ever gets.
fn how_it_ended(
    codes: &[Option<i32>],
    signals: &[Option<i32>],
    closed: &[bool],
    confinement: Option<&str>,
) -> crate::report::Outcome {
    let failed = failed_steps(codes, signals, closed);
    if !failed.is_empty() {
        crate::report::Outcome::Failed {
            detail: failed,
            confinement: confinement.map(str::to_string),
        }
    } else if closed.iter().any(|closed| *closed) {
        crate::report::Outcome::Closed {
            steps: closed
                .iter()
                .enumerate()
                .filter(|(_, closed)| **closed)
                .map(|(at, _)| at + 1)
                .collect(),
        }
    } else {
        crate::report::Outcome::Succeeded
    }
}

/// The steps that did not exit zero, each numbered from 1 by its place among the steps that ran,
/// joined by commas. A step ended by `SIGPIPE` after a later step of its pipeline exited zero is
/// not one of them (RUN-13): `closed` marks it.
///
/// A step that exited says its code. A step a signal ended says the signal's number and, where the
/// system names it, the name, so a planner can tell a program the kernel killed for memory
/// (`SIGKILL`) from one that was asked to end (`SIGTERM`) or that crashed (`SIGSEGV`). A step with
/// neither, which is one whose status could not be read, says only that it ended without an exit
/// code, since a signal that was not read is not one to name.
///
/// Read off the exit codes, the signals and the pipeline layout alone, so no byte of what the steps
/// printed reaches it. Empty where every step exited zero or was closed by a later step.
fn failed_steps(codes: &[Option<i32>], signals: &[Option<i32>], closed: &[bool]) -> String {
    codes
        .iter()
        .enumerate()
        .filter(|(at, code)| **code != Some(0) && !closed.get(*at).copied().unwrap_or(false))
        .map(
            |(at, code)| match (code, signals.get(at).copied().flatten()) {
                (Some(code), _) => format!("step {} exited {code}", at + 1),
                (None, Some(signal)) => format!(
                    "step {} was ended by {}",
                    at + 1,
                    crate::exec::signal_phrase(signal)
                ),
                (None, None) => format!("step {} ended without an exit code", at + 1),
            },
        )
        .collect::<Vec<_>>()
        .join(", ")
}

/// What one tool produced, before dispatch wraps it up.
///
/// Two audiences, kept apart. `text` goes to the planner and stays labelled, because the kernel
/// decides whether the planner may see it. `note` and `changes` go to a screen and are already
/// released, because a person is allowed to read what a planner is not.
struct Produced {
    cancelled: Option<crate::outcome::Cancellation>,
    text: Labelled<String>,
    origin: String,
    /// What to tell the person watching. A few words, never the result itself.
    note: String,
    failed: bool,
    /// Set by a read that reserved a file instead of opening it.
    deferred: Option<Deferral>,
    /// Set by a listing the planner may not read.
    entries: Option<Entries>,
    /// Whether the result is a sample rather than the whole answer, because a cap was reached.
    ///
    /// A fact about the result rather than text inside it, so it survives quarantine. The notice
    /// written into `text` reaches the planner only when the planner may read `text` at all,
    /// which by default it may not.
    incomplete: bool,
    /// Whether a listing's walk stopped at the time budget, as [`Output::timed_out`].
    timed_out: bool,
    /// How many symbolic links the walk behind this result did not enter, as [`Output::links`].
    links: usize,
    /// The sentence for a pattern that selected no file, as [`Output::unmatched`].
    unmatched: Option<String>,
    /// Where a further call resumes, or that this page fell past the end of the matches.
    ///
    /// Beside `incomplete` and for the same reason: knowing the answer is a sample is no use
    /// without the one argument that gets the rest of it, and by default the planner reads a
    /// reference rather than the body the notice is written into.
    paging: Option<Paging>,
    /// What a run printed, whole, where a cap cut down what the planner is shown.
    ///
    /// The cap is on what enters the conversation, not on what the command printed, so the whole
    /// of it is quarantined by the turn and the planner is handed the reference beside the sample.
    /// Without it the one case where the cap bites would be the one case with no way back to the
    /// middle short of running the command again.
    whole: Option<Labelled<String>>,
    /// Content that travels beside `text` under a label of its own.
    apart: Option<Apart>,
    /// What the person typed beside their answer, as [`Output::told`].
    told: Option<Labelled<String>>,
    /// The change a write made, for showing under the line it belongs to.
    changes: Vec<crate::diff::Change>,
    /// What the kernel counted for the diff behind `changes`, set where a write changed a file.
    written: Option<crate::report::Written>,
    /// Whether those lines are content nobody vouched for.
    untrusted: bool,
    /// Whether this call left a file on disk different from how it found it.
    ///
    /// The outcome, not the request. A write the person declined and a write plan mode refused
    /// both answer the planner and change nothing, and a turn that counted either as a change
    /// would tell the person their untouched workspace had been edited.
    changed_a_file: bool,
    /// Whether this call wrote a file that landed in the session's own directory. See
    /// [`Output::wrote_in_the_session_directory`].
    wrote_in_the_session_directory: bool,
    /// Whether this call ran a program.
    ///
    /// The outcome, for the reason above: a run the person declined is a command that did not
    /// happen, and a turn that counted it would say a change had been built when nothing had
    /// compiled it.
    ran_a_program: bool,
    /// Whether the program it ran ended badly (OPEN-1).
    command_failed: bool,
    /// Whether a program was started, including a call that was then refused or failed.
    started_a_program: bool,
    /// See [`Output::run_may_change_files`].
    run_may_change_files: bool,
    /// The directory each git stage of the command worked in.
    ran_git_in: Vec<std::path::PathBuf>,
    /// Which document a processor's answer is about, where it produced one.
    ///
    /// `Some(None)` is a processor that was given several documents and told which of them it
    /// was about by nobody: its answer is for no file in particular, and the turn records that
    /// so a write of it is refused rather than guessed at.
    answers_for: Option<bravebot_core::processor::ProcessorSpec>,
    /// What an isolated processor said about what it did, for the person watching.
    ///
    /// Never part of `text`, which is what the planner is told about: this half of a processor's
    /// answer reaches a screen and stops there.
    said: Option<Labelled<String>>,
    /// The number a processor chose, where the planner asked it to choose, as [`Output::chosen`].
    chosen: Option<ChosenNumber>,
    /// The sentence saying a processor was answered by a model other than the one named for processors.
    substituted: Option<String>,
    /// The content alone, where `text` has the driver's own notes after it.
    glimpsed: Option<Labelled<String>>,
    /// The window of a file `text` shows, for the record of what the planner has been shown.
    window: Option<crate::conversation::ReadWindow>,
    /// The count of tasks left open, where `text` is the planner's task list, accepted. A fact
    /// about the call rather than anything the turn reads out of `text`, so it can record where
    /// the list sits, and how much of it is open, without reading it.
    task_list: Option<Labelled<usize>>,
    /// How many of the open tasks in `task_list` wait on something outside the planner's hands,
    /// counted and labelled as `task_list` is. `Some` exactly where `task_list` is.
    waiting_tasks: Option<Labelled<usize>>,
    /// Whether `text` is workspace content rather than the driver's own words about the call.
    ///
    /// What the kernel does with a result is worth reporting only where the result is content:
    /// saying "the model has read it" about a sentence the driver wrote to explain a refusal is
    /// true, useless, and read by the person as a claim about their file.
    content: bool,
    /// What the tool spent at the model. Only a processor, an advisor, a delegate and a check spend
    /// anything.
    usage: Usage,
    /// Whether `usage` is a check's or a processor's, which a delegate's token budget does not
    /// count.
    usage_is_unbudgeted: bool,
    /// Retained so a parent can clip delegate requests to its own waits.
    inference_interval: Option<crate::timing::Interval>,
    /// Set where the check waited on could not be made, so no request completed.
    unanswered: bool,
    /// The command whose output this is and how it ended, where a run produced it.
    ///
    /// Recorded on the slot by the turn loop, because only a slot minted from a command may be
    /// offered to the user for reading.
    printed_by: Option<crate::report::Command>,
    /// As [`Output::look`].
    look: Option<Look>,
    /// Whether the line ran unasked because a record already covers it.
    ///
    /// What it changes is the advice on a quarantined result: telling a planner that a person
    /// vouching for every stage would make the output visible is advice about a prompt, and no
    /// prompt will be drawn for this line again until somebody deletes the entry.
    covered_by_record: bool,
    /// Whether the planner asked to read what the line printed, or what `fetch_url` or a server's
    /// tool returned, in this result.
    read_asked: bool,
    /// When the planner asked for the next tick of a self-paced loop.
    wakeup: Option<crate::turn::Wakeup>,
    /// The path the planner asked to have a standing watch armed on, with the look taken at the call.
    watch: Option<crate::watch::Armed>,
    /// The link the planner asked to have recorded, already checked against the person's lines.
    link: Option<crate::links::LinkSet>,
    /// The issues and pull requests the call's arguments named, as [`Output::references`].
    references: Vec<crate::links::Reference>,
    /// The kind of item the command created, as [`Output::created`].
    created: Option<crate::links::LinkKind>,
    /// The media type, where what this produced is a picture.
    ///
    /// Recorded on the slot by the turn loop, and what makes a picture reach a processor as a part
    /// rather than as a body. The driver's own, from a table of extensions.
    picture: Option<String>,
    /// How many bytes the file `picture` names holds, as [`Output::picture_bytes`].
    picture_bytes: usize,
    /// The pictures a server's tool result carried beside its text, as [`Output::pictures`].
    pictures: Vec<bravebot_mcp::Picture>,
    /// How many parts of a server's tool result its transport left out, as [`Output::left_out`].
    left_out: usize,
    /// Which spelling of the planner's name a read used, as [`Output::respelled`].
    respelled: Option<Respelled>,
    /// A picture `vet_content` promoted, for the turn to attach after the round's results.
    attached: Option<bravebot_core::vetting::Attached>,
    /// The delegates the kernel has approved and nobody has started yet.
    ///
    /// Started by the turn rather than here, because a delegate outlives the call that asked for
    /// one: the call answers immediately and the delegate goes on working. The turn is what is
    /// still there when it finishes.
    ///
    /// A list because one call may fan a task out over several.
    delegate: Vec<(crate::report::DelegateId, crate::delegate::Seeded)>,
    /// How many of `delegate` have to end before the planner is asked again (DELEGATE-17).
    wake_after: usize,
    /// The skill the planner loaded and how its file asks the rounds after it to run.
    ///
    /// Settled by the turn rather than here, because what it changes is the next request rather
    /// than this result: the model and the effort are the turn's to hold, and whether a name this
    /// machine cannot reach is worth a line is a question about configuration.
    ///
    /// Absent where nothing was loaded and where what was loaded asked for neither.
    loaded: Option<(String, crate::skills::RunsAs)>,
    /// The name of the skill this call loaded, as the catalogue spells it.
    skill: Option<String>,
    /// The tag for a `message_session` result whose message was delivered (MSG-11).
    message_sent: Option<crate::conversation::Composed>,
    /// The wire name of the server tool the planner loaded (SERVERS-16), for the turn to offer.
    tool_loaded: Option<String>,
}

impl Produced {
    /// A result the person watching is told about in the driver's own summary of it.
    fn new(text: Labelled<String>, origin: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            text,
            origin: origin.into(),
            note: note.into(),
            failed: false,
            cancelled: None,
            deferred: None,
            entries: None,
            incomplete: false,
            timed_out: false,
            links: 0,
            unmatched: None,
            paging: None,
            whole: None,
            apart: None,
            told: None,
            changes: Vec::new(),
            written: None,
            untrusted: false,
            changed_a_file: false,
            wrote_in_the_session_directory: false,
            ran_a_program: false,
            command_failed: false,
            started_a_program: false,
            run_may_change_files: false,
            ran_git_in: Vec::new(),
            answers_for: None,
            said: None,
            chosen: None,
            substituted: None,
            glimpsed: None,
            window: None,
            task_list: None,
            waiting_tasks: None,
            content: false,
            usage: Usage::default(),
            usage_is_unbudgeted: false,
            inference_interval: None,
            unanswered: false,
            printed_by: None,
            look: None,
            covered_by_record: false,
            read_asked: false,
            picture: None,
            picture_bytes: 0,
            pictures: Vec::new(),
            left_out: 0,
            respelled: None,
            attached: None,
            wakeup: None,
            watch: None,
            link: None,
            references: Vec::new(),
            created: None,
            delegate: Vec::new(),
            wake_after: 1,
            loaded: None,
            skill: None,
            message_sent: None,
            tool_loaded: None,
        }
    }

    /// A tool's own words about something that did not happen: an error, or a refusal. The driver
    /// wrote them, so they are trusted, and they double as the line the person watching sees.
    ///
    /// Distinct from workspace content, which is never trusted unless the trust map says so.
    fn problem(text: impl Into<String>) -> Produced {
        let text = text.into();
        Produced {
            failed: true,
            ..Produced::new(Labelled::trusted(text.clone()), String::new(), text)
        }
    }

    /// The number a processor chose, for the turn to store as a reference of its own kind.
    ///
    /// The result has no text: what the planner is told is the reference the turn makes, and it is
    /// the same words for every number. The tool sets this from the planner's own argument
    /// (`choose_upto`), so a call that asked for a number is a call that produced one, 0 included.
    fn chosen(choice: ChosenNumber, origin: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            chosen: Some(choice),
            ..Produced::new(Labelled::trusted(String::new()), origin, note)
        }
    }

    /// A refusal the planner is told the fact of and the person watching is told the detail of.
    ///
    /// [`Produced::problem`] says the same thing to both, which is right for almost everything:
    /// an error is about the call, and the planner is the one that has to do something
    /// differently. A credential found in what a write would leave is not about the call. What
    /// was found, where, and what it looks like is a record the planner must not be given
    /// ([`bravebot_core::credentials`]), and a screen is not the planner's context, so the detail
    /// goes to the note and the text says only that the write did not happen.
    fn refused_with_a_note(text: impl Into<String>, note: impl Into<String>) -> Produced {
        Produced {
            note: note.into(),
            ..Produced::problem(text)
        }
    }

    /// A delegate the kernel approved, for the turn to start.
    fn delegating(
        mut self,
        id: crate::report::DelegateId,
        seeded: crate::delegate::Seeded,
    ) -> Self {
        self.delegate.push((id, seeded));
        self
    }

    /// Say that what this produced is workspace content, not the driver's words about it.
    fn of_content(mut self) -> Self {
        self.content = true;
        self
    }

    /// Say that a file on disk is now different from how this call found it.
    ///
    /// Only where the write landed. Every refusal above the write leaves the file as it was, and
    /// the turn says at the end whether what changed was built.
    fn having_changed_a_file(mut self) -> Self {
        self.changed_a_file = true;
        self
    }

    /// [`Produced::having_changed_a_file`] for a write that landed at `landed`, the destination the
    /// workspace resolved, unless that is inside the session's own directory.
    ///
    /// What a turn writes there is for its own use, outside the project and gone when the session
    /// ends, so it leaves nothing to build and is not a change the work record counts. Decided on
    /// where the write landed and never on the name typed, so a link in the project into that
    /// directory is not counted either. Nothing the file holds is read. The write is marked
    /// apart, for the tally a delegate's finished note gives of it (GROUND-2).
    fn having_changed_a_file_at(mut self, workspace: &Workspace, landed: &std::path::Path) -> Self {
        if workspace.reaches_scratch(landed) {
            self.wrote_in_the_session_directory = true;
            self
        } else {
            self.having_changed_a_file()
        }
    }

    /// Say that this call ran a program.
    ///
    /// Only where it started. A line the person declined and one that failed to launch have both
    /// built nothing.
    fn having_run_a_program(mut self) -> Self {
        self.ran_a_program = true;
        self.started_a_program = true;
        self
    }

    /// Say that a program was started, though the call did not end as a run.
    ///
    /// For the returns after a stage has spawned: the checkout holds what the program did, and
    /// the driver has no record of it (CHECKOUT-15).
    fn having_started_a_program(mut self) -> Self {
        self.started_a_program = true;
        self
    }

    /// Say what this produced is a picture, and the media type it is to be sent under, and how many
    /// bytes the file holds.
    ///
    /// Its slot is marked, which is what lets a processor be given it as a part rather than as a
    /// body. The planner is never shown it, whatever the label says.
    fn of_a_picture(mut self, media: &str, bytes: usize) -> Self {
        self.picture = Some(media.to_string());
        self.picture_bytes = bytes;
        self
    }

    /// Say this carries a picture `vet_content` promoted, for the turn to attach.
    fn attaching(mut self, attached: bravebot_core::vetting::Attached) -> Self {
        self.attached = Some(attached);
        self
    }

    /// Say a pipeline was left running under this name.
    ///
    /// The name is the driver's own, so the planner is told it as text rather than being handed a
    /// reference: there is nothing quarantined about it, and nothing has been printed yet.
    ///
    /// It says the finish arrives by itself, because it does (CMDLINE-14), and a planner that does
    /// not know that spends a round per look asking whether a build has finished.
    fn started_in_the_background(mut self, job: String, kept: bool, beside: Beside) -> Self {
        self.text = Labelled::trusted(format!(
            "started in the background as {job}. Nothing has been read from it yet. If it ends \
             while this turn is still going you are told so, with how it ended and what it \
             printed, without having to ask{} {}",
            how_to_look_at_a_job(&job, beside),
            what_becomes_of_a_job(kept)
        ));
        self
    }

    /// Say a person stopped waiting for a line, and the job it went on running as.
    ///
    /// Whose choice it was comes first, because a planner that reads this as its own line coming
    /// back early will run it again. Driver-made text, like [`Produced::started_in_the_background`]:
    /// a name minted here and a count of seconds read off a clock.
    fn moved_to_the_background(
        mut self,
        job: String,
        after: std::time::Duration,
        kept: bool,
        beside: Beside,
    ) -> Self {
        self.text = Labelled::trusted(format!(
            "the user moved this command to the background after {:.1}s, and it is still running \
             as {job}. Do not run it again. Nothing has been read from it yet, including what it \
             printed before the move. If it ends while this turn is still going you are told so, \
             with how it ended and what it printed, without having to ask{} It no longer has a \
             deadline. {}",
            after.as_secs_f64(),
            how_to_look_at_a_job(&job, beside),
            what_becomes_of_a_job(kept)
        ));
        self
    }

    /// A read that reserved a file rather than opening it.
    fn deferring(path: Labelled<String>, origin: String, bytes: usize) -> Self {
        Self::new(
            Labelled::trusted(String::new()),
            origin.clone(),
            format!("{bytes} bytes, quarantined"),
        )
        .with_deferral(Deferral {
            path,
            origin,
            bytes,
        })
    }

    fn with_deferral(mut self, deferral: Deferral) -> Self {
        self.deferred = Some(deferral);
        self
    }

    /// Say the result was cut short by a cap, whoever ends up being allowed to read it.
    fn capped(mut self, incomplete: bool) -> Self {
        self.incomplete = incomplete;
        self
    }

    /// Say a listing's walk stopped at the time budget, whoever ends up being allowed to read it.
    fn timed_out(mut self, timed_out: bool) -> Self {
        self.timed_out = timed_out;
        self
    }

    /// How many symbolic links the walk did not enter, whoever ends up being allowed to read it.
    fn linked(mut self, links: usize) -> Self {
        self.links = links;
        self
    }

    /// Say a listing's pattern selected no file, whoever ends up being allowed to read it.
    fn unmatched(mut self, unmatched: Option<String>) -> Self {
        self.unmatched = unmatched;
        self
    }

    /// Where a further call continues, or that this page fell past the end.
    fn paging(mut self, paging: Option<Paging>) -> Self {
        self.paging = paging;
        self
    }

    /// A listing whose entries were reserved rather than shown.
    fn with_entries(mut self, entries: Entries) -> Self {
        self.entries = Some(entries);
        self
    }

    fn with_changes(mut self, changes: Vec<crate::diff::Change>) -> Self {
        self.changes = changes;
        self
    }

    fn with_written(mut self, written: Option<crate::report::Written>) -> Self {
        self.written = written;
        self
    }

    /// Say that the change came from content nobody vouched for.
    fn marked_untrusted(mut self, untrusted: bool) -> Self {
        self.untrusted = untrusted;
        self
    }

    fn costing(mut self, usage: Usage) -> Self {
        self.usage = usage;
        self
    }

    /// Say what a check over content nobody vouched for cost. Set by the tool whether or not the
    /// check was made, so the turn adds it to what it spent and never to what a delegate's token
    /// budget counts (DELEGATE-29).
    fn checking(mut self, usage: Usage) -> Self {
        self.usage = usage;
        self.usage_is_unbudgeted = true;
        self
    }

    /// Say what a processor cost. Set by the tool whether or not the processor answered, so the
    /// turn adds it to what it spent and never to what a delegate's token budget counts: the
    /// processor read content nobody vouched for, and how much it wrote depends on that content
    /// (DELEGATE-29, PROC-16).
    fn processing(mut self, usage: Usage) -> Self {
        self.usage = usage;
        self.usage_is_unbudgeted = true;
        self
    }

    /// Say which spelling of the planner's name a read used, where it used another. Not said of a
    /// failure, which is already worded about the name that failed.
    fn respelling(mut self, respelled: Option<Respelled>) -> Self {
        if !self.failed {
            self.respelled = respelled;
        }
        self
    }

    /// Say when the tool waited on the model for this.
    fn waiting(mut self, interval: Option<crate::timing::Interval>) -> Self {
        self.inference_interval = interval;
        self
    }

    /// Say that the check waited on could not be made, so the wait is not a completed request.
    fn unanswered(mut self, unanswered: bool) -> Self {
        self.unanswered = unanswered;
        self
    }

    /// Say when the planner asked for the next tick of a self-paced loop.
    fn scheduling(mut self, wakeup: crate::turn::Wakeup) -> Self {
        self.wakeup = Some(wakeup);
        self
    }

    /// Say which path the planner asked to have a standing watch armed on.
    fn watching(mut self, path: impl Into<String>, looked: crate::watch::Looked) -> Self {
        self.watch = Some(crate::watch::Armed {
            path: path.into(),
            looked,
        });
        self
    }

    /// Say which link the planner asked to have recorded.
    fn linking(mut self, kind: crate::links::LinkKind, url: impl Into<String>) -> Self {
        self.link = Some(crate::links::LinkSet {
            kind,
            url: url.into(),
            inferred: false,
        });
        self
    }
}

/// Whether a call by this name needs the lines the person typed, which only `set_link` does
/// outside the auto mode.
pub(crate) fn reads_typed_lines(name: &str) -> bool {
    strip_namespace(name) == "set_link"
}

/// Whether a turn whose front end `keeps` the links `set_link` records offers the tool.
///
/// Never in an incognito session, which saves nothing to keep a link in (SETLINK-2).
pub(crate) fn keeps_links(keeps: bool) -> bool {
    keeps && !bravebot_core::incognito::engaged()
}

/// Whether a call by this name changes a file.
///
/// The two write tools named in one place, so the driver can ask the question without knowing
/// which they are. Asked of a name the planner sent, so the namespace some models put in front
/// comes off first, exactly as it does before the call is dispatched.
pub(crate) fn writes_a_file(name: &str) -> bool {
    matches!(
        strip_namespace(name),
        "write_file" | "edit_file" | "apply_checkout" | "download_url"
    )
}

/// Whether the sandbox mode keeps the file tools from writing, as it keeps programs (SANDBOX-22).
///
/// A turn that confines nothing has no mode in force, which is why `confine_runs` is read too.
pub(crate) fn sandbox_refuses_writes(
    confine_runs: bool,
    sandbox: bravebot_sandbox::SandboxMode,
) -> bool {
    confine_runs && sandbox == bravebot_sandbox::SandboxMode::ReadOnly
}

/// Whether a call by this name runs a program.
pub(crate) fn runs_a_program(name: &str) -> bool {
    strip_namespace(name) == "run"
}

/// Whether this set of tools can run a program at all.
pub(crate) fn offer_runs(offered: &[Tool]) -> bool {
    offered
        .iter()
        .any(|tool| runs_a_program(&tool.function.name))
}

/// Whether this set of tools can change a file at all.
///
/// A run offered no write tool cannot be told to write something: a reader delegate is doing
/// exactly what it was built to do by never writing.
pub(crate) fn offer_writes(offered: &[Tool]) -> bool {
    offered
        .iter()
        .any(|tool| writes_a_file(&tool.function.name))
}

/// A tool name without the group some models put in front of it.
///
/// Only the one prefix, and only where something is left after it: this is for a name that means
/// one of ours, not a general invitation to guess.
pub(crate) fn strip_namespace(name: &str) -> &str {
    for prefix in ["functions.", "functions_"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && !rest.is_empty()
        {
            return rest;
        }
    }
    name
}

/// Which argument names what a call is about.
///
/// Chosen from the tool's own name, which dispatch already matches on, so this decides nothing
/// new. `None` for a tool with no single argument naming a target.
fn target_key(tool: &str) -> Option<&'static str> {
    match tool {
        "read_file" | "write_file" | "edit_file" | "download_url" => Some("path"),
        "apply_checkout" => Some("checkout"),
        "list_files" => Some("directory"),
        "search" => Some("pattern"),
        "read_git" => Some("query"),
        "repo_map" => Some("directory"),
        "lsp" => Some("path"),
        "load_skill" | "load_tool" => Some("name"),
        "search_skills" => Some("query"),
        "fetch_url" => Some("url"),
        "web_search" => Some("q"),
        "job_output" => Some("job"),
        "vet_content" => Some("ref"),
        "run" => Some("command"),
        "request_path" => Some("path"),
        "read_output" => Some("ref"),
        "watch_file" => Some("path"),
        "advisor" => Some("question"),
        "message_session" => Some("target"),
        "remember" => Some("key"),
        "propose_rule" => Some("text"),
        "set_link" => Some("url"),
        _ => None,
    }
}

/// Whether the target of a call to `tool` is a file's name or path, which a screen draws spelled
/// ([`Activity::shown_target`]).
///
/// Read off the same table as the target itself ([`target_key`]), so a tool added there with a
/// `path` or a `directory` is spelled without being listed twice. The tool's own name, which
/// dispatch matched against its table, decides it: the target is never read to find out.
pub fn names_a_file(tool: &str) -> bool {
    matches!(target_key(tool), Some("path" | "directory"))
}

/// How a `run` command line is drawn: its first line, and nothing at all when it carries a
/// value that declared itself a credential.
///
/// `run` refuses such a line before anyone is shown it (CRED-11), and the call is announced
/// before `run` is reached, so the line is scanned here first. The whole of it is scanned, since a
/// value on a later line is as visible as one on the first. Several lines are drawn as the first
/// and "...", because a transcript row is one row.
fn command_as_drawn(line: &str) -> String {
    if declares_a_credential("the command line", line) {
        return String::new();
    }
    let mut lines = line.lines().map(str::trim).filter(|line| !line.is_empty());
    let first = lines.next().unwrap_or_default();
    if lines.next().is_some() {
        format!("{first} ...")
    } else {
        first.to_string()
    }
}

/// Whether `text` carries a value that declared itself a credential, on any of its lines.
fn declares_a_credential(what: &str, text: &str) -> bool {
    bravebot_core::credentials::scan(what, text, bravebot_core::credentials::run_salt())
        .iter()
        .any(|finding| finding.kind.is_declared())
}

/// How a call to a server's tool is drawn: `alias:tool`, then the arguments the server is sent,
/// as compact JSON. Compact JSON escapes a line break inside a value, so the arguments are one
/// line as they stand, and one longer than a previewed line is cut to that width and "...", for
/// the reason a preview is: one argument holding a document would otherwise wrap across the
/// whole screen.
///
/// They are left off where any string in them, a key or a value, carries a value that declared
/// itself a credential, since a transcript is one of the places CRED-11 keeps one out of. Each
/// string is scanned as it is rather than as JSON spells it: the escaping takes the line breaks
/// out of a private key's armour, and the armour is how the scan recognises one. The scan reads
/// all of them before anything is cut, so a value cannot be drawn in part.
fn server_call_as_drawn(named: &str, compact: &str) -> String {
    let parsed: Value = serde_json::from_str(compact).unwrap_or(Value::Null);
    let mut strings = Vec::new();
    strings_in(&parsed, &mut strings);
    if declares_a_credential("the arguments", &strings.join("\n")) {
        return named.to_string();
    }
    let width = crate::turn::PREVIEW_WIDTH;
    if compact.chars().count() > width {
        let cut: String = compact.chars().take(width).collect();
        return format!("{named} {cut} ...");
    }
    format!("{named} {compact}")
}

/// Every key and string value in `value`, at any depth.
fn strings_in<'a>(value: &'a Value, into: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => into.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings_in(item, into)),
        Value::Object(pairs) => pairs.iter().for_each(|(key, item)| {
            into.push(key);
            strings_in(item, into);
        }),
        _ => {}
    }
}

/// `alias:tool` from the wire name a server's tool was offered under, less its `mcp__`.
///
/// An alias and a tool word may each hold `__`, so a name that can be divided in more than one
/// place is drawn as it was offered rather than divided at a guess.
fn server_named(wire: &str) -> String {
    let dividers = wire
        .as_bytes()
        .windows(2)
        .filter(|pair| *pair == b"__")
        .count();
    match wire.split_once("__") {
        Some((alias, tool)) if dividers == 1 => format!("{alias}:{tool}"),
        _ => wire.to_string(),
    }
}

/// What a call is about, for the line shown while it runs, and the same target as the planner
/// wrote it.
///
/// The second is what an export writes (SESSION-17): the first can name a file the planner was
/// not told, and an export is a file a later turn may read. Both come out of one release, shaped
/// together, so no second pass reads the text.
///
/// Which argument names the target depends on the tool, so the key is chosen from the tool's
/// own name. The value is the model's word for it, released to a screen and nowhere else: it
/// goes on a line a person reads and is never compared, matched, or routed anywhere.
///
/// `server` is `alias:tool` where the call is to a server's tool. A server is sent the whole of
/// the arguments, so the whole of them is what such a call is about.
fn target_of<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
    server: Option<&str>,
    slots: &SlotStore,
    arguments: &Value,
) -> (String, String) {
    // A question is about nothing in the workspace, and its subject is the question itself,
    // which the person is about to read anyway. How many were asked is the useful thing on a
    // line that goes by while they answer, and a count is structure rather than content, so
    // nothing is released to say it.
    if tool == "propose_plan" {
        return (String::new(), String::new());
    }
    if tool == "ask_user" {
        let asked = match arguments.get("questions").and_then(Value::as_array) {
            Some(asked) if asked.len() > 1 => tally(asked.len(), "question", "questions"),
            _ => String::new(),
        };
        return (asked.clone(), asked);
    }

    // A processor has no single argument naming a target: what it is working on is the set of
    // references it was given, which are names the driver handed out and can read back.
    let named = if tool == "spawn_processor" {
        references_in(arguments)
    } else {
        let text = if server.is_some() {
            Some(arguments.to_string())
        } else {
            let Some(key) = target_key(tool) else {
                return (String::new(), String::new());
            };

            // A call that named a reference instead of a path says so with the reference, which
            // is then resolved below: it is the planner that cannot know the name, not the person.
            let key = if !names(arguments, key) && names(arguments, "path_ref") {
                "path_ref"
            } else {
                key
            };
            target_text(arguments, key)
        };

        match text {
            Some(text) => Labelled::new(text, bravebot_core::label::Label::untrusted_public()),
            None => return (String::new(), String::new()),
        }
    };

    // The line a person reads says which file, always. A reference means something to the
    // planner and nothing at all to the person watching their own workspace being worked on.
    //
    // Substituted inside the kernel, while the value is still labelled, because finding a
    // reference in the text is reading the text. A driver that released the bytes first and
    // searched them afterwards would be inspecting content under a witness minted to put it on a
    // screen, which LABEL-6 refuses. Text with no reference in it comes back as it went in, so
    // there is nothing left here to test it for.
    //
    // A server is sent a reference as the planner wrote it, so it is drawn as written too, and no
    // file is named for a server's call: naming them records a release the line never makes.
    let display_names = match server {
        Some(_) => Vec::new(),
        None => policy.names_for_display(slots),
    };
    let shaped = policy.render_in_place(tool, &named, |text| {
        if let Some(server) = server {
            let drawn = server_call_as_drawn(server, &text);
            return (drawn.clone(), drawn);
        }
        let text = if tool == "run" {
            command_as_drawn(&text)
        } else {
            text
        };
        (name_references(&text, &display_names), text)
    });
    let proof = policy.authorise_display_release("what a tool is working on");
    shaped.declassify(&proof)
}

/// Why the planner made a call, in its own words, for the line the call is drawn on.
///
/// At the integrity of the context it was written in, and released to a screen and nowhere else
/// (TOOL-5). No tool is handed it and nothing is decided from it, so a call without one runs as a
/// call with one does.
fn why_of<S: Sink>(policy: &mut Policy<'_, S>, tool: &str, arguments: &Value) -> String {
    let said = policy.label_model_output(
        tool,
        arguments
            .get(WHY)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    let proof = policy.authorise_display_release("why the planner made a call");
    said.declassify(&proof)
}

/// The reason a stored call gave, for the transcript of a session read back off disk.
///
/// No policy, for the reason [`describe_stored_call`] has none: this is the text a person watching
/// was shown the first time round, going back to a screen.
pub fn stored_why(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|parsed| parsed.get(WHY).and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// How a call reads in the transcript of a session read back off disk.
///
/// The same words a live call is announced with, from the same two functions, so a resumed
/// transcript and a running one describe the same call the same way.
///
/// No policy, and none to be had: a stored conversation is plain messages, whose labels went
/// when it was written down. Nothing here is being released that was not released already. The
/// call is in the record because the planner was allowed to hold it, and this puts the same
/// words on a screen that the person watching saw the first time round. It reaches a transcript
/// line and nothing else.
pub fn describe_stored_call(tool: &str, arguments: &str) -> String {
    let tool = strip_namespace(tool);
    let parsed: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);

    let target = if let Some(wire) = tool.strip_prefix("mcp__") {
        // A call whose arguments are not JSON is announced as the verb alone, since it never runs.
        serde_json::from_str::<Value>(arguments)
            .map(|parsed| server_call_as_drawn(&server_named(wire), &parsed.to_string()))
            .unwrap_or_default()
    } else if tool == "spawn_processor" {
        let names: Vec<&str> = parsed
            .get("reads")
            .and_then(Value::as_array)
            .map(|entries| entries.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        names.join(", ")
    } else {
        target_key(tool)
            .and_then(|key| target_text(&parsed, key))
            .map(|text| {
                if tool == "run" {
                    command_as_drawn(&text)
                } else {
                    text
                }
            })
            .unwrap_or_default()
    };

    Activity::running(crate::report::verb_for(tool), target)
        .naming_a_file(names_a_file(tool))
        .line()
}

/// Shape a count out of a labelled result and release it to the person watching.
///
/// The reshape happens inside the kernel and only the shaped line comes out, so the driver
/// counts nothing it is not allowed to hold. A screen is one of the destinations a display
/// release exists for, and a count cannot feed an effect.
fn note_for<S: Sink, T: Clone>(
    policy: &mut Policy<'_, S>,
    tool: &'static str,
    content: &Labelled<T>,
    shape: impl FnOnce(T) -> String,
) -> String {
    let shaped = policy.render_in_place(tool, content, shape);
    let proof = policy.authorise_display_release("what a tool produced");
    shaped.declassify(&proof)
}

/// A count with the right noun, so a line does not read "1 lines".
pub(crate) fn tally(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// What a search says about the symbolic links its walk did not enter, without the punctuation that
/// sets it off: a number and no name. `search` writes it in parentheses after its matches, and the
/// reference of a quarantined result carries it as a sentence of its own.
pub(crate) fn search_links_clause(links: usize) -> String {
    format!(
        "{} not entered, so files behind {} were not searched",
        tally(links, "symbolic link was", "symbolic links were"),
        if links == 1 { "it" } else { "them" }
    )
}

/// How a listing that ran out of time is narrowed, in the words both its body and the sentence
/// beside a quarantined one end with. Fixed words and no name (LIST-4).
pub(crate) const LISTING_NARROWING: &str = "narrow it with a directory, a pattern or a depth";

/// What a listing says when its `pattern` selected no file. Fixed words, with the advice of
/// [`reads_as_an_unsupported_glob`] where the pattern leans on syntax the matcher does not have.
///
/// An empty answer for a pattern reads the same as an empty directory, and a bounded walk lists
/// its directories whatever the pattern says, so a listing of only those reads as one where the
/// pattern worked. Matching is case-sensitive and the tool has no switch for it, so the sentence
/// says how to cover both cases.
pub(crate) fn unmatched_pattern_note(advice: Option<&str>) -> String {
    let mut text = "(no file matched the pattern, so this says nothing about whether such a file \
                    exists; matching is case-sensitive, so readme* does not match README.md \
                    and {r,R}eadme* matches both)"
        .to_string();
    if let Some(advice) = advice {
        text.push_str(&format!("\n\n({advice})"));
    }
    text
}

/// Unchanged lines kept around each run of changes, so a hunk can be read in context.
const DIFF_CONTEXT: usize = 3;

/// How many lines of a quarantined file are shown when offering to vouch for it.
///
/// Enough to tell what the file is, not so much that the prompt becomes a document nobody reads.
/// The decision being asked for is about the path, not about these lines.
///
/// The cut is made before the characters that draw as nothing are written out and counted, so the
/// count covers these lines and no others (PROMPT-13).
const VOUCH_PREVIEW: usize = 20;

/// What a write would do, told three ways, all of them built in one place.
///
/// The sentence and the hunks are what a person is told afterwards; the diff is what the
/// question before it is drawn from. All three are the same comparison, so all three are made
/// together, inside the gate, and released once.
#[derive(Debug, Clone)]
pub(crate) struct Reviewed {
    /// The line saying how much changed.
    pub note: String,
    /// The hunks, condensed to what is worth the room in a transcript.
    pub changes: Vec<crate::diff::Change>,
    /// What the diff counted and where its rows stand, made with it. `None` for a write of a file
    /// that already existed and that left every line as it was.
    pub written: Option<crate::report::Written>,
    /// The whole comparison, for the prompt a person approves from.
    pub diff: Diff,
}

/// Compare what a write would leave against what is there, and release the result for the
/// screen.
///
/// The comparison happens inside [`Policy::render_pair_in_place`], on the body while it is still
/// labelled and on a labelled peek of the file it replaces, and what is released is the finished
/// rows. The order is the whole point. A driver that released the body first and diffed it
/// afterwards would be counting and searching bytes under a witness minted to put them on a
/// screen, which is the read `docs/specs/labels.md` LABEL-6 refuses: reshaping content for
/// display is not a fourth destination, and it is not a read a driver may do for itself.
pub(crate) fn review_a_write<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
    intent: Intent,
    existing: &Labelled<String>,
    body: &Labelled<String>,
    replaced_age: Option<std::time::Duration>,
    landed: &std::path::Path,
) -> Reviewed {
    let reviewed = policy.render_pair_in_place(tool, existing, body, |existing, written| {
        // Written out in the same reshape, as the output and vet prompts do (PROMPT-13): what a yes
        // writes is what the rows are read from, and a character that draws as nothing is a row
        // that omits part of it. The comparison is of the real lines, so a line differing only in such a
        // character is still a change.
        let diff = Diff::compute(&existing, &written).spelled();
        let (note, changes) = change_report(intent, &diff, replaced_age);
        // A new file is a change whatever it holds. A file that was there and has every line it
        // had is not, so its call carries no counts and nothing lists it (FILES-1).
        let written =
            (intent == Intent::Create || !diff.is_empty()).then(|| crate::report::Written {
                landed: landed.to_path_buf(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(DIFF_CONTEXT),
            });
        Reviewed {
            note,
            changes,
            written,
            diff,
        }
    });
    let proof = policy.authorise_display_release("what a write would change");
    reviewed.declassify(&proof)
}

/// The file a write lands on, as the workspace resolves it, for the key a screen lists written
/// files under (FILES-1). The name as given where it cannot be resolved, which a write refuses
/// before it reaches here. Routing only: it is carried beside the display text and drawn nowhere.
pub(crate) fn landing_of(workspace: &Workspace, proposed_path: &str) -> std::path::PathBuf {
    workspace
        .resolve(proposed_path)
        .unwrap_or_else(|_| std::path::PathBuf::from(proposed_path))
}

/// Where a write of `proposed_path` lands, for the gate that holds an answer about references that
/// are no file to a new file in the scratch directory. Resolved as the write resolves it, so a link
/// or a `..` in the name that leads out of the directory is seen for what it reaches. Routing only.
pub(crate) fn write_site(
    workspace: &Workspace,
    proposed_path: &str,
) -> bravebot_core::policy::WriteSite {
    let landing = landing_of(workspace, proposed_path);
    let is_new = matches!(
        std::fs::symlink_metadata(&landing),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    bravebot_core::policy::WriteSite {
        landing: landing.to_string_lossy().into_owned(),
        is_new,
    }
}

/// What a write did, for the person watching: a line saying how much changed, and the hunks
/// themselves where showing them is worth the room.
///
/// Reads the comparison rather than the two sides of it, and is reached only from inside the
/// reshape [`review_a_write`] runs. Neither side is ever a string this crate holds outside that
/// gate.
///
/// A new file says it is new and shows what it now holds. It used to show a line count and
/// nothing else, on the grounds that every line of it is an addition and the body would fill the
/// screen. The display trims what it draws, so it does not; and in a directory the user has
/// vouched for a create is never reviewed either, so that count was the only thing they were
/// ever going to be told about a file that had just appeared in their workspace.
fn change_report(
    intent: Intent,
    diff: &Diff,
    replaced_age: Option<std::time::Duration>,
) -> (String, Vec<crate::diff::Change>) {
    let changes = diff.condensed(DIFF_CONTEXT);
    if intent == Intent::Create {
        return (
            format!("new file, {}", tally(diff.added(), "line", "lines")),
            changes,
        );
    }

    // The counts of a diff that could not be lined up treat the part it gave up on as replaced.
    let limit = if diff.is_exact() { "" } else { "at most " };
    let changed = format!(
        "added {limit}{}, removed {limit}{}",
        tally(diff.added(), "line", "lines"),
        tally(diff.removed(), "line", "lines")
    );

    // An overwrite says so, and says how old what it replaced was. "Replaced the file" was not
    // enough on its own: a user watching a session write a file for the first time reads it as
    // this session's own work being rewritten, and asks why the agent never said it created
    // anything. The answer is usually that the file was there before the session started, which
    // is a thing the age says and the count of lines does not.
    //
    // An edit needs no such word, since naming a passage to replace is what it is.
    let note = match (intent, replaced_age) {
        (Intent::Overwrite, Some(age)) => format!(
            "replaced a file written {}, {changed}",
            crate::report::how_long_ago(age)
        ),
        (Intent::Overwrite, None) => format!("replaced the file, {changed}"),
        _ => changed,
    };
    (note, changes)
}

/// Run one tool call the model asked for.
///
/// Errors are returned as text rather than failing the turn: a model that asked for a
/// missing file should be told so and allowed to try again, exactly as it would be told
/// about a compile error.
pub fn dispatch<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    call: &ToolCall,
) -> Output {
    dispatch_with(policy, tools, confirmer, reporter, call, None)
}

/// The most reads of one round that run at the same time (TURN-10).
pub(crate) const READS_AT_ONCE: usize = 8;

/// The tools that read files, ask no one anything and need nothing but their arguments, which is
/// what lets one run beside another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    ListFiles,
    Search,
}

impl Reading {
    fn named(tool: &str) -> Option<Self> {
        match strip_namespace(tool) {
            "list_files" => Some(Self::ListFiles),
            "search" => Some(Self::Search),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::ListFiles => "list_files",
            Self::Search => "search",
        }
    }

    fn run<S: Sink>(
        self,
        policy: &mut Policy<'_, S>,
        workspace: &Workspace,
        arguments: &Value,
    ) -> Produced {
        match self {
            Self::ListFiles => list_files(policy, workspace, arguments),
            Self::Search => search(policy, workspace, arguments),
        }
    }
}

/// Whether `call` is one [`read_ahead`] can run for this run: a read of the kind above, which this
/// run offers and the command line has not taken away, with arguments that are JSON and that the
/// schema in `offered` declares, so a call [`dispatch_with`] refuses is not read (TOOL-3).
pub(crate) fn can_read_ahead(
    call: &ToolCall,
    confined_to: Option<&[String]>,
    offered: Option<&[Tool]>,
) -> bool {
    Reading::named(&call.function.name).is_some_and(|reading| {
        bravebot_core::tool_set::allows(reading.name())
            && confined_to.is_none_or(|offered| offered.iter().any(|tool| tool == reading.name()))
            && call.arguments().is_ok_and(|arguments| {
                // The mode words the refusal and does not decide whether there is one.
                undeclared_argument(
                    offered,
                    reading.name(),
                    &arguments,
                    crate::PermissionMode::default(),
                )
                .is_none()
            })
    })
}

/// A read that ran before the turn reached it, with the decisions it took.
///
/// Nothing of it has happened as far as the policy, the trail or the screen are concerned until
/// [`dispatch_with`] reaches the call it answers: the events wait in `record`, and the result
/// waits here.
pub(crate) struct Ahead {
    reading: Reading,
    produced: Produced,
    record: RecordingSink,
    context: Label,
}

impl Ahead {
    /// This read, if the planner's context has met nothing since it ran. A call runs under the
    /// context the calls before it left, so a read taken under an earlier one is not that call's
    /// answer, and the call runs for itself.
    pub(crate) fn if_still_current<S: Sink>(self, policy: &Policy<'_, S>) -> Option<Self> {
        (self.context == policy.context_label()).then_some(self)
    }

    fn rejoin<S: Sink>(self, policy: &mut Policy<'_, S>) -> Produced {
        policy.rejoin(self.record);
        self.produced
    }
}

/// Run the reads in `calls` beside each other, at most `at_once` at a time, each under a branch of
/// `policy` that records into a buffer of its own. One entry per call, in call order, and `None`
/// for a call that is not a read of the kind above, whose arguments were not JSON, or that was not
/// started because `cancel` had been set by the time a thread came to it.
///
/// Returns when every started read has finished. Nothing has reached the policy, the trail or a
/// screen: each [`Ahead`] is handed to [`dispatch_with`] at its call's turn, so events and results
/// take the order a sequential run gives them.
pub(crate) fn read_ahead<S: Sink>(
    policy: &Policy<'_, S>,
    workspace: &Workspace,
    facts: &ReachFacts<'_>,
    calls: &[&ToolCall],
    at_once: usize,
    cancel: &bravebot_core::cancel::Cancel,
) -> Vec<Option<Ahead>> {
    let context = policy.context_label();
    let mut records: Vec<RecordingSink> = calls.iter().map(|_| RecordingSink::new()).collect();
    let mut finished: Vec<Option<Produced>> = calls.iter().map(|_| None).collect();
    let mut readings: Vec<Option<Reading>> = calls.iter().map(|_| None).collect();
    {
        let jobs: Vec<_> = records
            .iter_mut()
            .zip(calls)
            .enumerate()
            .filter_map(|(index, (record, call))| {
                let reading = Reading::named(&call.function.name)?;
                let arguments = call.arguments().ok()?;
                readings[index] = Some(reading);
                let reach = facts.for_call(policy, workspace, &call.function.name);
                Some((index, reading, policy.branch(record), arguments, reach))
            })
            .collect();
        let workers = at_once.min(jobs.len());
        let queue = std::sync::Mutex::new(jobs.into_iter());
        let done = std::sync::Mutex::new(&mut finished);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    while !cancel.is_cancelled() {
                        let next = queue
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .next();
                        let Some((index, reading, mut branch, arguments, reach)) = next else {
                            break;
                        };
                        // A copy of its own, so the reach this read holds is not the one a read
                        // beside it holds.
                        let held = workspace.clone();
                        held.hold_bypass_reach(reach);
                        let produced = reading.run(&mut branch, &held, &arguments);
                        drop(branch);
                        done.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)[index] =
                            Some(produced);
                    }
                });
            }
        });
    }
    records
        .into_iter()
        .zip(finished)
        .zip(readings)
        .map(|((record, produced), reading)| {
            Some(Ahead {
                reading: reading?,
                produced: produced?,
                record,
                context,
            })
        })
        .collect()
}

/// Names a built-in reads although the schema it was offered with does not declare them, each to
/// answer it in a way a spec pins, so a call naming one reaches the tool rather than this check.
///
/// `watch_file` and `download_url` read `path_ref` to refuse a real reference with their own
/// sentence and let a blank one by. `fetch_url` reads `read` in a turn whose schema lost it for
/// want of `vet_content`, and keeps the page as a reference. `spawn_processor` reads `read` to say
/// the argument is `reads` (SPAWN-5). `schedule_next` takes `stop` from a turn arranging a look,
/// whose schema does not offer it, and does not read it there (SCHED-4).
const READ_THOUGH_UNDECLARED: &[(&str, &str)] = &[
    ("watch_file", "path_ref"),
    ("download_url", "path_ref"),
    ("fetch_url", "read"),
    ("web_search", "read"),
    ("spawn_processor", "read"),
    ("schedule_next", "stop"),
];

/// The refusal for a built-in call that names an argument the schema it was offered with does not
/// declare, or `None` where every name it gives is declared (TOOL-3).
///
/// A tool the list does not hold is not checked here: the arms that answer an unknown name answer
/// it. An argument given as `null` reads as absent, as the tools read a declared one. The name in
/// the sentence is the planner's own, and the list is the schema's, which the driver wrote.
fn undeclared_argument(
    offered: Option<&[Tool]>,
    name: &str,
    arguments: &Value,
    mode: crate::PermissionMode,
) -> Option<String> {
    let declared = declared_arguments(offered, name)?;
    let (argument, _) = arguments.as_object()?.iter().find(|(argument, value)| {
        !value.is_null()
            && !declared.contains_key(argument.as_str())
            && !READ_THOUGH_UNDECLARED.contains(&(name, argument.as_str()))
    })?;
    let takes = declared
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let mut said = format!("error: {name} takes no argument '{argument}'. It takes: {takes}.");
    if name == "run" {
        said.push_str(
            " To set an environment variable for a program, write it in front of the program in \
             the command, as `NAME=value cargo build`.",
        );
        // Only bypass mode compiles an expansion in that position (CMDLINE-17).
        if mode == crate::PermissionMode::Bypass {
            said.push_str(
                " In this mode `NAME=$VAR cmd` and `NAME=\"$(cmd)\" cmd` are accepted there too.",
            );
        }
    }
    Some(said)
}

/// The arguments the schema `name` was offered with declares, or `None` where no schema offered
/// this turn has that name or it declares none.
fn declared_arguments<'a>(
    offered: Option<&'a [Tool]>,
    name: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    offered?
        .iter()
        .find(|tool| tool.function.name == name)?
        .function
        .parameters
        .get("properties")?
        .as_object()
}

/// The error for a call whose arguments are not valid JSON (TOOL-3, BACKEND-44).
///
/// It says the call was not run, names the tool, gives the parser's reason, quotes a short piece
/// of the text received around the position the parser reported, lists what a built-in takes as
/// TOOL-3 does, and asks for the whole call again. It also says the call was not empty, because the
/// next request may carry it with empty arguments (BACKEND-44). The text received is the planner's
/// own, so it is quoted only where [`Policy::read_planner_argument`] releases it, and a result that
/// quotes it is labelled as what the planner wrote, from the context (LABEL-5, TOOL-7); from a
/// context that has met untrusted content the sentence says it is not repeated. A server's tool,
/// or a name nothing was offered under, lists nothing.
///
/// The tool is named by the driver's own string for it: the schema offered under that name this
/// turn, or the wire name a server's tool was offered under. A name neither holds is "a tool
/// call". The name as the call spelled it is what the planner wrote, so it is never repeated in a
/// result labelled as the driver's. The person watching is told the first sentence alone, so a
/// quote from a private context does not reach a screen unlabelled.
fn unparsed_arguments<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    call: &ToolCall,
    name: &str,
    error: &serde_json::Error,
) -> Produced {
    let named = tools
        .offered
        .and_then(|offered| offered.iter().find(|tool| tool.function.name == name))
        .map(|tool| tool.function.name.as_str())
        .or_else(|| tools.mcp.and_then(|offer| offer.wire_name_of(name)));
    let first = match named {
        Some(named) => format!(
            "error: the {named} call was not run, because its arguments were not valid JSON: \
             {error}."
        ),
        None => format!(
            "error: a tool call was not run, because its arguments were not valid JSON: {error}."
        ),
    };
    let gated_as = named.unwrap_or("unoffered");
    let mut said = first.clone();
    let received = Labelled::new(
        call.function.arguments.clone().unwrap_or_default(),
        bravebot_core::label::Label::untrusted_public(),
    );
    let quoted = match policy.read_planner_argument(gated_as, "arguments", &received) {
        Ok(received) => {
            let piece = bravebot_core::excerpt::around(&received, error.line(), error.column());
            said.push_str(&format!(
                " Its arguments were: `{piece}`. The conversation may show this call with empty \
                 arguments; those arguments were the text quoted here."
            ));
            true
        }
        Err(_) => {
            said.push_str(
                " The conversation may show this call with empty arguments; it was not empty, but \
                 the text it received is not repeated here, because this context has met \
                 untrusted content.",
            );
            false
        }
    };
    let a_server_tool =
        name.starts_with("mcp__") || tools.mcp.is_some_and(|offer| offer.find(name).is_some());
    if !a_server_tool && let Some(declared) = declared_arguments(tools.offered, name) {
        let takes = declared
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        said.push_str(&format!(" It takes: {takes}."));
    }
    said.push_str(" Send the whole call again, with its arguments written as valid JSON.");
    if quoted {
        // The quote is the planner's words, so the result takes the label of the context they
        // were written in, which holds private content where the planner was shown some.
        Produced {
            failed: true,
            ..Produced::new(
                policy.label_model_output(gated_as, said),
                String::new(),
                first,
            )
        }
    } else {
        Produced::refused_with_a_note(said, first)
    }
}

/// [`dispatch`], answering a call from a read that already ran where there is one.
///
/// The read stands in for the tool's own run, at the place in the call the run would have been,
/// so everything the call shows and records comes in the order it would have run in. Where the
/// call is refused before the tool is reached the read is dropped unused, and with it the events
/// it buffered.
pub(crate) fn dispatch_with<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    call: &ToolCall,
    ahead: Option<Ahead>,
) -> Output {
    let mut ahead = ahead;
    // Some models namespace a tool by the group it was offered in: "functions_todo_write" and
    // "functions.todo_write" both mean todo_write, and answering "no such tool" to those spends
    // a round on a typo of our own making. The prefix is stripped before the name is matched
    // against the table, which is the driver's own list of literals: nothing the model writes
    // reaches anything but that comparison.
    let name = strip_namespace(&call.function.name).to_string();
    let verb = crate::report::verb_for(&name);

    let arguments = match call.arguments() {
        Ok(value) => value,
        Err(e) => {
            let produced = unparsed_arguments(policy, tools, call, &name, &e);
            // Announced and closed in one breath, because there was never a call to watch.
            let call_id = CallId::next();
            reporter.tool_started(Activity::running(verb, "").for_call(call_id).of_tool(&name));
            reporter.tool_finished(
                Activity::running(verb, "")
                    .for_call(call_id)
                    .of_tool(&name)
                    .failed(produced.note.clone()),
            );
            return output_of(produced, call.id.clone(), call_id, name);
        }
    };

    // Announced before the call runs, so a slow one is visible while it is slow. This is the
    // difference between a turn that looks stuck and one that is plainly working.
    // A server's tool is found by the wire name it was offered under, and only in a run that
    // offered it, which is one holding a grant for its server. A name this run did not offer is
    // answered as any other unknown name is.
    let server_tool = tools.mcp.and_then(|offer| {
        offer
            .find(&name)
            .map(|(alias, tool)| (offer, alias.to_string(), tool.to_string()))
    });
    // A server's name that no offer holds is drawn from the name alone, as a resumed session
    // draws every server's call, so the two describe it alike.
    let server = server_tool
        .as_ref()
        .map(|(_, alias, tool)| format!("{alias}:{tool}"))
        .or_else(|| name.strip_prefix("mcp__").map(server_named));
    let (target, as_the_planner_had_it) =
        target_of(policy, &name, server.as_deref(), tools.slots, &arguments);
    // A look at a job is drawn with the command the job runs: its name alone says nothing to
    // the person watching, who never saw the planner choose it.
    let target = match name.as_str() {
        "job_output" => tools
            .jobs
            .drawn_with_its_command(policy, &arguments)
            .unwrap_or(target),
        _ => target,
    };
    // The planner's form is always kept beside the shown one. Whether they differ is not asked:
    // that would compare the target's bytes.
    let as_the_planner_had_it = Some(as_the_planner_had_it);
    // `load_tool` is offered only where a turn defers its server tools, so a definition's `tools:`
    // line cannot name it and `--tools` cannot take it away: a `tools:` line without `mcpServers:`
    // selects no server, and with either holding the server's tools back the offer is not deferred.
    let loads_a_tool = name == "load_tool" && tools.mcp.is_some_and(|offer| offer.is_deferred());
    let why = why_of(policy, &name, &arguments);
    let call_id = CallId::next();
    reporter.tool_started(
        Activity::running(verb, target.clone())
            .for_call(call_id)
            .of_tool(&name)
            .naming_a_file(names_a_file(&name))
            .as_the_planner_had_it(as_the_planner_had_it.clone())
            .saying_why(why.clone()),
    );

    // Read once, so a write refused below is recorded under the mode that refused it.
    let mode = tools.permission_mode.get();
    // A server's tool is sent its arguments as the server describes them, so only a built-in's are
    // checked against the schema it was offered with (TOOL-3).
    let undeclared = match server_tool {
        Some(_) => None,
        None => undeclared_argument(tools.offered, &name, &arguments, mode),
    };
    tools
        .workspace
        .hold_bypass_reach(tools.reach_facts().for_call(policy, tools.workspace, &name));
    let mut produced = match name.as_str() {
        // A server's tool is not one of this program's, so no definition's `tools:` line names
        // it, and it was offered only where the run holds its server.
        unoffered
            if server_tool.is_none()
                && !loads_a_tool
                && tools
                    .confined_to
                    .is_some_and(|offered| !offered.iter().any(|tool| tool == unoffered)) =>
        {
            Produced::problem(format!("error: no such tool '{unoffered}'"))
        }
        // What the command line took away is refused whoever asks, a delegate included, since the
        // flags limit the run and not one turn of it.
        taken_away
            if server_tool.is_none()
                && !loads_a_tool
                && !bravebot_core::tool_set::allows(taken_away) =>
        {
            Produced::problem(format!("error: no such tool '{taken_away}'"))
        }
        // Refused before the tool is reached, so nothing runs and nobody is asked about a call
        // that would have done something other than what the planner meant.
        _ if undeclared.is_some() => Produced::problem(undeclared.clone().unwrap_or_default()),
        // A mode that refuses writes refuses them whether or not anybody would have been asked,
        // which is what makes it a statement about the turn rather than an answer given on the
        // person's behalf. Checked before the tool runs, so nothing is read and no path resolved.
        writing if mode.refuses_writes() && writes_a_file(writing) => {
            policy.refuse_write_in_mode(strip_namespace(writing), mode.name());
            Produced::problem(
                "refused: the session is in plan mode, so writing is refused however the user would \
                 have answered. Do not retry; say what you would change and why.",
            )
        }
        // Before the permission mode is reached, so accepting edits and bypassing, which answer the
        // write prompt, cannot hand back what `read-only` withholds from programs (SANDBOX-22).
        writing
            if sandbox_refuses_writes(tools.confine_runs, tools.sandbox)
                && writes_a_file(writing) =>
        {
            policy.refuse_write_in_sandbox(strip_namespace(writing), tools.sandbox.name());
            Produced::problem(
                "refused: the sandbox mode is read-only, so nothing is written to the session's \
                 files whatever the user would have answered; nothing was asked. Do not retry; say \
                 what you would change and why. Only the user can choose another sandbox mode.",
            )
        }
        // A tool the offer holds and has not loaded is not offered, so naming it is a mistake
        // worth correcting and not a call to put to the person (SERVERS-16).
        unloaded if tools.mcp.is_some_and(|offer| offer.is_unloaded(unloaded)) => {
            let shown = server_tool.as_ref().map_or_else(
                || name.clone(),
                |(_, alias, tool)| format!("{alias}:{tool}"),
            );
            Produced::problem(format!(
                "refused: {shown} is not loaded, so it was not called. Call load_tool with the \
                 name {shown} first."
            ))
        }
        "load_tool" if loads_a_tool => load_tool(policy, tools.mcp, &arguments),
        "read_file" => read_file(policy, tools, confirmer, reporter, &arguments),
        // `read` asks for vet_content's release in the result, so it is checked to be a boolean
        // and carried, as fetch_url's is, and a delegate's is ignored as fetch_url's would be
        // were it offered one (VET-6).
        "list_files" => match read_asked_of(&arguments) {
            Ok(read_asked) => {
                let mut produced = match ahead
                    .take()
                    .filter(|read| read.reading == Reading::ListFiles)
                {
                    Some(read) => read.rejoin(policy),
                    None => list_files(policy, tools.workspace, &arguments),
                };
                produced.read_asked = read_asked && !tools.delegated;
                produced
            }
            Err(refusal) => *refusal,
        },
        "search" => match read_asked_of(&arguments) {
            Ok(read_asked) => {
                let mut produced = match ahead.take().filter(|read| read.reading == Reading::Search)
                {
                    Some(read) => read.rejoin(policy),
                    None => search(policy, tools.workspace, &arguments),
                };
                produced.read_asked = read_asked && !tools.delegated;
                produced
            }
            Err(refusal) => *refusal,
        },
        "read_git" => read_git(policy, tools, confirmer, &arguments),
        "repo_map" => repo_map(policy, tools.workspace, &arguments),
        "lsp" => lsp(policy, tools, confirmer, &arguments),
        "write_file" => write_file(policy, tools, confirmer, &arguments),
        "edit_file" => edit_file(
            policy,
            tools.workspace,
            tools.slots,
            tools.recording(),
            tools.permission_mode.get(),
            tools.permission_mode.writes_are_always_asked(),
            tools.servers.as_deref_mut(),
            confirmer,
            &arguments,
        ),
        // A delegate's work coming back into the person's tree is asked for by the turn that
        // started it, so a delegate calling this is answered as an unknown name is.
        "apply_checkout" if !tools.delegated => {
            apply_checkout(policy, tools, confirmer, &arguments)
        }
        // The list on the screen belongs to the turn the person is watching, so a delegate that
        // names this is answered the way any other unknown name is rather than replacing what
        // they were reading with the steps of a sub-task they did not ask about.
        "todo_write" if !tools.delegated => todo_write(policy, reporter, tools.slots, &arguments),
        "spawn_processor" => spawn_processor(policy, tools, &arguments),
        // Offered to a delegate by where it sits, and dispatched whoever asks: a delegate at the
        // bottom of the tree that calls it anyway is refused by the kernel, which is the check
        // that holds whatever the list said, and the trail then says why.
        "spawn_agent" => spawn_agent(policy, tools, reporter, &arguments),
        "load_skill" => load_skill(policy, tools.skills, tools.beside, &arguments),
        "search_skills" => search_skills(policy, tools.skills, &arguments),
        // A delegate's task came from a planner, so the question would ask the person to
        // arbitrate something they never set up. Refused here as well as absent from the list.
        "ask_user" if !tools.delegated => {
            ask_user(policy, confirmer, &arguments, tools.vouched_channel)
        }
        // Offered only in plan mode and only to the turn the person is watching: a delegate's
        // plan came from a planner, and approving it would end a mode the person set for the
        // session. Refused here as well as absent from the list.
        "propose_plan" if !tools.delegated => propose_plan(policy, tools, confirmer, &arguments),
        "request_path" if !tools.delegated => request_path(policy, tools, confirmer, &arguments),
        "run" => run(policy, tools, confirmer, reporter, &arguments),
        "read_output" => read_output(policy, tools, confirmer, reporter, &arguments),
        // Not offered to a delegate, so a call from one is answered the way any other unknown
        // name is. What crosses back from a delegate is its own set of rules, and a delegate
        // promoting a slot would put bytes into a context the person watching never sees.
        "vet_content" if !tools.delegated => {
            vet_content(policy, tools, confirmer, reporter, &arguments)
        }
        // A kind's network capability buys the driver's own model call and nothing a delegate can
        // point somewhere, so this one is refused here too: the gate would pass it, since the
        // capability really is held.
        "fetch_url" if !tools.delegated => match read_asked_of(&arguments) {
            Ok(read_asked) => {
                let mut produced = fetch_url(policy, tools, confirmer, &arguments);
                produced.read_asked = read_asked;
                produced
            }
            Err(refusal) => *refusal,
        },
        // The same refusal for the same reason, and a write on top of it.
        "download_url" if !tools.delegated => download_url(policy, tools, confirmer, &arguments),
        // Offered only where a variable or a key file offered a key when the turn started, and never
        // to a delegate: a call to it from anywhere else is answered as an unknown name is. Refused
        // here as well as absent from the list. The list the turn settled decides it, and the
        // environment is not asked again, so a variable removed since is reported as a key that
        // cannot be used and not as an unknown tool. `read` asks for vet_content's release in the
        // result, as fetch_url's does (WEBSEARCH-9).
        "web_search"
            if !tools.delegated
                && tools.offered.is_some_and(|list| {
                    list.iter().any(|tool| tool.function.name == "web_search")
                }) =>
        {
            match read_asked_of(&arguments) {
                Ok(read_asked) => {
                    let mut produced = web_search(policy, tools, confirmer, &arguments);
                    produced.read_asked = read_asked;
                    produced
                }
                Err(refusal) => *refusal,
            }
        }
        "job_output" => job_output(policy, tools, reporter, &arguments),
        // A delegate ends when it answers, a tick the person timed has its next look coming
        // already, and a turn nothing will ask again has nowhere for a wait to go, so none of the
        // three is offered this and a call from any of them is answered as an unknown name.
        // Refused here as well as absent from the list.
        "schedule_next" if !tools.delegated && tools.scheduling.offered() => {
            schedule_next(policy, tools.scheduling, &arguments)
        }
        // A delegate is refused here. A surface that keeps no watches is offered no way to arm
        // one and is refused by `watch_file` itself: the function takes the arming state as an
        // argument, and its `Arming::Unavailable` arm answers with the words of the catch-all
        // below and arms nothing, so a guard on `Arming::offered()` here would give the same
        // answer and no test could tell the two apart. `schedule_next` above keeps its guard
        // because that function takes no state that says it was not offered, so the dispatch arm
        // is the only place that refuses it.
        "watch_file" if !tools.delegated => watch_file(
            policy,
            tools.workspace,
            tools.slots,
            tools.arming,
            tools.armed,
            &arguments,
        ),
        // Offered to the planner of a session that named an advisor, and to nothing else: a
        // delegate's task came from a planner, and a call from one is answered as an unknown name
        // is. Refused here as well as absent from the list.
        "advisor" if !tools.delegated && tools.advising.is_some() => {
            advise(policy, tools, &arguments)
        }
        // Offered to the turn the person is watching and to nothing else: a delegate's task came
        // from a planner, and what a planner keeps is for the person to have asked for (RECALL-5).
        // Refused here as well as absent from the list.
        "remember" if !tools.delegated => remember(policy, tools, &arguments),
        "propose_rule" if !tools.delegated => propose_rule(policy, tools, confirmer, &arguments).0,
        // Offered to the planner of a session the person has typed to, and never to a delegate:
        // the address has to be one the person wrote, and a delegate's task is the driver's
        // sentence. Refused here as well as absent from the list.
        "set_link" if !tools.delegated && tools.keeping_links => {
            set_link(policy, tools, &arguments)
        }
        // Offered with `remember`, to the same turn and for the same reason (RECALL-10).
        "recall" if !tools.delegated => recall(policy, tools, &arguments),
        // Offered to the planner of a background session, and to nothing else: a delegate's task
        // came from a planner, so a message it wants to send would ask the person to arbitrate
        // something they never set up (MSG-12). Refused here as well as absent from the list.
        "message_session" if !tools.delegated && tools.messaging.is_some() => {
            message::message_session(policy, tools, confirmer, reporter, &arguments)
        }
        other => match &server_tool {
            // A server declared to ask every call is put to the person, and a delegate's task came
            // from a planner and not from them, so the call is refused unasked (SERVERS-21).
            Some((offer, alias, tool)) if tools.delegated && offer.asks_every_call(alias) => {
                policy.refuse_asked_every_call_in_a_delegate(alias, tool);
                Produced::problem(format!(
                    "refused: every call to {alias} is put to the user, and a delegate is not \
                     able to ask. Do not retry {alias}:{tool}: say in your report what you \
                     needed from it."
                ))
            }
            Some((offer, alias, tool)) => {
                let egress = tools.chat.egress;
                // The `read` this process offered is not the server's: it is taken out of the
                // arguments before the call, so the server is sent what the planner wrote and no
                // more. A server that lists a `read` of its own is sent it as written.
                let split = if offer.takes_read(alias, tool) {
                    split_read(&arguments)
                } else {
                    Ok((arguments.clone(), false))
                };
                match split {
                    Ok((sent, read_asked)) => {
                        let mut produced = call_server_tool(
                            policy,
                            offer,
                            egress,
                            confirmer,
                            reporter,
                            alias,
                            tool,
                            &sent,
                            &mut *tools.slots,
                        );
                        produced.read_asked = read_asked;
                        produced
                    }
                    Err(refusal) => *refusal,
                }
            }
            None => Produced::problem(format!("error: no such tool '{other}'")),
        },
    };
    // Held for the call alone, so nothing that reads the workspace between calls finds a reach.
    tools.workspace.hold_bypass_reach(None);
    for reached in tools.workspace.take_new_reaches() {
        policy.record_bypass_reach(
            &reached.directory.display().to_string(),
            reached.mode,
            &reached.tool,
            reached.field,
        );
    }

    // The words the person typed beside their answer, where this call asked one. Taken whether the
    // call went on to succeed or fail, since either way they were said about this call.
    produced.told = policy.take_note();
    // Whatever the planner's arguments named, taken whether the call went on or was refused after
    // its arguments were read.
    produced.references = std::mem::take(&mut tools.found);

    // The wait the call already measured, on the line the call already draws. The turn's totals
    // have it too, as part of what the turn spent at the model, but a total cannot answer which
    // call was the slow one or whether a model or this machine was what took the time.
    let finished = Activity::running(verb, target)
        .for_call(call_id)
        .of_tool(&name)
        .naming_a_file(names_a_file(&name))
        .as_the_planner_had_it(as_the_planner_had_it)
        .saying_why(why)
        .loading_skill(produced.skill.clone())
        .with_changes(std::mem::take(&mut produced.changes))
        .with_written(produced.written.take())
        .marked_untrusted(produced.untrusted)
        .after_waiting(
            produced
                .inference_interval
                .map(crate::timing::Interval::duration),
        );
    let note = std::mem::take(&mut produced.note);
    reporter.tool_finished(if produced.failed {
        finished.failed(note)
    } else {
        finished.done(note)
    });

    output_of(produced, call.id.clone(), call_id, name)
}

/// A finished call's [`Output`], from what the tool produced.
fn output_of(
    produced: Produced,
    call_id: Option<String>,
    reported_as: CallId,
    tool: String,
) -> Output {
    Output {
        cancelled: produced.cancelled,
        call_id,
        reported_as: Some(reported_as),
        tool,
        text: produced.text,
        origin: produced.origin,
        deferred: produced.deferred,
        entries: produced.entries,
        incomplete: produced.incomplete,
        timed_out: produced.timed_out,
        links: produced.links,
        unmatched: produced.unmatched,
        paging: produced.paging,
        whole: produced.whole,
        apart: produced.apart,
        told: produced.told,
        answers_for: produced.answers_for,
        said: produced.said,
        chosen: produced.chosen,
        substituted: produced.substituted,
        glimpsed: produced.glimpsed,
        window: produced.window,
        task_list: produced.task_list,
        waiting_tasks: produced.waiting_tasks,
        content: produced.content,
        changed_a_file: produced.changed_a_file,
        wrote_in_the_session_directory: produced.wrote_in_the_session_directory,
        ran_a_program: produced.ran_a_program,
        command_failed: produced.command_failed,
        started_a_program: produced.started_a_program,
        run_may_change_files: produced.run_may_change_files,
        ran_git_in: produced.ran_git_in,
        usage: produced.usage,
        usage_is_unbudgeted: produced.usage_is_unbudgeted,
        inference_interval: produced.inference_interval,
        unanswered: produced.unanswered,
        printed_by: produced.printed_by,
        look: produced.look,
        covered_by_record: produced.covered_by_record,
        read_asked: produced.read_asked,
        picture: produced.picture,
        picture_bytes: produced.picture_bytes,
        pictures: produced.pictures,
        left_out: produced.left_out,
        respelled: produced.respelled,
        attached: produced.attached,
        wakeup: produced.wakeup,
        watch: produced.watch,
        link: produced.link,
        references: produced.references,
        created: produced.created,
        delegate: produced.delegate,
        wake_after: produced.wake_after,
        loaded: produced.loaded,
        skill: produced.skill,
        message_sent: produced.message_sent,
        tool_loaded: produced.tool_loaded,
    }
}

/// The answer to a call that was held and not run (TURN-9): the driver's sentence in place of a
/// result, shown to the person as a failed call.
pub fn refused_without_running<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    reporter: &mut R,
    mcp: Option<&crate::mcp::Offer>,
    slots: &SlotStore,
    call: &ToolCall,
    text: String,
) -> Output {
    let name = strip_namespace(&call.function.name).to_string();
    let verb = crate::report::verb_for(&name);
    let produced = Produced::problem(text);
    // The reason the planner gave is drawn on both lines, as `dispatch` draws it: a call that is
    // held is still one the person watching reads, and arguments that are not JSON have none.
    let arguments = call.arguments().unwrap_or(Value::Null);
    let why = why_of(policy, &name, &arguments);
    // The target is drawn the same way `dispatch_with` draws it, so a call held for being a
    // repeat is shown with what it was about rather than nothing.
    let server_tool = mcp.and_then(|offer| {
        offer
            .find(&name)
            .map(|(alias, tool)| (offer, alias.to_string(), tool.to_string()))
    });
    let server = server_tool
        .as_ref()
        .map(|(_, alias, tool)| format!("{alias}:{tool}"))
        .or_else(|| name.strip_prefix("mcp__").map(server_named));
    let (target, as_the_planner_had_it) =
        target_of(policy, &name, server.as_deref(), slots, &arguments);
    let as_the_planner_had_it = Some(as_the_planner_had_it);
    let call_id = CallId::next();
    reporter.tool_started(
        Activity::running(verb, target.clone())
            .for_call(call_id)
            .of_tool(&name)
            .as_the_planner_had_it(as_the_planner_had_it.clone())
            .saying_why(why.clone()),
    );
    reporter.tool_finished(
        Activity::running(verb, target)
            .for_call(call_id)
            .of_tool(&name)
            .as_the_planner_had_it(as_the_planner_had_it)
            .saying_why(why)
            .failed(produced.note.clone()),
    );
    output_of(produced, call.id.clone(), call_id, name)
}

/// What a write is refused with, and what the person watching is told about why.
///
/// Both halves are the driver's own words. The planner's half names the path and says a
/// credential would have landed in it, which is what it needs to stop retrying the same write and
/// write a reference instead; it carries no finding, so nothing about what was found is in a
/// model's context. The person's half is the findings, each said as a kind, a location, a
/// fingerprint and a masked preview, which is the whole of what a finding may hold.
fn credential_refusal(path: &str, scanned: &Scanned, after_an_edit: bool) -> Produced {
    let found: Vec<String> = scanned
        .refused()
        .iter()
        .map(|finding| finding.describe())
        .collect();
    // Where the body is the text the person saved from their editor (WRITE-7), the planner is
    // told the contents the user changed are the reason and is not told to put a reference in the
    // file, since the text that would hold the value is not its own.
    let told = if after_an_edit {
        format!(
            "refused: the contents the user changed for {path} would put a credential in the \
             tree, so nothing was written. Do not write {path} again; tell the user what the \
             file was for and let them decide what to do about the value."
        )
    } else {
        format!(
            "refused: writing {path} would put a credential in the tree, so nothing was \
             written. {}",
            do_this_instead(scanned.only_the_value)
        )
    };
    Produced::refused_with_a_note(
        told,
        format!(
            "refused, a credential would have landed here: {}",
            found.join("; ")
        ),
    )
}

/// What the planner is told to do instead of the write, which depends on what the file would
/// have held.
///
/// A value inside a document was copied there from somewhere, so the answer is the reference: the
/// secret still sits wherever it already sat, and the file gets a name for it.
///
/// A file that is the value and nothing else is the other thing. It has no room for a reference,
/// and a value with no prior location is one this turn brought into existence, which is the case
/// [CRED-13](../../../docs/specs/credential-protection.md) is written about. There is nothing
/// here to create a credential into, so what the planner is owed is that the value does not
/// exist and that generating another one and writing it somewhere else is the same refusal a
/// second time. Saying what it could not create is the whole of what the turn can do, and the
/// person does the rest.
fn do_this_instead(only_the_value: bool) -> &'static str {
    match only_the_value {
        false => {
            "Put a reference to the value in the file instead, and tell the user which secret \
             they have to set and where."
        }
        true => {
            "That file would hold the value and nothing else, so there is no reference to put in \
             it and nothing was created. Do not generate another and do not write one elsewhere: \
             tell the user what the value is for and where it has to go, and let them create it."
        }
    }
}

/// A list of findings said the one way a finding may be said.
fn describe_all(findings: &[&bravebot_core::credentials::Finding]) -> Vec<String> {
    findings.iter().map(|finding| finding.describe()).collect()
}

/// What the planner is told when a person declined a write the scan had doubts about.
///
/// A plain rejection tells it to ask what the user would prefer, which is right when a person
/// simply did not want the change. Where the scan raised something, the planner is told that much
/// and no more: enough to write a reference instead of asking again, and nothing about what was
/// found, because a finding still may not enter a model's context.
fn credential_aware_rejection(path: &str, scanned: &Scanned, told: bool) -> Produced {
    if scanned.to_approve().is_empty() {
        return Produced::problem(format!(
            "refused: the user did not approve writing {path}. Do not retry the same write{}",
            what_next(told)
        ));
    }
    Produced::refused_with_a_note(
        format!(
            "refused: the user did not approve writing {path}, which looks like it would put a \
             credential in the tree. {}",
            do_this_instead(scanned.only_the_value)
        ),
        "not approved".to_string(),
    )
}

/// What a person already said about a write creating a credential in the file it lands in, and
/// which of the two standing answers the prompt may offer them this time (CRED-13).
///
/// Keyed by the file the write lands in as [`Workspace::resolve`] finds it, and never by the name
/// the planner gave or the one the prompt draws. The same name in another checkout, or one a link
/// sends somewhere else, is another file, and an answer about a spelling would cover every file
/// that spelling reaches.
pub(crate) struct StandingAnswer<'a> {
    file: Option<std::path::PathBuf>,
    store: Option<(crate::remembered::Store, &'a str)>,
    /// Whether the prompt may offer to answer for this file for the rest of the session.
    pub(crate) may_always: bool,
    /// The record the prompt may offer to add this file to, past the session.
    pub(crate) record: Option<std::path::PathBuf>,
}

impl<'a> StandingAnswer<'a> {
    /// Read what stands for the file `path` lands in, and empty `to_approve` where an earlier
    /// answer covers that file.
    ///
    /// The record is read here, immediately before the question, for the reason a run reads its
    /// own: the file is shared by every session begun in this directory, so an entry recorded or
    /// deleted a minute ago in another one counts now. A turn with nobody to put a prompt to has no
    /// session to name and reads nothing, so no recorded answer is exercised where nobody could
    /// have given it.
    pub(crate) fn read<S: Sink>(
        policy: &mut Policy<'_, S>,
        workspace: &Workspace,
        recording: crate::findings::Recording<'a>,
        tool: &str,
        path: &str,
        to_approve: &mut Vec<String>,
    ) -> Self {
        let file = workspace.resolve(path).ok();
        let store = recording
            .home
            .zip(recording.session)
            .map(|(home, session)| {
                (
                    crate::remembered::Store::new(home, workspace.root()),
                    session,
                )
            });
        if to_approve.is_empty() {
            return Self {
                file,
                store,
                may_always: false,
                record: None,
            };
        }
        if let Some((store, _)) = &store {
            policy.recall(store.read());
        }
        if file
            .as_deref()
            .is_some_and(|file| policy.credential_creation_is_answered(tool, file))
        {
            to_approve.clear();
        }
        // Offered only where there is still a question, and only for a file the driver can name.
        let may_always = !to_approve.is_empty() && file.is_some();
        // And the record only for a file inside the tree it is kept for, where this session may
        // add to it at all: a private one says no here and again inside the store.
        let record = store
            .as_ref()
            .filter(|_| may_always && crate::remembered::may_be_added_to())
            .filter(|_| {
                file.as_deref()
                    .is_some_and(|file| file.starts_with(workspace.root()))
            })
            .map(|(store, _)| store.path().to_path_buf());
        Self {
            file,
            store,
            may_always,
            record,
        }
    }

    /// Keep what the person answered, and only what the prompt offered them.
    ///
    /// Asked of what this read rather than of the answer alone: a front end answering with a key
    /// the prompt never drew must not be able to excuse a file from the question, and the file kept
    /// is the one this resolved, not anything the front end could send back.
    pub(crate) fn keep<S: Sink>(&self, policy: &mut Policy<'_, S>, answer: WriteDecision) {
        let Some(file) = self.file.as_deref() else {
            return;
        };
        if !answer.approved() {
            return;
        }
        if answer.remember && self.may_always {
            policy.allow_creating_a_credential(file);
        }
        if answer.record
            && self.record.is_some()
            && let Some((store, session)) = &self.store
        {
            store.remember_file(file, session);
        }
    }
}

/// What the person watching is told beside a change that carries a credential it did not write.
///
/// A change is refused for what the turn wrote and reported for what it found already there: a
/// turn that reformats or moves a file holding a key produces a change carrying that key without
/// having written it, and the person is the one who can decide what to do about a secret that was
/// in their tree before this session started.
pub(crate) fn carried_note(note: String, scanned: &Scanned) -> String {
    if scanned.carried.is_empty() {
        return note;
    }
    let found: Vec<String> = scanned
        .carried
        .iter()
        .map(|finding| finding.describe())
        .collect();
    format!(
        "{note}; carries a credential that was already there: {}",
        found.join("; ")
    )
}

/// How a refusal ends: with the question for the person, unless they wrote what they want instead
/// ([PROMPT-12]).
///
/// [PROMPT-12]: ../../../docs/specs/prompting.md
fn what_next(told: bool) -> &'static str {
    if told {
        "."
    } else {
        "; ask what they would prefer."
    }
}

/// Keep what the person typed beside their answer for this call's result, where they typed any
/// ([PROMPT-12]). `approved` only chooses the driver's introduction; the words are not read.
///
/// [PROMPT-12]: ../../../docs/specs/prompting.md
pub(crate) fn keep_the_note<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
    approved: bool,
    note: Option<&str>,
) {
    let Some(note) = note else {
        return;
    };
    let lead = if approved {
        "The user approved this and wrote: "
    } else {
        "The user declined this and wrote: "
    };
    policy.record_note(tool, lead, note);
}

/// Whether a call asked, with a `read` argument, for what it returns to be read in its result.
///
/// Absent means no, which leaves the result exactly as it would be without the argument. Present
/// but not a boolean is refused, so a planner that believed it had asked is not handed a reference
/// instead. The argument is the planner's own boolean, and nothing it returns is consulted.
fn read_asked_of(arguments: &Value) -> Result<bool, Box<Produced>> {
    match arguments.get("read") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(read)) => Ok(*read),
        Some(_) => Err(Box::new(Produced::problem(
            "error: 'read' must be true or false",
        ))),
    }
}

/// The arguments of a server's tool less the `read` this process offered it, and whether it asked.
/// The server is sent what remains, so it never sees an argument it did not declare.
fn split_read(arguments: &Value) -> Result<(Value, bool), Box<Produced>> {
    let read_asked = read_asked_of(arguments)?;
    let mut sent = arguments.clone();
    if let Some(object) = sent.as_object_mut() {
        object.remove(crate::mcp::READ_ARGUMENT);
    }
    Ok((sent, read_asked))
}

/// A tool's own words about something that did happen, with what to show for it.
fn confirmed(text: impl Into<String>, note: impl Into<String>) -> Produced {
    Produced::new(Labelled::trusted(text.into()), "", note)
}

/// A target argument as one line, whether it was given as a string or a list.
///
/// `search` takes either, and a call that named three patterns must not announce itself with a
/// blank where the subject goes. Joining is enough: this reaches a transcript line and a
/// progress line, and nothing compares it or routes on it.
fn target_text(arguments: &Value, key: &str) -> Option<String> {
    match arguments.get(key)? {
        Value::String(one) => Some(one.clone()),
        Value::Array(many) => {
            let parts: Vec<&str> = many.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        _ => None,
    }
}

/// Extract a string argument the model supplied, labelled untrusted because it is.
fn argument(arguments: &Value, key: &str) -> Option<Labelled<String>> {
    let raw = arguments.get(key)?.as_str()?.to_string();
    Some(Labelled::new(
        raw,
        bravebot_core::label::Label::untrusted_public(),
    ))
}

/// Which references [`accept_named_reference`] lists when it refuses a name no slot holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamesToList {
    /// The references a run's output was put under.
    Output,
    /// Every reference holding text.
    Text,
    /// The choices a processor made.
    Choices,
}

/// Accept a reference the planner named, and refuse a name no slot holds with the way to write one.
///
/// [`Policy::accept_reference`] answers for the context and the label of the name and nothing
/// else, so a name that is not a slot at all (`3`, `ref:99`) came out of it as a slot id and was
/// then taken for a slot that exists and is not what the tool wants. The slot store is asked here
/// whether the name is written, which is driver metadata: a yes or a no about a name the planner
/// typed, never a byte of what any slot holds.
///
/// The refusal echoes the planner's own argument, escaped, and names the references the driver
/// has minted, which are `ref:N` and carry no content. `listing` narrows that list to the
/// references the tool can use: a run's output for a tool that reads nothing else, or the
/// choices a processor made for one that takes nothing else.
pub(crate) fn accept_named_reference<S: Sink>(
    policy: &mut Policy<'_, S>,
    slots: &SlotStore,
    tool: &str,
    field: &str,
    named: &Labelled<String>,
    listing: NamesToList,
) -> Result<SlotId, String> {
    // The latest few, so a long session does not turn a refusal into a listing.
    const SHOWN: usize = 8;
    let slot = policy
        .accept_reference(tool, field, named)
        .map_err(|denial| format!("refused: {denial}"))?;
    // A choice is not text, so a name that holds one is refused in the words used for a name
    // nothing holds. A text slot named where a choice is expected goes on to the kernel, which
    // says it holds no choice.
    let usable = match listing {
        NamesToList::Choices => slots.is_written(&slot),
        NamesToList::Output | NamesToList::Text => slots.holds_text(&slot),
    };
    if usable {
        return Ok(slot);
    }
    policy.refuse_unknown_reference(tool, field);
    let known: Vec<String> = match listing {
        NamesToList::Choices => slots.choice_names(),
        NamesToList::Output | NamesToList::Text => slots.text_names(),
    }
    .into_iter()
    .filter(|id| listing != NamesToList::Output || slots.is_from_command(id))
    .map(|id| id.to_string())
    .collect();
    let held = if known.is_empty() {
        match listing {
            NamesToList::Output => {
                "No program has printed anything under a reference yet.".to_string()
            }
            NamesToList::Choices => "No processor has made a choice yet.".to_string(),
            NamesToList::Text => "No reference has been given yet.".to_string(),
        }
    } else {
        let skipped = known.len().saturating_sub(SHOWN);
        let latest = known[skipped..].join(", ");
        let which = match listing {
            NamesToList::Output => "The references to output so far",
            NamesToList::Choices => "The references to choices so far",
            NamesToList::Text => "The references given so far",
        };
        if skipped == 0 {
            format!("{which} are {latest}.")
        } else {
            format!("{which} include the latest {SHOWN}: {latest}.")
        }
    };
    Err(format!(
        "refused: no reference is named {:?}. A reference is written ref:N, for example ref:3, \
         and is one a result gave you. {held}",
        slot.as_str()
    ))
}

/// An argument that names something, where a blank one names nothing.
///
/// A planner sent a schema whose optional fields are all strings will sometimes fill every one
/// of them, and the ones it did not mean to use come back as `""`. For a path or a reference an
/// empty string is never a choice anyone made, so it is the same as the field being left out.
/// Counting it as given turned `{"path": "a.html", "path_ref": ""}` into "not both", every time,
/// and a planner that could not see which half was wrong sent the same call again.
///
/// Not for `contents`, where an empty string is a request for an empty file.
///
/// Decided on the call as it arrived, before anything is labelled: whether a field was filled in
/// is the shape of the call, the same thing asking whether the key is present is.
fn named_argument(arguments: &Value, key: &str) -> Option<Labelled<String>> {
    if names(arguments, key) {
        argument(arguments, key)
    } else {
        None
    }
}

/// Whether the call gave `key` a value that names something. See [`named_argument`].
fn names(arguments: &Value, key: &str) -> bool {
    match arguments.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(given)) => !given.trim().is_empty(),
        Some(_) => true,
    }
}

/// The reference names in a `reads` argument, as one line.
///
/// Wrapped like every other argument: what the planner asked for is model output, and the
/// driver reads it only where a gate says so. Entries that are not strings are left out here
/// and refused where the call is actually made.
fn references_in(arguments: &Value) -> Labelled<String> {
    let names: Vec<&str> = arguments
        .get("reads")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    Labelled::new(
        names.join(", "),
        bravebot_core::label::Label::untrusted_public(),
    )
}

/// The deadline asked for by a `run` call, held to the figures in force.
///
/// An absent field or `null` (frequent in model tool calls for omitted optional parameters) takes
/// `deadlines.default`. A value outside the bounds is clamped to between [`crate::exec::FLOOR`] and
/// `deadlines.ceiling`. Non-integers are refused.
///
/// The figures are handed in rather than read from the constants, because a person may name either
/// in a settings file and the figure in force is the one their call has to run under (RUN-23).
fn deadline_from(
    arguments: &Value,
    deadlines: crate::exec::Deadlines,
) -> Result<std::time::Duration, &'static str> {
    match arguments.get("deadline_seconds") {
        Some(value) if !value.is_null() => match value.as_i64() {
            Some(seconds) => Ok(deadlines.held_to(seconds)),
            None => Err("error: 'deadline_seconds' must be a whole number of seconds"),
        },
        _ => Ok(deadlines.default),
    }
}

/// How long a `job_output` call asked to wait, or `None` where it asked for none.
///
/// Refused rather than clamped, which is the opposite of what [`deadline_from`] does with a
/// deadline. A deadline cut short still ends the run it was given for and the answer says how long
/// that took, so the caller can see what it got. A wait cut short hands back the silence of a
/// window nobody asked about, and silence is read as an answer.
fn wait_from(
    arguments: &Value,
    deadlines: crate::exec::Deadlines,
) -> Result<Option<std::time::Duration>, String> {
    let longest = deadlines
        .wait_ceiling
        .as_secs()
        .max(crate::exec::WAIT_FLOOR.as_secs());
    let bounds = crate::exec::WAIT_FLOOR.as_secs() as i64..=longest as i64;
    match arguments.get("wait_seconds") {
        Some(value) if !value.is_null() => match value.as_i64() {
            Some(seconds) if bounds.contains(&seconds) => {
                Ok(Some(std::time::Duration::from_secs(seconds as u64)))
            }
            Some(_) => Err(format!(
                "error: 'wait_seconds' must be between {} and {longest}, and a value outside \
                 that is refused rather than adjusted: a wait quietly shortened hands back \
                 silence about a window nobody watched. The longest wait is held under the \
                 prompt cache lifetime, because the request after a longer wait writes the \
                 whole conversation to the cache again. To keep watching, make another call.",
                crate::exec::WAIT_FLOOR.as_secs()
            )),
            None => Err("error: 'wait_seconds' must be a whole number of seconds".to_string()),
        },
        _ => Ok(None),
    }
}

/// Whether a `job_output` call asked for a wait that output does not end.
///
/// Refused where the flag is not a boolean, is `true` with no `wait_seconds` to bound it, or is
/// `true` beside `kill`. A flag taken as `false` hands back a wait that output ends, which is what
/// the caller asked not to have, and one that chose a bound of its own would hand back a window
/// nobody asked about, which is what [`wait_from`] refuses to do to a value outside its range. A
/// `kill` is carried out after the wait, so beside this flag it would be held back by the whole
/// bound on any job that is still running.
fn until_exit_from(
    arguments: &Value,
    wait: Option<std::time::Duration>,
    kill: bool,
) -> Result<bool, &'static str> {
    match arguments.get("until_exit") {
        Some(value) if !value.is_null() => match value.as_bool() {
            Some(true) if wait.is_none() => Err(
                "error: 'until_exit' needs 'wait_seconds' beside it, which is the longest the \
                 call waits for the job to end",
            ),
            Some(true) if kill => Err(
                "error: 'until_exit' waits for the job to end and 'kill' ends it after the wait, \
                 so a call gives one of them",
            ),
            Some(until_exit) => Ok(until_exit),
            None => Err("error: 'until_exit' must be true or false"),
        },
        _ => Ok(false),
    }
}

/// The cell a `read_file` call asked the saved outputs of, where it asked.
///
/// Refused where it is not a whole number from 1, or the file is not named as a notebook: a value
/// taken as another would hand back the outputs of a cell the planner did not name.
fn output_of_from(arguments: &Value, path: &str) -> Result<Option<usize>, String> {
    let Some(value) = arguments.get("output_of").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    if !crate::notebook::is_notebook_path(path) {
        return Err(
            "error: 'output_of' names a cell of a .ipynb notebook, and this is not one. Leave it \
             out to read the file"
                .to_string(),
        );
    }
    match value.as_u64() {
        Some(cell @ 1..) => usize::try_from(cell).map(Some).map_err(|_| {
            "error: 'output_of' is not a cell number this notebook can have".to_string()
        }),
        _ => Err(
            "error: 'output_of' must be a cell number from 1, as a read of the notebook \
                  shows them"
                .to_string(),
        ),
    }
}

/// The saved outputs of one cell of a notebook the planner may read, as a reference.
///
/// The notebook's structure is read under its own trusted label, to say whether the cell exists
/// and holds outputs. The text of the outputs is shaped under a label tainted untrusted, inside the
/// kernel, so the planner is handed a reference and a processor is the way to the words.
fn notebook_outputs<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    path: &Labelled<String>,
    (field, shown_path): (&str, &str),
    cell: usize,
) -> Produced {
    let source = match workspace.read(policy, path) {
        Ok(contents) => contents,
        Err(e) => return file_tool_problem(policy, "read_file", field, &e, shown_path),
    };
    let Ok(current) = policy.read_trusted_content("read_file", &source) else {
        return Produced::problem(closed_to_reading_outputs(policy, source.label()));
    };
    if let Err(reason) = crate::notebook::check_outputs_of(&current, cell) {
        return Produced::problem(format!("error: {reason}"));
    }
    let carried = Labelled::new(
        current,
        bravebot_core::label::taint_all([
            source.label(),
            bravebot_core::label::Label::untrusted_public(),
        ]),
    );
    let outputs = policy.render_in_place("read_file", &carried, |json| {
        crate::notebook::outputs_view(&json, cell)
    });
    Produced::new(
        outputs,
        shown_path.to_string(),
        format!("outputs of cell {cell}, quarantined"),
    )
    .of_content()
}

fn read_file<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let workspace = tools.workspace;
    let mut found = match path_argument(
        policy,
        tools.workspace,
        "read_file",
        Purpose::Read,
        tools.slots,
        arguments,
    ) {
        Ok(found) => found,
        Err(refusal) => return Produced::problem(refusal),
    };
    // The name the planner typed, which is the only name a not-found hint may be about.
    let typed = found.released.clone();
    // A name that matches no file but differs from one only in the kind of space in its last
    // component is read as that file (READ-11). The candidates are made from the typed name and
    // probed by path, so no directory is listed and nothing read decides it.
    //
    // The other spelling is built by the driver from the typed name alone and chosen by which
    // candidate is a file, so it is not the planner's own words. It carries the label of the
    // typed argument, which `path_argument` assigned and which no byte of any filename changes.
    // Only the gate that asks about a path is asked again here: a deny rule covers the file and
    // not the spelling. The refusal names the spelling the planner typed. Every later gate
    // (confinement, trust key, quarantine, vouch offer, scan, change token) is keyed by the
    // spelling in `found`, which is now the one that will be opened.
    let mut respelled = None;
    if found.destination == Destination::Named
        && let Some(other) = workspace.respelled_when_missing(&found.released)
    {
        if let Err(refusal) =
            refuse_denied_path_as(policy, tools.workspace, Purpose::Read, &other, &found.shown)
        {
            return Produced::problem(refusal);
        }
        found = PathArgument {
            path: Labelled::new(other.clone(), found.path.label()),
            destination: Destination::Named,
            released: other,
            // The name the planner typed, which is what a reference to the file is described by:
            // the spelling on disk was chosen by the file system, so it is not the planner's to
            // be told.
            shown: found.shown.clone(),
        };
        respelled = Some(Respelled);
    }
    // What the reference that comes back is said to be of. The planner's own path where it
    // typed one, and the reference's name where it did not: a read through a reference must not
    // hand back the filename the reference exists to hold.
    let (proposed, destination, shown_path, proposed_path) =
        (found.path, found.destination, found.shown, found.released);

    let path = match destination {
        // The promotion the model's own choice of file already gets: the read is confined to the
        // workspace and changes nothing in it.
        Destination::Named => match policy.promote_confined_read("read_file", "path", &proposed) {
            Ok(p) => p,
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
        // Already promoted, by the gate that took the name out of the reference. Promoting it
        // again would work and would record that the model proposed a path it never saw.
        Destination::Reference => proposed.clone(),
    };

    // A model that omits these gets the head of the file, which is the useful default.
    let offset = arguments
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX)
        .min(usize::MAX as u64) as usize;

    // The trust question, put where it bites rather than only at startup. A file is quarantined
    // because nobody vouched for it, and that is the user's decision to make: they are shown the
    // path and the first lines of it and can vouch on the spot, after which this read and every
    // later one sees the file. A yes writes the same rule `@` and the startup question write, so
    // nothing here is a second route to trusting content.
    //
    // Checked before it is asked, as every prompt that promotes content is. The person is told what
    // a confined second opinion made of the file, and the word decides nothing: what promotes the
    // file is their answer. Without this the quickest way past a check would be to ask to read the
    // file, which is the defect this covers.
    //
    // Asked once per path per turn, and only for a path that is quarantined, so a planner retrying
    // a read does not put the same question up twice. An answer thrown away because the file
    // changed while the person was answering frees the path (`vouch_if_unchanged`), so the read
    // the result below invites asks again.
    //
    // Never asked about a picture. What a yes grants is that a file's *text* may be read, and the
    // branch below hands a picture back as a reference whatever the trust map says, so the
    // question would promise something the answer could not deliver. Decided from the extension,
    // out of the driver's own table, so nothing read chooses it.
    //
    // Never asked about a path that does not name a file either. The prompt names one file and a
    // yes writes a rule covering everything beneath the name, so on a directory it would grant
    // `/add-dir`'s reach from a question that said "file", and on `.` the whole workspace. Both
    // guards are the path and `stat`, so what settles whether the question is put stays out of
    // reach of anything read.
    //
    // `should_offer_vouch` is last because it is the only one of these that does anything: it marks
    // the path asked about. Every test that could still call the prompt off has to come before
    // it, or the path spends its one question of the turn on a prompt nobody sees and the trail
    // records an offer that was never made. That is the defect this block was rewritten to fix,
    // and the order is what holds it fixed.

    // The name the map holds a rule about this file under, which is the relative one where the
    // planner spelled a file in the project by its absolute path. Every question below is about the
    // map, so all of them ask about that name and not about the spelling.
    let keyed = workspace.trust_key(&proposed_path);

    // Taken from a name the planner typed alone. The name behind a reference came out of a
    // directory nobody vouched for, so a picture's reference for a `.png` and the text of a `.txt`
    // would tell the planner what the hidden name ends in. A reference to a file that may be read
    // is read like any such file, and what it holds decides the answer (READ-3, READ-5).
    let media = (destination == Destination::Named)
        .then(|| bravebot_filetype::by_name::media_for(&proposed_path))
        .flatten();

    // What a check over this file cost, where one was made. Carried out of the block so it can be
    // put on whatever this call ends up handing back: a check is a model request, and the turn's
    // figure has to cover the requests the driver made on its own behalf as well as the planner's.
    let mut spent = Usage::default();
    let mut waited = None;
    let mut unanswered = false;

    // Whether a yes was given and could not be spent, carried out of the block for the same
    // reason: the person answered, and what they are told back has to say that their answer did
    // not land rather than reading as the file simply being quarantined.
    let mut approval_outlived = false;
    // Whether that answer was a mode's, so nobody was asked and the result says so.
    let mut nobody_asked = false;

    // The extension and `stat` of a name decide whether the question is put only where the name is
    // the planner's own words. The name behind a reference came out of a directory nobody vouched
    // for, so a decision made from it would put the question for a `.txt` and not for a `.png`, and
    // under a mode that answers the question for the person the planner would be handed text in one
    // case and told the file holds none in the other. A reference to a file nobody vouched for is
    // offered like any other. The listing that issued it gave references to files only.
    let is_askable = destination == Destination::Reference
        || (media.is_none() && workspace.names_a_file(&proposed_path));

    // The other names the file has are looked up only for a quarantined file the person could be
    // asked about. A read of it is asked under all of them (TRUST-2), and a yes writes a rule about
    // one name, so where another is distrusted, or the lookup of them did not finish, a yes would
    // change nothing and the question is not put.
    if policy.read_is_quarantined(&keyed) && is_askable && {
        let other_names = workspace.other_trust_keys(&proposed_path);
        policy.should_offer_vouch(&keyed, &other_names.keys, other_names.complete)
    } {
        // Whether the question is put has already been settled. What the file holds shapes the
        // preview and decides nothing further: the head of it is cut inside the kernel, so the
        // driver never holds the text, and a file with nothing to show is asked about like any
        // other. The prompt says so in place of the preview.
        let (body, preview_stamp) = policy.capture_files_shared(|policy| {
            (
                workspace.peek_labelled_for_review(&proposed_path),
                policy.file_authority().stamp_of(&keyed),
            )
        });
        // The characters that draw as nothing in the head are written out in the same reshape and
        // counted there, so the driver reads the text for neither. The cut is made first and the
        // spelling keeps the lines the head had, so `truncated` still says whether the file has
        // more lines than the head shows.
        let shaped = policy.render_in_place("read_file", &body, |text| {
            let head: Vec<&str> = text.lines().take(VOUCH_PREVIEW).collect();
            let truncated = text.lines().nth(VOUCH_PREVIEW).is_some();
            let spelled = bravebot_core::invisible::spelled(head.join("\n"));
            (spelled.text, truncated, spelled.hidden)
        });
        let label = shaped.label().to_string();

        // Read here once, and used for the preview and the check below alike, so those two agree
        // with each other (MODE-8). The confirmer reads the mode again when it answers, so the mode
        // can change between the two reads.
        //
        // Bypassing answers the question without showing anybody anything, so no preview is
        // released and the trail does not record one as shown to the user (TRACE-7). If the mode
        // changes to one that asks before the answer, the prompt says the file has nothing to show
        // in place of the preview. If it changes to bypass, the preview was released and the check
        // ran, and the answer is recorded as the mode's.
        let bypassing = tools.permission_mode.get() == crate::PermissionMode::Bypass;
        let (preview, truncated, hidden) = if bypassing {
            (String::new(), false, 0)
        } else {
            let proof = policy.authorise_display_release("the head of a quarantined file");
            shaped.declassify(&proof)
        };

        // The second opinion, over the whole file rather than over the preview. What a yes here
        // grants is that this file's text may be read, so the lines under the cut are part of what
        // is being trusted, and a check over the head alone would report on the part of the file an
        // injection attempt has the least reason to be in.
        //
        // Not made in the one mode that draws no prompt: bypassing answers this question yes without
        // showing anybody anything, so a check there is a model call whose word nobody reads. A
        // verdict is still filled in, and it is the one that claims nothing.
        //
        // The plain mode test and not the one the two promotion gates ask, because screening does not
        // answer a vouch: it is a standing rule about a path rather than one slot's bytes, so under
        // bypass nobody reads this word however the run was started.
        let checked = (!bypassing).then(|| {
            let spec = policy.before_vetting_a_path(&proposed_path, body);
            let asked_at = std::time::Instant::now();
            (
                crate::vet::run(policy, &mut tools.chat, reporter, &spec),
                Some(crate::timing::Interval::since(asked_at)),
            )
        });
        let (verdict, (reason, reason_label)) = match checked {
            // A stop is not a verdict: nobody is asked to vouch, and the file is asked about
            // again by the next read. The stop sentence names the typed name, and the file the
            // check was about is the respelled one, so the note is set here: `respelling` leaves
            // a failure alone (READ-11).
            Some((Err(stopped), elapsed)) => {
                let mut produced = check_stopped(&shown_path, &stopped).waiting(elapsed);
                produced.respelled = respelled;
                return produced;
            }
            Some((Ok(checked), elapsed)) => {
                spent = checked.usage;
                unanswered = !checked.completed;
                waited = elapsed;
                let said = released_reason(policy, checked.reason);
                (checked.verdict, said)
            }
            None => (
                Verdict::Inconclusive("the check was not made"),
                (None, None),
            ),
        };

        let request = crate::confirm::VouchRequest {
            path: proposed_path.clone(),
            rule: policy.file_authority().key(&keyed),
            preview,
            label,
            truncated,
            hidden,
            verdict,
            reason,
            reason_label,
        };
        // The trail line for the offer is written once the answer says who gave it: a person was
        // offered the chance, and a mode that approved offered nobody anything (TRACE-7). The name
        // is the mode's own fixed word, so nothing read reaches the trail through it.
        let (decision, answered_by) = confirmer.confirm_vouch_noting_the_mode(&request);
        match answered_by {
            None => policy.note_vouch_offered(&keyed),
            Some(mode) => policy.record_vouch_answered_by_mode(&keyed, mode),
        }
        if decision == Decision::Approve {
            approval_outlived = !policy.vouch_if_unchanged(&keyed, preview_stamp, answered_by);
            nobody_asked = answered_by.is_some();
            // A record that outlived the yes would take it back on the next turn (MEMORY-5).
            if !approval_outlived {
                crate::memory::trusted_again(
                    workspace.memories(),
                    &policy.file_authority().key(&keyed),
                    crate::workspace::volume_case_folding(workspace.root()).restricts(),
                );
            }
        }
    }

    // What a check cost, on whatever comes back. Every way out below goes through this, including
    // the ones that report a refusal: the request was made and somebody is paying for it whether or
    // not the read that followed handed anything over.
    let priced = |produced: Produced| {
        produced
            .checking(spent)
            .waiting(waited)
            .unanswered(unanswered)
            .respelling(respelled)
    };

    // A yes that arrived too late is said so, rather than left to read as a file nobody was asked
    // about. The bytes the answer was about are gone, so the only thing to do with the answer is
    // throw it away, and the person who gave it is owed the reason: reading again puts the question
    // again, about what the file holds now. Where a mode answered, nobody was asked, and only a
    // write to the path or a decision about it throws the answer away (TRUST-4), so the result says
    // that instead.
    if approval_outlived && nobody_asked {
        return priced(confirmed(
            format!(
                "The permission mode answered the offer to vouch for {shown_path}, so nobody was \
                 asked. {shown_path} was written, or a decision about it or a directory above it \
                 was made, since this read began, so nothing was trusted. Read it again if you \
                 still need it."
            ),
            format!("{shown_path} changed since the read began"),
        ));
    }
    if approval_outlived {
        return priced(confirmed(
            format!(
                "{shown_path} changed while the user was being asked about it, so the answer they \
                 gave was about text that is no longer there and nothing was trusted. Read it \
                 again if you still need it, and they will be asked about what is in it now."
            ),
            format!("{shown_path} changed while it was being asked about"),
        ));
    }

    // A reference to a file the planner may not read already is that file, so reading it has
    // nothing to hand back but another name for the same thing, which reads as the read having
    // failed. One planner went four references deep before giving up. Nothing to do but say so.
    if destination == Destination::Reference && policy.read_is_quarantined(&keyed) {
        // The move that names a processor is left out where the run is not offered
        // `spawn_processor`, rather than offered and refused. The result goes back to the file as
        // `path_ref`, which a run that cannot write is not told of.
        let work_on_it = match (policy.processors_offered(), policy.writing_is_offered()) {
            (true, true) => format!(
                " Give {shown_path} to spawn_processor to work on, and name {shown_path} as \
                 path_ref to write what comes back to the same file."
            ),
            (true, false) => format!(" Give {shown_path} to spawn_processor to work on."),
            (false, _) => String::new(),
        };
        return priced(confirmed(
            format!(
                "{shown_path} already names that file, and nothing will show you what is in \
                 it.{work_on_it} If the work would go better with you reading it yourself, say \
                 so in your reply: the user can vouch for the file, and then you will be shown \
                 it. They know which file {shown_path} is even though you do not."
            ),
            format!("nothing to read: {shown_path} already holds it"),
        ));
    }

    // A picture, decided from the extension of a name the planner typed: the driver's own table, so
    // nothing read chooses this. Never for a reference, whose name is not the planner's to learn.
    //
    // Handed back as a reference whatever the trust map says, unlike text. A processor may look at
    // it and the planner may not, for the reason PASTE-2 gives: a picture in a planner's context
    // may only be one a person put there themselves, and a screenshot with words in it is exactly
    // the content this arrangement exists to keep out of it. So a vouched-for directory does not
    // make a picture readable, and there is no trust question here to ask.
    if let Some(media) = media {
        let read = workspace.read_attachment_counted(policy, &path, media);
        return priced(match read {
            // The size is the one the person is shown beside it, counted when the file was read.
            // A survey of the file answers for text and fails for the bytes of a picture. It is
            // written as counted and compared with nothing, so no unit or plural is chosen from it.
            Ok((encoded, bytes)) => {
                Produced::new(encoded, shown_path, format!("{media}, {bytes} bytes"))
                    .of_content()
                    .of_a_picture(media, bytes)
            }
            Err(e) => read_file_problem(policy, workspace, destination, &typed, &e, &shown_path),
        });
    }

    // A file the planner may not see need not be opened yet. Whether it may see it is a question
    // about the trust map, keyed by the path it named. Nothing about any file's contents decides
    // it, but `survey` does sniff the file's first bytes for text or binary, which is the seventh
    // of labels.md's Known costs.
    //
    // The offset and the limit do not enter it either. They describe a slice of what the planner
    // would have read, and a quarantined read hands back a reference rather than text, so there
    // is nothing for them to be a slice of: honouring them would mean opening the file at the
    // moment the planner asked, which is the one thing this branch exists to avoid. The reference
    // is of the file, and what is read from it is read when something needs the bytes.
    if policy.read_is_quarantined(&keyed) {
        return priced(match workspace.survey(&proposed_path) {
            // Deferred under the keyed name as well. The slot carries the map's answer about this
            // file, and the answer that quarantined it is the one it has to carry: keyed one way
            // and labelled the other, a file held back for being untrusted would arrive in the
            // slot as trusted.
            Ok(bytes) => {
                Produced::deferring(Labelled::trusted(keyed), shown_path, bytes).of_content()
            }
            // A path that names nothing is said so now, exactly as an eager read would have.
            Err(e) => read_file_problem(policy, workspace, destination, &typed, &e, &shown_path),
        });
    }

    // The saved outputs of one notebook cell, when the planner asked for them (READ-10). Only
    // reached for a file the planner may see: a notebook held back above is already a reference to
    // the whole file, outputs included. What comes back is untrusted whatever the notebook's path
    // is trusted as, because a vouch covers what a person wrote into the cells and an output is
    // what a program, a web page or a dataset printed. It is a reference, never text.
    match output_of_from(arguments, &proposed_path) {
        Ok(Some(cell)) => {
            return priced(notebook_outputs(
                policy,
                workspace,
                &path,
                (path_field(destination), &shown_path),
                cell,
            ));
        }
        Ok(None) => {}
        Err(refusal) => return priced(Produced::problem(refusal)),
    }

    // The planner already holds this window if an earlier answer put it there and the file has not
    // been written since (READ-8). Looked up by the path as the trust map keys it and the window as
    // the read applies it, so a spelling or a limit that comes to the same lines is the same read.
    let (first_line, line_limit) = crate::workspace::window_of(offset, limit);
    let window_key = workspace.window_key(&proposed_path);
    let earlier = tools
        .reads
        .token_of(&window_key, first_line, line_limit)
        .map(str::to_string);
    let shown_token;
    let page = match workspace.read_page_unless_unchanged(
        policy,
        &path,
        offset,
        limit,
        earlier.as_deref(),
    ) {
        Ok(crate::workspace::Reading::Page { page, token }) => {
            workspace.record_directories(&shown_path);
            shown_token = token;
            page
        }
        Ok(crate::workspace::Reading::Unchanged) => {
            // Both halves are the driver's own words: the token is metadata of the file and the
            // window is what the planner asked for. Nothing the file holds is in either.
            let token = earlier.unwrap_or_default();
            let asked = match arguments.get("limit").and_then(Value::as_u64) {
                Some(limit) => format!("from line {first_line}, at most {limit} lines"),
                None => format!("from line {first_line}"),
            };
            return priced(Produced {
                origin: shown_path.clone(),
                ..confirmed(
                    format!(
                        "{shown_path} ({asked}) has not changed since it was shown to you earlier \
                         in this conversation, so its lines are not sent again: they are in that \
                         earlier result.\n\n(change token {token})"
                    ),
                    format!("unchanged since it was shown, not sent again ({token})"),
                )
            });
        }
        Err(e) => {
            return priced(read_file_problem(
                policy,
                workspace,
                destination,
                &typed,
                &e,
                &shown_path,
            ));
        }
    };

    // What this read would put in a model's context, scanned before it gets there. The planner's
    // context goes to whoever performs inference, so a file handed to it has been handed to them,
    // and a scan that ran afterwards would be reporting a disclosure that had already happened
    // (CRED-15).
    //
    // Over the window rather than the whole file, because the window is what is disclosed: a key
    // five thousand lines below the page the planner asked for is not this read's exposure, and
    // holding the read back over it would refuse work on grounds the person cannot see in what
    // they were shown. `offset` is where the window starts, so a finding names the line they
    // would open their editor at; it is the caller's own number and no part of the file.
    let body = policy.render_in_place("read_file", &page, |p| p.lines.join("\n"));
    let found = policy.scan_a_read("read_file", &shown_path, offset, &body);

    if !found.is_empty() && !policy.read_exposure_is_allowed(&keyed) {
        // Written down before anybody is asked, which is the other half of where a finding goes:
        // a line drawn while nobody was looking is gone when the turn ends, and the scan exists
        // to tell a person what is in their own tree (CRED-19). Written whichever way the
        // question below is answered, because a refusal and an approval are equally findings.
        //
        // Here rather than beside the scan so that a planner reading one file on round after
        // round writes one entry rather than one per round. The session's answer is what bounds
        // it: this branch is reached once per path per session.
        tools
            .recording()
            .record(workspace.root(), &found.iter().collect::<Vec<_>>());

        let request = crate::confirm::ExposureRequest {
            path: shown_path.clone(),
            credentials: described(&found),
        };
        let (decision, answered_by) = confirmer.confirm_exposing_read_noting_the_mode(&request);
        if decision != Decision::Approve {
            policy.refuse_unapproved_exposure("read_file");
            // The planner is told that much and no more. Enough to stop it reading the same file
            // a second way, and nothing about what was found, because a finding may not enter a
            // model's context however the question was answered.
            return priced(Produced::refused_with_a_note(
                format!(
                    "refused: {shown_path} holds what looks like a credential, and the user did \
                     not agree to you being shown it, so none of its text was read. Do not try \
                     to read it another way: work without it, or say in your reply what you \
                     needed from it."
                ),
                format!("not shown, it holds {}", exposure_note(&found)),
            ));
        }
        policy.allow_exposing_read(&keyed, answered_by);
    }

    // A read that was refused above is no touch of the file. Nor is one through a reference, where
    // `shown_path` is the reference's name and the file's own was never the planner's to say
    // (INSTR-16). The spelling recorded is the one opened, which after a respelling is not the one
    // typed and is the one that names the file.
    if destination == Destination::Named {
        workspace.record_file(&proposed_path);
    }

    // Reshaped inside the kernel, so the driver never holds the text. Only
    // `Policy::present` decides whether the planner sees what comes out.
    let note = note_for(policy, "read_file", &page, |p| {
        tally(p.lines.len(), "line", "lines")
    });
    let rendered =
        policy.render_in_place("read_file", &page, |p| render_page(&p, ChangeToken::Shown));
    // Without the paging and change token after it, which are the driver's words: a glimpse that
    // counted them would say a file had two more lines than it has, and draw them for a short one.
    let glimpsed = policy.render_in_place("read_file", &page, |p| p.lines.join("\n"));
    priced(Produced {
        glimpsed: Some(glimpsed),
        window: Some(crate::conversation::ReadWindow {
            file: window_key,
            offset: first_line,
            limit: line_limit,
            token: shown_token,
        }),
        ..Produced::new(rendered, shown_path, exposed_note(note, &found)).of_content()
    })
}

/// The findings in a file, each said the one way a finding may be said.
fn described(found: &[bravebot_core::credentials::Finding]) -> Vec<String> {
    describe_all(&found.iter().collect::<Vec<_>>())
}

/// The same list as one phrase, for a line rather than a screen.
fn exposure_note(found: &[bravebot_core::credentials::Finding]) -> String {
    described(found).join("; ")
}

/// What the person watching is told beside a read that carried a credential to the planner.
///
/// The person's half of the result and never the planner's, which is the same split the write
/// gate's findings go through: the text reached a model because somebody agreed it should, and
/// the line they read afterwards says what went with it.
fn exposed_note(note: String, found: &[bravebot_core::credentials::Finding]) -> String {
    if found.is_empty() {
        return note;
    }
    format!("{note}; shown despite holding {}", exposure_note(found))
}

/// What the lines an edit comes back with hold that would authenticate somewhere and that the
/// planner has not already been given.
///
/// Scans exactly the lines the result shows, at their numbers in the file: a key between two changed
/// regions, in the part the excerpt cuts, or past its context, is not in the planner's context, and
/// a question about it would be one the person cannot connect to anything they were shown.
///
/// A value that nothing in the file held before the edit is left out, because the edit's own words
/// wrote it. The write was put to the person with it on the screen, and the result repeating it
/// discloses nothing the planner lacked. A value the file already held is not left out, even where
/// the planner's words repeat it.
///
/// Not recorded here: `scan_a_write` ran over the whole of the edited file and recorded every
/// finding in it at these same lines, and a second entry for each would be a duplicate.
fn credentials_an_edit_would_show<S: Sink>(
    policy: &mut Policy<'_, S>,
    shown_path: &str,
    region: &Labelled<Option<crate::replace::Excerpt>>,
    written: &Scanned,
) -> Vec<bravebot_core::credentials::Finding> {
    // The caller has already checked that this is trusted, so the driver may read it. Only where
    // each run starts is taken from it: the text is shaped again below, inside the kernel, so that
    // it reaches the scan carrying its label.
    let Ok(Some(excerpt)) = policy.read_trusted_content("edit_file", region) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (at, (first_line, _)) in excerpt.runs().enumerate() {
        let run = policy.render_in_place("edit_file", region, |region| {
            region
                .and_then(|excerpt| excerpt.runs().nth(at))
                .map(|(_, text)| text)
                .unwrap_or_default()
        });
        found.extend(policy.scan_a_read("edit_file", shown_path, first_line, &run));
    }
    found.retain(|finding| {
        !written
            .authored
            .iter()
            .any(|authored| authored.fingerprint == finding.fingerprint)
    });
    found
}

/// The text a slot holds for a file, read at the moment something needs it.
///
/// The same shaping an eager read would have applied, from the same two functions, so a
/// deferred read and an immediate one put the same bytes in the same slot. Deferring changes
/// when a file is read and nothing else about it.
///
/// A file in another encoding is read as the text it decodes to, and the result says so, so that
/// the slot records it and nothing writes it back as the file (READ-3).
pub(crate) fn read_into_slot(
    workspace: &Workspace,
    path: &str,
) -> Result<bravebot_core::Fill, crate::workspace::WorkspaceError> {
    workspace.page_decoded(path).map(|mut page| {
        // The note about a decoding is for the planner reading a page. It is not part of the file.
        let decoded = std::mem::replace(&mut page.decoding, bravebot_filetype::Decoding::Utf8)
            != bravebot_filetype::Decoding::Utf8;
        let mut text = render_page(&page, ChangeToken::Withheld);
        // A file that went through a slot used to come back a byte shorter than it went in,
        // because the lines are joined with newlines between them and none after. Every
        // processed file lost its last newline, which the next diff anybody reads calls
        // "no newline at end of file".
        if page.ends_with_newline && !text.ends_with('\n') {
            text.push('\n');
        }
        if decoded {
            bravebot_core::Fill::decoded(text)
        } else {
            text.into()
        }
    })
}

/// Read the files any of these slots is still waiting on.
///
/// Called by every consumer of a slot before it asks for the bytes. Doing nothing where the
/// slots hold their contents already, so a consumer need not know whether an earlier one got
/// there first.
pub(crate) fn materialise<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    slots: &mut SlotStore,
    tool: &str,
    wanted: &[SlotId],
) -> Result<Vec<String>, String> {
    // The files this actually opened, for the line the person reads. A read deferred until a
    // processor needed it is still a read of their workspace, and until it was reported the only
    // reads on the screen were the planner's, which are the ones that read nothing.
    // Exclusive: `Policy::materialise` emits the slot's `SlotWritten` and trail lines inside
    // this closure, and a shared capture must emit nothing while it is held.
    let nobody_to_ask = policy.nobody_to_ask();
    // The reach the call holds, judged as a read: a reference is opened to be read whatever the
    // call goes on to do with it, and a call that holds no reach (outside bypass) is given none.
    workspace.reading_under_the_reach(|| {
        materialise_held(policy, workspace, slots, tool, wanted, nobody_to_ask)
    })
}

fn materialise_held<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    slots: &mut SlotStore,
    tool: &str,
    wanted: &[SlotId],
    nobody_to_ask: bool,
) -> Result<Vec<String>, String> {
    policy.capture_files(|policy, _capture| {
        let mut opened = Vec::new();
        for slot in wanted {
            let was_unread = slots.is_unread(slot);
            if was_unread {
                // A rule covers the file, not the spelling of it (PERM-7). The name the slot
                // stored was judged when it was issued, and the file it lands on may have
                // changed since: a link put there afterwards reaches a file a rule covers under
                // a name no rule spells. The same question a `path_ref` is asked, before the
                // file is opened, and worded about the slot because the path came out of a
                // directory nobody vouched for. Decided from the path and the rules, never from
                // anything the file holds.
                let stored = policy
                    .promote_reference_for_read(tool, "slot", slot, slots)
                    .map_err(|denial| format!("refused: {denial}"))?
                    .into_trusted()
                    .map_err(|_| "error: the reference was not promoted".to_string())?;
                refuse_denied_path_as(
                    policy,
                    workspace,
                    Purpose::Read,
                    &stored,
                    &slot.to_string(),
                )?;
            }
            // A picture or PDF is opened as one, from the driver's table of extensions as
            // `read_file` decides it, so nothing read chooses. Read as text it would be refused
            // as binary, and a slot that holds one has to say so for a check to look at the file.
            let mut picture = None;
            let mut escaped = None;
            let read = policy
                // Worded about the slot, never about the file it stands for. A deferred read
                // opens a path that came out of a directory nobody vouched for, and what the
                // failure is put into is a sentence the planner reads as the driver's own, so
                // the name goes back as the reference the planner is holding.
                .materialise(tool, slot, slots, |path| {
                    match bravebot_filetype::by_name::media_for(path) {
                        Some(media) => {
                            picture = Some(media);
                            workspace
                                .attachment_text(path, media)
                                .map(bravebot_core::Fill::from)
                        }
                        None => read_into_slot(workspace, path),
                    }
                    .map_err(|e| {
                        let (said, remedy) = describe_for(&e, &slot.to_string(), nobody_to_ask);
                        escaped = remedy;
                        said
                    })
                });
            // Recorded here rather than where it is worded, which is inside the policy's read.
            if let Some(remedy) = escaped {
                record_confinement_refusal(policy, tool, &slot.to_string(), remedy);
            }
            read.map_err(|denial| format!("refused: {denial}"))?;
            if let Some(media) = picture {
                policy.holds_a_picture(slot, media, slots);
            }
            if was_unread {
                opened.push(slot.clone());
            }
        }

        if opened.is_empty() {
            return Ok(Vec::new());
        }
        let named = policy.names_for_display(slots);
        Ok(opened
            .iter()
            .map(|slot| {
                named
                    .iter()
                    .find(|(id, _, _)| id == slot)
                    .map(|(slot, label, path)| format!("{slot}{label}:{path}"))
                    .unwrap_or_else(|| slot.to_string())
            })
            .collect())
    })
}

/// The path a call is about, from `path` or from a reference to a file.
///
/// A planner working in a directory it may not read has no filename to type, so it names the
/// reference the listing gave it instead. What comes back is the same in both cases: the path,
/// and how it was arrived at, which is what decides whether an approval can be skipped.
fn path_argument<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    tool: &'static str,
    purpose: Purpose,
    slots: &SlotStore,
    arguments: &Value,
) -> Result<PathArgument, String> {
    let named = named_argument(arguments, "path");
    let referenced = named_argument(arguments, "path_ref");

    match (named, referenced) {
        (Some(_), Some(_)) => Err(
            "error: give 'path' or 'path_ref', not both. Use path_ref alone for a file you \
             were never shown the name of."
                .to_string(),
        ),
        (None, None) => Err("error: one of 'path' or 'path_ref' is required".to_string()),
        (Some(path), None) => {
            let mut shown = policy
                .read_planner_argument(tool, "path", &path)
                .map_err(|denial| format!("refused: {denial}"))?;
            refuse_denied_path(policy, workspace, purpose, &shown)?;
            // A `..` into an open directory is reported, trusted and keyed as the absolute path it
            // lands on, which is what the rules above were also asked about (TRUST-10).
            //
            // The labelled value is respelled the same way, with the label it arrived with: a
            // write is endorsed for `released` and its gate consumes the endorsement for the
            // value it is handed, so the two have to be the one string.
            let path = match workspace.climbed_to(&shown) {
                Some(landing) => {
                    shown = landing;
                    Labelled::new(shown.clone(), path.label())
                }
                None => path,
            };
            Ok(PathArgument {
                path,
                destination: Destination::Named,
                released: shown.clone(),
                shown,
            })
        }
        (None, Some(reference)) => {
            let slot = accept_named_reference(
                policy,
                slots,
                tool,
                "path_ref",
                &reference,
                NamesToList::Text,
            )?;
            // Which gate the name comes out of is decided by what this call will do with it: a
            // read may promote it, an effect may not and needs a person instead. Asking the
            // wrong one is not possible from here, because the caller says which it is.
            //
            // Both hand the name back as well as the value, because this name is not the
            // planner's words and must not be read as though it were: it came out of a
            // directory nobody vouched for, which is the whole reason the reference exists.
            // The gate that released it is the one above, which is why nothing below asks a
            // second one for the same string.
            let (path, resolved) = match purpose {
                Purpose::Read => {
                    let promoted = policy
                        .promote_reference_for_read(tool, "path_ref", &slot, slots)
                        .map_err(|denial| format!("refused: {denial}"))?;
                    let resolved = promoted
                        .clone()
                        .into_trusted()
                        .map_err(|_| "error: the reference was not promoted".to_string())?;
                    (promoted, resolved)
                }
                Purpose::Effect => {
                    let named = policy
                        .destination_from_reference(tool, "path_ref", &slot, slots)
                        .map_err(|denial| format!("refused: {denial}"))?;
                    // Untrusted and public, which is what a name out of a directory nobody
                    // vouched for is. The endorsement is what will authorise it, not its label.
                    let path = Labelled::new(
                        named.clone(),
                        bravebot_core::label::Label::untrusted_public(),
                    );
                    (path, named)
                }
            };
            // A rule covers the file, not the spelling of it. A path that arrived through a
            // reference is the same file as one the planner typed, so the rule that would have
            // refused the second refuses the first. The path itself stays out of the refusal:
            // what goes back to the planner names the reference, as it does everywhere else.
            refuse_denied_path_as(policy, workspace, purpose, &resolved, &slot.to_string())?;
            Ok(PathArgument {
                path,
                destination: Destination::Reference,
                shown: slot.to_string(),
                released: resolved,
            })
        }
    }
}

/// Refuse a path a `deny` rule covers, before the file is opened.
///
/// Both what the call will do and what it may see are asked about: a read consults the `Read`
/// rules, and an effect consults `Edit` and `Read` both, because a file nothing may read is not
/// protected if it can be overwritten.
///
/// The rules are asked about the name as given and again about the name it lands on, since a rule
/// covers the file and a link, a case variant or a short name reaches the same file under another
/// spelling (PERM-7). The refusal names the path as given: the landed name can come from a
/// directory nobody vouched for.
fn refuse_denied_path<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    purpose: Purpose,
    path: &str,
) -> Result<(), String> {
    refuse_denied_path_as(policy, workspace, purpose, path, path)
}

/// Whether a `deny` rule refuses a write to `path` now: the walk every write tool makes before
/// it asks the person, asked again by the workspace right before the bytes are released, so a link
/// put at the destination while the question was open is judged by the file it leads to
/// (PERM-7, TRUST-10). The refusal text is not used: the workspace words its own, about the name
/// the person was shown.
pub(crate) fn refuse_write_by_rules<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    path: &str,
) -> Result<(), String> {
    refuse_denied_path(policy, workspace, Purpose::Effect, path)
}

/// Whether a `deny` rule refuses a read of `path`, asked exactly as a read tool asks it, for a
/// caller that has no planner to word a refusal for: a file a person asked to see is refused for
/// the same names as a file a call asked to read.
pub(crate) fn read_is_refused_by_a_rule<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    path: &str,
) -> bool {
    refuse_denied_path(policy, workspace, Purpose::Read, path).is_err()
}

/// [`refuse_denied_path`], naming `shown` in the refusal where the path itself is not to be said.
fn refuse_denied_path_as<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    purpose: Purpose,
    path: &str,
    shown: &str,
) -> Result<(), String> {
    let ask = |policy: &mut Policy<'_, S>, name: &str| {
        match purpose {
            Purpose::Read => policy.before_read(name),
            Purpose::Effect => policy.refuse_denied_write(name),
        }
        .map_err(|_| {
            let reason = reason_for_denial(policy, purpose, name);
            denied_by_rule(shown, reason.as_deref())
        })
    };
    ask(policy, path)?;
    // Spelled out as well, because a rule anchored at the home directory never matches `~/x`, and
    // the landing below exists only once the home is opened, so until then the refusal would offer
    // opening it for a file a rule refuses once it is.
    let expanded = workspace.expanded(path);
    if expanded != path {
        ask(policy, &expanded)?;
    }
    let landing = workspace.landing(path);
    if let Some(landed) = &landing {
        ask(policy, landed)?;
    }
    // A hard link opens the file under a name no rule spells, and nothing in the name says which.
    // Where the walk for the file's other names gave up, a rule about a path may be about
    // this file, so it is refused rather than passed.
    let hard_links = workspace.hard_links(path);
    match &hard_links {
        Some(names) => {
            for name in names {
                ask(policy, name)?;
            }
        }
        None if policy.any_path_is_denied() => return Err(denied_by_rule(shown, None)),
        None => {}
    }
    // A second path to a directory that is open (a firmlink, a bind mount) spells the file in a way
    // no rule about the first path matches, and no link is followed to say so.
    // Where a name is not UTF-8 no rule can be written about it, so a rule about a path is refused
    // rather than passed, as for the hard links above.
    match workspace.same_tree_names(path) {
        Some(names) => {
            for name in &names {
                ask(policy, name)?;
            }
        }
        None if policy.any_path_is_denied() => return Err(denied_by_rule(shown, None)),
        None => {}
    }
    // A definition's limit is written about workspace-relative names, so it is judged on the name
    // given in that spelling (`/work/docs/a.md` is `docs/a.md`) and on the file the name lands on
    // (`docs/link` is wherever the link goes). Both have to be inside: a link outside the limit
    // that points inside it is outside.
    if purpose == Purpose::Effect {
        let given = workspace.spelled_in_workspace(path);
        let mut judged: Vec<&str> = given
            .as_deref()
            .into_iter()
            .chain(landing.as_deref())
            .collect();
        if judged.is_empty() {
            judged.push(path);
        }
        for name in judged {
            policy
                .refuse_write_outside_limits(name)
                .map_err(|_| outside_write_limit(shown))?;
        }
        // A hard link inside the limit to a file outside it is the same file, so every name the
        // file has is judged and one outside refuses the write. Where the walk for the names gave
        // up, a name outside may exist, so a limit in force refuses (DELEGATE-28).
        match &hard_links {
            Some(names) => {
                for name in names {
                    policy
                        .refuse_write_outside_limits(name)
                        .map_err(|_| outside_write_limit(shown))?;
                }
            }
            None if policy.has_write_limits() => return Err(outside_write_limit(shown)),
            None => {}
        }
    }
    Ok(())
}

/// Word a workspace failure about `named` for the planner, recording it first when it is a path
/// refused for leaving the workspace.
///
/// A tool reports such a failure through here, so the planner cannot be told of a refusal the
/// trail leaves out (TRACE-1). The record is worded from `named` for the reason
/// [`WorkspaceError::describe`] gives. A write refused as stale is recorded too, in fixed words
/// that say whether a person was asked. The other failures are not gate decisions and are only
/// worded. A deferred read is worded inside the policy's own read, so [`materialise`] records its
/// refusal itself.
///
/// [`WorkspaceError::describe`]: crate::workspace::WorkspaceError::describe
fn workspace_failure<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
    field: &str,
    e: &crate::workspace::WorkspaceError,
    named: &str,
) -> String {
    let (said, remedy) = describe_for(e, named, policy.nobody_to_ask());
    if let Some(remedy) = remedy {
        record_confinement_refusal(policy, &format!("{tool}.{field}"), named, remedy);
    }
    match e {
        crate::workspace::WorkspaceError::Stale { .. } => policy.refuse_stale_write(tool, true),
        crate::workspace::WorkspaceError::StaleUnasked { .. } => {
            policy.refuse_stale_write(tool, false)
        }
        _ => {}
    }
    said
}

/// Said after a refusal for leaving the workspace whose remedy named a route only a person can
/// take, where nobody can be asked in this run (CLI-1, TRUST-10). Names no directory to open and
/// no file to drop, and tells the planner what it can do instead.
const NOBODY_TO_ASK: &str = ". Nobody can be asked in this run, so nothing this session accepts \
     makes that path reachable: say in the reply which path the work needs and what for";

/// `e` worded about `named`, and the remedy the planner was told where `e` is a refusal for
/// leaving the workspace, so the trail records what the planner read (TRACE-1).
///
/// Where nobody can be asked, a remedy that names `/add-dir`, `--add-dir` or a file drop is said as
/// the remedy that names none, followed by [`NOBODY_TO_ASK`] (TRUST-10). Decided from the remedy and
/// the session's setting, never from anything a file or a program holds.
fn describe_for(
    e: &crate::workspace::WorkspaceError,
    named: &str,
    nobody_to_ask: bool,
) -> (String, Option<crate::workspace::Remedy>) {
    use crate::workspace::WorkspaceError;
    let WorkspaceError::Escapes { path, remedy } = e else {
        return (e.describe(named), None);
    };
    match nobody_to_ask
        .then(|| remedy.when_nobody_can_be_asked())
        .flatten()
    {
        Some(instead) => {
            let said = WorkspaceError::Escapes {
                path: path.clone(),
                remedy: instead,
            }
            .describe(named);
            (format!("{said}{NOBODY_TO_ASK}"), Some(instead))
        }
        None => (e.describe(named), Some(*remedy)),
    }
}

/// Put the refusal of a path the workspace's confinement turned away into the trail, worded as the
/// planner was told it: a path inside the workspace is not recorded as one that leaves it.
fn record_confinement_refusal<S: Sink>(
    policy: &mut Policy<'_, S>,
    call: &str,
    named: &str,
    remedy: crate::workspace::Remedy,
) {
    match remedy {
        crate::workspace::Remedy::Inside | crate::workspace::Remedy::InsideClimb => {
            policy.refuse_path_inside_workspace(call, named, remedy.offered());
        }
        _ => policy.refuse_outside_workspace(call, named, remedy.offered()),
    }
}

/// Whether a `read_file` of the path `edit_file` refused would put the vouch question, and who
/// would answer it (EDIT-2, TRUST-8). Settled from the turn's list, the mode, the trust map and
/// the path, never from the file's bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadToVouch {
    /// No read would put the question: the turn is not offered `read_file`, the path was already
    /// asked about this turn, a yes could not open it, or nobody is there to ask.
    Not,
    /// A read puts the question to the person.
    AsksThePerson,
    /// A read puts the question and the bypass mode answers it, so nobody is asked.
    TheModeAnswers,
}

/// What `edit_file` says of a file it will not edit because the file is not trusted: it is not
/// vouched for, or untrusted content was written to it (EDIT-2). Fixed text about the path's
/// state. It holds no byte of the file and no name, and the label is the one the driver assigned.
/// It names `write_file` and `run` only where the turn is offered them (TOOL-6). Where a
/// `read_file` of the path would put the vouch question, the read is named as the way to put it,
/// and under bypass as a read the mode vouches for, with no person named. Otherwise the vouch is
/// named as the person's to give, on every turn, a delegate's included, because nothing on the
/// turn says it cannot ask a person.
fn closed_to_editing<S: Sink>(policy: &Policy<'_, S>, label: Label, read: ReadToVouch) -> String {
    let tools = match (policy.writing_is_offered(), policy.running_is_offered()) {
        (true, true) => Some("Write the whole file with write_file, edit it through run"),
        (true, false) => Some("Write the whole file with write_file"),
        (false, true) => Some("Edit it through run"),
        (false, false) => None,
    };
    let vouch = match (read, tools.is_some()) {
        (ReadToVouch::Not, false) => "Only the person can open it, by vouching for the path",
        (ReadToVouch::Not, true) => "ask the person to vouch for the path",
        (ReadToVouch::AsksThePerson, false) => {
            "Read it with read_file, which asks the person to vouch for the path, and edit it \
             with edit_file once they have"
        }
        (ReadToVouch::AsksThePerson, true) => {
            "read it with read_file, which asks the person to vouch for the path, and edit it \
             with edit_file once they have"
        }
        (ReadToVouch::TheModeAnswers, false) => {
            "Read it with read_file, which in this mode vouches for the path without asking \
             anybody, and then edit it with edit_file"
        }
        (ReadToVouch::TheModeAnswers, true) => {
            "read it with read_file, which in this mode vouches for the path without asking \
             anybody, and then edit it with edit_file"
        }
    };
    let routes = match tools {
        Some(tools) => format!("{tools}, or {vouch}"),
        None => vouch.to_string(),
    };
    format!(
        "refused: this file is closed to edit_file, label {label}: it is not vouched for, or \
         untrusted content was written to it, and an edit has to compare its text. {routes}"
    )
}

/// The refusal of `output_of` for a notebook whose contents the driver may not examine, worded
/// from the tools the turn is offered (TOOL-6).
///
/// A read of a file the trust map does not vouch for answers with a reference before `output_of`
/// is looked at, so this is reached only if the map changed during the call. The text holds no
/// byte of the file and no name, and the label is the one the driver assigned. It names
/// `spawn_processor` and `run` only where the turn is offered them, and always the vouch, which
/// is the person's to give and which nothing on the turn says a delegate cannot ask for. A plain
/// read of the file is named only beside `spawn_processor`, because the reference it returns is
/// of use to the planner only through a processor.
fn closed_to_reading_outputs<S: Sink>(policy: &Policy<'_, S>, label: Label) -> String {
    let routes = match (policy.processors_offered(), policy.running_is_offered()) {
        (true, true) => {
            "Read the file with read_file and no output_of for a reference to give to \
             spawn_processor, read it through run, or ask the person to vouch for the path"
        }
        (true, false) => {
            "Read the file with read_file and no output_of for a reference to give to \
             spawn_processor, or ask the person to vouch for the path"
        }
        (false, true) => "Read it through run, or ask the person to vouch for the path",
        (false, false) => "Only the person can open it, by vouching for the path",
    };
    format!(
        "refused: the outputs of this notebook are closed to read_file, label {label}: it is not \
         vouched for, or untrusted content was written to it. {routes}"
    )
}

/// The refusal of a `read_git` answer the driver may not examine, worded from whether the turn is
/// offered `run` (TOOL-6).
///
/// An answer is labelled from the trust map before it is examined and an untrusted one is not
/// examined at all, so this is reached only if the map changed during the call. A reference to the
/// answer is not offered as a way out, because this tool does not return one. `run` is named only
/// where the turn holds it.
fn closed_to_read_git(running: Running, label: Label) -> String {
    let routes = if running.offered() {
        "Ask git through run, or ask the person to vouch for the repository"
    } else {
        "Only the person can open it, by vouching for the repository"
    };
    format!(
        "refused: this answer is closed to read_git, label {label}: the repository is not \
         vouched for, or untrusted content was written to it. {routes}"
    )
}

/// The examination of a `read_git` answer for the credential scan, or the refusal the planner is
/// given where the driver may not examine it (TOOL-6).
///
/// `read_git` calls it only for an answer labelled trusted, so the refusal is not reached from a
/// turn: the label the guard tests and the label this checks are the one the driver assigned.
/// The refusal is built here, apart from the guard, so a test can give it an answer the guard
/// would not let through.
fn examine_git_answer<S: Sink>(
    policy: &mut Policy<'_, S>,
    answer: &Labelled<crate::git::Answer>,
    running: Running,
) -> Result<crate::git::Answer, String> {
    policy
        .read_trusted_content("read_git", answer)
        .map_err(|_| closed_to_read_git(running, answer.label()))
}

/// [`workspace_failure`] as the `error:` result a file tool returns, so each tool's refusal of a
/// path the workspace turned away is worded and recorded in one place.
fn file_tool_problem<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
    field: &str,
    e: &crate::workspace::WorkspaceError,
    named: &str,
) -> Produced {
    Produced::problem(format!(
        "error: {}",
        workspace_failure(policy, tool, field, e, named)
    ))
}

/// [`file_tool_problem`] for `read_file`, which adds to a not-found on the name the planner typed
/// that the file may be named with another kind of space (READ-11).
///
/// The hint is decided from `typed` and not from `shown_path` or from any name read from disk, and
/// it is added only where the destination is the named path: a name reached through a reference
/// is an entry of a directory listing, which may not pick a sentence (LIST-2).
fn read_file_problem<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    destination: Destination,
    typed: &str,
    e: &crate::workspace::WorkspaceError,
    shown_path: &str,
) -> Produced {
    let hint = match destination {
        Destination::Named => workspace.kind_of_space_hint(e, typed),
        Destination::Reference => "",
    };
    Produced::problem(format!(
        "error: {}{hint}",
        workspace_failure(policy, "read_file", path_field(destination), e, shown_path)
    ))
}

/// The argument a path arrived in.
fn path_field(destination: Destination) -> &'static str {
    match destination {
        Destination::Named => "path",
        Destination::Reference => "path_ref",
    }
}

/// The result of a write the workspace turned away, worded about `shown_path` and recorded as
/// [`workspace_failure`] records it.
///
/// A write that leaves the workspace can still be made by a line the person approves with no
/// sandbox, which the planner has no other way to learn of (SANDBOX-29). A path inside the
/// workspace does not leave it, and its relative spelling is the way.
fn write_refused<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    tool: &str,
    destination: Destination,
    e: &crate::workspace::WorkspaceError,
    shown_path: &str,
) -> Produced {
    use crate::workspace::{Remedy, WorkspaceError};
    let way_out = match e {
        WorkspaceError::Escapes {
            remedy: Remedy::Inside | Remedy::InsideClimb,
            ..
        } => "",
        WorkspaceError::Escapes { .. } => unconfined_way_out(policy, tools),
        _ => "",
    };
    Produced::problem(format!(
        "error: {}{way_out}",
        workspace_failure(policy, tool, path_field(destination), e, shown_path)
    ))
}

/// The result of a download whose destination the workspace turned away, worded about `shown_path`
/// and recorded as [`workspace_failure`] records it.
fn download_refused<S: Sink>(
    policy: &mut Policy<'_, S>,
    e: &crate::workspace::WorkspaceError,
    shown_path: &str,
) -> Produced {
    Produced::problem(format!(
        "error: {}",
        workspace_failure(policy, "download_url", "path", e, shown_path)
    ))
}

/// Whether a write to `path` is to be put to a person, asked about the name as given and about
/// the name it lands on and the other names of a file with a hard link. A rule that asks, or a table that does, about either one is enough: the
/// second name can add a question and never removes one (PERM-7). A file whose other names could
/// not all be found (the walk stopped at its cap or could not list a directory) asks, since a name
/// that was not found may be the one a rule is written about.
pub(crate) fn write_needs_approval<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    path: &str,
    contents: Label,
    destination: Destination,
) -> bool {
    policy.write_needs_approval(path, contents, destination)
        || workspace.is_the_named_settings_file(path)
        || workspace.landing(path).is_some_and(|landed| {
            policy.write_needs_approval(&landed, contents, Destination::Named)
        })
        || workspace.hard_links(path).is_none_or(|names| {
            names
                .iter()
                .any(|name| policy.write_needs_approval(name, contents, Destination::Named))
        })
}

/// Whether a write to `path` is put to the confirmer in a run with nobody to ask, where
/// [`write_needs_approval`] said it need not be.
///
/// The trust map such a run holds was written by its own command line, and what it vouches for is
/// what the tree's files say, never a write the planner composed, so a write the map alone let
/// through is still put (CLI-1). A rule the person wrote is what does decide a write there, so one
/// that allows the write under every name it is asked about (the name given, the name it lands on
/// and the other names of a file with a hard link, PERM-7) is not put. A file whose other names
/// could not all be found is put, since a name that was not found may be one no rule allows.
pub(crate) fn put_where_nobody_can_be_asked<S: Sink>(
    policy: &Policy<'_, S>,
    workspace: &Workspace,
    path: &str,
) -> bool {
    use bravebot_core::permissions::{Decision, Ruling, Subject};
    let allowed = |name: &str| {
        policy.permissions().for_path(Subject::Edit, name) == Decision::Ruled(Ruling::Allow)
    };
    !(allowed(path)
        && workspace
            .landing(path)
            .is_none_or(|landed| allowed(&landed))
        && workspace
            .hard_links(path)
            .is_some_and(|names| names.iter().all(|name| allowed(name))))
}

/// Whether `path`, as named or as it lands after links are followed, is one a write asks about
/// whatever the mode or a rule says (MODE-2, PERM-9). Read off the routing path alone, and the
/// other names of a file with a hard link, which a link is not followed to. A file whose other
/// names could not all be found is protected: one of them may be a protected path.
pub(crate) fn write_is_protected(workspace: &Workspace, path: &str) -> bool {
    bravebot_core::protected::is_protected(path)
        || workspace.is_the_named_settings_file(path)
        || workspace
            .landing(path)
            .is_some_and(|landed| bravebot_core::protected::is_protected(&landed))
        || workspace.hard_links(path).is_none_or(|names| {
            names
                .iter()
                .any(|name| bravebot_core::protected::is_protected(name))
        })
}

/// Whether a file a `run` line redirects output into is one a write asks about whatever the mode
/// or a rule says (MODE-2, PERM-9): the name as the line spelled it, the names it reaches once a
/// `..` is taken out or every link is followed, and the other names of a file with several hard
/// links. A file whose other names could not all be found is protected, as in
/// [`write_is_protected`], and so is one reached under a name that is not UTF-8. Read off the
/// routing path and the file system alone, never a byte of
/// any file, and asked for `>` and `>>` alike since only the path is read.
pub(crate) fn redirection_is_protected(workspace: &Workspace, path: &std::path::Path) -> bool {
    let mut reached = workspace.paths_reached(path);
    reached.push(path.to_path_buf());
    if reached
        .iter()
        .any(|name| workspace.reaches_the_named_settings_file(name))
    {
        return true;
    }
    // A name that is not UTF-8 has no spelling a rule can be written about, and its lossy
    // rendering is the name of a different file, so the file it opens could be a hard link to a
    // protected one. It is protected, as a file whose other names could not all be found is
    // (PATH-001).
    if reached.iter().any(|name| name.to_str().is_none()) {
        return true;
    }
    let names: Vec<String> = reached
        .iter()
        .map(|name| workspace.relative_display(name))
        .collect();
    names.iter().any(|name| {
        bravebot_core::protected::is_protected(name)
            || workspace.hard_links(name).is_none_or(|others| {
                others
                    .iter()
                    .any(|other| bravebot_core::protected::is_protected(other))
            })
    })
}

/// The reason the person wrote beside the rule that refuses `name`, where there is one (PERM-1).
///
/// The rules are asked which rule decides, exactly as the gate just did, and the reason is read off
/// it. A reason reaches here only from the person's own settings file: `bravebot_config` drops the
/// key in every other layer, so a sentence a checkout wrote is already absent by the time this is
/// asked and cannot be put in front of the planner.
///
/// The subjects are the ones the gate consulted, in the order it consulted them, so the reason
/// named is the one from the rule that refused.
fn reason_for_denial<S: Sink>(
    policy: &Policy<'_, S>,
    purpose: Purpose,
    name: &str,
) -> Option<String> {
    use bravebot_core::permissions::{Ruling, Subject};
    let subjects: &[Subject] = match purpose {
        Purpose::Read => &[Subject::Read],
        Purpose::Effect => &[Subject::Edit, Subject::Read],
    };
    subjects.iter().find_map(|subject| {
        // Which rule decided is routing, and so is which list it was in. The reason itself is
        // never read here: it is carried into the refusal as it was written.
        match policy.permissions().rule_for_path(*subject, name) {
            Some((Ruling::Deny, rule)) => rule.reason().map(str::to_string),
            _ => None,
        }
    })
}

/// What the planner is told when a rule refused. Says the rule is the reason and that retrying is
/// not the answer, because a planner told only "refused" tries the same call again.
///
/// `reason` is what the person wrote beside the rule in their own settings file, and is quoted as
/// theirs because it is theirs: no other layer's reason reaches this (PERM-7). It says why the file
/// is off limits, which is what tells the planner whether to work without it or ask about it.
fn denied_by_rule(shown: &str, reason: Option<&str>) -> String {
    let because = reason
        .map(|reason| format!(" The user's rule gives a reason: {reason}"))
        .unwrap_or_default();
    format!(
        "refused: a deny rule in the user's settings covers {shown}, so nothing here can \
         reach it.{because} Do not retry, and do not look for another way to the same file: work \
         without it, or say in your reply what you needed it for."
    )
}

/// What the planner is told when the definition it runs under limits the files it may write
/// (DELEGATE-28). Says the limit is the reason, as [`denied_by_rule`] does, since a planner told
/// only "refused" tries the same call again.
fn outside_write_limit(shown: &str) -> String {
    format!(
        "refused: the definition you run under limits the files you may write, and {shown} is \
         outside them. Do not retry, and do not look for another way to write it: write only \
         what is inside the limit, or say in your reply what is left to be written."
    )
}

/// What a call is going to do with the path it asked for.
///
/// The two take different routes out of a reference, and neither is reachable by asking for the
/// other: a read is promoted, and an effect is not promoted at all but endorsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Read,
    Effect,
}

/// A path a call is about, and what the planner may be told about it.
///
/// `shown` is the difference. A path the planner typed is its own words coming back, and a path
/// out of a reference is a name it has never seen: saying it in a result would hand over the
/// thing the reference exists to keep, so what goes back is the reference's own name. The person
/// watching is told the real path either way, on the line under it and in the approval.
struct PathArgument {
    path: Labelled<String>,
    destination: Destination,
    shown: String,
    /// The same path as plain text, released once by whichever gate this came out of.
    ///
    /// Carried rather than read again by each caller. Reading it twice would put two identical
    /// lines in the trail for one path and read as the driver having looked at it twice, and for
    /// a reference it is not even the same string: `shown` is the reference's name.
    released: String,
}

/// The fact that `read_file` read a file under another spelling of the name it was given, in the
/// kind of space in the last component (READ-11). It carries no name: the turn's sentence about it
/// is fixed words, so no filename is in a sentence the driver signs for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Respelled;

/// Put the file a reference names into a line a person is about to read.
///
/// Literal matching, not a pattern: the names are `ref:0`, `ref:1` and so on, the driver handed
/// them out itself, and a regular expression over text a model wrote is attack surface for no
/// gain. A name that has no file behind it, which is anything a processor produced, is left as
/// the model wrote it.
pub(crate) fn name_references(text: &str, named: &[(SlotId, Label, String)]) -> String {
    put_references_through(text, named, |slot, label, path| {
        format!("{slot}{label}:{path}")
    })
}

/// As [`name_references`], saying only the file: for a person who has never been told what a
/// reference is, so a name beside the path is a second thing to decode.
fn name_files(text: &str, named: &[(SlotId, Label, String)]) -> String {
    put_references_through(text, named, |_, _, path| path.to_string())
}

fn put_references_through(
    text: &str,
    named: &[(SlotId, Label, String)],
    say: impl Fn(&SlotId, &Label, &str) -> String,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find("ref:") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "ref:".len()..];
        let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
        let name = format!("ref:{}", &after[..digits]);

        // A name with no file behind it, which is anything a processor produced, stays as the
        // planner wrote it: there is nothing truer to put in its place.
        // The reference, its label, and the file: all three, because the planner has only the
        // first of them. A bare filename on a line about a call the planner made reads as
        // though it knew the name, and the whole arrangement is that it does not.
        match named.iter().find(|(slot, _, _)| slot.as_str() == name) {
            Some((slot, label, path)) if digits > 0 => out.push_str(&say(slot, label, path)),
            _ => out.push_str(&name),
        }
        rest = &after[digits..];
    }

    out.push_str(rest);
    out
}

/// Render a page, saying what was left out.
///
/// The counts matter more than they look: a model handed a silent window of a large file
/// will answer as though it read the whole thing.
fn render_page(page: &Page, token: ChangeToken) -> String {
    let mut notes = Vec::new();

    if !page.lines.is_empty() && (page.first_line > 1 || page.next_line().is_some()) {
        notes.push(format!(
            "showing lines {}-{} of {}",
            page.first_line,
            page.first_line + page.lines.len() - 1,
            page.total_lines
        ));
    }
    if let Some(next) = page.next_line() {
        notes.push(format!("continue with offset {next}"));
    }
    if page.long_lines > 0 {
        notes.push(format!("{} long line(s) were shortened", page.long_lines));
    }
    // A reading of the file and not its bytes, said so the planner does not take an edit of it to
    // be possible: a read that is written back is of the file's own text only (READ-3).
    if let Some(how) = decoding_note(page.decoding) {
        notes.push(format!("{how}, so edit_file cannot change this file"));
    }
    if token == ChangeToken::Shown {
        notes.push(format!("change token {}", page.change_token));
    }

    // An empty file and an offset past the end have no body, and the token belongs on them most of
    // all: "tell me when the log appears" is asked of a file with nothing in it yet, and a read
    // that answered it with nothing to compare would leave the next look nothing to compare against.
    let body = match (page.lines.is_empty(), page.total_lines) {
        (true, 0) => "(the file is empty)".to_string(),
        (true, total) => format!("(no lines at that offset; the file has {total} lines)"),
        (false, _) => page.lines.join("\n"),
    };

    if notes.is_empty() {
        body
    } else {
        format!("{body}\n\n({})", notes.join("; "))
    }
}

/// How a page that is not the file's own text was read, in the words a note about it uses.
fn decoding_note(decoding: bravebot_filetype::Decoding) -> Option<&'static str> {
    use bravebot_filetype::Decoding;
    match decoding {
        Decoding::Utf8 => None,
        Decoding::Utf8Replaced => Some("bytes that are not valid UTF-8 are shown as U+FFFD"),
        Decoding::Utf16Le => Some("decoded from UTF-16 little-endian"),
        Decoding::Utf16Be => Some("decoded from UTF-16 big-endian"),
        Decoding::Utf32Le => Some("decoded from UTF-32 little-endian"),
        Decoding::Utf32Be => Some("decoded from UTF-32 big-endian"),
    }
}

/// Whether a rendered page carries the file's change token.
///
/// A read hands it over, which is what makes a question about change answerable at all: the planner
/// keeps it and compares it with the token from the next look. A slot fill does not, because what a
/// slot holds is the file's text for a processor to work on or a write to put back, and a note about
/// the file is not part of the file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangeToken {
    Shown,
    Withheld,
}

fn list_files<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    arguments: &Value,
) -> Produced {
    let proposed = argument(arguments, "directory").unwrap_or_else(|| {
        Labelled::new(
            ".".to_string(),
            bravebot_core::label::Label::untrusted_public(),
        )
    });

    let directory = match policy.promote_confined_read("list_files", "directory", &proposed) {
        Ok(d) => d,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // The directory a listing would walk. A rule that keeps a tree from being read keeps it from
    // being enumerated too: the names in a directory are what is in it.
    let proposed_dir = match policy.read_planner_argument("list_files", "directory", &proposed) {
        Ok(directory) => directory,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    if let Err(refusal) = refuse_denied_path(policy, workspace, Purpose::Read, &proposed_dir) {
        return Produced::problem(refusal);
    }
    let proposed_dir = workspace.climbed_to(&proposed_dir).unwrap_or(proposed_dir);

    // A filter only narrows a confined, non-destructive read, so it is promotable on the
    // same terms as the directory itself.
    let pattern = match argument(arguments, "pattern") {
        Some(proposed) => match policy.promote_confined_read("list_files", "pattern", &proposed) {
            Ok(p) => Some(p),
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
        None => None,
    };

    // Read as a plain number, like the offset and limit on a read: it narrows a confined read
    // and cannot name anything, so there is no destination for it to decide.
    let depth = arguments
        .get("depth")
        .and_then(Value::as_u64)
        .map(|depth| depth.max(1).min(usize::MAX as u64) as usize);

    match workspace.list(policy, &directory, pattern.as_ref(), depth) {
        Ok(listing) => {
            let note = note_for(policy, "list_files", &listing, |listing| {
                tally(
                    listing.files.len() + listing.directories.len() + listing.partly_listed.len(),
                    "entry",
                    "entries",
                )
            });

            // Asked of the pattern alone, which the planner typed and the promote gate left
            // trusted, as `search` asks it of an `include`: it reads no content of the tree.
            let bad_glob = pattern.as_ref().and_then(|pattern| {
                pattern
                    .clone()
                    .into_trusted()
                    .ok()
                    .and_then(|g| reads_as_an_unsupported_glob(&g))
            });
            let had_pattern = pattern.is_some();
            let unmatched_note = unmatched_pattern_note(bad_glob);

            // A listing the planner may not read is handed over one reference per entry rather
            // than as one document it can do nothing with. The names stay wrapped the whole way:
            // this reshapes the listing into a list of them inside the kernel and carries it
            // out, and the kernel is what turns each into a slot.
            //
            // The label decides, not the contents: whether the planner may see these names is
            // the same question `present` would ask a moment later.
            // Shape, not content, and released for the same reason as the count below: a planner
            // handed exactly the cap with nothing said about it reads a sample as the whole tree.
            let (truncated, unreadable, links, timed_out) = {
                let shaped = policy.render_in_place("list_files", &listing, |listing| {
                    (
                        listing.truncated,
                        listing.unreadable,
                        listing.links,
                        listing.timed_out,
                    )
                });
                let proof = policy.authorise_display_release(
                    "whether a listing hit its cap, ran out of time, left out a directory it \
                     could not open, or did not enter symbolic links, and how many",
                );
                shaped.declassify(&proof)
            };
            // A listing missing a directory is as incomplete as a capped one, and the planner
            // reads it the same way. `incomplete` is the caps and the unreadable directory alone:
            // the clock is its own fact, `timed_out`, with its own sentence, so a listing that a
            // cap and the clock both cut says both and one that only the clock cut says no cap.
            let incomplete = truncated || unreadable;

            if !listing.label().is_trusted() {
                // The three numbers out of one release: how many entries there are, how many of
                // them are the directories a bounded walk stopped at, and how many are files.
                // The second is what keeps the quarantined listing describing the same tree the
                // trusted one does, where a directory is rendered with a trailing slash among the
                // files. The third says whether a pattern selected anything.
                // The fourth is how many files and how many subdirectories each directory a
                // per-directory cap cut left out, counts of directory entries like the others.
                let (count, directories, files, left_out, at_the_cap) = {
                    let shaped = policy.render_in_place("list_files", &listing, |listing| {
                        (
                            listing.files.len()
                                + listing.directories.len()
                                + listing.partly_listed.len(),
                            listing.directories.len() + listing.partly_listed.len(),
                            listing.files.len(),
                            listing
                                .partly_listed
                                .iter()
                                .map(|(_, files, directories)| (*files, *directories))
                                .collect::<Vec<(usize, usize)>>(),
                            listing.at_the_cap,
                        )
                    });
                    let proof = policy.authorise_display_release(
                        "how many entries a listing has, how many of them it stopped at, how \
                         many are files, how many files and subdirectories each directory it cut \
                         left out and whether it reached the cap over the whole walk",
                    );
                    shaped.declassify(&proof)
                };
                // The directories last, which is the order the kernel reads the two apart by. A
                // directory a per-directory cap cut is a directory reference like one the walk
                // stopped at, so it is named the only way a quarantined listing names anything.
                // Those come after the ones the walk stopped at, so which references they are is
                // decided by the counts alone and `left_out` lines up with the last of them.
                let paths = policy.render_in_place("list_files", &listing, |listing| {
                    let mut paths = listing.files.clone();
                    paths.extend(listing.directories.iter().cloned());
                    paths.extend(
                        listing
                            .partly_listed
                            .iter()
                            .map(|(name, _, _)| name.clone()),
                    );
                    paths
                });
                // A call that asked to read gets the listing as one document too, under the
                // listing's label, for the turn to release in this result where the mode or a
                // check answers vet_content (VET-6). The planner's argument decides, and the turn
                // hands the planner the entries instead wherever the release is not made.
                let text = if read_asked_of(arguments).unwrap_or(false) {
                    render_listing(policy, &listing, had_pattern, &unmatched_note)
                } else {
                    Labelled::trusted(String::new())
                };
                return Produced::new(text, proposed_dir.clone(), note)
                    .of_content()
                    .capped(incomplete)
                    .timed_out(timed_out)
                    .linked(links)
                    // A listing that ran out of time did not look at the whole tree, so its empty
                    // selection is not the pattern's doing and the time sentence says so instead.
                    .unmatched((had_pattern && files == 0 && !timed_out).then_some(unmatched_note))
                    .with_entries(Entries {
                        origin: format!("an entry in \"{proposed_dir}\""),
                        paths,
                        count,
                        directories,
                        left_out,
                        at_the_cap,
                    });
            }

            let rendered = render_listing(policy, &listing, had_pattern, &unmatched_note);
            // The entries alone: "(no files)" and the notices after a listing are the driver's
            // words and are not glimpsed (VIEW-24).
            let glimpsed = policy.render_in_place("list_files", &listing, |listing| {
                let mut entries: Vec<String> = listing.files.clone();
                entries.extend(listing.directories.iter().map(|name| format!("{name}/")));
                entries.extend(
                    listing
                        .partly_listed
                        .iter()
                        .map(|(name, _, _)| format!("{name}/")),
                );
                entries.sort();
                entries.join("\n")
            });
            Produced {
                glimpsed: Some(glimpsed),
                ..Produced::new(rendered, proposed_dir, note)
                    .of_content()
                    .capped(incomplete)
                    .timed_out(timed_out)
                    .linked(links)
            }
        }
        // Worded about the name typed. The error carries where it landed, which a link can choose.
        Err(e) => file_tool_problem(policy, "list_files", "directory", &e, &proposed_dir),
    }
}

/// What a directory a per-directory cap cut left out, worded for the end of its line in a listing:
/// its files, its subdirectories, or both. A count of directory entries and no name. `more` puts
/// "more" before the noun, as a listing's own line does, and leaves it out of the sentence a
/// quarantined listing carries beside its references.
pub(crate) fn left_out_clause(files: usize, directories: usize, more: bool) -> String {
    let (file, many_files, directory, many_directories) = if more {
        (
            "more file",
            "more files",
            "more subdirectory",
            "more subdirectories",
        )
    } else {
        ("file", "files", "subdirectory", "subdirectories")
    };
    let files = (files > 0).then(|| tally(files, file, many_files));
    let directories = (directories > 0).then(|| tally(directories, directory, many_directories));
    [files, directories]
        .into_iter()
        .flatten()
        .collect::<Vec<String>>()
        .join(" and ")
}

/// A listing as the planner reads it where it may: one entry a line, a directory with a trailing
/// slash, and the driver's sentences about a cap, a pattern that matched nothing, a directory that
/// could not be opened and links not entered. Rendered inside the kernel under the listing's label.
fn render_listing<S: Sink>(
    policy: &mut Policy<'_, S>,
    listing: &Labelled<Listing>,
    had_pattern: bool,
    unmatched_note: &str,
) -> Labelled<String> {
    policy.render_in_place("list_files", listing, |listing| {
        // Before the directories are folded in among the files below: a pattern selects
        // files, and a bounded walk lists its directories whatever the pattern says.
        let selected_no_file = listing.files.is_empty();
        let mut entries: Vec<String> = listing.files.clone();
        entries.extend(listing.directories.iter().map(|name| format!("{name}/")));
        // A directory a per-directory cap cut, with how many of its files and subdirectories are
        // not shown, so the planner knows the listing of it is incomplete (LIST-5).
        entries.extend(
            listing
                .partly_listed
                .iter()
                .map(|(name, files, directories)| {
                    format!(
                        "{name}/ ({} not listed)",
                        left_out_clause(*files, *directories, true)
                    )
                }),
        );
        entries.sort();
        let cut_per_directory = !listing.partly_listed.is_empty();
        let files_cut = listing.partly_listed.iter().any(|(_, files, _)| *files > 0);
        let directories_cut = listing
            .partly_listed
            .iter()
            .any(|(_, _, directories)| *directories > 0);
        let listing = Listing {
            files: entries,
            directories: Vec::new(),
            truncated: listing.truncated,
            at_the_cap: listing.at_the_cap,
            unreadable: listing.unreadable,
            links: listing.links,
            partly_listed: Vec::new(),
            timed_out: listing.timed_out,
        };
        // An empty walk that ran out of time says so in place of `(no files)`, which would read as
        // an empty directory. Where links were met the sentence for them already says the directory
        // is not known to be empty, and the time sentence follows it.
        let empty_for_want_of_time =
            listing.files.is_empty() && listing.timed_out && listing.links == 0;
        let mut body = if empty_for_want_of_time {
            format!(
                "(no files were found before this listing ran out of time, so this does not say \
                 the directory is empty; {LISTING_NARROWING})"
            )
        } else if listing.files.is_empty() && listing.links > 0 {
            // An empty listing beside links is not an empty directory: a skills
            // directory made only of links read as one, and the model went looking
            // elsewhere for files that were one step away.
            format!(
                "(no files; {} not entered, so this does not say the directory is \
                         empty)",
                tally(listing.links, "symbolic link was", "symbolic links were")
            )
        } else if listing.files.is_empty() {
            "(no files)".to_string()
        } else if listing.truncated {
            // Said plainly, because a model given a silently capped listing will treat
            // it as the whole tree and conclude a file does not exist. Each cap that left
            // files or subdirectories out has its own sentence, as `listing_capped_note`
            // does for a quarantined listing: "stopped at N files" misstates a
            // per-directory cut, and the per-directory sentences alone hide that the walk
            // stopped as well.
            let mut body = listing.files.join("\n");
            if listing.at_the_cap || !cut_per_directory {
                body.push_str(&format!(
                    "\n\n(this listing stopped at {} files and is incomplete; \
                     list a subdirectory to see more)",
                    listing.files.len()
                ));
            }
            if files_cut {
                // A listing of that directory with a depth is cut the same way, so the
                // rest is reached with none.
                body.push_str(&format!(
                    "\n\n(this listing took at most {} files from any one directory and \
                     is incomplete; to see the rest of a directory marked \"not \
                     listed\", list that directory by its name with no depth)",
                    crate::workspace::MAX_FILES_PER_DIRECTORY
                ));
            }
            if directories_cut {
                // A listing with no depth walks into every subdirectory and names files only, so
                // it shows what is under the subdirectories left out and none of them by name.
                // No other tool is named: this sentence is written without the turn's list of
                // tools (TOOL-6).
                body.push_str(&format!(
                    "\n\n(this listing took at most {} subdirectories from any one directory \
                     and is incomplete; for a directory marked \"not listed\", list it by its \
                     name with no depth to see the files under all of its subdirectories, \
                     since a listing with no depth names files only)",
                    crate::workspace::MAX_SUBDIRECTORIES_PER_DIRECTORY
                ));
            }
            body
        } else {
            listing.files.join("\n")
        };
        if listing.timed_out && !empty_for_want_of_time {
            body.push_str(&format!(
                "\n\n(this listing ran out of time and is incomplete; {LISTING_NARROWING})"
            ));
        }
        // Not said of a walk that ran out of time: it did not look at the whole tree, so the
        // pattern selecting nothing is not what the empty answer is about.
        if had_pattern && selected_no_file && !listing.timed_out {
            body.push_str(&format!("\n\n{unmatched_note}"));
        }
        if listing.unreadable {
            // Said without a name: the directory's own name is a filename out of the
            // tree, and this sentence is the driver's to word (LIST-2).
            body.push_str(
                "\n\n(part of this tree could not be read and is not in this \
                         listing, so it says nothing about what is there)",
            );
        }
        if listing.links > 0 && !listing.files.is_empty() {
            // A number and no names, like the sentence above: a link's name is a
            // filename out of the tree (LIST-2).
            body.push_str(&format!(
                "\n\n({} not entered and {} not in this listing)",
                tally(listing.links, "symbolic link was", "symbolic links were"),
                if listing.links == 1 { "is" } else { "are" }
            ));
        }
        body
    })
}

/// `repo_map`: the declarations of the source files beneath a directory that the trust map
/// vouches for, ranked, as a result the planner may read.
fn repo_map<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    arguments: &Value,
) -> Produced {
    let proposed = argument(arguments, "directory").unwrap_or_else(|| {
        Labelled::new(
            ".".to_string(),
            bravebot_core::label::Label::untrusted_public(),
        )
    });
    let directory = match policy.promote_confined_read("repo_map", "directory", &proposed) {
        Ok(d) => d,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let proposed_dir = match policy.read_planner_argument("repo_map", "directory", &proposed) {
        Ok(directory) => directory,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    // Mapping a tree is reading it, so a rule that keeps it from being read keeps it from being
    // mapped.
    if let Err(refusal) = refuse_denied_path(policy, workspace, Purpose::Read, &proposed_dir) {
        return Produced::problem(refusal);
    }
    // A plain number like a depth or a limit: it sizes a confined read and names nothing.
    let budget = arguments
        .get("budget")
        .and_then(Value::as_u64)
        .map_or(crate::repo_map::DEFAULT_BUDGET, |budget| {
            budget.min(usize::MAX as u64) as usize
        });

    match workspace.repo_map(policy, &directory, budget) {
        Ok(map) => {
            let note = note_for(policy, "repo_map", &map, |map| {
                format!(
                    "{}, {} shown",
                    tally(map.found, "declaration", "declarations"),
                    map.shown
                )
            });
            let incomplete = {
                let shaped = policy.render_in_place("repo_map", &map, |map| {
                    map.truncated || map.unreadable || map.shown < map.found
                });
                let proof = policy.authorise_display_release(
                    "whether a repository map left declarations or files out",
                );
                shaped.declassify(&proof)
            };
            let rendered = policy.render_in_place("repo_map", &map, |map| map_as_told(&map));
            let glimpsed = policy.render_in_place("repo_map", &map, |map| map.body);
            Produced {
                glimpsed: Some(glimpsed),
                ..Produced::new(rendered, proposed_dir, note)
                    .of_content()
                    .capped(incomplete)
            }
        }
        Err(e) => file_tool_problem(policy, "repo_map", "directory", &e, &proposed_dir),
    }
}

/// A map with what it left out said after it, in the driver's words.
///
/// Counts only: a file left out is never named, and the three reasons a file is skipped for
/// are one count so a finding is not told to the planner.
fn map_as_told(map: &crate::workspace::RepoMap) -> String {
    let mut told = if map.found == 0 {
        "(no declarations found in the source files you may read)".to_string()
    } else {
        map.body.clone()
    };
    let mut notes = Vec::new();
    if map.shown < map.found {
        notes.push(format!(
            "this map shows {} of {} declarations, most referenced first: map a subdirectory, \
             or raise budget, to see others",
            map.shown, map.found
        ));
    }
    if map.truncated {
        notes.push(
            "the walk stopped at a cap on files or bytes, so this map is incomplete: map a \
             subdirectory"
                .to_string(),
        );
    }
    if map.unreadable {
        notes.push(
            "part of this tree could not be read and is not in this map, so it says nothing \
             about what is there"
                .to_string(),
        );
    }
    if map.unvouched > 0 {
        notes.push(format!(
            "{} left out because nobody vouched for {}",
            tally(map.unvouched, "source file was", "source files were"),
            if map.unvouched == 1 { "it" } else { "them" }
        ));
    }
    if map.skipped > 0 {
        notes.push(format!(
            "{} left out as unreadable, too large or not safe to show",
            tally(map.skipped, "source file was", "source files were")
        ));
    }
    if !notes.is_empty() {
        told.push_str("\n\n(");
        told.push_str(&notes.join("; "));
        told.push(')');
    }
    told
}

/// How many lines of a processor's remark are drawn beside the diff it describes.
///
/// Fewer than the transcript keeps, because this is the box a decision is read in: a processor
/// that answers with a screenful of prose would otherwise push the lines an approval is given
/// from out of view, which is the thing showing the remark here is meant to prevent. The fuller
/// preview is in the transcript above, and the block says how many lines it is not showing.
pub(crate) const REMARK_LINES: usize = 4;

/// The width a line of it is trimmed to.
///
/// Far narrower than the transcript's cap, because the box is narrower and a line wider than the
/// box is several rows rather than one: four lines of the transcript's hundred and sixty
/// characters is a dozen rows in a prompt, which is the diff below the fold. A remark is two or
/// three sentences of prose rather than a minified file, so a sentence's width loses nothing that
/// the transcript above is not still holding in full.
pub(crate) const REMARK_WIDTH: usize = 72;

/// Resolve a reference into the bytes a write will carry.
///
/// Three steps, each one a gate. The name is accepted as a reference rather than read as
/// content; the kernel resolves it, refusing a name that points at nothing; and what comes back
/// is released for a write that stays inside the workspace. The bytes are wrapped throughout, so
/// nothing here can look at what it is about to write.
fn quarantined_body<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    slots: &mut SlotStore,
    named: &Labelled<String>,
    path: &str,
    destination: Destination,
) -> Result<Quarantined, String> {
    let slot = accept_named_reference(
        policy,
        slots,
        "write_file",
        "contents_ref",
        named,
        NamesToList::Text,
    )?;

    // The bytes are needed now, so a slot still holding only a path reads its file here.
    materialise(
        policy,
        workspace,
        slots,
        "write_file",
        std::slice::from_ref(&slot),
    )?;

    // Where an answer belongs, decided when the processor was asked and not now.
    policy
        .write_belongs_here(path, &write_site(workspace, path), &slot, slots)
        .map_err(|denial| format!("refused: {denial}"))?;

    // A picture is written as the file it encodes, and only under the name its type has (WRITE-9).
    // Settled from the driver's record of what the slot is and from the path the planner named,
    // before the bytes leave the slot, so a refusal releases nothing.
    let picture = slots.picture_of(&slot).map(str::to_string);
    if let Some(media) = &picture {
        a_picture_may_be_written_here(&slot, media, destination, path)?;
    }

    // Asked before the bytes are taken, and answered from where the slot came from rather than
    // from what it holds.
    let changes = policy.write_would_change(path, &slot, slots);

    let content = policy
        .resolve("write_file", &slot, slots)
        .map_err(|denial| format!("refused: {denial}"))?;

    // What the processor that produced these bytes said about them, for the question that is
    // about to be asked about the bytes. Asked of the slot, so it is the claim made about this
    // document and not whatever was said last.
    let remark = policy
        .remark_for_review(&slot, slots, REMARK_LINES, REMARK_WIDTH)
        .map(|(preview, lines, label)| Remark {
            preview,
            lines,
            label: label.to_string(),
        });

    Ok(Quarantined {
        body: policy.declassify_into_workspace(&slot, path, content),
        changes,
        remark,
        picture,
    })
}

/// What a `contents_ref` came to: the bytes a write will carry, still labelled, and what is known
/// of them without reading them.
struct Quarantined {
    body: Labelled<String>,
    /// `false` only where the kernel filled the slot from the very file it is written to.
    changes: bool,
    remark: Option<Remark>,
    /// The media type of the picture or PDF the slot holds, where it holds one. The body is then
    /// the data URI the slot is kept as, and the file is the bytes it encodes (WRITE-9).
    picture: Option<String>,
}

/// Whether the picture or PDF `slot` holds, of this `media` type, may be written to `path`.
///
/// Compared with the table a read decides a file is a picture by, which is the driver's own and
/// holds each media type once for each extension it is written with. Only a path the planner named
/// is compared: the name `path_ref` stands for came out of a listing nobody vouched for, so what
/// its extension is would be a fact about untrusted bytes, and a write to such a path is refused
/// instead.
fn a_picture_may_be_written_here(
    slot: &SlotId,
    media: &str,
    destination: Destination,
    path: &str,
) -> Result<(), String> {
    if destination == Destination::Reference {
        return Err(format!(
            "refused: {slot} holds a {media} file, which is written to a path you name with \
             'path'. A name you were never shown has an extension nothing here may read, so it \
             cannot be checked against the file's type. Nothing was written."
        ));
    }
    if bravebot_filetype::by_name::media_for(path) == Some(media) {
        return Ok(());
    }
    let extensions: Vec<String> = bravebot_filetype::by_name::ATTACHABLE
        .iter()
        .filter(|(_, named)| *named == media)
        .map(|(extension, _)| format!(".{extension}"))
        .collect();
    Err(format!(
        "refused: {slot} holds a {media} file, which is written to a path ending in {}, and {path} \
         does not. Nothing was written.",
        extensions.join(" or ")
    ))
}

/// The refusal for a write by a run that holds no `file_write` capability, such as a reader or a
/// checker delegate. Asked before the path is read, scanned or put to anybody, so a person is never
/// asked about a write that cannot land. `Ok` where the run may write.
fn refuse_unless_writing_is_held<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
) -> Result<(), &'static str> {
    if policy.holds_capability(bravebot_core::capability::Capability::FileWrite) {
        return Ok(());
    }
    policy.refuse_write_without_capability(tool);
    Err(
        "refused: this kind of delegate writes nothing, so no file was changed and nobody was \
         asked. Do not retry; put the content in your report to the session that started you.",
    )
}

/// Write a file, after a person approves it.
///
/// The order matters: the user sees the exact path and body *before* any grant exists, and
/// the grant is issued only for what they saw. Issuing it earlier would mean approving a
/// value that could still change.
fn write_file<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let landing = match write_landing(policy, tools, arguments) {
        Ok(landing) => landing,
        Err(refusal) => return Produced::problem(refusal),
    };
    // Resolved as the write resolves it, before the body is released into the path or anybody is
    // asked: no answer makes a destination the workspace does not reach possible. The write
    // resolves it again once the person has answered, since a directory on the way can have become
    // a link while they read the question (TRUST-10).
    if let Err(unreachable) = tools.workspace.resolve(&landing.proposed_path) {
        return write_refused(
            policy,
            tools,
            "write_file",
            landing.destination,
            &unreachable,
            &landing.shown_path,
        );
    }

    let written = argument(arguments, "contents");
    let named = named_argument(arguments, "contents_ref");

    // Two sources would leave the driver deciding which one was meant, and they say different
    // things about what lands in the file. Neither is a decision taken from content: both
    // arguments are the planner's, and this only reports which of them are present.
    // A write of a document the kernel filled from this very file puts it back exactly as it
    // is. Set below, from the slot's provenance, never from comparing what it holds.
    let mut changes_anything = true;

    // What a processor said about the body, where a processor produced it. Drawn beside the diff
    // in the question below, and nothing else reads it.
    let mut remark = None;

    // Whether the body is the planner's own text, which is what WRITE-6 may reformat. Set from
    // which argument was given, never from anything either body holds.
    let mut planner_written = true;

    // What the planner called the body, for the account it is given afterwards. Its own words
    // either way: the reference it named, or its own text.
    let body_from = match &named {
        Some(reference) => {
            let proof = policy.authorise_display_release("which reference a write carried");
            reference.clone().declassify(&proof)
        }
        None => "the contents you gave".to_string(),
    };

    let body = match (written, named) {
        (Some(_), Some(_)) => {
            return Produced::problem(
                "error: give 'contents' or 'contents_ref', not both. Use contents_ref alone \
                 when the file is to hold quarantined content.",
            );
        }
        (None, None) => {
            return Produced::problem("error: one of 'contents' or 'contents_ref' is required");
        }
        // The body is the model's words. Its integrity is that of the context the model was
        // working from, which the kernel tracked: nothing here upgrades anything.
        (Some(contents), None) => match policy.adopt_model_output("write_file", contents) {
            Ok(body) => {
                policy.declassify_written_into_workspace("write_file", &landing.shown_path, body)
            }
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
        // Quarantined content, going where the planner said without the planner or the driver
        // having read a byte of it. The user still sees it, which is what an approval is.
        (None, Some(reference)) => {
            match quarantined_body(
                policy,
                tools.workspace,
                tools.slots,
                &reference,
                &landing.proposed_path,
                landing.destination,
            ) {
                Ok(Quarantined {
                    body,
                    picture: Some(media),
                    ..
                }) => {
                    return write_a_picture(
                        policy,
                        tools,
                        confirmer,
                        landing,
                        PictureBody {
                            body,
                            media,
                            body_from,
                        },
                    );
                }
                Ok(Quarantined {
                    body,
                    changes,
                    remark: said,
                    picture: None,
                }) => {
                    changes_anything = changes;
                    remark = said;
                    planner_written = false;
                    body
                }
                Err(refusal) => return Produced::problem(refusal),
            }
        }
    };
    put_in_the_workspace(
        policy,
        tools,
        confirmer,
        "write_file",
        landing,
        Body {
            body,
            check_with_server: true,
            changes_anything,
            remark,
            body_from,
            written_since_checkout: false,
            planner_written,
        },
        false,
    )
}

/// Where a `write_file` call lands, as [`path_argument`] settled it, or the refusal of a run that
/// holds no writing, which is asked before the path is read.
///
/// The path is routing, so naming a destination from it is not a content decision, and the gate
/// that released it ran where the argument was read.
fn write_landing<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    arguments: &Value,
) -> Result<Landing, String> {
    refuse_unless_writing_is_held(policy, "write_file").map_err(String::from)?;
    let found = path_argument(
        policy,
        tools.workspace,
        "write_file",
        Purpose::Effect,
        tools.slots,
        arguments,
    )?;
    Ok(Landing {
        path: found.path,
        destination: found.destination,
        shown_path: found.shown,
        proposed_path: found.released,
    })
}

/// Where a write lands, as [`path_argument`] settled it.
struct Landing {
    path: Labelled<String>,
    destination: Destination,
    shown_path: String,
    proposed_path: String,
}

/// What a write carries, and what is known about it without reading it.
struct Body {
    body: Labelled<String>,
    /// Whether a language server that is already running is asked about the file once it is
    /// written (LSP-12). Only `write_file` says so.
    check_with_server: bool,
    /// `false` only where the kernel filled the slot from the very file it is written to.
    changes_anything: bool,
    remark: Option<Remark>,
    /// What the planner called the body, for the account it is given afterwards.
    body_from: String,
    /// Whether the driver's record holds a write to this path in the working directory since the
    /// checkout the body comes from was made, which the question says (CHECKOUT-14).
    written_since_checkout: bool,
    /// Whether the body is the planner's own `contents`, and so may be written in the line
    /// endings and byte order mark of the file it replaces (WRITE-6). A body that came from
    /// anywhere else is written as it is.
    planner_written: bool,
}

/// Whether a write prompt may offer the person their editor on the body (WRITE-7): only the
/// planner's own text, written in a context that is trusted, and already released into the
/// workspace, which is what makes it public.
///
/// Decided from which argument supplied the body and from the body's label, never from what it
/// says. A body out of a quarantined reference is the case the edit is not offered for, since the
/// text a person saved from it would still be made of bytes nobody read, and the body of a planner
/// that has met untrusted content is the same case one step removed.
fn offers_an_edit(planner_written: bool, label: bravebot_core::label::Label) -> bool {
    planner_written && label.is_trusted() && label.is_public()
}

/// Put `given` in the file `landing` names: the scan, the question, the grant and the write, which
/// are the same whatever the body came from.
///
/// `always_ask` puts the write to a person even where the trust map's table would ask nothing
/// (CHECKOUT-14). `call` is the tool the write is made for, which names it where the person does
/// not approve it (TRACE-1).
fn put_in_the_workspace<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    call: &str,
    landing: Landing,
    given: Body,
    always_ask: bool,
) -> Produced {
    let workspace = tools.workspace;
    let Landing {
        path,
        destination,
        shown_path,
        proposed_path,
    } = landing;
    let Body {
        body,
        check_with_server,
        changes_anything,
        remark,
        body_from,
        written_since_checkout,
        planner_written,
    } = given;

    // What the file holds now, carried rather than read: the bytes of a file nobody vouched for
    // are untrusted content, and a driver holding them as a bare `String` is what LABEL-4 stops.
    // Whether there is a file here at all is a separate question, answered from the path and
    // `stat`, because the labelled peek cannot say: it reports a file it could not decode as
    // text the same way it reports one that is not there.
    //
    // Taken shared: the capture reads a file, which can be large, and enters no effect, so it
    // runs beside other readers and still waits for an effect (TRUST-4). The revision is read
    // inside the same boundary, so it and the bytes are of one state of the map. The stamp is the
    // revision and the count of `run` lines started, so a program that started while the question
    // was open voids the yes even if it touched no path the authority was told of (TRUST-4).
    let (existing, pre_image, other_encoding, replaces, approved_at) =
        policy.capture_files_shared(|policy| {
            let key = workspace.trust_key(&proposed_path);
            let peeks = workspace.peek_labelled_for_write(policy, &proposed_path);
            (
                peeks.0,
                peeks.1,
                peeks.2,
                workspace.names_a_file(&proposed_path),
                policy.file_authority().stamp_of(&key),
            )
        });
    // A file the trust map vouches for that is text in an encoding other than UTF-8 is not replaced
    // by a body that lands as UTF-8 (WRITE-8). Refused before the format step, the scan and the
    // question, since no answer changes what the file is. A file nobody vouched for is not asked
    // about: its encoding is a fact about untrusted bytes, and the write replaces it as WRITE-1
    // says.
    if replaces && crate::workspace::holds_another_encoding(policy, call, &other_encoding) {
        return write_refused(
            policy,
            tools,
            call,
            destination,
            &crate::workspace::WorkspaceError::NotUtf8 {
                path: shown_path.clone(),
            },
            &shown_path,
        );
    }
    // The same bytes, so the comparison below can be made inside the kernel. Taken from the one
    // peek rather than read a second time: two reads of a file somebody else may be writing can
    // disagree, and then the diff a person approves is of a version that never existed.
    let replaced = crate::workspace::peeked_for_review(&existing, replaces);
    // The planner's text in the format of the file it replaces, settled here so that the scan,
    // the diff a person approves and the bytes that land are all the same text (WRITE-6). The
    // pre-image used is the one labelled as the trust map holds the path, not the pessimistic
    // copy kept for the review: that one is untrusted whatever the map says, so converting on it
    // would convert nothing anywhere. A file nobody vouched for still converts nothing, and the
    // driver never sees what ending it used.
    let mut body = if replaces && planner_written {
        crate::workspace::in_the_replaced_files_format(policy, "write_file", &pre_image, body)
    } else {
        body
    };
    // Read before the write, since afterwards the age is the age of this write.
    let replaced_age = workspace.age_of(&proposed_path);
    let intent = Intent::of_a_write(replaces);

    // Nothing to do, and nothing to ask about. The slot holds what this very file was read as, so
    // writing it puts the file back exactly as it is: a diff with nothing in it, put to a person
    // once per file that turned out not to need changing. Approvals that say nothing are how the
    // ones that say something get waved through.
    //
    // The planner is told what it would have been told anyway. Which file this is says nothing
    // about anybody's contents, and those do not go into its context.
    if !changes_anything {
        return confirmed(
            format!("{shown_path} holds what {body_from} holds. Nothing further to do for it."),
            "unchanged, nothing written",
        );
    }

    // Whether the prompt may offer the person their editor on this body (WRITE-7).
    let may_edit = offers_an_edit(planner_written, body.label());
    // Whether the body now is text the person saved from their editor, which the planner is told
    // and the question is put again for.
    let mut edited_by_person = false;
    // Whether a person was put the question, which decides how the stamp is compared when the
    // bytes are released: a `run` line that started meanwhile voids what a person was shown, and
    // voids nothing where the mode or a rule answered (TRUST-4).
    let mut person_asked = false;

    let (scanned, reviewed, reach) = loop {
        let body_label = body.label();

        // What this would leave in the tree, before it is put to anybody and before a byte of it is
        // written. Asked here rather than after the write so that a refused value never lands: a
        // file deleted afterwards has still held the secret, and whatever was watching the
        // directory has still seen it. Asked before the approval prompt for a smaller reason: a
        // person should not be shown a diff to approve that is going to be refused whatever they
        // answer.
        //
        // The pre-image goes to the scan labelled as the trust map holds the path, and the scan
        // reads it through the trusted-content gate: prior bytes a sibling effect left untrusted
        // are refused there, so they cannot excuse a credential in this body.
        //
        // A body the person saved from their editor goes through it as well, for the same reason:
        // what lands is what was scanned, whoever wrote it.
        let scanned = policy.scan_a_write(
            "write_file",
            &shown_path,
            replaces.then_some(&pre_image),
            &body,
        );
        // And written down, which is the other half of where a finding goes: a line drawn while
        // nobody was looking is gone when the turn ends, and the scan exists to tell a person what
        // is in their own tree (CRED-19). Written before the refusal below and before anybody is
        // asked, because a refusal, an approval and a decline are equally findings.
        //
        // A later round writes down only what the person's text brought: the values the file
        // already held were recorded with the first body's findings, and writing them again would
        // list each of them once per edit.
        let found: Vec<_> = if edited_by_person {
            scanned.authored.iter().collect()
        } else {
            scanned.all()
        };
        tools.recording().record(workspace.root(), &found);
        if !scanned.refused().is_empty() {
            return credential_refusal(&shown_path, &scanned, edited_by_person);
        }

        // A guess goes to the person rather than deciding by itself, and asking is the only way to
        // put it to them, so a body the scan has doubts about is a body somebody looks at even
        // where the path's own rule would not have asked.
        let mut to_approve = describe_all(&scanned.to_approve());
        // Unless the person already answered for this file, for the session or past it.
        let standing = StandingAnswer::read(
            policy,
            workspace,
            tools.recording(),
            "write_file",
            &proposed_path,
            &mut to_approve,
        );

        // What the write would change, compared inside the kernel and released once. The question
        // below is drawn from it and so is the line the person is told afterwards, which is the
        // same comparison and so is made once.
        let landed = landing_of(workspace, &proposed_path);
        // What the destination reaches as the question is drawn, which the write compares with
        // what it reaches when the bytes are released. Taken before the questions below, so a link
        // put in place between them and the write is not read as what was asked.
        let reach = workspace.reach_of(&proposed_path);
        let reviewed = review_a_write(
            policy,
            "write_file",
            intent,
            &replaced,
            &body,
            replaced_age,
            &landed,
        );

        // A body the person edited is put again whatever the path's own rule says: they asked to
        // see the write before it happened, and the diff is of text the question has not shown
        // yet. A permission mode that answers writes unasked answers this one as it answers any
        // other, and still leaves a write that would create a credential, or land in a protected
        // path, to the person.
        if write_needs_approval(policy, workspace, &proposed_path, body_label, destination)
            || always_ask
            || (tools.permission_mode.writes_are_always_asked()
                && put_where_nobody_can_be_asked(policy, workspace, &proposed_path))
            || !to_approve.is_empty()
            || edited_by_person
        {
            // Released for display only, and inside the branch because there is no screen on the
            // other one: the reviewer reads this before approving, and nothing after the write
            // reads it, so releasing it where nobody is asked would put a declassification in the
            // trail with no audience for it. A display release cannot feed an effect.
            let shown = {
                let proof = policy.authorise_display_release("proposed write");
                body.clone().declassify(&proof)
            };
            // The file this replaces, released for the same screen and for the same reason: the
            // reviewer sees what they are about to lose, and nothing else reads it. Nothing is
            // released where there is no file to replace.
            //
            // A file that is there but is not text is released too, empty, because whether the peek
            // found anything is a question about the file's own bytes and this is the one place
            // that may not ask it. What the reviewer is shown of such a file is as empty as what is
            // released, so the trail and the screen say the same thing.
            let existing = replaces.then(|| {
                let proof = policy.authorise_display_release("the file a write replaces");
                existing.clone().declassify(&proof)
            });
            let request = WriteRequest {
                written_since_checkout,
                intent,
                existing,
                path: proposed_path.clone(),
                contents: shown,
                diff: reviewed.diff.clone(),
                // The reviewer is the only one who will read this. Say what they are reading.
                untrusted: !body_label.is_trusted(),
                remark: remark.clone(),
                credentials: to_approve,
                protected: write_is_protected(workspace, &proposed_path),
                may_always: standing.may_always,
                record: standing.record.clone(),
                may_edit,
            };

            // Read as the question is put, because a person can change the mode while they decide
            // it, and the trail must not credit the new mode with an answer they gave.
            let mode = tools.permission_mode.get();
            let (answer, who) = confirmer.confirm_write_noting_the_mode(&request);
            // Who answered is the confirmer's word where it gives one, so a mode changed between
            // the read above and the confirmer's own look cannot leave a person's question
            // compared as if nobody was asked.
            let answered_by = crate::confirm::answered_by(who, || {
                mode.answers_a_write_unasked(!request.credentials.is_empty(), request.protected)
                    .then(|| mode.name())
            });
            person_asked |= answered_by.is_none();
            // The body the person saved is not an answer to this question: it replaces what the
            // question was about, and the write is put to them again with the new diff. Taken only
            // where the prompt offered it, so an edit sent where none was drawn is a refusal below,
            // whatever else the answer says.
            if let Some(saved) = answer.edited() {
                // The kernel refuses a proposal that was not trusted as well, so a front end that
                // sends an edit for one reaches the refusal below however it got there.
                if may_edit
                    && let Ok(adopted) =
                        policy.adopt_edited_body("write_file", &body, saved.to_string())
                {
                    body = adopted;
                    edited_by_person = true;
                    continue;
                }
                policy.refuse_unapproved_write(call);
                return credential_aware_rejection(&shown_path, &scanned, false);
            }
            policy.record_answer(answered_by);
            keep_the_note(policy, "write_file", answer.approved(), answer.note());
            if !answer.approved() {
                policy.refuse_unapproved_write(call);
                return credential_aware_rejection(&shown_path, &scanned, answer.note().is_some());
            }
            standing.keep(policy, answer);
        }
        break (scanned, reviewed, reach);
    };
    // Read once the loop is done, since an edit replaced the body and its label with the person's.
    let body_label = body.label();

    // The approval is the path's authority rather than a relabelling of it, and it is bound to
    // this exact value. Issued for the path the question named, which an edit does not change.
    policy.issue_grant("file_write", "path", proposed_path.clone());

    match workspace.write_endorsed_at_revision(
        policy,
        &path,
        &body,
        Some(crate::workspace::Prepared::new(approved_at, person_asked)),
        Some(&reach),
    ) {
        Ok(landed) => {
            workspace.record_write((destination == Destination::Named).then_some(&shown_path));
            let Reviewed {
                note,
                changes,
                written,
                ..
            } = reviewed;
            let checked = if check_with_server {
                diagnostics_after_a_write(policy, tools.servers.as_deref_mut(), &proposed_path)
            } else {
                None
            };

            // What the model is told, which is what its own account of the turn will repeat. It
            // used to be told "wrote" either way, and would go on to say it had created a file
            // it had in fact replaced, which is the opposite of what the user needed to hear.
            //
            // A write through a reference says the same in the only terms the planner has. It
            // used to read "replaced ref:1, which was already there", which says a reference was
            // replaced rather than a file, and does not say the work is done: one planner read
            // that, could not tell whether anything had happened, and wrote both files a second
            // time. So this says what landed where, and that there is nothing left to do.
            let done = match (intent, destination) {
                (Intent::Create, Destination::Named) => format!("created {shown_path}"),
                (_, Destination::Named) => {
                    format!("replaced {shown_path}, which was already there")
                }
                (Intent::Create, Destination::Reference) => format!(
                    "created the file {shown_path} names, from {}. It is written; do not write \
                     {shown_path} again.",
                    body_from
                ),
                (_, Destination::Reference) => format!(
                    "replaced the file {shown_path} names, which was already there, from {}. It \
                     is written; do not write {shown_path} again.",
                    body_from
                ),
            };
            let done = match checked {
                Some(said) => followed_by(done, &said),
                None => done,
            };
            // That the person changed the body, and nothing about what it holds now (WRITE-7).
            let done = if edited_by_person {
                followed_by(
                    done,
                    "The user changed the contents before they were written, so the file may \
                     differ from what you gave.",
                )
            } else {
                done
            };
            confirmed(done, carried_note(note, &scanned))
                .with_changes(changes)
                .with_written(written)
                .marked_untrusted(!body_label.is_trusted())
                .having_changed_a_file_at(workspace, &landed)
        }
        Err(e) => write_refused(policy, tools, "write_file", destination, &e, &shown_path),
    }
}

/// What a write of a picture or PDF carries.
struct PictureBody {
    /// The data URI the slot is kept as, released into the workspace at the label it was
    /// quarantined at, which the file's recorded trust follows.
    body: Labelled<String>,
    /// The media type the driver recorded for the slot.
    media: String,
    /// What the planner called the body, for the account it is given afterwards.
    body_from: String,
}

/// Write the file the picture or PDF in a slot encodes, after a person approves it (WRITE-9).
///
/// The person is shown the slot, its media type, the size of the file and the path it lands in,
/// and nothing the file holds: it is not text, so there is no diff, no scan and no body to edit.
/// The bytes leave the slot decoded by the effect that writes them, which is handed the proof that
/// the destination was endorsed, and nothing here reads one of them. The file is recorded as
/// holding what the slot held, so a picture from a server's tool result leaves an untrusted file.
#[expect(
    clippy::too_many_lines,
    reason = "one write from its question to the effect that makes it, as the other writes here are"
)]
fn write_a_picture<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    landing: Landing,
    given: PictureBody,
) -> Produced {
    let workspace = tools.workspace;
    let Landing {
        path,
        shown_path,
        proposed_path,
        ..
    } = landing;
    let PictureBody {
        body,
        media,
        body_from,
    } = given;
    let label = body.label();

    // What the file holds now is not read: it is replaced whole, and the person is told so.
    let (replaces, approved_at) = policy.capture_files(|_, capture| {
        (
            workspace.names_a_file(&proposed_path),
            capture.stamp_of(&workspace.trust_key(&proposed_path)),
        )
    });
    let intent = Intent::of_a_write(replaces);
    // What the destination reaches as the question is drawn, which the write compares with what it
    // reaches when the bytes are released.
    let reach = workspace.reach_of(&proposed_path);

    // How many bytes the file will hold, counted where the URI is turned back into them and shown
    // to the person who is asked. A count, as a reference's byte count is.
    let size = {
        let counted = policy.render_in_place("write_file", &body, |uri| {
            picture_bytes(&uri).map_or(0, |bytes| bytes.len())
        });
        let proof = policy.authorise_display_release("the size of a picture a write would create");
        counted.declassify(&proof)
    };

    // Always put to the person, whatever the trust map would have said about the path: the bytes
    // are nobody's and the file is somebody's, so the pair is the one thing they can answer for
    // (WRITE-4).
    let request = WriteRequest {
        written_since_checkout: false,
        intent,
        existing: replaces.then(String::new),
        // The slot and the path in the one field every front end draws as the destination. There
        // is no body to put in `contents` and nothing for a diff to say.
        path: format!(
            "{} -> {proposed_path}",
            crate::report::write_picture_source(body_from.as_str(), media.as_str(), size)
        ),
        contents: String::new(),
        diff: Diff::compute("", ""),
        untrusted: !label.is_trusted(),
        remark: None,
        protected: write_is_protected(workspace, &proposed_path),
        credentials: Vec::new(),
        may_always: false,
        record: None,
        may_edit: false,
    };
    let mode = tools.permission_mode.get();
    let (answer, who) = confirmer.confirm_write_noting_the_mode(&request);
    let answered_by = crate::confirm::answered_by(who, || {
        mode.answers_a_write_unasked(false, request.protected)
            .then(|| mode.name())
    });
    let person_asked = answered_by.is_none();
    policy.record_answer(answered_by);
    keep_the_note(policy, "write_file", answer.approved(), answer.note());
    if !answer.approved() {
        policy.refuse_unapproved_write("write_file");
        return Produced::problem(format!(
            "refused: the user did not approve writing {shown_path}. Do not retry the same \
             write{}",
            what_next(answer.note().is_some())
        ));
    }

    // The approval is the path's authority, and it is bound to this exact value.
    policy.issue_grant("file_write", "path", proposed_path.clone());
    let outcome = workspace.write_streamed_endorsed(
        policy,
        &path,
        label,
        Some(crate::workspace::Prepared::new(approved_at, person_asked)),
        Some(&reach),
        |file, proof| {
            use std::io::Write;
            // No detail of why it did not decode: the error would name a byte of the URI.
            let bytes = picture_bytes(&body.declassify(proof))
                .map_err(|_| std::io::Error::other("the picture did not decode"))?;
            file.write_all(&bytes)
        },
    );
    match outcome {
        Ok(landed) => {
            workspace.record_write(Some(&shown_path));
            let verb = if replaces { "replaced" } else { "created" };
            confirmed(
                format!(
                    "{verb} {shown_path} from {body_from} ({media}). It is written and \
                     quarantined: you cannot read it, and there is nothing further to do for \
                     it. Do not write {shown_path} again."
                ),
                format!("{media}, {size} bytes"),
            )
            .marked_untrusted(!label.is_trusted())
            .having_changed_a_file_at(workspace, &landed)
        }
        Err(e) => write_refused(
            policy,
            tools,
            "write_file",
            Destination::Named,
            &e,
            &shown_path,
        ),
    }
}

/// `done` with `sentence` after it, as one more sentence of a result.
fn followed_by(done: String, sentence: &str) -> String {
    if done.ends_with('.') {
        format!("{done} {sentence}")
    } else {
        format!("{done}. {sentence}")
    }
}

/// What a language server that is already running says about a file the planner has just written
/// or edited, as a sentence for the result, or `None` where there is nothing to say (LSP-12).
///
/// Never starts a server: that is an approved action and a write or an edit is not the approval. The
/// sentence is counts and line numbers in this repository's words, so it is the driver's own
/// text; what makes it the file's is that it is only given where the trust map vouches for the
/// file it describes, the same footing [LSP-3](../../../docs/specs/tools/lsp.md) puts a location
/// on. Nothing branches on what the server said: the outcome goes to the renderer whole.
fn diagnostics_after_a_write<S: Sink>(
    policy: &mut Policy<'_, S>,
    servers: Option<&mut LanguageServers>,
    proposed_path: &str,
) -> Option<String> {
    let servers = servers?;
    // Without `.` components, since the server is matched by the URI of the path it was sent.
    let absolute = servers
        .root()
        .join(proposed_path)
        .components()
        .collect::<std::path::PathBuf>()
        .to_str()?
        .to_owned();
    // Asked before anything is recorded, so a write with no server to ask leaves no trace of one.
    if !servers.covers(&absolute) {
        return None;
    }
    // The file's own entry, which the write has just set from the body's trust (TRUST-4), so a
    // body that came out of quarantined content leaves a file that reports nothing.
    let label = policy
        .observe_paths(
            bravebot_core::capability::Capability::LanguageServer,
            [absolute.as_str()],
        )
        .ok()?;
    if !label.is_trusted() {
        return None;
    }
    let outcome = servers.diagnostics(policy, &absolute)?;
    Some(crate::lsp::describe_diagnostics(&outcome))
}

/// Bring the files a delegate wrote in a kept checkout back into the working directory, one
/// path at a time (CHECKOUT-14).
///
/// Each file is a write of the checkout file's bytes through the gate every other write takes. It
/// is put to the person whatever the trust map's table would have said, so the one question the
/// driver cannot answer, which is whether their file changed since the checkout was made, is
/// theirs to answer from the difference they are shown. The bytes keep the label the checkout's
/// path has, and nothing here reads them: they are carried from the file to the write.
fn apply_checkout<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let workspace = tools.workspace;
    let Some(named) = named_argument(arguments, "checkout") else {
        return Produced::problem(
            "error: 'checkout' is required: the number a delegate's report gave its checkout, \
             such as c1",
        );
    };
    let id = match policy.read_planner_argument("apply_checkout", "checkout", &named) {
        Ok(id) => id,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let Some(kept) = workspace
        .session_checkouts()
        .into_iter()
        .find(|kept| kept.id == id)
    else {
        return Produced::problem(format!(
            "error: the session keeps no checkout {id}. Use the number a delegate's report gave \
             for one it kept."
        ));
    };

    let given: Option<Vec<String>> = match arguments.get("paths") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => {
            let mut given = Vec::new();
            for item in items {
                match item.as_str() {
                    Some(path) => given.push(path.to_string()),
                    None => return Produced::problem("error: 'paths' holds only strings"),
                }
            }
            Some(given)
        }
        Some(_) => return Produced::problem("error: 'paths' is a list of paths"),
    };
    let offered = offered(policy, workspace, &kept, given.as_deref(), tools.beside);
    let paths: Vec<String> = match given {
        None => offered.paths.iter().cloned().collect(),
        Some(given) => match only_recorded(&id, &offered, given) {
            Ok(paths) => paths,
            Err(refusal) => return Produced::problem(refusal),
        },
    };
    bring_back(policy, tools, confirmer, &id, &kept, &offered, paths)
}

/// What can come back from a kept checkout: the paths the driver recorded a write to, and the
/// ones a status over the checkout listed (CHECKOUT-13).
struct Offered {
    /// Both kinds, each once.
    paths: std::collections::BTreeSet<String>,
    /// The ones only the status listed, which the file read has to be told are candidates.
    listed: std::collections::BTreeSet<String>,
    /// The paths the status listed as deleted, which are named and not removed (CHECKOUT-14).
    removed: Vec<String>,
    /// The driver's sentence about what the status could not say, where it could not say all.
    gap: Option<String>,
}

/// The paths of the kept checkout that may come back. The status is read afresh on every call,
/// since a program may have written there since the last one, unless `given` names only paths the
/// driver recorded itself, which no status could add to or take from.
///
/// `beside` is this turn's tool list, read only to say which reader a status that could not be
/// read names, where reading it fails (TOOL-6).
fn offered<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    kept: &crate::workspace::SessionCheckout,
    given: Option<&[String]>,
    beside: Beside,
) -> Offered {
    let mut paths = kept.candidates.named.clone();
    let mut listed = std::collections::BTreeSet::new();
    let mut removed = Vec::new();
    if given.is_some_and(|given| given.iter().all(|path| paths.contains(path))) {
        return Offered {
            paths,
            listed,
            removed,
            gap: None,
        };
    }
    let gap = match workspace.checkout_status(policy, &kept.id) {
        Ok(listing) => {
            for path in listing.changed {
                if paths.insert(path.clone()) {
                    listed.insert(path);
                }
            }
            // A file written and then deleted is not there to come back.
            for path in &listing.removed {
                paths.remove(path);
            }
            removed = listing.removed;
            (!listing.complete).then(|| {
                "The status left something out (a path a rule withholds, a file it did not \
                 compare, a conflict or a directory it did not open, or it was cut), so a file \
                 may be missing here."
                    .to_string()
            })
        }
        Err(declined) => Some(format!(
            "The status could not be read, so a file a program wrote other than by a redirection \
             is not found: {}",
            declined.describe_as("the checkout", checkout_reader(beside))
        )),
    };
    Offered {
        paths,
        listed,
        removed,
        gap,
    }
}

/// `given`, each path once, if every one is a candidate the driver recorded for the kept checkout
/// `id`; otherwise the refusal naming the first that is not. Nothing is written either way.
fn only_recorded(
    id: &str,
    offered: &Offered,
    mut given: Vec<String>,
) -> Result<Vec<String>, String> {
    if let Some(deleted) = given.iter().find(|path| offered.removed.contains(*path)) {
        return Err(format!(
            "refused: the status of checkout {id} lists {deleted} as deleted there, and a \
             deletion is not brought back. Nothing was written."
        ));
    }
    if let Some(unlisted) = given.iter().find(|path| !offered.paths.contains(*path)) {
        return Err(format!(
            "refused: the driver recorded no write to {unlisted} in checkout {id} and its status \
             does not list it, and only a path it recorded or a status listed can be brought \
             back. Nothing was written."
        ));
    }
    // Once each, so a path named twice is not put to the person twice.
    let mut seen = std::collections::BTreeSet::new();
    given.retain(|path| seen.insert(path.clone()));
    Ok(given)
}

/// What `/checkouts apply` brought back: the driver's own account of each file, and whether any
/// file was written.
pub(crate) struct Brought {
    /// One line a file, after the summary line `apply_checkout` leads with.
    pub text: String,
    pub applied: bool,
}

/// Bring back the files of the session's kept checkout `id`, on a person's typed request rather
/// than a planner's call (CHECKOUT-14): every candidate where `paths` is empty, otherwise the
/// candidates it names, and nothing at all if one of them is not a candidate.
///
/// The number and the paths are the person's own, so no argument gate stands in front of them:
/// what follows is `apply_checkout`'s own loop, which puts each file to the person whatever the
/// trust map says.
pub(crate) fn apply_kept_checkout<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    id: &str,
    paths: Vec<String>,
) -> Result<Brought, String> {
    let Some(kept) = tools
        .workspace
        .session_checkouts()
        .into_iter()
        .find(|kept| kept.id == id)
    else {
        return Err(format!("the session keeps no checkout {id}"));
    };
    let given = (!paths.is_empty()).then_some(paths.as_slice());
    let offered = offered(policy, tools.workspace, &kept, given, tools.beside);
    let paths = match paths.is_empty() {
        true => offered.paths.iter().cloned().collect(),
        false => only_recorded(id, &offered, paths)?,
    };
    let produced = bring_back(policy, tools, confirmer, id, &kept, &offered, paths);
    let applied = produced.changed_a_file;
    // The driver's own sentences, which carry no byte of any file.
    let text = produced
        .text
        .into_trusted()
        .unwrap_or_else(|_| "see the note beside this call".to_string());
    // A refusal is one line and is all there is to say; a run is a summary line and then a line a
    // file, and the summary is the heading the caller words for itself.
    let text = match text.split_once('\n') {
        Some((_summary, files)) => files.to_string(),
        None => text,
    };
    Ok(Brought { text, applied })
}

/// The driver's sentence naming the paths a checkout's status lists as deleted, which are not
/// removed from the working directory (CHECKOUT-14). Empty where there are none.
fn removed_sentence(removed: &[String]) -> String {
    const SHOWN: usize = 20;
    if removed.is_empty() {
        return String::new();
    }
    let mut names: Vec<String> = removed
        .iter()
        .take(SHOWN)
        .map(|path| format!("`{path}`"))
        .collect();
    if removed.len() > SHOWN {
        names.push(format!("{} more", removed.len() - SHOWN));
    }
    format!(
        " The checkout's status lists {} as deleted there; a deletion is not brought back, and \
         the file stays in the working directory.",
        names.join(", ")
    )
}

/// Put each of `paths`, which are candidates of the kept checkout `id`, through the write gate.
///
/// The half of [`apply_checkout`] that does not care who named the checkout: the planner's call
/// and the person's `/checkouts apply` both end here, and ask the same questions (CHECKOUT-14).
fn bring_back<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    id: &str,
    kept: &crate::workspace::SessionCheckout,
    offered: &Offered,
    paths: Vec<String>,
) -> Produced {
    let produced = bring_back_files(policy, tools, confirmer, id, kept, offered, paths);
    // Whoever asked, the planner or the person, a checkout whose files came back is no longer
    // waiting on anybody (OPEN-1).
    if produced.changed_a_file {
        tools.workspace.checkout_applied(id);
    }
    produced
}

/// [`bring_back`]'s work: put each path to the person and write back the ones they allow.
fn bring_back_files<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    id: &str,
    kept: &crate::workspace::SessionCheckout,
    offered: &Offered,
    paths: Vec<String>,
) -> Produced {
    let workspace = tools.workspace;
    if paths.is_empty() {
        return Produced::problem(format!(
            "error: the driver recorded no write by name in checkout {id} and its status lists \
             no changed file, so there is nothing to bring back with this. A file written through \
             a reference is not found.{}{}",
            offered
                .gap
                .as_deref()
                .map(|gap| format!(" {gap}"))
                .unwrap_or_default(),
            removed_sentence(&offered.removed)
        ));
    }

    let mut said = Vec::new();
    let mut notes = Vec::new();
    let mut changes = Vec::new();
    let mut changed = false;
    let mut untrusted = false;
    let mut applied = 0;
    for relative in &paths {
        let ask = json!({ "path": relative });
        let found = match path_argument(
            policy,
            workspace,
            "apply_checkout",
            Purpose::Effect,
            tools.slots,
            &ask,
        ) {
            Ok(found) => found,
            Err(refusal) => {
                said.push(format!("{relative}: {refusal}"));
                continue;
            }
        };
        // A path the working directory does not reach, which a directory made a link since the
        // checkout was made can be, is refused before its file is read out of the checkout and
        // released into the path, and before the person is asked (TRUST-10).
        let produced = if let Err(unreachable) = workspace.resolve(&found.released) {
            write_refused(
                policy,
                tools,
                "apply_checkout",
                found.destination,
                &unreachable,
                &found.shown,
            )
        } else {
            let body = match workspace.read_checkout_file(policy, id, relative, &offered.listed) {
                Ok(body) => body,
                Err(why) => {
                    said.push(format!(
                        "{relative}: not brought back, since {}",
                        why.describe()
                    ));
                    continue;
                }
            };
            let body = policy.declassify_checkout_into_workspace(
                &format!("{id}/{relative}"),
                &found.shown,
                body,
            );
            put_in_the_workspace(
                policy,
                tools,
                confirmer,
                "apply_checkout",
                Landing {
                    path: found.path,
                    destination: found.destination,
                    shown_path: found.shown,
                    proposed_path: found.released,
                },
                Body {
                    check_with_server: false,
                    body,
                    changes_anything: true,
                    remark: None,
                    body_from: format!("checkout {id}"),
                    written_since_checkout: workspace.written_since_checkout(id, relative),
                    planner_written: false,
                },
                true,
            )
        };
        // The driver's own sentence about one file, which is trusted whichever way the write
        // went: no byte of the file is in it.
        said.push(
            produced
                .text
                .clone()
                .into_trusted()
                .unwrap_or_else(|_| format!("{relative}: see the note beside this call")),
        );
        // What the person typed at this file's question is kept for the call's result, which holds
        // one note, so each goes beside the line of the file it was typed at (PROMPT-12).
        if let Some(Ok(told)) = policy.take_note().map(Labelled::into_trusted) {
            said.push(told);
        }
        notes.push(produced.note.clone());
        if produced.changed_a_file {
            applied += 1;
            changed = true;
            changes.extend(produced.changes);
            untrusted |= produced.untrusted;
        }
    }

    if applied > 0 {
        crate::workspace::record_checkout(
            policy.sink(),
            crate::workspace::Happened::Applied,
            &kept.path,
        );
    }
    let summary = format!(
        "{applied} of {} from checkout {id} brought back.",
        tally(paths.len(), "file", "files")
    );
    if let Some(gap) = &offered.gap {
        said.push(gap.clone());
    }
    if !offered.removed.is_empty() {
        said.push(removed_sentence(&offered.removed).trim().to_string());
    }
    let text = format!("{summary}\n{}", said.join("\n"));
    let produced = confirmed(text, format!("{summary} {}", notes.join("; ")));
    let produced = Produced {
        failed: applied == 0,
        ..produced
    };
    match changed {
        true => produced
            .with_changes(changes)
            .marked_untrusted(untrusted)
            .having_changed_a_file(),
        false => produced,
    }
}

/// An `old_text` and the `new_text` that replaces it, as the planner wrote them.
type EditPair = (Labelled<String>, Labelled<String>);

/// An `edit_file` call that names a notebook cell instead of a passage.
struct CellRequest {
    cell: usize,
    action: CellAction,
    kind: crate::notebook::CellKind,
    new_text: Option<Labelled<String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellAction {
    Replace,
    InsertAfter,
    Delete,
}

/// The cell an `edit_file` call addresses, where it gives `cell` (EDIT-8).
///
/// `cell` is the number the read showed, from 1, or 0 for an insert at the start. Every argument is
/// the planner's own, and none of them is read from the file.
fn cell_request(arguments: &Value) -> Result<Option<CellRequest>, String> {
    let Some(cell) = arguments.get("cell").filter(|cell| !cell.is_null()) else {
        if arguments.get("cell_action").is_some() || arguments.get("cell_type").is_some() {
            return Err(
                "error: 'cell_action' and 'cell_type' go with 'cell', which names the \
                        notebook cell to change"
                    .to_string(),
            );
        }
        return Ok(None);
    };
    let Some(cell) = cell.as_u64().map(|cell| cell as usize) else {
        return Err("error: 'cell' must be a whole number, the cell's number from the read".into());
    };
    if ["old_text", "edits", "replace_all"]
        .iter()
        .any(|key| arguments.get(key).is_some())
    {
        return Err(
            "error: 'cell' addresses a notebook cell, so it cannot be given with \
                    'old_text', 'edits' or 'replace_all'"
                .to_string(),
        );
    }
    let action = match arguments.get("cell_action").and_then(Value::as_str) {
        None | Some("replace") => CellAction::Replace,
        Some("insert_after") => CellAction::InsertAfter,
        Some("delete") => CellAction::Delete,
        Some(_) => {
            return Err(
                "error: 'cell_action' must be \"replace\", \"insert_after\" or \"delete\"".into(),
            );
        }
    };
    let kind = match arguments.get("cell_type").and_then(Value::as_str) {
        None => crate::notebook::CellKind::Code,
        Some(name) => crate::notebook::CellKind::named(name)
            .ok_or("error: 'cell_type' must be \"code\" or \"markdown\"")?,
    };
    if action != CellAction::InsertAfter && arguments.get("cell_type").is_some() {
        return Err("error: 'cell_type' is for an insert_after, which makes a new cell".into());
    }
    let new_text = argument(arguments, "new_text");
    match (action, &new_text) {
        (CellAction::Delete, Some(_)) => {
            return Err("error: deleting a cell takes no 'new_text'".into());
        }
        (CellAction::Replace | CellAction::InsertAfter, None) => {
            return Err(
                "error: 'new_text' is required and must be a string: the cell's source".into(),
            );
        }
        _ => {}
    }
    if action != CellAction::InsertAfter && cell == 0 {
        return Err(
            "error: cells are numbered from 1; 0 is only for an insert_after at the start".into(),
        );
    }
    Ok(Some(CellRequest {
        cell,
        action,
        kind,
        new_text,
    }))
}

/// The passages an `edit_file` call carries: the `edits` array, or the single `old_text` and
/// `new_text` pair, never both.
fn edit_pairs(arguments: &Value) -> Result<Vec<EditPair>, &'static str> {
    let Some(edits) = arguments.get("edits") else {
        let Some(old_text) = argument(arguments, "old_text") else {
            return Err("error: 'old_text' is required and must be a string, or give 'edits'");
        };
        let Some(new_text) = argument(arguments, "new_text") else {
            return Err("error: 'new_text' is required and must be a string");
        };
        return Ok(vec![(old_text, new_text)]);
    };
    if arguments.get("old_text").is_some() || arguments.get("new_text").is_some() {
        return Err("error: give either 'edits' or 'old_text' and 'new_text', not both");
    }
    let Some(edits) = edits.as_array().filter(|edits| !edits.is_empty()) else {
        return Err("error: 'edits' must be a non-empty array of old_text and new_text pairs");
    };
    edits
        .iter()
        .map(|pair| {
            let old_text = argument(pair, "old_text");
            let new_text = argument(pair, "new_text");
            old_text.zip(new_text)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("error: every entry of 'edits' needs 'old_text' and 'new_text' strings")
}

/// What `edit_file` says of `cell` on a file that is not a `.ipynb` (EDIT-8). Fixed text, said
/// for a typed name at once and for a reference only once the file is known to be trusted.
const NOT_A_NOTEBOOK: &str = "error: 'cell' addresses a cell of a .ipynb notebook, and this is \
     not one. Give 'old_text' and 'new_text' for a passage";

/// Replace an exact passage in a file, after a person approves the diff.
///
/// Same endorsement shape as [`write_file`], since the model never decides a write destination,
/// but the reviewer is shown a diff of a located passage rather than a whole body, which is
/// the point of having this tool at all.
///
/// The file is read through the gates rather than peeked at, so the read is recorded and
/// the contents carry their label. The replacement then happens on released bytes, and the
/// result is written back only if the file still matches what was read.
///
/// `recording` reaches no further than the scan below: an edit's findings are written to the same
/// record a whole-file write's are.
#[expect(
    clippy::too_many_arguments,
    reason = "each is borrowed from the turn's tools or the dispatch and owned by a different holder, so no struct exists to bundle them"
)]
fn edit_file<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    slots: &SlotStore,
    recording: crate::findings::Recording<'_>,
    mode: crate::PermissionMode,
    nobody_to_ask: bool,
    servers: Option<&mut LanguageServers>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    if let Err(refused) = refuse_unless_writing_is_held(policy, "edit_file") {
        return Produced::problem(refused);
    }
    let found = match path_argument(
        policy,
        workspace,
        "edit_file",
        Purpose::Effect,
        slots,
        arguments,
    ) {
        Ok(found) => found,
        Err(refusal) => return Produced::problem(refusal),
    };
    let (proposed, destination, shown_path, proposed_path) =
        (found.path, found.destination, found.shown, found.released);
    let cell_request = match cell_request(arguments) {
        Ok(request) => request,
        Err(problem) => return Produced::problem(problem),
    };
    let pairs = match &cell_request {
        // A cell edit carries its text in `new_text` alone and locates nothing by it.
        //
        // The extension is tested here only for a name the planner typed, which is its own
        // words. The name behind a reference came out of a directory nobody vouched for, so
        // testing it before the trust check below would tell the planner whether that name ends
        // in `.ipynb`: a `.txt` would be refused as not a notebook and a `.ipynb` as closed to
        // editing. For a reference the test is made once the file is known to be trusted.
        Some(_)
            if destination == Destination::Named
                && !crate::notebook::is_notebook_path(&proposed_path) =>
        {
            return Produced::problem(NOT_A_NOTEBOOK);
        }
        Some(_) => Vec::new(),
        None => match edit_pairs(arguments) {
            Ok(pairs) => pairs,
            Err(problem) => return Produced::problem(problem),
        },
    };
    // Absent or non-boolean means the strict single-match behaviour, which is the safe
    // reading of an ambiguous argument.
    let replace_all = arguments
        .get("replace_all")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Both are the planner's own words, and locating a passage by comparing them against the
    // file is a decision taken from them. The gate is what says the planner's words may be read
    // at all: it refuses once this context has met anything untrusted, which is the moment they
    // stop being the planner's own.
    //
    // Asked before the file is opened. A refusal here means no edit is going to happen, and a
    // refusal that has already spent a read capability and put an observation in the trail is a
    // refusal that did something.
    let mut passages = Vec::with_capacity(pairs.len());
    for (old_text, new_text) in &pairs {
        let old_text = match policy.read_planner_argument("edit_file", "old_text", old_text) {
            Ok(text) => text,
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        };
        let new_text = match policy.read_planner_argument("edit_file", "new_text", new_text) {
            Ok(text) => text,
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        };
        passages.push((old_text, new_text));
    }
    // The cell's source is the planner's own text, held to the same gate as a passage.
    let cell_edit = match &cell_request {
        Some(request) => {
            let source = match &request.new_text {
                Some(text) => match policy.read_planner_argument("edit_file", "new_text", text) {
                    Ok(text) => text,
                    Err(denial) => return Produced::problem(format!("refused: {denial}")),
                },
                None => String::new(),
            };
            Some(match request.action {
                CellAction::Replace => crate::notebook::CellEdit::Replace {
                    cell: request.cell,
                    source,
                },
                CellAction::InsertAfter => crate::notebook::CellEdit::Insert {
                    after: request.cell,
                    kind: request.kind,
                    source,
                },
                CellAction::Delete => crate::notebook::CellEdit::Delete { cell: request.cell },
            })
        }
        None => None,
    };

    // Reading to locate the passage is non-destructive and confined, so the path may be
    // promoted here exactly as it is for read_file. The write below is what needs a person.
    let path = match policy.promote_confined_read("edit_file", "path", &proposed) {
        Ok(p) => p,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // Worded about `shown_path`, which is `ref:N` where the planner named a reference, and never
    // about the path the read was made on. A failure here is the planner's first sight of this
    // call: it arrives before anybody is asked to approve the edit, and before
    // `read_trusted_content` below, which is the gate that refuses an untrusted file. The name
    // behind a reference is a filename out of a directory nobody vouched for, and what a tool
    // produces is trusted text, so interpolating the error's own path would hand the planner
    // exactly what the reference exists to withhold, in the driver's own voice. The deny-rule
    // road in `path_argument` already swaps the same name in, through `denied_by_rule`.
    // The stamp is taken in the boundary the read is, so it and the text are of one state of the
    // map. The approval below is spent only on that stamp (TRUST-4).
    let (source, read_at) = match workspace.read_stamped(policy, &path) {
        Ok(read) => read,
        Err(e) => {
            return file_tool_problem(
                policy,
                "edit_file",
                path_field(destination),
                &e,
                &shown_path,
            );
        }
    };

    // Locating the passage means comparing text, which is a decision. It is only permissible
    // on trusted content: doing it on untrusted bytes would let file content decide whether an
    // effect happens, which is the one thing this design forbids. An untrusted file is refused
    // rather than edited blind. The user can vouch for the path if they want edits there.
    //
    // Confidentiality is not the question here. Workspace content is private, and staying
    // inside the process to locate a passage releases nothing; only integrity decides whether
    // this comparison is safe to make.
    let Ok(current) = policy.read_trusted_content("edit_file", &source) else {
        // Whether a read of this path would put the vouch question, asked with the guards
        // `read_file` asks before it does, from the path, the map, the mode and the turn's list.
        // A run with nobody to ask outside bypass gets no question from a read, so it is not sent
        // to one.
        //
        // Not named for a reference. Whether a read would put the question depends on the
        // extension of the name and on what the file system says of it, and the name behind a
        // reference came out of a directory nobody vouched for, so the sentence would differ by
        // a name the planner was not shown. A reference gets the vouch as the person's to give.
        let read = if destination == Destination::Named
            && !nobody_to_ask
            && policy.reading_files_is_offered()
            && bravebot_filetype::by_name::media_for(&proposed_path).is_none()
            && workspace.names_a_file(&proposed_path)
            && {
                let keyed = workspace.trust_key(&proposed_path);
                let other_names = workspace.other_trust_keys(&proposed_path);
                policy.would_offer_vouch(&keyed, &other_names.keys, other_names.complete)
            } {
            if mode == crate::PermissionMode::Bypass {
                ReadToVouch::TheModeAnswers
            } else {
                ReadToVouch::AsksThePerson
            }
        } else {
            ReadToVouch::Not
        };
        return Produced::problem(closed_to_editing(policy, source.label(), read));
    };

    // The file is trusted, so a name a person vouched for may be tested. For a reference this is
    // the first moment the extension may be looked at (see the check above for a typed name).
    if cell_request.is_some()
        && destination == Destination::Reference
        && !crate::notebook::is_notebook_path(&proposed_path)
    {
        return Produced::problem(NOT_A_NOTEBOOK);
    }

    // In order, each on the text the one before it left (EDIT-6). Nothing is kept until every pair
    // has applied, so a refusal part-way leaves the file as it was.
    let total = passages.len();
    let mut running = current.clone();
    let mut occurrences = 0;
    if let Some(edit) = &cell_edit {
        // A cell is found by its number, which the planner gave, and the file is read as the
        // notebook it is: the file is trusted, as `read_trusted_content` above established.
        match crate::notebook::apply(&current, edit) {
            Ok(contents) => running = contents,
            Err(e) => return Produced::problem(format!("error: {e}")),
        }
        occurrences = 1;
    }
    for (at, (old_text, new_text)) in passages.iter().enumerate() {
        match crate::replace::replace(&running, old_text, new_text, replace_all) {
            Ok(r) => {
                running = r.contents;
                occurrences += r.occurrences;
            }
            Err(e) if total == 1 => {
                return Produced::problem(format!(
                    "error: {}",
                    e.said(policy.writing_is_offered())
                ));
            }
            Err(e) => {
                return Produced::problem(format!(
                    "error: edit {} of {total}: {}. No edit in this call was applied",
                    at + 1,
                    e.said(policy.writing_is_offered())
                ));
            }
        }
    }
    if running == current {
        return Produced::problem(
            "error: the edits together leave the file as it was, so this call would change nothing",
        );
    }
    let replaced = crate::replace::Replaced {
        contents: running,
        occurrences,
    };

    // The result is the model's edit applied to trusted text, so its integrity is that of the
    // context the model was working from.
    let body = policy.label_model_output("edit_file", replaced.contents);
    let body = policy.declassify_written_into_workspace("edit_file", &shown_path, body);
    let body_label = body.label();

    // The same scan a whole-file write goes through, for the same reason and at the same moment:
    // an edit is a write of the file with a passage swapped, and a secret pasted into a passage
    // lands in the tree exactly as one written whole does. The pre-image here is the text the
    // passage was located in, which the edit already read.
    let scanned = policy.scan_a_write("edit_file", &shown_path, Some(&source), &body);
    // And written down where a whole-file write's findings are, for the same reason (CRED-19).
    recording.record(workspace.root(), &scanned.all());
    if !scanned.refused().is_empty() {
        return credential_refusal(&shown_path, &scanned, false);
    }

    // As in a whole-file write: a guess is put to a person rather than deciding on its own, and
    // asking is the only way to put it.
    let mut to_approve = describe_all(&scanned.to_approve());
    let standing = StandingAnswer::read(
        policy,
        workspace,
        recording,
        "edit_file",
        &proposed_path,
        &mut to_approve,
    );

    // The same comparison the question is drawn from and the line afterwards states, made once
    // and inside the kernel. The pre-image is the labelled value the read produced rather than
    // the copy released to locate the passage in.
    let landed = landing_of(workspace, &proposed_path);
    let reach = workspace.reach_of(&proposed_path);
    let reviewed = review_a_write(
        policy,
        "edit_file",
        Intent::Edit,
        &source,
        &body,
        None,
        &landed,
    );

    let mut person_asked = false;
    if write_needs_approval(policy, workspace, &proposed_path, body_label, destination)
        || (nobody_to_ask && put_where_nobody_can_be_asked(policy, workspace, &proposed_path))
        || !to_approve.is_empty()
    {
        // Released for display only, and inside the branch for the reason a whole-file write
        // releases inside its own: nothing past the question reads it.
        let shown = {
            let proof = policy.authorise_display_release("proposed edit");
            body.clone().declassify(&proof)
        };
        let request = WriteRequest {
            written_since_checkout: false,
            path: proposed_path.clone(),
            contents: shown,
            existing: Some(current.clone()),
            diff: reviewed.diff.clone(),
            intent: Intent::Edit,
            untrusted: !body_label.is_trusted(),
            // An edit is the planner's own words over a file it read. No processor was involved,
            // so there is nothing anybody said about it.
            remark: None,
            credentials: to_approve,
            protected: write_is_protected(workspace, &proposed_path),
            may_always: standing.may_always,
            record: standing.record.clone(),
            may_edit: false,
        };

        let (answer, who) = confirmer.confirm_write_noting_the_mode(&request);
        let answered_by = crate::confirm::answered_by(who, || {
            mode.answers_a_write_unasked(!request.credentials.is_empty(), request.protected)
                .then(|| mode.name())
        });
        person_asked = answered_by.is_none();
        policy.record_answer(answered_by);
        keep_the_note(policy, "edit_file", answer.approved(), answer.note());
        if answer.approved() {
            standing.keep(policy, answer);
        } else {
            policy.refuse_unapproved_write("edit_file");
            let told = answer.note().is_some();
            if !scanned.to_approve().is_empty() {
                return credential_aware_rejection(&shown_path, &scanned, told);
            }
            return Produced::problem(format!(
                "refused: the user did not approve editing {shown_path}. Do not retry the \
                 same edit{}",
                what_next(told)
            ));
        }
    }

    policy.issue_grant("file_write", "path", proposed_path.clone());

    let occurrences = replaced.occurrences;
    // The path as the planner gave it, not the copy promoted above for the read. A write routed
    // on a promoted value would be routed by the model's own proposal.
    match workspace.write_endorsed_if_unchanged_reaching(
        policy,
        &proposed,
        &body,
        &current,
        crate::workspace::Prepared::new(read_at, person_asked),
        Some(&reach),
    ) {
        Ok(landed) => {
            workspace.record_write((destination == Destination::Named).then_some(&shown_path));
            let Reviewed {
                note,
                changes,
                written,
                ..
            } = reviewed;
            let note = carried_note(note, &scanned);
            let headline = match &cell_request {
                Some(request) => match request.action {
                    CellAction::Replace => {
                        format!("edited {shown_path}: replaced cell {}", request.cell)
                    }
                    CellAction::InsertAfter => {
                        format!(
                            "edited {shown_path}: inserted a cell after cell {}",
                            request.cell
                        )
                    }
                    CellAction::Delete => {
                        format!("edited {shown_path}: deleted cell {}", request.cell)
                    }
                },
                None => format!("edited {shown_path}: {occurrences} replacement(s)"),
            };
            // What a server that is already running makes of the file now (LSP-12). It reads the
            // file, which this edit has just changed, and the sentence is counts and lines.
            let headline = match diagnostics_after_a_write(policy, servers, &proposed_path) {
                Some(said) => format!("{headline}. {said}"),
                None => headline,
            };

            // What the edit produced, not only that it produced something. A count of
            // replacements is not a result a planner can check: one wrote eighteen files in a
            // session on nothing but these lines, never saw a single one of them afterwards, and
            // never compiled any of it either. The lines around the change answer the question it
            // would otherwise have to spend a round asking.
            //
            // Shaped inside the kernel and handed over labelled, exactly as a read is, so
            // `Policy::present` decides whether the planner sees it. Nothing is declassified here
            // and no label is built by hand: the excerpt carries the label the edited body
            // carries, which is the integrity of the context this edit was worked out in.
            //
            // Only where that label is trusted. A quarantined excerpt would take the confirmation
            // down with it, and a planner that cannot be told its edit landed is worse off than
            // one that is told only that.
            //
            // The label is the context's, which is not the file's. The file is trusted by this
            // point because `read_trusted_content` above refuses to locate a passage in anything
            // else. A context that has met something untrusted is refused earlier still, by
            // `read_planner_argument` and `Policy::promote_confined_read`, so no turn reaches the
            // count-alone arm today. It stays as a second refusal, the way a resumed context's
            // refusal does in `Policy`, and `what_an_edit_result_shows` is the one place that
            // chooses: it reads a `Label` and nothing else.
            let shows = what_an_edit_result_shows(body_label);
            let lines = if shows == EditResultShows::LinesAndCount {
                let region = policy.render_in_place("edit_file", &body, |contents| {
                    crate::replace::changed_region(&current, &contents)
                });

                // These lines are text from the file handed to the planner, which is a read of
                // it, so they are scanned and put to the person as one is (CRED-15). The edit
                // itself has landed and stays: it was approved as a diff, and what is held back
                // is the planner's view of the lines around it.
                //
                // Asked about once the write has succeeded rather than before it, so a stale edit
                // does not put a question about lines the planner is never going to be given.
                let exposed =
                    credentials_an_edit_would_show(policy, &shown_path, &region, &scanned);
                let keyed = workspace.trust_key(&proposed_path);
                if !exposed.is_empty() && !policy.read_exposure_is_allowed(&keyed) {
                    let request = crate::confirm::ExposureRequest {
                        path: shown_path.clone(),
                        credentials: described(&exposed),
                    };
                    let (decision, answered_by) =
                        confirmer.confirm_exposing_read_noting_the_mode(&request);
                    if decision != Decision::Approve {
                        policy.refuse_unapproved_exposure("edit_file");
                        // The planner is told its edit landed and why the lines are missing, and
                        // nothing about what was found: a finding may not enter a model's context
                        // however the question was answered.
                        return confirmed(
                            format!(
                                "{headline}. The lines around the change are not shown: \
                                 {shown_path} holds what looks like a credential, and the user \
                                 did not agree to you being shown it. Do not try to read it \
                                 another way: work without them, or say in your reply what you \
                                 needed from them."
                            ),
                            format!(
                                "{note}; the lines around the change were not shown to the planner"
                            ),
                        )
                        .with_changes(changes)
                        .with_written(written)
                        .marked_untrusted(false)
                        .having_changed_a_file_at(workspace, &landed);
                    }
                    policy.allow_exposing_read(&keyed, answered_by);
                }

                Some(
                    policy.render_in_place("edit_file", &region, |region| match region {
                        Some(excerpt) => format!("{}\n\n{headline}", excerpt.render()),
                        None => headline.clone(),
                    }),
                )
            } else {
                None
            };
            edit_result(shows, headline, note, lines)
                .with_changes(changes)
                .with_written(written)
                .having_changed_a_file_at(workspace, &landed)
        }
        // As the read above: the name the planner is told is the one it asked with. `Stale` is the
        // arm that reaches here in practice, and it carries the path the write was routed on.
        Err(e) => file_tool_problem(
            policy,
            "edit_file",
            path_field(destination),
            &e,
            &shown_path,
        ),
    }
}

/// What an edit's result holds (EDIT-4 in docs/specs/tools/edit-file.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditResultShows {
    /// The lines the edit produced, then the count of replacements.
    LinesAndCount,
    /// The count of replacements alone, marked untrusted.
    CountAlone,
}

/// Which of the two an edit's result is, chosen from the label of the edited body alone.
///
/// The body carries the integrity of the context the edit was worked out in. The function takes
/// that label and no bytes, so the choice is not a decision taken from the file's contents.
fn what_an_edit_result_shows(body_label: Label) -> EditResultShows {
    if body_label.is_trusted() {
        EditResultShows::LinesAndCount
    } else {
        EditResultShows::CountAlone
    }
}

/// The result of an edit that landed, assembled from what the result shows.
///
/// `LinesAndCount` is the lines the edit produced and then the count, under the label they were
/// shaped with, and is not marked untrusted. `CountAlone` is the headline alone, marked untrusted,
/// whatever lines are handed in. The choice was made by `what_an_edit_result_shows` and nothing
/// here looks at the lines to make another. A `LinesAndCount` with no lines, which the caller does
/// not build, comes out as the count alone, the narrower of the two.
fn edit_result(
    shows: EditResultShows,
    headline: String,
    note: String,
    lines: Option<Labelled<String>>,
) -> Produced {
    match (shows, lines) {
        (EditResultShows::LinesAndCount, Some(told)) => {
            Produced::new(told, "", note).marked_untrusted(false)
        }
        _ => confirmed(headline, note).marked_untrusted(true),
    }
}

/// Record the task list and show it.
///
/// The one tool here with no workspace effect at all: nothing is read, nothing is written, and
/// there is no path to endorse. It is the planner's own note to itself, carried to a screen.
///
/// Two things follow from that. The list is model output, so its integrity is the context's, and
/// it is never upgraded on the way through. And the whole list arrives every time, because
/// amending a single entry would mean locating it by model-authored text, which is a comparison
/// on untrusted content. Replacing the list wholesale compares nothing.
fn todo_write<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    reporter: &mut R,
    slots: &SlotStore,
    arguments: &Value,
) -> Produced {
    let Some(todos) = arguments.get("todos").and_then(Value::as_array) else {
        return Produced::problem("error: 'todos' is required and must be an array");
    };

    // Parsing is not a decision about what happens: every entry becomes an item, and an
    // unreadable status becomes outstanding work rather than being rejected. A malformed entry
    // is skipped only because there is nothing to show for it.
    let items: Vec<Item> = todos
        .iter()
        .filter_map(|entry| {
            let content = entry.get("content")?.as_str()?.to_string();
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .map(Status::parse)
                .unwrap_or(Status::Pending);
            Some(Item::new(content, status))
        })
        .collect();

    if items.len() != todos.len() {
        return Produced::problem(
            "error: every todo needs a 'content' string; the list was not changed. Send the \
             whole list again.",
        );
    }

    // The model's words, at the integrity of the context they came from.
    let list = policy.label_model_output("todo_write", List::new(items));

    // The planner writes its list in the only terms it has, which are reference names. The
    // person reading the list has the opposite problem: "write ref:1 back to its file" says
    // nothing about their own workspace, and they are the only one entitled to know which file
    // that is. So the names go in on the way to the screen and nowhere else.
    let named = policy.names_for_display(slots);

    // Shaped inside the kernel, because choosing a glyph means reading the statuses and the
    // driver may not hold them. Every item yields a row, so nothing in the content decides
    // what the user is shown the existence of.
    //
    // The names go in here too, in the same reshape: finding a reference in a row is reading it,
    // and a driver that released the rows first and searched them afterwards would be inspecting
    // content under a witness minted to put it on a screen, which LABEL-6 refuses.
    let rows = policy.render_in_place("todo_write", &list, |list| {
        let mut rows = todo::rows(&list);
        for row in &mut rows {
            row.content = name_references(&row.content, &named);
        }
        rows
    });

    // Showing a person what the model is doing is a release to a screen, which is one of the
    // destinations a witness exists for. It cannot feed an effect.
    let proof = policy.authorise_display_release("task list");
    reporter.todos(rows.declassify(&proof));

    // The model gets its own list back as the tool result, which is how it knows what is next:
    // the turn keeps no state, so the echo in the conversation *is* the memory. Rendered through
    // the kernel like everything else, then presented under the usual gate by the caller.
    let summary = policy.render_in_place("todo_write", &list, |list| {
        if list.is_empty() {
            return "the task list is now empty".to_string();
        }
        let lines = list
            .items
            .iter()
            .map(|item| format!("[{}] {}", item.status, item.content))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{}\n{lines}", counted(&list))
    });

    let note = note_for(policy, "todo_write", &list, |list| counted(&list));

    // How many tasks are open, counted where the statuses can be looked at and labelled as the list
    // is. The turn asks about it when it answers (TURN-12), and may read it only where the list is
    // trusted: whether to say something to the planner is a decision, and a decision may not come
    // from content that is not.
    let open = policy.render_in_place("todo_write", &list, |list| list.open());
    // And how many of those wait on something outside the planner's hands, counted and labelled the
    // same way, so the question can say so without saying what any task was.
    let waiting = policy.render_in_place("todo_write", &list, |list| list.waiting());

    let mut produced = Produced::new(summary, "", note);
    produced.task_list = Some(open);
    produced.waiting_tasks = Some(waiting);
    produced
}

/// How far the list has got, in one line.
///
/// Cancelled tasks are counted apart from finished ones and only where there are any: a plan
/// nobody dropped anything from reads as it always did, and "1 of 3 done" where one of the three
/// was abandoned would report work that never happened.
fn counted(list: &List) -> String {
    let cancelled = list.cancelled();
    if cancelled == 0 {
        format!("{} of {} done", list.done(), list.len())
    } else {
        format!(
            "{} of {} done, {cancelled} cancelled",
            list.done(),
            list.len()
        )
    }
}

/// Say when this turn should be asked again.
///
/// The narrowest tool here. Nothing is read, nothing is written, and there is no path to endorse:
/// the whole of what it decides is how long the caller waits before sending the same prompt
/// again. What that prompt says is not on this surface at all, which is what makes the decision
/// one a person could have approved on its own. "Ask me that again in twenty minutes" is
/// readable without knowing what "that" is, and a field for the next prompt would have made it
/// unreadable.
///
/// The wait is held to its bounds where it is turned into a [`crate::turn::Wakeup`], so what the
/// planner is told back is what will actually happen rather than what it asked for.
fn schedule_next<S: Sink>(
    policy: &mut Policy<'_, S>,
    scheduling: Scheduling,
    arguments: &Value,
) -> Produced {
    // Only a tick of a self-paced loop can end the loop it belongs to. Anywhere else `stop` is
    // not read, and the call is held to the requirements of any other (SCHED-6).
    let stopping = scheduling == Scheduling::PacingALoop
        && arguments.get("stop").and_then(Value::as_bool) == Some(true);
    let seconds = arguments.get("delay_seconds").and_then(Value::as_u64);
    if seconds.is_none() && !stopping {
        return Produced::problem(
            "error: 'delay_seconds' is required, as a whole number of seconds",
        );
    }
    let Some(quiet) = arguments.get("noop").and_then(Value::as_bool) else {
        return Produced::problem(
            "error: 'noop' is required: true where this run found nothing to do, false where \
             something happened",
        );
    };

    let wakeup = match seconds {
        Some(seconds) if !stopping => crate::turn::Wakeup::asked(seconds, quiet),
        _ => crate::turn::Wakeup::finished(quiet),
    };
    let held = wakeup.after.as_secs();

    // The planner's own words about what it is waiting on, at the integrity of the context they
    // came from. They reach a screen and stop there: nothing waits on them, and no later turn
    // reads them back.
    let reason = policy.label_model_output(
        "schedule_next",
        arguments
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    let note = note_for(policy, "schedule_next", &reason, move |reason| {
        let reason = reason.trim().to_string();
        if stopping {
            if reason.is_empty() {
                "loop finished".to_string()
            } else {
                format!("loop finished: {reason}")
            }
        } else if reason.is_empty() {
            format!("next in {held}s")
        } else {
            format!("next in {held}s: {reason}")
        }
    });

    // Which of the two happened is worth telling the planner apart, because the answer it is
    // writing differs: a tick reports what this look found and leaves the rest to the next one,
    // while a turn that has just started a watch is telling somebody a watch now exists.
    // The withheld cases do not reach here: a tick the person timed, a line typed under the
    // session's own loop, goal or watch, and a turn nothing will ask again. Dispatch answers a
    // call from any of them as an unknown name, for want of anything a confirmation could
    // truthfully say about a wait nothing keeps.
    let confirmation = match scheduling {
        _ if stopping => "scheduled: this loop has ended and no further tick will run".to_string(),
        Scheduling::PacingALoop
        | Scheduling::TheirInterval
        | Scheduling::UnderALoop
        | Scheduling::UnderAGoal
        | Scheduling::UnderAWatch
        | Scheduling::NoLaterLook => format!(
            "scheduled: this loop runs again in {held} seconds, sending the user's own prompt \
             unchanged"
        ),
        Scheduling::ArrangingALook => format!(
            "scheduled: you will be asked again in {held} seconds, with the user's own prompt \
             sent unchanged, so the next look is arranged and needs nothing from the user"
        ),
    };

    Produced::new(Labelled::trusted(confirmation), "", note).scheduling(wakeup)
}

/// Arm a standing watch on one path.
///
/// Narrow on purpose, and narrower than a read. One field, a path, and the whole of what arming
/// it decides is that this session will be asked again when that path looks written to. Nothing
/// is opened: the look is a `stat`, and what comes back to the planner is a sentence saying the
/// watch exists.
///
/// The gate is the read's, unchanged: inside the workspace that is the promotion a read of a
/// model's own choice of file already gets, and outside it whatever the trust map answers. A
/// person who asked to be told when a file changes has asked for less than a read of it, so a
/// prompt of its own here would put a second question to somebody who has already answered the
/// first. Nothing is granted beyond it either, which is why the watch is refused where the read
/// would have been.
///
/// Three refusals before the gate, each of which the planner can act on: a session already doing
/// something without anybody typing, a session with no room, and a turn arming more than the
/// session can hold. Two after it: a path that names something that is not a file, and a path
/// the session cannot reach.
fn watch_file<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    slots: &SlotStore,
    arming: crate::watch::Arming,
    armed: &mut usize,
    arguments: &Value,
) -> Produced {
    use crate::watch::Arming;

    let free = match arming {
        Arming::Allowed { free } => free,
        Arming::UnderALoop => {
            return Produced::problem(
                "refused: a loop is running, and a session does one thing at a time that happens \
                 without anybody typing. The next tick of that loop is the next look, so answer \
                 from a read now and leave the watch. Say so, since the user can stop the loop \
                 and ask again.",
            );
        }
        Arming::UnderAGoal => {
            return Produced::problem(
                "refused: this session is working towards a goal, and a fire would spend rounds \
                 the user set aside for the work. Answer from a read now, and say that a standing \
                 watch needs the goal cleared first.",
            );
        }
        Arming::Full => {
            return Produced::problem(
                "refused: this session already holds as many watches as it keeps. Say so: the \
                 user ends one with /watch stop <n>, and /status lists them.",
            );
        }
        Arming::Unavailable => {
            return Produced::problem("error: no such tool 'watch_file'");
        }
    };

    // The bound is on the session and this turn may have armed some of it already, so what is
    // left is counted here rather than read off the answer the turn began with.
    if *armed >= free {
        return Produced::problem(
            "refused: this session already holds as many watches as it keeps. Say so: the user \
             ends one with /watch stop <n>, and /status lists them.",
        );
    }

    // A reference is refused rather than resolved, which is why this tool advertises no
    // `path_ref`. The name behind one came out of a directory nobody vouched for, and a fire's
    // prompt is the one line in this program that can carry no name off a filesystem: resolving
    // it here would put that name into the user's own role hours later, which is exactly what
    // the sentence is built to prevent. Refused before the argument is read, so nothing resolves
    // the reference on the way to saying no.
    if names(arguments, "path_ref") {
        return Produced::problem(
            "refused: watch_file takes 'path' and no reference. A reference names a file this \
             conversation was never shown the name of, and a watch reports the path it was armed \
             on, so there is nothing here a reference could be. Read the file by reference \
             instead.",
        );
    }
    let found = match path_argument(
        policy,
        workspace,
        "watch_file",
        Purpose::Read,
        slots,
        arguments,
    ) {
        Ok(found) => found,
        Err(refusal) => return Produced::problem(refusal),
    };
    // The same promotion a read of the planner's own choice of file gets, and the reason the
    // watch needs no prompt of its own. What it hands back is the path a read would open, which
    // a watch does not: the look below is a `stat` on the released name.
    if let Err(denial) = policy.promote_confined_read("watch_file", "path", &found.path) {
        return Produced::problem(format!("refused: {denial}"));
    }
    // The absolute path a `~` stands for: the session looks again from a working directory that
    // may have moved, where `~/x` reads as a relative path and would end the watch.
    let path = workspace.expanded(&found.released);

    // A directory is refused at the surface rather than watched and reported on. What changed
    // inside one is a file name the filesystem produced, and putting that in a fire's prompt is
    // untrusted content in the one position nothing can label; reporting only that the directory
    // moved is a fire nobody can act on.
    if workspace.names_something_else(&path) {
        return Produced::problem(
            "refused: 'path' must name a file, or a path with nothing at it yet. A directory \
             cannot be watched: what changed inside one is a name off the filesystem, and a fire \
             may not carry one.",
        );
    }
    // A path with nothing at it is armed, and the first look records that it is absent. Only a
    // path the session cannot reach is refused, as a read of it would be, and recorded as one: the
    // promotion above says nothing about where the path lands (TRACE-1). Named as the planner typed
    // it, since this tool takes no reference.
    if let Err(e) = workspace.resolve(&path) {
        return Produced::problem(format!(
            "refused: {}",
            workspace_failure(policy, "watch_file", "path", &e, &found.shown)
        ));
    }

    // The first look, taken here and travelling back with the path: the watch measures a change
    // from the moment somebody asked about the file (FSWATCH-1), and the rest of this turn may
    // rewrite it before anything else would look.
    let looked = workspace.look(&path, workspace.root());
    *armed += 1;
    let shown = found.shown;
    Produced::new(
        Labelled::trusted(format!(
            "watching: {shown}. You will be asked again, in a turn of its own, when that path \
             looks written to, no longer exists, or now exists. Nothing is read until then and the fire says nothing about the \
             file beyond which watch fired and on what path. Tell the user the watch exists and \
             that /watch stop ends it."
        )),
        "",
        format!("watching {shown}"),
    )
    .watching(path, looked)
}

/// Show a command's output to the user, and give it to the planner if they agree.
///
/// One of the places bytes cross out of quarantine into the planner's context on nothing but a
/// person's say-so, and the order is what makes that defensible: the output is checked, released for
/// display, put in front of the person in full, and only then, if they agree, does an endorsement
/// exist for the kernel to consume.
///
/// **While auto-vetting is off, which is the default, the verdict decides nothing here.** The word
/// travels to the prompt, the prompt draws it, and the answer is what releases the bytes. Where
/// somebody turned it on, a verdict of nothing found releases them with no prompt drawn and every
/// other verdict still draws it, which is CHECK-12 and the third of the known costs
/// `docs/specs/labels.md` admits.
///
/// What a `read_output` call without an offset or a pattern does with quarantined output, said for
/// the session as it stands: a person is asked only where one answers. Names neither the mode nor
/// a check, as MODE-3 and CHECK-9 require, and is the wording of `ReadOutputIs::stated` (OUTPUT-5).
fn whole_of_it(tools: &Tools<'_>) -> &'static str {
    let mode = tools.permission_mode.get();
    if !mode.checks_before_promoting(tools.auto_vetting) {
        "read_output brings the whole of it back to you as text you can read"
    } else if tools.auto_vetting {
        "read_output brings the whole of it back to you as text you can read, or tells you it \
         was kept back"
    } else {
        "read_output asks the user to show you the whole of it"
    }
}

/// What an unfiltered `read_output` costs, for a slot whose stated size is above `cap`: it returns
/// all of it, which stays in every later request, and a filter given the reference as `stdin_ref`
/// reads part of it. Said only from the size the slot states, so no byte of the output is read or
/// branched on. A size at or below the cap, or none stated, says nothing, and the filter half is
/// left out of a list without `run`.
pub(crate) fn read_output_cost(bytes: Option<usize>, cap: usize, beside: Beside) -> String {
    let Some(bytes) = bytes.filter(|bytes| *bytes > cap) else {
        return String::new();
    };
    let filter = if beside.holds(Named::Run) {
        " To read part of it without running the command again, call run with a filter such as \
         tail -n 100 or grep, stdin_ref set to the reference, and read: true."
    } else {
        ""
    };
    format!(
        " read_output returns all {bytes} bytes of it, about {} tokens, which stay in every later \
         request.{filter}",
        about_tokens(bytes)
    )
}

/// The approximate number of tokens `bytes` bytes come to: one for every four bytes, which is the
/// figure `read_output_cost` and the sentence about `vet_content` both state. Derived from a size
/// the slot states, so no byte of the content is read.
pub(crate) fn about_tokens(bytes: usize) -> usize {
    bytes / 4
}

/// The cost sentence for a quarantined slot, from the size it states.
fn stated_cost(tools: &Tools<'_>, slot: &SlotId) -> String {
    read_output_cost(
        tools.slots.stated_bytes(slot),
        tools.output_cap,
        tools.beside,
    )
}

/// The driver never reads the output. The text goes from the slot to the screen and, on approval,
/// from the kernel to the planner; nothing here branches on a byte of it.
fn read_output<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let Some(named) = argument(arguments, "ref") else {
        return Produced::problem(
            "error: 'ref' is required and must be a reference name, e.g. \"ref:5\"",
        );
    };

    let slot = match accept_named_reference(
        policy,
        tools.slots,
        "read_output",
        "ref",
        &named,
        NamesToList::Output,
    ) {
        Ok(slot) => slot,
        Err(refusal) => return Produced::problem(refusal),
    };

    // Refused here as well as in the kernel, so a planner naming a file is told what to do about
    // it rather than being told a gate said no. The name is a slot by now, so what is left is a
    // slot that holds something other than what a program printed.
    if !tools.slots.is_from_command(&slot) {
        // `vet_content` is offered only where the turn's list holds it, which a delegate's never
        // does (the dispatch arm) and a definition's tools line or the command line can also
        // decide. A caller is pointed at it only where it is there, and told nothing where it is
        // not, since no other tool shows a quarantined reference.
        let route = if tools.delegated {
            " A delegate has no way to ask for it to be shown; say in your report what you \
             needed from it."
        } else if tools.beside.holds(Named::VetContent) {
            " Where you have to read it yourself, vet_content is how to ask for it."
        } else {
            ""
        };
        // `run` is named only where the turn's list holds it (TOOL-6).
        let only = if tools.running.offered() {
            "Only a reference that came back from run can be read this way."
        } else {
            "Only a reference to what a program printed can be read this way."
        };
        return Produced::problem(format!(
            "refused: {slot} is not something a program printed, so there is nothing to show. \
             {only}{route}"
        ));
    }

    // A literal, like a log's skip: it names nothing, so there is no destination for it to decide.
    let offset = match arguments.get("offset") {
        None | Some(Value::Null) => None,
        Some(offset) => match offset.as_u64() {
            Some(offset) => Some(usize::try_from(offset).unwrap_or(usize::MAX)),
            None => {
                return Produced::problem(
                    "error: 'offset' must be a whole number of bytes, e.g. 16384",
                );
            }
        },
    };

    // A routing string rather than a literal, so it comes through the gate every one of those
    // comes through. Read here, before the label decides which route the call takes, so that a
    // pattern on a quarantined slot is refused for the slot and not for anything in the pattern.
    let proposed_patterns = patterns_in(arguments);
    let mut patterns = Vec::with_capacity(proposed_patterns.len());
    for proposed in &proposed_patterns {
        match policy.promote_confined_read("read_output", "pattern", proposed) {
            Ok(pattern) => patterns.push(pattern),
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        }
    }

    // Output whose label already lets the planner read it, kept out of a result by a cap on the
    // room it takes. Nobody is asked and nothing is endorsed: a yes here would be a person
    // answering for bytes the label had already answered for, and the trail would credit them
    // with a release that was never theirs to make.
    if tools.slots.label_of(&slot).is_some_and(Label::is_trusted) {
        if patterns.is_empty() {
            return read_a_page(policy, tools, &slot, Shown::Page(offset.unwrap_or(0)));
        }
        if offset.is_some() {
            return Produced::problem(
                "error: 'pattern' and 'offset' cannot be given together. A pattern searches the \
                 whole of the output and numbers each match with the byte it starts at; pass one \
                 of those bytes as 'offset' on its own to read the page from there.",
            );
        }
        let compiled = match compiled_patterns(&patterns) {
            Ok(compiled) => compiled,
            Err(problem) => return *problem,
        };
        return read_a_page(policy, tools, &slot, Shown::Matches(&compiled));
    }
    if offset.is_some() {
        return Produced::problem(format!(
            "refused: an offset pages through output you may already read, and {slot} is \
             quarantined. Without one, {}.{}",
            whole_of_it(tools),
            stated_cost(tools, &slot)
        ));
    }
    if !patterns.is_empty() {
        return Produced::problem(format!(
            "refused: a pattern searches output you may already read, and {slot} is quarantined. \
             Which lines matched, how many there are and where the result stopped are all worked \
             out from the bytes, so searching this one would answer with numbers nobody vouched \
             for. Without a pattern, {}{}.{}",
            whole_of_it(tools),
            if tools.running.offered() {
                ", and run can put the output through a program instead"
            } else {
                ""
            },
            stated_cost(tools, &slot)
        ));
    }

    // A slot that holds no bytes has nothing for a check to read or for anybody to be asked about,
    // so no check is made and nothing is released (RUN-24). Answered from the figure the slot
    // states for its reference, which the planner was shown, so no byte is read to decide. The
    // answer is the driver's own sentence and names no reference.
    if tools.slots.holds_no_bytes(&slot) {
        policy.record_empty_output("read_output", &slot);
        return confirmed(
            format!("{slot} printed nothing, so there is nothing to read."),
            "0 bytes, nothing to read",
        );
    }

    // The second opinion, before the question rather than after it. No `expects`: the planner asked
    // for the output to be read, not for it to be checked, and it has said nothing about what the
    // command printed.
    //
    // Skipped only where nothing would read the word: bypassing with no screening asked for answers
    // this question yes without showing anybody anything, so a check there is a model call nobody
    // reads. A verdict is still filled in, and it is the one that claims nothing.
    //
    // Not made again for a slot a check in this turn already said `unsafe` about: the run's read
    // made it, and a second look at the same bytes costs a model request to hear the same word
    // (OUTPUT-3). The reason that check wrote was not kept, so the prompt is drawn with the word
    // alone. Only `unsafe` is kept, because a check that could not be made says nothing about the
    // content.
    let mut spent = Usage::default();
    let mut waited = None;
    let mut unanswered = false;
    let checks = tools
        .permission_mode
        .get()
        .checks_before_promoting(tools.auto_vetting);
    let objected = checks && tools.objections.reuse(policy, &slot);
    let spec = match (checks, objected) {
        (false, _) | (true, true) => None,
        (true, false) => match policy.before_vetting(&slot, None, tools.slots) {
            Ok(spec) => Some(spec),
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
    };
    let (verdict, said) = match &spec {
        None if objected => (Verdict::Unsafe, (None, None)),
        None => (
            Verdict::Inconclusive("the check was not made"),
            (None, None),
        ),
        Some(spec) => {
            let asked_at = std::time::Instant::now();
            let checked = crate::vet::run(policy, &mut tools.chat, reporter, spec);
            waited = Some(crate::timing::Interval::since(asked_at));
            // A stop is not a verdict: no prompt, no refusal on the record, and the slot stays
            // as it was so the next turn can ask for it again.
            let checked = match checked {
                Ok(checked) => checked,
                Err(stopped) => return check_stopped(&slot, &stopped).waiting(waited),
            };
            spent = checked.usage;
            unanswered = !checked.completed;
            let said = released_reason(policy, checked.reason);
            (checked.verdict, said)
        }
    };

    // How many lines to tell the planner it got: the check's count where one was made, and what was
    // actually drawn where somebody was asked. The two are the same slot and agree.
    let counted = spec.as_ref().map_or(0, |spec| spec.lines());

    match release_output(
        policy,
        tools.slots,
        tools.permission_mode.get(),
        tools.auto_vetting,
        confirmer,
        &slot,
        verdict,
        said,
        counted,
    ) {
        Ok((text, counted, endorsed)) => {
            let lines = tally(counted, "line", "lines");
            Produced::new(
                text,
                format!("what {slot} held"),
                released_note(format!("{lines}, read"), endorsed),
            )
            .of_content()
            .checking(spent)
            .waiting(waited)
            .unanswered(unanswered)
        }
        Err(refused) => (*refused)
            .checking(spent)
            .waiting(waited)
            .unanswered(unanswered),
    }
}

/// The row note for one slot's bytes once released, saying who released them. The trail tells the
/// three apart too, but nobody reads it while a turn runs.
///
/// Matched whole so a fourth way of releasing fails to compile here rather than borrowing a
/// person's wording. Nothing the check wrote reaches these words.
fn released_note(done: String, endorsed: Endorsed) -> String {
    match endorsed {
        Endorsed::ByAPerson => done,
        Endorsed::ByASafeVerdict => format!("{done} without asking: a check found nothing"),
        Endorsed::ByBypassing => format!("{done} without asking or checking"),
    }
}

/// Which shape of output the planner may already read one call asks for: a page from a byte, or
/// the lines matching the patterns it gave.
///
/// Both are the same slot's own bytes, so both are taken inside the one gate below rather than
/// each opening the content for itself. Nothing here derives from a byte of that content: a page's
/// offset and a pattern are the planner's own arguments.
#[derive(Clone, Copy)]
enum Shown<'a> {
    Page(usize),
    Matches(&'a [crate::regex::Regex]),
}

/// The planner's patterns compiled, or the result to hand back saying why one would not.
///
/// Compiled before the slot is opened, as a search's are: a pattern that cannot be parsed is a
/// mistake in the call and is reported as one, with nothing read.
fn compiled_patterns(
    patterns: &[Labelled<String>],
) -> Result<Vec<crate::regex::Regex>, Box<Produced>> {
    let mut compiled = Vec::with_capacity(patterns.len());
    for pattern in patterns {
        let Ok(needle) = pattern.clone().into_trusted() else {
            return Err(Box::new(Produced::problem(
                "refused: a pattern must be your own words, not something you were handed",
            )));
        };
        if needle.is_empty() {
            return Err(Box::new(Produced::problem(
                "error: 'pattern' must not be empty, e.g. \"error|warning\"",
            )));
        }
        match crate::regex::Regex::compile(&needle) {
            Ok(regex) => compiled.push(regex),
            Err(problem) => {
                return Err(Box::new(Produced::problem(format!(
                    "error: that pattern will not compile: {problem}"
                ))));
            }
        }
    }
    Ok(compiled)
}

/// One result of output the planner may already read: a page from an offset, or the lines matching
/// a pattern, and no longer than a run's result may be.
///
/// The page keeps the slot's label on the way back, so what puts it in the planner's context is the
/// kernel's `present`, deciding from that label as it would have had the cap not cut it.
fn read_a_page<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    slot: &SlotId,
    shown: Shown<'_>,
) -> Produced {
    let content = match policy.resolve("read_output", slot, tools.slots) {
        Ok(content) => content,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let cap = tools.output_cap;
    let page = policy.render_in_place("read_output", &content, |text| match shown {
        Shown::Page(offset) => page_of(&text, slot, offset, cap),
        Shown::Matches(patterns) => matches_of(&text, slot, patterns, cap),
    });
    Produced::new(
        page,
        format!("what {slot} held"),
        match shown {
            Shown::Page(offset) => format!("from byte {offset}"),
            Shown::Matches(_) => "matching lines".to_owned(),
        },
    )
    .of_content()
}

/// How many matching lines one search of a slot answers with. The rest are counted and the byte
/// the next one starts at is named, so a narrower pattern or an offset carries on from there.
const OUTPUT_MATCH_CAP: usize = 100;

/// Lines of context either side of a match, fixed: this narrows a log down to where to page from
/// rather than standing in for reading it.
const OUTPUT_MATCH_CONTEXT: usize = 2;

/// The lines of `text` matching any of `patterns`, each numbered with its line and the byte it
/// starts at, with context either side, at most `OUTPUT_MATCH_CAP` matches and at most `cap`
/// bytes.
///
/// The byte numbers are the point of it: a match found here is read in full by a second call
/// naming that byte as `offset`, so a 200 KB log is narrowed without running anything again.
///
/// Nothing is decided here. This runs inside `render_in_place`, the gate that releases a slot's
/// content to a closure, and what it returns is labelled again by the kernel on the way back. The
/// driver reads none of it, and the route into this function was chosen from the slot's label and
/// the planner's own arguments before any byte was opened.
fn matches_of(text: &str, slot: &SlotId, patterns: &[crate::regex::Regex], cap: usize) -> String {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        lines.push((at, line.trim_end_matches('\n').trim_end_matches('\r')));
        at += line.len();
    }

    let matched: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, (_, line))| patterns.iter().any(|pattern| pattern.matches(line)))
        .map(|(index, _)| index)
        .collect();
    if matched.is_empty() {
        return format!(
            "(nothing in {slot} matches, out of {} in {} bytes.)",
            tally(lines.len(), "line", "lines"),
            text.len()
        );
    }

    let mut out = String::new();
    let mut written: Option<usize> = None;
    let mut drawn = 0;
    for index in matched.iter().copied().take(OUTPUT_MATCH_CAP) {
        if out.len() >= cap {
            break;
        }
        let wanted = index.saturating_sub(OUTPUT_MATCH_CONTEXT);
        let to = (index + OUTPUT_MATCH_CONTEXT).min(lines.len().saturating_sub(1));
        let mut block = String::new();
        if let Some(last) = written
            && wanted > last + 1
        {
            block.push_str("--\n");
        }
        let from = written.map_or(wanted, |last| wanted.max(last + 1));
        for (number, &(byte, line)) in lines.iter().enumerate().take(to + 1).skip(from) {
            let mark = if number == index { ':' } else { '-' };
            block.push_str(&format!("{}{mark}{byte}{mark} {line}\n", number + 1));
        }

        // A match's own context can be longer than what is left of `cap`, so the block built
        // above is bounded against the budget before it joins `out` rather than after: appending
        // it first and checking the total afterwards is the overshoot this guards against. The
        // first match still writes something, cut to a character boundary within the budget, on
        // the same footing `page_of` holds a page to at least one character: a cap smaller than
        // one match's context must not come back empty.
        if out.len() + block.len() > cap {
            if out.is_empty() {
                let mut end = cap.min(block.len());
                while end > 0 && !block.is_char_boundary(end) {
                    end -= 1;
                }
                out.push_str(&block[..end]);
                drawn += 1;
            }
            break;
        }
        out.push_str(&block);
        written = Some(to.max(index));
        drawn += 1;
    }

    let total = matched.len();
    let rest = if drawn < total {
        let next = lines[matched[drawn]].0;
        format!(
            ", and {} further back, the next from byte {next}",
            tally(total - drawn, "match", "matches")
        )
    } else {
        String::new()
    };
    format!(
        "{out}\n({} in {slot}, {drawn} shown{rest}. Each line is numbered with the byte it starts \
         at: read_output with that byte as offset gives the page from there.)",
        tally(total, "matching line", "matching lines")
    )
}

/// `text` from `offset`, at most `cap` bytes of it, with a line saying where it stopped.
///
/// Both ends land on a character boundary, as a sample's cut does, and a page holds at least one
/// character so a cap smaller than one cannot stop every page where it began.
fn page_of(text: &str, slot: &SlotId, offset: usize, cap: usize) -> String {
    let total = text.len();
    if offset >= total {
        return format!("({slot} holds {total} bytes, so nothing starts at byte {offset}.)");
    }
    let before = |mut at: usize| {
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let start = before(offset);
    let mut end = before(start.saturating_add(cap).min(total));
    if end == start {
        end = (start + 1..=total)
            .find(|at| text.is_char_boundary(*at))
            .unwrap_or(total);
    }
    let rest = if end == total {
        "which is the end of it".to_owned()
    } else {
        format!("read_output with offset {end} gives the next part")
    };
    format!(
        "{}\n\n(bytes {start} to {end} of {total} in {slot}; {rest}.)",
        &text[start..end]
    )
}

/// What [`read_in_the_result`] produced: the released text where there is one, and what the check
/// it made cost, so the turn reports a check made here as it reports one `read_output` makes.
pub(crate) struct ReadInTheResult {
    /// The slot's bytes at the label `read_output` gives them, or `None` where the slot stays
    /// quarantined.
    pub text: Option<Labelled<String>>,
    /// What the check spent. Zero where none was made.
    pub usage: Usage,
    /// How long the turn waited on the check. `None` where none was made.
    pub waited: Option<crate::timing::Interval>,
    /// Set where the person stopped the turn while the check ran. `text` is `None` then too, and
    /// the turn ends as stopped once the run's own result is recorded.
    pub stopped: Option<crate::outcome::Cancellation>,
    /// Whether a model request completed for the check: false where none was made, where the
    /// person stopped it, and where it could not be made (SESSION-40).
    pub completed: bool,
}

/// What a read is told when the person stopped the turn while its check ran, with the cost of the
/// attempts that completed.
///
/// The words are the driver's, and say nothing about the content or the check: only the person can
/// cause a stop. The reference can be asked for again, since nothing was refused. The stop is
/// carried beside the words so the turn ends as stopped rather than failed (TOOL-4).
fn check_stopped(subject: impl std::fmt::Display, stopped: &crate::vet::Stopped) -> Produced {
    let mut produced = Produced::problem(format!(
        "stopped: the turn was stopped before the check on {subject} finished, so nothing was \
         decided about it. {subject} was not refused and can be asked for again."
    ))
    .checking(stopped.usage);
    produced.cancelled = Some(crate::outcome::Cancellation {
        attempts: stopped.attempts,
    });
    produced
}

/// A refusal of a read the planner is told was kept back, with the line the person reads beside
/// it.
///
/// The planner's `text` is the same in every mode and says nothing about what decided. Where the
/// mode is bypass and auto-vetting is on, nobody was asked and a check's word answered, so the
/// person is also told that, and which setting lifts it. The condition is the two facts the
/// session was started with and never the verdict, the reason or a byte of the content, so an
/// unsafe word and a check that could not be made read alike. Elsewhere a person was asked and
/// said no, so there is nothing to add.
fn kept_back(mode: crate::PermissionMode, auto_vetting: bool, text: String) -> Produced {
    match mode == crate::PermissionMode::Bypass && auto_vetting {
        true => {
            let note = format!("{text} {}", crate::report::vetting_kept_back());
            Produced::refused_with_a_note(text, note)
        }
        false => Produced::problem(text),
    }
}

/// What a run that asked to read what it printed is handed in the same result, where the answer
/// `read_output` would get does not need a person: bypassing permissions with no screening asked
/// for, or a check whose word is `safe` where screening was asked for.
///
/// The release is `read_output`'s own, down to the check that precedes it, the confirmer the mode
/// answers and the trail entry crediting whoever released, so the label and the record cannot
/// differ between the two routes and only the round between them is gone. The planner's flag
/// decides whether this is called, and it was set before the output existed.
///
/// Where screening was asked for, any word other than `safe` leaves the slot quarantined and
/// draws no prompt: the planner has not seen the size or the shape of what was printed, and the
/// person is asked when it calls `read_output`, as it would have been without the flag. The same
/// holds where the check could not be made. `text` is `None` for each of those, and for a release
/// the kernel or the confirmer refused, which leaves the slot as it would have been had the
/// planner not asked.
///
/// The only branch on the check is on its word, the one `release_output` already makes. Nothing
/// here reads a byte of the output or the reason the check wrote.
#[expect(
    clippy::too_many_arguments,
    reason = "the policy, confirmer, chat and reporter are `&mut` borrows of the turn while the rest is read-only state of one read, so bundling them would split one borrow across two values"
)]
pub(crate) fn read_in_the_result<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    slots: &SlotStore,
    mode: crate::PermissionMode,
    auto_vetting: bool,
    confirmer: &mut C,
    chat: &mut Chat<'_>,
    reporter: &mut R,
    objections: &mut Objections,
    slot: &SlotId,
) -> ReadInTheResult {
    let nothing = |text| ReadInTheResult {
        text,
        usage: Usage::default(),
        waited: None,
        stopped: None,
        completed: false,
    };
    if !mode.checks_before_promoting(auto_vetting) {
        return nothing(
            release_output(
                policy,
                slots,
                mode,
                auto_vetting,
                confirmer,
                slot,
                Verdict::Inconclusive("the check was not made"),
                (None, None),
                0,
            )
            .ok()
            .map(|(text, _, _)| text),
        );
    }

    let Ok(spec) = policy.before_vetting(slot, None, slots) else {
        return nothing(None);
    };
    let asked_at = std::time::Instant::now();
    let checked = crate::vet::run(policy, chat, reporter, &spec);
    let waited = Some(crate::timing::Interval::since(asked_at));
    // A stop leaves the slot quarantined and unrefused, and the turn ends stopped.
    let checked = match checked {
        Ok(checked) => checked,
        Err(stopped) => {
            return ReadInTheResult {
                text: None,
                usage: stopped.usage,
                waited,
                stopped: Some(crate::outcome::Cancellation {
                    attempts: stopped.attempts,
                }),
                completed: false,
            };
        }
    };
    let usage = checked.usage;
    let completed = checked.completed;
    // The word, and only when it is `unsafe`, is kept for `read_output` to reuse. A check that
    // could not be made is not kept: it says nothing about the content.
    if checked.verdict == Verdict::Unsafe {
        objections.note(slot);
    }
    // Before `release_output`, which would put anything but `safe` to the person: nobody is
    // shown this output here.
    let text = if checked.verdict.is_safe() {
        release_output(
            policy,
            slots,
            mode,
            auto_vetting,
            confirmer,
            slot,
            checked.verdict,
            (None, None),
            spec.lines(),
        )
        .ok()
        .map(|(text, _, _)| text)
    } else {
        // Nothing is returned and the planner is told only that `read: true` did not return the
        // output, so the person is told here what decided, as `read_output` tells them beside its
        // refusal. The condition is the mode and the setting, never the word the check gave.
        if mode == crate::PermissionMode::Bypass && auto_vetting {
            reporter.notice(crate::report::vetting_kept_back());
        }
        None
    };
    ReadInTheResult {
        text,
        usage,
        waited,
        stopped: None,
        completed,
    }
}

/// What a check said about content, released for the screen an approval is read on, with the label
/// it was released under. The label travels beside the words (LABEL-10).
fn released_reason<S: Sink>(
    policy: &mut Policy<'_, S>,
    reason: Option<Labelled<String>>,
) -> (Option<String>, Option<String>) {
    match reason {
        Some(reason) => {
            let label = reason.label().to_string();
            let proof = policy.authorise_display_release("what a check said about content");
            (Some(reason.declassify(&proof)), Some(label))
        }
        None => (None, None),
    }
}

/// Put one slot a program printed to whoever answers for reading it, and hand the planner a new
/// value if they agree. The half of `read_output` that comes after the reference is accepted and
/// any check has spoken, returning the text, how many lines it holds and who released it, or the
/// result to hand back instead.
#[expect(
    clippy::too_many_arguments,
    reason = "the policy and confirmer are `&mut` borrows of the turn while the rest is read-only state of one read, so bundling them would split one borrow across two values"
)]
fn release_output<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    slots: &SlotStore,
    mode: crate::PermissionMode,
    auto_vetting: bool,
    confirmer: &mut C,
    slot: &SlotId,
    verdict: Verdict,
    (reason, reason_label): (Option<String>, Option<String>),
    mut counted: usize,
) -> Result<(Labelled<String>, usize, Endorsed), Box<Produced>> {
    // Who the trail is credited to, which the mode decides along with the verdict. The one branch
    // on a verdict that decides more than which sentence a person reads first is inside it, and it
    // is reachable only where somebody turned auto-vetting on: `Safe` is the only word that answers
    // there, and unsafe, and every way a check can fail to complete, fall through to the prompt
    // with the banner they would have carried anyway. As `vet_content`, because the grant is the
    // same shape on both routes: one slot, once, with no rule written.
    let endorsed = mode.released_by(auto_vetting, verdict);

    // A `match` rather than an `if`, so a fourth way of endorsing cannot be added and default to
    // skipping the prompt: a new variant stops compiling here until somebody says which it is.
    // Bypassing with nothing screening asks in the sense that the request is built and put to a
    // confirmer; what answers it is the mode, which is why it is credited as itself below.
    let ask = match endorsed {
        Endorsed::ByAPerson | Endorsed::ByBypassing => true,
        Endorsed::ByASafeVerdict => false,
    };

    if ask {
        // Released for the person to read, which is the whole of what a prompt here is for. A
        // display release cannot feed an effect, and both of these feed a screen. Inside the branch
        // because there is no screen on the other one: releasing for a display nobody is looking at
        // would put a declassification in the trail with no audience for it.
        //
        // Bypassing is the standing exception, as it was before screening existed: the mode answers
        // this without drawing anything, so the branch means a prompt would be drawn wherever there
        // is anybody to draw it for, and never that somebody read what was released.
        // The count is taken here, inside the reshape, rather than off the bytes afterwards.
        // Counting released bytes is the read LABEL-6 refuses, and it is the same question the
        // kernel already answers for a slot: how much there is, not what it says.
        //
        // The characters that draw as nothing are written out in the same reshape, and counted
        // there for the same reason. What the person reads is the copy with them spelled, and
        // what the planner receives below is the slot's own bytes, which this does not touch.
        let (shown, lines, hidden, label) = {
            let content = match policy.resolve("read_output", slot, slots) {
                Ok(content) => content,
                Err(denial) => {
                    return Err(Box::new(Produced::problem(format!("refused: {denial}"))));
                }
            };
            let measured = policy.render_in_place("read_output", &content, |text| {
                let lines = text.lines().count();
                let spelled = bravebot_core::invisible::spelled(text);
                (spelled.text, lines, spelled.hidden)
            });
            let label = measured.label().to_string();
            let proof =
                policy.authorise_display_release("command output the planner asked to read");
            let (shown, lines, hidden) = measured.declassify(&proof);
            (shown, lines, hidden, label)
        };

        counted = lines;

        let request = crate::confirm::OutputRequest {
            command: slots.command_of(slot).unwrap_or("a command").to_string(),
            output: shown,
            label,
            lines,
            hidden,
            reference: slot.to_string(),
            verdict,
            reason,
            reason_label,
        };

        // Says the bytes are not coming and not who decided that. Under bypass with screening asked
        // for, nobody was asked and a check answered in their place, so naming the user would be a
        // false claim and naming the check would hand the planner the word it must not read.
        if confirmer.confirm_read_output(&request) == Decision::Reject {
            policy.refuse_unapproved_release("read_output", &slot.to_string());
            let text = format!(
                "refused: {slot} was kept back from you. Do not ask for it again. Work with what \
                 you have, or say in your reply what you needed from it."
            );
            return Err(Box::new(kept_back(mode, auto_vetting, text)));
        }
    }

    // The endorsement is what makes these bytes readable, and it is bound to this exact reference
    // whichever of the two answered.
    policy.issue_grant("read_output", "ref", slot.to_string());

    match policy.read_output(slot, slots, endorsed) {
        Ok(text) => Ok((text, counted, endorsed)),
        Err(denial) => Err(Box::new(Produced::problem(format!("refused: {denial}")))),
    }
}

/// Put one quarantined slot of text to whoever answers for reading it, and hand the planner the
/// text if they agree. The half of `vet_content` that comes after the reference is accepted and
/// the check has spoken, returning the text, how many lines it holds and who released it, or the
/// result to hand back instead.
///
/// Shared by `vet_content` and by a read made in the result of the call that produced the slot, so
/// the prompt, the grant and the kernel's release are one piece of code and the label and the trail
/// cannot differ between the two routes. `expects` is what the planner claimed the slot holds, and
/// is `None` where no claim was made, which the prompt shows as empty.
#[expect(
    clippy::too_many_arguments,
    reason = "the policy and confirmer are `&mut` borrows of the turn while the rest is read-only state of one read, so bundling them would split one borrow across two values"
)]
fn release_vetted_text<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    slots: &SlotStore,
    mode: crate::PermissionMode,
    auto_vetting: bool,
    confirmer: &mut C,
    slot: &SlotId,
    verdict: Verdict,
    said: Option<Labelled<String>>,
    spec: Option<&bravebot_core::vetting::VettingSpec>,
    expects: Option<&Labelled<String>>,
    beside: Beside,
) -> Result<(Labelled<String>, usize, Endorsed), Box<Produced>> {
    // Who the trail is credited to, which the mode decides along with the verdict. The one branch
    // on a verdict that decides more than which sentence a person reads first is inside it, and it
    // is reachable only where somebody turned auto-vetting on: `Safe` is the only word that answers
    // there, and unsafe, and every way a check can fail to complete, fall through to the prompt
    // with the banner they would have carried anyway. Written down as the third known cost in
    // `docs/specs/labels.md`.
    let endorsed = mode.released_by(auto_vetting, verdict);

    // How many lines the planner is told it got: the check's count where one was made, and what
    // the prompt was built over where none was. The two are the same slot and agree.
    let mut counted = spec.map_or(0, |spec| spec.lines());

    // A `match` rather than an `if`, so a fourth way of endorsing cannot be added and default to
    // skipping the prompt: a new variant stops compiling here until somebody says which it is.
    // Bypassing with nothing screening asks in the sense that the request is built and put to a
    // confirmer; what answers it is the mode, which is why it is credited as itself below.
    let ask = match endorsed {
        Endorsed::ByAPerson | Endorsed::ByBypassing => true,
        Endorsed::ByASafeVerdict => false,
    };
    if ask {
        // Released for the person to read, which is the whole of what a prompt here is for. A
        // display release cannot feed an effect, and both of these feed a screen. Inside the
        // branch because there is no screen on the other one: releasing for a display nobody is
        // looking at would put a declassification in the trail with no audience for it.
        //
        // Bypassing is the standing exception, as it was before screening existed: the mode answers
        // this without drawing anything, so the branch means a prompt would be drawn wherever there
        // is anybody to draw it for, and never that somebody read what was released.
        // Counted inside the reshape, as `read_output` counts what it shows, and for the same
        // reason: the bytes below are released for a screen and nothing in this crate may read
        // them, a count included.
        let (shown, lines, hidden, label) = {
            let content = match policy.resolve("vet_content", slot, slots) {
                Ok(content) => content,
                Err(denial) => {
                    return Err(Box::new(Produced::problem(format!("refused: {denial}"))));
                }
            };
            // Counted and spelled in the reshape, as `release_output` does and for the same
            // reason: the person reads the copy with the characters that draw as nothing written
            // out, and the planner is handed the slot's own bytes.
            let measured = policy.render_in_place("vet_content", &content, |text| {
                let lines = text.lines().count();
                let spelled = bravebot_core::invisible::spelled(text);
                (spelled.text, lines, spelled.hidden)
            });
            let label = measured.label().to_string();
            let proof = policy.authorise_display_release("content the planner asked to be shown");
            let (shown, lines, hidden) = measured.declassify(&proof);
            (shown, lines, hidden, label)
        };
        let (reason, reason_label) = released_reason(policy, said);

        // The origin comes off the kernel's own record either way, rather than off the spec on
        // the branch that has one: a slot's address is untrusted content, so reading it out here
        // would be a second, uncounted way for one to leave `bravebot-core`. The gate records the
        // release, and it answers what the spec was built from.
        //
        // The words are released for a display, not through `before_vetting`, which releases
        // them whatever their label and checks nothing about it. No second model is sent them on
        // this branch, so what is left is the person's own content going to the person's own
        // screen, which is what a display release is for. It reaches that screen and stops: no
        // gate reads it.
        //
        // A processor's output is recorded as the planner's copy of its origin, which names
        // references. The person has no use for `ref:0`, so they are put back as the files they
        // name, as the reporter's line for the same slot does (VET-1).
        let origin = policy.release_where_a_slot_came_from(slot, slots);
        let origin = name_files(&origin, &policy.names_for_display(slots));
        let expects = match (spec, expects) {
            (Some(spec), _) => spec.expects().unwrap_or_default().to_string(),
            (None, Some(expects)) => {
                let proof =
                    policy.authorise_display_release("what the planner expects a slot to hold");
                expects.clone().declassify(&proof)
            }
            (None, None) => String::new(),
        };

        counted = lines;

        let request = crate::confirm::VetRequest {
            origin,
            expects,
            content: shown,
            label: Some(label),
            lines,
            hidden,
            verdict,
            reason,
            reason_label,
            picture: None,
        };

        // Says the bytes are not coming and not who decided that, for the reason `read_output`
        // carries: under bypass with screening asked for the answer came from a check rather than
        // from a person, and neither fact is the planner's to be told.
        if confirmer.confirm_vetted_read(&request) == Decision::Reject {
            policy.refuse_unapproved_release("vet_content", &slot.to_string());
            let text = format!(
                "refused: {slot} was kept back from you. Do not ask for it again. Work with what \
                 you have, {}or say in your reply what you needed from it.",
                pass_to_a_processor(beside, slot)
            );
            return Err(Box::new(kept_back(mode, auto_vetting, text)));
        }
    }

    // The endorsement is what makes these bytes readable, and it is bound to this exact reference
    // whichever of the two answered.
    policy.issue_grant("vet_content", "ref", slot.to_string());

    match policy.promote_vetted(slot, slots, endorsed) {
        Ok(text) => Ok((text, counted, endorsed)),
        Err(denial) => Err(Box::new(Produced::problem(format!("refused: {denial}")))),
    }
}

/// The result of a `vet_content` call that released text.
fn vetted_text(
    slot: &SlotId,
    text: Labelled<String>,
    counted: usize,
    endorsed: Endorsed,
) -> Produced {
    let lines = tally(counted, "line", "lines");
    Produced::new(
        text,
        format!("what {slot} held"),
        released_note(format!("{lines}, read"), endorsed),
    )
    .of_content()
}

/// What a call that asked to read what it returned is handed in the same result, where the answer
/// `vet_content` would get does not need a person: bypassing permissions with no screening asked
/// for, or a check whose word is `safe` where screening was asked for.
///
/// The release is `vet_content`'s own, down to the check that precedes it, the confirmer the mode
/// answers and the trail entry crediting whoever released, so the label and the record cannot
/// differ between the two routes and only the round between them is gone. The planner's flag
/// decides whether this is called, and it was set before the result existed. The check is made
/// without a claim about what the slot holds, as the one `read_in_the_result` makes is.
///
/// Where screening was asked for, any word other than `safe` leaves the slot quarantined and
/// draws no prompt: the planner has not seen the size or the shape of what was returned, and the
/// person is asked when it calls `vet_content`, as it would have been without the flag. The same
/// holds where the check could not be made. The slot is always text: a server's reply that holds
/// a picture is presented as references only and is never read in its result (MCP-16).
///
/// The only branch on the check is on its word, the one `release_vetted_text` already makes.
/// Nothing here reads a byte of the result, the reason the check wrote, or whether the server
/// called the result a failure.
#[expect(
    clippy::too_many_arguments,
    reason = "the policy, confirmer, chat and reporter are `&mut` borrows of the turn while the rest is read-only state of one read, so bundling them would split one borrow across two values"
)]
pub(crate) fn vet_in_the_result<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    slots: &SlotStore,
    mode: crate::PermissionMode,
    auto_vetting: bool,
    confirmer: &mut C,
    chat: &mut Chat<'_>,
    reporter: &mut R,
    objections: &mut Objections,
    slot: &SlotId,
    beside: Beside,
) -> ReadInTheResult {
    let nothing = |text| ReadInTheResult {
        text,
        usage: Usage::default(),
        waited: None,
        stopped: None,
        completed: false,
    };
    let release = |policy: &mut Policy<'_, S>,
                   confirmer: &mut C,
                   verdict: Verdict,
                   spec: Option<&bravebot_core::vetting::VettingSpec>| {
        release_vetted_text(
            policy,
            slots,
            mode,
            auto_vetting,
            confirmer,
            slot,
            verdict,
            None,
            spec,
            None,
            beside,
        )
        .ok()
        .map(|(text, _, _)| text)
    };
    if !mode.checks_before_promoting(auto_vetting) {
        return nothing(release(
            policy,
            confirmer,
            Verdict::Inconclusive("the check was not made"),
            None,
        ));
    }

    let Ok(spec) = policy.before_vetting(slot, None, slots) else {
        return nothing(None);
    };
    let asked_at = std::time::Instant::now();
    let checked = crate::vet::run(policy, chat, reporter, &spec);
    let waited = Some(crate::timing::Interval::since(asked_at));
    // A stop leaves the slot quarantined and unrefused, and the turn ends stopped.
    let checked = match checked {
        Ok(checked) => checked,
        Err(stopped) => {
            return ReadInTheResult {
                text: None,
                usage: stopped.usage,
                waited,
                stopped: Some(crate::outcome::Cancellation {
                    attempts: stopped.attempts,
                }),
                completed: false,
            };
        }
    };
    let usage = checked.usage;
    let completed = checked.completed;
    // Kept for `vet_content` to reuse, as `read_in_the_result` keeps it for `read_output`.
    if checked.verdict == Verdict::Unsafe {
        objections.note(slot);
    }
    // Before `release_vetted_text`, which would put anything but `safe` to the person: nobody is
    // shown this result here.
    let text = if checked.verdict.is_safe() {
        release(policy, confirmer, checked.verdict, Some(&spec))
    } else {
        None
    };
    ReadInTheResult {
        text,
        usage,
        waited,
        stopped: None,
        completed,
    }
}

/// Check one quarantined slot, show it to the user with what the check said, and give it to the
/// planner if they agree.
///
/// The second place bytes cross out of quarantine into the planner's context on a person's
/// say-so, and the order is what makes that defensible. The check runs first, so its word is on
/// the screen when the question is asked. The content is released for display and put in front of
/// the person in full. Only then, if they agree, does an endorsement exist for the kernel to
/// consume.
///
/// **The verdict decides who answers, and nothing else.** With auto-vetting off, which is the
/// default and every session nobody turned it on for, the word travels to the prompt, the prompt
/// draws it, and what decides is the person's answer. With auto-vetting on, a verdict of `safe`
/// answers in their place and every other verdict falls back to the same prompt with the same
/// warning on it. What a verdict never decides is which slot, what label, or how long: those are
/// the same down both paths.
///
/// Unlike `read_output` this covers any quarantined slot, including a file, and it is still not a
/// second answer to what a file is worth: nothing written here reaches the trust map, so a later
/// read of the same path is quarantined exactly as it is today.
fn vet_content<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let Some(named) = argument(arguments, "ref") else {
        return Produced::problem(
            "error: 'ref' is required and must be a reference name, e.g. \"ref:5\"",
        );
    };
    let Some(expects) = argument(arguments, "expects") else {
        return Produced::problem(
            "error: 'expects' is required and must say what you think this holds, e.g. \"the \
             release notes for version 2\"",
        );
    };

    // Read once for the whole call: the check, the release and the picture's copy are one decision,
    // and a mode changed part way through must not give them different answers.
    let mode = tools.permission_mode.get();
    let slot = match accept_named_reference(
        policy,
        tools.slots,
        "vet_content",
        "ref",
        &named,
        NamesToList::Text,
    ) {
        Ok(slot) => slot,
        Err(refusal) => return Produced::problem(refusal),
    };

    // A reference to a file the driver reserved and never opened has no bytes yet, and there is
    // nothing to check or to show until it does. Opened here rather than refused, because the
    // planner naming one is the ordinary way to ask about a file it may not read.
    if let Err(refusal) = materialise(
        policy,
        tools.workspace,
        tools.slots,
        "vet_content",
        std::slice::from_ref(&slot),
    ) {
        return Produced::problem(refusal);
    }

    // The driver's own record of what kind of file the slot holds, from its table of extensions at
    // the read. Which way this goes is never decided by the bytes.
    let picture = tools.slots.picture_of(&slot).map(str::to_string);

    // Before the gate below rather than inside it: a model the roster lists as not taking this
    // kind of file can neither check it nor be given it, and bypassing with no screening makes no
    // check to refuse it in. The list asked for is the one for the model this turn runs on, which
    // is also the model the check runs on.
    let model = tools.chat.model.unwrap_or(&tools.chat.config.default_model);
    let listed = tools.chat.config.listed_inputs(model);
    if let Err(denial) = policy.before_promoting(&slot, tools.slots, listed) {
        return Produced::problem(format!("refused: {denial}"));
    }

    // The second opinion, before the question rather than after it.
    //
    // Skipped only where nothing would read the word: bypassing with no screening asked for answers
    // this question yes without showing anybody anything, so a check there is a model call nobody
    // reads. A verdict is still filled in, and it is the one that claims nothing. The same gate
    // `read_output` carries, and the exemption `docs/specs/permission-modes.md` MODE-4 states from
    // the other side. The vouch offer in `read_file` keeps the plain one: no screening answers it,
    // so under bypass nothing reads that word whatever was asked for.
    //
    // Not made again for a slot a check in this turn already said `unsafe` about, which is the
    // check a `fetch_url` or a server's tool made for `read: true` (OUTPUT-3). The reason that
    // check wrote was not kept, so the prompt is drawn with the word alone.
    let checks = mode.checks_before_promoting(tools.auto_vetting);
    let objected = checks && tools.objections.reuse(policy, &slot);
    let spec = match (checks, objected) {
        (false, _) | (true, true) => None,
        (true, false) => match policy.before_vetting(&slot, Some(&expects), tools.slots) {
            Ok(spec) => Some(spec),
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
    };

    let mut spent = Usage::default();
    let mut waited = None;
    let mut unanswered = false;
    let mut said = None;
    let verdict = match &spec {
        None if objected => Verdict::Unsafe,
        None => Verdict::Inconclusive("the check was not made"),
        Some(spec) => {
            let asked_at = std::time::Instant::now();
            let checked = crate::vet::run(policy, &mut tools.chat, reporter, spec);
            waited = Some(crate::timing::Interval::since(asked_at));
            // A stop is not a verdict, so nobody is asked and nothing is refused (`read_output`).
            let checked = match checked {
                Ok(checked) => checked,
                Err(stopped) => return check_stopped(&slot, &stopped).waiting(waited),
            };
            spent = checked.usage;
            unanswered = !checked.completed;
            said = checked.reason;
            checked.verdict
        }
    };

    // Text is released by the route a read in the result takes as well, so the prompt, the grant
    // and the kernel's release are written once. A picture is put in front of the person as a copy
    // and attached, which only this call does.
    let Some(media) = picture.as_deref() else {
        return match release_vetted_text(
            policy,
            tools.slots,
            mode,
            tools.auto_vetting,
            confirmer,
            &slot,
            verdict,
            said,
            spec.as_ref(),
            Some(&expects),
            tools.beside,
        ) {
            Ok((text, counted, endorsed)) => vetted_text(&slot, text, counted, endorsed)
                .checking(spent)
                .waiting(waited)
                .unanswered(unanswered),
            Err(refusal) => (*refusal)
                .checking(spent)
                .waiting(waited)
                .unanswered(unanswered),
        };
    };

    // Who the trail is credited to, which the mode decides along with the verdict. The one branch
    // on a verdict that decides more than which sentence a person reads first is inside it, and it
    // is reachable only where somebody turned auto-vetting on: `Safe` is the only word that answers
    // there, and unsafe, and every way a check can fail to complete, fall through to the prompt
    // with the banner they would have carried anyway. Written down as the third known cost in
    // `docs/specs/labels.md`.
    let endorsed = mode.released_by(tools.auto_vetting, verdict);

    // A `match` rather than an `if`, so a fourth way of endorsing cannot be added and default to
    // skipping the prompt: a new variant stops compiling here until somebody says which it is.
    // Bypassing with nothing screening asks in the sense that the request is built and put to a
    // confirmer; what answers it is the mode, which is why it is credited as itself below.
    let ask = match endorsed {
        Endorsed::ByAPerson | Endorsed::ByBypassing => true,
        Endorsed::ByASafeVerdict => false,
    };
    // Held until the prompt below has closed, which is what removes the copy however it closes.
    let mut _copy = None;
    if ask {
        // Released for the person to read, which is the whole of what a prompt here is for. A
        // display release cannot feed an effect, and both of these feed a screen. Inside the
        // branch because there is no screen on the other one: releasing for a display nobody is
        // looking at would put a declassification in the trail with no audience for it.
        //
        // Bypassing is the standing exception, as it was before screening existed: the mode answers
        // this without drawing anything, so the branch means a prompt would be drawn wherever there
        // is anybody to draw it for, and never that somebody read what was released.
        // Counted inside the reshape, as `read_output` counts what it shows, and for the same
        // reason: the bytes below are released for a screen and nothing in this crate may read
        // them, a count included.
        //
        // A picture is put in front of the person as a copy to open rather than as text, and not
        // at all where the mode answers without drawing anything: bypassing answers this from the
        // verdict alone, so a copy there would be a file written for nobody to open, and a machine
        // naming no cache directory would refuse what the mode answers yes.
        let shown_picture = if mode == crate::PermissionMode::Bypass {
            None
        } else {
            match copy_a_picture(policy, tools.slots, tools.cache, &slot, media) {
                Ok((copy, bytes)) => {
                    let shown = crate::confirm::PictureShown {
                        path: copy.path().to_path_buf(),
                        media: media.to_string(),
                        bytes,
                    };
                    _copy = Some(copy);
                    Some(shown)
                }
                Err(detail) => {
                    return Produced::refused_with_a_note(
                        format!(
                            "refused: {slot} was kept back from you. Do not ask for it again. \
                             Work with what you have, {}or say in your reply what you needed \
                             from it.",
                            pass_to_a_processor(tools.beside, &slot)
                        ),
                        detail,
                    )
                    .checking(spent)
                    .waiting(waited)
                    .unanswered(unanswered);
                }
            }
        };
        let (reason, reason_label) = released_reason(policy, said);

        // The origin comes off the kernel's own record either way, rather than off the spec on
        // the branch that has one: a slot's address is untrusted content, so reading it out here
        // would be a second, uncounted way for one to leave `bravebot-core`. The gate records the
        // release, and it answers what the spec was built from.
        //
        // The words are released for a display, not through `before_vetting`, which releases
        // them whatever their label and checks nothing about it. No second model is sent them on
        // this branch, so what is left is the person's own content going to the person's own
        // screen, which is what a display release is for. It reaches that screen and stops: no
        // gate reads it.
        //
        // A processor's output is recorded as the planner's copy of its origin, which names
        // references. The person has no use for `ref:0`, so they are put back as the files they
        // name, as the reporter's line for the same slot does (VET-1).
        let origin = policy.release_where_a_slot_came_from(&slot, tools.slots);
        let origin = name_files(&origin, &policy.names_for_display(tools.slots));
        let expects = match &spec {
            // `expects` is never absent on this route: the argument is required above, and this
            // is the one entry point into a check that carries what the planner claimed.
            Some(spec) => spec.expects().unwrap_or_default().to_string(),
            None => {
                let proof =
                    policy.authorise_display_release("what the planner expects a slot to hold");
                expects.clone().declassify(&proof)
            }
        };

        let request = crate::confirm::VetRequest {
            origin,
            expects,
            content: String::new(),
            label: None,
            lines: 0,
            hidden: 0,
            verdict,
            reason,
            reason_label,
            picture: shown_picture,
        };

        // Says the bytes are not coming and not who decided that, for the reason `read_output`
        // carries: under bypass with screening asked for the answer came from a check rather than
        // from a person, and neither fact is the planner's to be told.
        if confirmer.confirm_vetted_read(&request) == Decision::Reject {
            policy.refuse_unapproved_release("vet_content", &slot.to_string());
            let text = format!(
                "refused: {slot} was kept back from you. Do not ask for it again. Work with what \
                 you have, {}or say in your reply what you needed from it.",
                pass_to_a_processor(tools.beside, &slot)
            );
            return kept_back(mode, tools.auto_vetting, text)
                .checking(spent)
                .waiting(waited)
                .unanswered(unanswered);
        }
    }

    // The prompt has closed, so the copy goes before anything is promoted.
    drop(_copy);

    // The endorsement is what makes these bytes readable, and it is bound to this exact reference
    // whichever of the two answered.
    policy.issue_grant("vet_content", "ref", slot.to_string());

    match policy.promote_vetted_picture(&slot, tools.slots, endorsed) {
        // The result is the driver's words and not the file: a picture is looked at, and a data
        // URI in the text of a result is read as characters. The turn attaches it after the round's
        // results, in a message of its own (VET-4).
        Ok(attached) => {
            let kind = kind_of_file(media);
            Produced::new(
                Labelled::trusted(format!(
                    "{slot} was let through. It is {kind}, and it is attached to the message \
                     after these results, where you can look at it."
                )),
                format!("what {slot} held"),
                released_note(format!("{kind}, attached"), endorsed),
            )
            .attaching(attached)
            .checking(spent)
            .waiting(waited)
            .unanswered(unanswered)
        }
        Err(denial) => Produced::problem(format!("refused: {denial}")),
    }
}

/// How a picture or a PDF is named to the planner and to the person watching.
pub(crate) fn kind_of_file(media: &str) -> &'static str {
    match media == bravebot_core::vetting::PDF {
        true => "a PDF",
        false => "a picture",
    }
}

/// The extension a copy of a file of this media type is written under, from the attachable table.
fn extension_for(media: &str) -> &'static str {
    bravebot_filetype::by_name::ATTACHABLE
        .iter()
        .find(|(_, named)| *named == media)
        .map_or("bin", |(extension, _)| *extension)
}

/// The bytes a picture's data URI encodes.
///
/// The URI is this client's own encoding, made at the read or, for a server's picture, by the
/// transport, which takes only base64 in its canonical form, so this cannot fail on anything a
/// picture holds.
fn picture_bytes(uri: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    let encoded = uri.split_once(',').map_or("", |(_, encoded)| encoded);
    base64::engine::general_purpose::STANDARD.decode(encoded)
}

/// Write the picture `slot` holds to a copy the person can open, with how many bytes it holds.
///
/// The bytes are the ones the slot's data URI encodes, turned back inside the kernel's reshape and
/// released at a gate of their own, which the trail records as a copy written for the person to
/// open. The URI is this client's own encoding, made at the read or, for a server's picture, by the
/// transport, which takes only base64 in its canonical form, so turning it back cannot fail on
/// anything the picture holds. An `Err` is the driver's sentence for the person watching.
fn copy_a_picture<S: Sink>(
    policy: &mut Policy<'_, S>,
    slots: &SlotStore,
    cache: Option<&std::path::Path>,
    slot: &SlotId,
    media: &str,
) -> Result<(crate::vet::PictureCopy, usize), String> {
    let Some(cache) = cache else {
        return Err(format!(
            "{slot} holds {}, and this machine names no cache directory to put a copy in for you \
             to open, so it was kept back",
            kind_of_file(media)
        ));
    };
    let content = policy
        .resolve("vet_content", slot, slots)
        .map_err(|denial| format!("refused: {denial}"))?;
    let decoded = policy.render_in_place("vet_content", &content, |uri| {
        let bytes = picture_bytes(&uri).unwrap_or_default();
        let size = bytes.len();
        (bytes, size)
    });
    let proof = policy.authorise_a_copy_of_a_picture(slot, media);
    let (bytes, size) = decoded.declassify(&proof);
    let copy =
        crate::vet::PictureCopy::write(cache, extension_for(media), &bytes).map_err(|error| {
            format!(
                "{slot} holds {}, and a copy could not be written for you to open ({error}), so \
                 it was kept back",
                kind_of_file(media)
            )
        })?;
    Ok((copy, size))
}

/// The record of lines somebody asked to be remembered past this session, for this workspace.
///
/// `None` where this session has none, which is two cases with one answer: a turn with nobody to
/// put a prompt to, and a machine that names no state directory. Both mean the record says nothing
/// and every run asks, which is what a session did before the key existed.
///
/// A session that adds nothing to `~/.bravebot` is not one of them: it still honours what an
/// earlier session recorded, for the reason it still reads the model and the theme, and what it
/// does not do is add to it ([`crate::remembered::may_be_added_to`]).
///
/// Keyed by the workspace root rather than by whatever directory this call would run in. A line is
/// only ever recorded where it runs at the root, so the root is the tree the person answered about.
fn remembered_record(tools: &Tools<'_>) -> Option<crate::remembered::Store> {
    tools.remembering?;
    Some(crate::remembered::Store::new(
        tools.home?,
        tools.workspace.root(),
    ))
}

/// The directories a file that a bare program name resolves to may not lie in (CMDLINE-8): those
/// the session opened ([`crate::workspace::Workspace::unproven_directories`]), the ones a stage
/// was let write with `request_path` (in the same set), the ones a remembered grant lets a
/// stage write, such as the `bin` of a virtualenv outside the project, and the ones an earlier
/// process let a stage write ([`bravebot_sandbox::programs::recorded_earlier`]).
///
/// The toolchain lists ([`bravebot_sandbox::toolchain::Toolchain`]) are not added. An install is
/// read and never written, and a cache is not a directory a program is looked up in, so neither can
/// hold a file that a name resolves to. Each listed directory is a path the driver computed from
/// the record and the file system, and no byte a program printed is looked at.
fn unproven_directories(tools: &Tools<'_>) -> Vec<std::path::PathBuf> {
    let mut directories = tools.workspace.unproven_directories();
    // Read where the confinement reads the same record, and for the same reason: a session with
    // nobody to put a prompt to has no grant of its own.
    if let (Some(home), Some(session), Some(profile)) =
        (tools.home, tools.remembering, tools.profile)
    {
        for granted in crate::reach::Store::new(home).written_directories(
            session,
            tools.workspace.root(),
            profile,
            Some(home),
        ) {
            if !directories.contains(&granted) {
                directories.push(granted);
            }
        }
    }
    // What the lookups of a program bravebot starts leave out because an earlier process recorded
    // it, read from the state directory when the process began. This session may never have opened
    // the directory, so the set the workspace holds does not have it.
    for earlier in bravebot_sandbox::programs::recorded_earlier() {
        if !directories.contains(&earlier) {
            directories.push(earlier);
        }
    }
    directories
}

/// What a `deny` rule covering a command line says back to the planner.
///
/// One wording for the two gates that raise it, the compiler's, which answers before the program
/// is looked for, and the plan's, which also covers where a redirection would land. What the
/// planner is being told is the same thing either way: a person decided this in advance, so there
/// is nothing to rewrite and nothing to substitute.
fn refused_by_a_rule(denial: &bravebot_core::policy::Denial) -> Produced {
    Produced::problem(format!(
        "refused: {denial}. Do not retry, and do not look for another program that would \
         do the same thing: say in your reply what you needed it for."
    ))
}

/// What a line is refused with before it runs, because the line itself carries a credential.
///
/// Both halves are the driver's own words. The planner's half says the line did not run and why
/// in the terms it can act on, which is to put the value somewhere an authority holds it and
/// name that instead; it carries no finding. The person's half is the findings, each said as a
/// kind, a location, a fingerprint and a masked preview.
fn credential_refusal_in_a_line(scanned: &Scanned) -> Produced {
    let found = describe_all(&scanned.refused());
    Produced::refused_with_a_note(
        "refused: the command line carries a credential, so none of it ran. A command line is \
         read by every account on this machine while the program lives, and it is put on a \
         screen and into the record of this session. Do not run it again with the value in it: \
         tell the user which secret has to be set and where, and name it rather than writing it.",
        format!(
            "refused, the command line carried a credential: {}",
            found.join("; ")
        ),
    )
}

/// How much of a destination a line wrote is kept and read back for the credential scan.
///
/// Two files' worth of memory per destination, so the figure is a fraction of what a rewind
/// budgets across a whole turn. Past it nothing is kept and nothing is scanned: a value could
/// not be put back out of a file whose prior contents are not here, and a scan whose finding
/// cannot be acted on would refuse a line and leave the credential where it landed.
const MAX_SCANNED_BYTES: usize = 8 * 1024 * 1024;

/// A destination a line is about to open, as it stood before the line opened it.
///
/// Read in the notification [`crate::exec::run_plan_observed`] makes before each file may be
/// opened, which is the last moment any of this can be asked. `held` is carried and never read
/// here: the bytes go back to the path they came from, and the driver has no business looking at
/// them on the way.
struct Standing {
    /// The name the file authority and the trust map hold this destination under.
    key: String,
    /// The keys of the file's other names (a symbolic link's landing, hard links, a second path to
    /// the tree), which the effect is entered under with `key` (TRUST-4).
    others: Vec<String>,
    /// The name a person and a finding see, which is the name relative to the workspace root.
    shown: String,
    /// The name the planner is told: `shown` with the directory the line ran in spelled as the
    /// planner typed it, where `shown` has the place a link in the tree led.
    told: String,
    /// Where the file is, for reading what the line left and for putting back what it held.
    resolved: std::path::PathBuf,
    /// What the path held before the line opened it.
    held: crate::workspace::Before,
    /// What the trust map said about the path at that same moment.
    prior: bravebot_core::label::Integrity,
}

/// What a refusal of a program named by a relative path adds: where that path was looked for, and
/// whether the directory was carried from an earlier call (CMDLINE-12). Empty for any other
/// refusal, since a bare name is looked for on `$PATH` and an absolute path names itself.
///
/// `told` is the directory as the planner typed it, `None` at the workspace root; where a link led
/// is never named (LABEL-3). Every branch reads the planner's own line and argument and the
/// driver's record of which call named the directory, never a byte of a file.
fn looked_for_in(refused: &crate::cmdline::Refused, told: Option<&str>, carried: bool) -> String {
    if refused.reason != crate::cmdline::Reason::NotAtPath
        || !std::path::Path::new(&refused.text).is_relative()
    {
        return String::new();
    }
    match (told, carried) {
        (None, _) => {
            ". It was looked for at the workspace root; pass `directory` to look elsewhere"
                .to_string()
        }
        (Some(told), false) => format!(". It was looked for in '{told}'"),
        (Some(told), true) => format!(
            ". It was looked for in '{told}', carried from the last run that named a directory; \
             pass `directory` to change it"
        ),
    }
}

/// The name the planner is told for a destination a line opened.
///
/// A destination under the directory the line ran in is told as that directory spelled the way
/// the planner typed it, followed by the rest of the path. The path itself was joined onto where
/// the directory landed, and where a link led is a name whoever wrote the tree chose, which the
/// planner is not told (LABEL-3, TOOL-4). `typed` is `None` for a line run at the root, and then,
/// like a destination elsewhere, the name is the one relative to the root. Both halves are strings
/// the planner wrote or paths the driver resolved: nothing is read from a file to build the name.
fn told_destination(
    workspace: &Workspace,
    path: &std::path::Path,
    landing: &std::path::Path,
    typed: Option<&str>,
) -> String {
    match (typed, path.strip_prefix(landing)) {
        (Some(typed), Ok(rest)) => {
            let rest: Vec<std::borrow::Cow<'_, str>> = rest
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect();
            format!("{}/{}", typed.trim_end_matches('/'), rest.join("/"))
        }
        _ => workspace.relative_display(path),
    }
}

/// A failed run as the planner is told it. A file exec could not open is named by the path it
/// joined onto where the directory landed, so it is respelled under the directory as typed. That
/// holds for a file the line opens and for one a `$(...)` in its assignments opens, which exec
/// keeps inside the substitution's error. A name the closures that make a
/// [`crate::exec::ExecError::Redirection`] already spelled that way, or an absolute one the
/// planner wrote, is not under the landing and is left as it is. The decision is made on the
/// `PathBuf` exec held and the string is built from the directory the planner typed, so nothing is
/// read from a file (PATH-001, LABEL-3, TOOL-4).
fn told_failure(
    workspace: &Workspace,
    error: crate::exec::ExecError,
    landing: &std::path::Path,
    typed: Option<&str>,
) -> crate::exec::ExecError {
    use crate::exec::{ExecError, SubstitutionFailure};
    match error {
        ExecError::Unopened { path, detail } if path.strip_prefix(landing).is_ok() => {
            ExecError::Redirection {
                path: told_destination(workspace, &path, landing, typed),
                detail,
            }
        }
        ExecError::Substitution {
            source,
            failure: SubstitutionFailure::Inside(inner),
        } => ExecError::Substitution {
            source,
            failure: SubstitutionFailure::Inside(Box::new(told_failure(
                workspace, *inner, landing, typed,
            ))),
        },
        other => other,
    }
}

/// What a line left at the destinations it opened.
struct Left<'a> {
    /// Every finding across every destination, so one note names them all and one test of it
    /// decides the line.
    scanned: Scanned,
    /// The destinations a value that declared itself a credential landed in, which are the ones
    /// to be put back.
    refused_at: Vec<&'a Standing>,
}

/// Scan what a line left at each destination it opened, once the line has stopped.
///
/// This is the question a write tool asks before it writes, asked in the one place it cannot be
/// asked first: the program opens the file itself, so there is no moment in which the driver
/// holds the bytes and the file does not. What a finding buys here is therefore the value being
/// put back out of the tree rather than never reaching it.
///
/// The label handed to the scan is the one the destination's own effect is completed under: the
/// line's revalidated label met with what the map already said about the path. So a line that
/// read something nobody vouched for is not scanned, and neither is a destination that was
/// already quarantined. That is the same rule a write tool's body goes through, for the same
/// reason: examining bytes nobody vouched for to decide whether to refuse would be a decision
/// taken from untrusted content.
///
/// A destination whose prior contents were not kept is passed over rather than scanned. A
/// finding there could not be acted on, and a refusal that leaves the value in the file is worse
/// than saying nothing: it stops the line and protects nothing.
fn what_the_line_left<'a, S: Sink>(
    policy: &mut Policy<'_, S>,
    destinations: &'a [Standing],
    label: Label,
) -> Left<'a> {
    let mut left = Left {
        scanned: Scanned::default(),
        refused_at: Vec::new(),
    };
    for destination in destinations {
        let held = match &destination.held {
            crate::workspace::Before::NotKept => continue,
            held => held,
        };
        let Some(text) = crate::workspace::left_at(&destination.resolved, MAX_SCANNED_BYTES) else {
            continue;
        };
        // What the path holds now, at the integrity the kernel already fixed for this line's
        // output and is about to record on this destination. Not a claim about the file: it is
        // handed in rather than worked out from what was read.
        let after = Labelled::new(
            text,
            Label::new(
                destination.prior.meet(label.integrity),
                label.confidentiality,
            ),
        );
        // The pre-image places a value the file already held, and carries the integrity the map
        // gave the path: scan_a_write reads it only where that is trusted, so prior bytes
        // something else left untrusted cannot excuse a credential this line wrote beside them.
        let before = match held {
            crate::workspace::Before::Bytes(bytes) => Some(Labelled::new(
                String::from_utf8_lossy(bytes).into_owned(),
                Label::new(destination.prior, label.confidentiality),
            )),
            _ => None,
        };
        let scanned = policy.scan_a_write("run", &destination.shown, before.as_ref(), &after);
        if !scanned.refused().is_empty() {
            left.refused_at.push(destination);
            // Carried from the destinations being refused for and no others, since it decides
            // what the planner is told to do instead, and a second destination holding an
            // ordinary document has nothing to say about a first one that is the value.
            left.scanned.only_the_value |= scanned.only_the_value;
        }
        left.scanned.authored.extend(scanned.authored);
        left.scanned.carried.extend(scanned.carried);
    }
    left
}

/// What the person watching is told about a value a line left that only looks like a secret.
///
/// A write tool puts this on the approval it is already asking for, and a person answering the
/// question is the whole of what the inferred layer is for: the rule that catches a generated
/// framework key also catches a development password and a test fixture. A line has no such
/// prompt, so the choice here is between telling the person once the line has stopped and
/// telling nobody. Afterwards is late and is not a decision, which is the cost the credential
/// spec records; saying nothing would leave a generated key in the tree with nothing said about
/// it at all.
///
/// Nothing is said about a value in the line itself. The line is drawn on the approval prompt
/// in full, so the person has already read it.
fn inferred_note(note: String, scanned: &Scanned) -> String {
    let found = describe_all(&scanned.to_approve());
    if found.is_empty() {
        return note;
    }
    format!(
        "{note}; wrote something that looks like a credential: {}",
        found.join("; ")
    )
}

/// What a line is refused with once it has stopped and what it left holds a credential.
///
/// Both halves are the driver's own words. The planner's half names the line and the paths and
/// says a credential landed in them and was taken back out, which is what it needs to stop
/// running the same line and write a reference instead; it carries no finding, so nothing about
/// what was found reaches a model's context. The person's half is the findings, each said as a
/// kind, a location, a fingerprint and a masked preview, which is the whole of what a finding
/// may hold.
///
/// A path that would not go back is named to both. The planner is told because the value is
/// still there and the next thing it does must not assume otherwise, and the person is told
/// because they are the only one who can do anything about it.
fn credential_refusal_after_a_line(said: &str, left: &Left<'_>, stuck: &[String]) -> Produced {
    let paths: Vec<&str> = left
        .refused_at
        .iter()
        .map(|destination| destination.told.as_str())
        .collect();
    let instead = do_this_instead(left.scanned.only_the_value);
    let text = match stuck.is_empty() {
        true => format!(
            "refused: `{said}` put a credential in {}, so what it wrote there was taken \
             back out. Do not run it again. {instead}",
            paths.join(", ")
        ),
        false => format!(
            "refused: `{said}` put a credential in {}, and {} could not be put back, so the \
             value is still there. Do not run it again, and tell the user which file to clear \
             out. {instead}",
            paths.join(", "),
            stuck.join(", ")
        ),
    };
    let found = describe_all(&left.scanned.refused());
    let note = match stuck.is_empty() {
        true => format!(
            "refused, a credential landed here and was taken back out: {}",
            found.join("; ")
        ),
        false => format!(
            "refused, a credential landed here and {} could not be put back: {}",
            stuck.join(", "),
            found.join("; ")
        ),
    };
    Produced::refused_with_a_note(text, note)
}

/// What a refused request for a path says, whichever setting or rule refused it: the planner learns
/// that nothing was granted and not which setting withheld the tool.
const NOT_ACCEPTING_PATHS: &str = "refused: this session does not accept a request for a path. \
     Say which path the work needs, and what for, so the person can arrange it.";

/// What a request to write a path says where the programs of this session may not write.
const NOT_ACCEPTING_WRITES: &str = "refused: programs in this session may not be given a path to \
     write, whatever the person answers; nothing was asked. A request to read a path is still \
     accepted.";

/// The longest reason a request for a path is drawn with, in characters.
const LONGEST_PATH_REASON: usize = 300;

/// Whether this turn may be asked for reach to a path at all (SANDBOX-28).
///
/// The same turns that may be asked for a credential scope: confined, which the sandbox mode `off`
/// is not, and in a workspace the person has trusted. A project file or a settings layer has no way
/// to say yes to this, since the only road to a grant is the confirmer, and a turn nobody is at to
/// ask is refused by the one it is given.
fn path_requests_accepted<S: Sink>(policy: &Policy<'_, S>, tools: &Tools<'_>) -> bool {
    requests_are_accepted(
        tools.confine_runs,
        tools.sandbox,
        workspace_is_trusted(policy, tools.workspace),
    )
}

/// The tools that name a path the workspace resolves, the argument each names it in and whether the
/// call writes it (MODE-4, PATHREQ-7). The one list the reach outside the workspace is read from, so
/// a tool that names a path is covered by it or named in
/// [`TOOLS_NAMING_A_PATH_WITHOUT_REACH`], and a test fails for one that is neither.
pub(crate) const TOOLS_NAMING_A_PATH: &[(&str, &str, bool)] = &[
    ("read_file", "path", false),
    ("write_file", "path", true),
    ("edit_file", "path", true),
    ("download_url", "path", true),
    ("list_files", "directory", false),
    ("repo_map", "directory", false),
    ("search", "directory", false),
    ("read_git", "repository", false),
    ("run", "directory", false),
    ("watch_file", "path", false),
];

/// The tools that open a reference to a file the driver has not read yet, with the argument that
/// names it (MODE-4, PATHREQ-7). The reference stands for a path a listing or a search found, and
/// opening it resolves that path through the workspace, so the call is given the reach a call of
/// `read_file` is given, judged as a read. A tool that is also in [`TOOLS_NAMING_A_PATH`] is read
/// from there, and [`materialise`] judges the opening of a reference as a read in either case.
pub(crate) const TOOLS_READING_A_REFERENCE: &[(&str, &str)] =
    &[("vet_content", "ref"), ("spawn_processor", "reads")];

/// The tools that take a path, or a directory, and are given no reach by the mode, each with why.
#[cfg(test)]
pub(crate) const TOOLS_NAMING_A_PATH_WITHOUT_REACH: &[(&str, &str)] = &[
    (
        "request_path",
        "it asks for reach for programs and opens a directory itself (PATHREQ-7)",
    ),
    (
        "lsp",
        "the server indexes the workspace root and the path is not resolved through the workspace (LSP-1)",
    ),
    (
        "apply_checkout",
        "its paths are names inside a kept checkout, which come back through the write gate \
         (CHECKOUT-14)",
    ),
];

/// The facts a call's reach outside the workspace is decided from while the permission mode is
/// bypass (MODE-4, PATHREQ-7), read where a call is dispatched and where the reads of a round are
/// run ahead of it.
pub(crate) struct ReachFacts<'a> {
    pub(crate) permission_mode: crate::LiveMode,
    pub(crate) profile: Option<&'a std::path::Path>,
}

impl ReachFacts<'_> {
    /// What a call of `tool` reaches outside the workspace, or `None` where it reaches nothing.
    ///
    /// Read from the live mode at each call, so the reach ends when the key leaves bypass (MODE-8).
    /// Nothing here is a path or a file's content: the path the call names is judged where the
    /// workspace resolves it, against the refusals [`BypassReach`] carries. A workspace the person
    /// has not trusted reaches nothing (PATHREQ-5). The facts are the person's, and the tool's name
    /// and the field it names a path in are the driver's own. A write the sandbox mode `read-only`
    /// keeps is refused before a tool is reached (SANDBOX-22), so the reach does not hold it back.
    pub(crate) fn for_call<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        workspace: &Workspace,
        tool: &str,
    ) -> Option<crate::workspace::BypassReach> {
        self.reach_for(tool, workspace, || workspace_is_trusted(policy, workspace))
    }

    /// [`ReachFacts::for_call`] with the answer to "is the workspace trusted" supplied by the
    /// caller and asked only where the mode is bypass. For a caller with no call in progress and
    /// no policy, which is a session looking at a watch.
    pub(crate) fn reach_for(
        &self,
        tool: &str,
        workspace: &Workspace,
        trusted: impl FnOnce() -> bool,
    ) -> Option<crate::workspace::BypassReach> {
        let mode = self.permission_mode.get();
        if mode != crate::PermissionMode::Bypass || !trusted() {
            return None;
        }
        let tool = strip_namespace(tool);
        let (field, write) = TOOLS_NAMING_A_PATH
            .iter()
            .find(|(named, _, _)| *named == tool)
            .map(|&(_, field, write)| (field, write))
            .or_else(|| {
                TOOLS_READING_A_REFERENCE
                    .iter()
                    .find(|(named, _)| *named == tool)
                    .map(|&(_, field)| (field, false))
            })?;
        let person = bravebot_sandbox::rules::resolve(
            &bravebot_config::sandbox_filesystem(),
            self.profile,
            workspace.state_directory(),
            workspace.root(),
        );
        Some(crate::workspace::BypassReach {
            tool: tool.to_string(),
            field,
            mode: mode.name(),
            profile: self.profile.map(std::path::Path::to_path_buf),
            person,
            excluded: workspace.checkout_exclusions(),
            write,
        })
    }
}

/// Whether the person has trusted the working directory, which is what a request for a credential
/// scope, a path or a line with no sandbox is accepted on (TRUST-7).
pub(crate) fn workspace_is_trusted<S: Sink>(policy: &Policy<'_, S>, workspace: &Workspace) -> bool {
    policy.trusts_path(&workspace.trust_key("."))
}

/// The reach a watch armed under a reach is looked at with now, or `None` where the live mode no
/// longer gives one (MODE-8, PATHREQ-7, FSWATCH-7).
///
/// For a session looking at a watch with no call in progress, which has no policy: `trusted` is
/// whether the working directory is trusted by the session's map with the record of untrusted
/// memories applied, asked the way [`workspace_is_trusted`] asks it, and it is asked only where the
/// mode is bypass. Read from the live mode at each look, so the reach ends when the key leaves
/// bypass, and from the person's rules at each look, so a path is judged where it leads now. Nothing
/// here is a file's content.
pub fn reach_of_a_watch(
    permission_mode: &crate::LiveMode,
    profile: Option<&std::path::Path>,
    workspace: &Workspace,
    trusted: impl FnOnce() -> bool,
) -> Option<crate::workspace::BypassReach> {
    ReachFacts {
        permission_mode: permission_mode.clone(),
        profile,
    }
    .reach_for("watch_file", workspace, trusted)
}

/// One look at a watched path, for a session with no call in progress (FSWATCH-7).
///
/// The reach is derived here from the live mode, the person's rules and `trust`, which is the
/// session's map before the record of untrusted memories is applied to it. Outside bypass nothing
/// beyond the workspace is reachable, so the profile directory and the record are not read at all.
/// The terminal and the bridge both call this, so a watch is judged one way.
pub fn look_at_a_watch(
    workspace: &Workspace,
    permission_mode: &crate::LiveMode,
    trust: &bravebot_core::trust::TrustStore,
    path: &str,
    under: &std::path::Path,
) -> crate::watch::Looked {
    if permission_mode.get() != crate::PermissionMode::Bypass {
        return workspace.look_reaching(path, under, None);
    }
    let profile = crate::home::profile();
    let reach = reach_of_a_watch(permission_mode, profile.as_deref(), workspace, || {
        workspace.is_trusted_in(&crate::memory::with_recorded(
            trust,
            workspace,
            crate::home::directory().as_deref(),
        ))
    });
    workspace.look_reaching(path, under, reach)
}

/// Whether a turn with these facts accepts a `scopes` argument on `run` and a `request_path` call,
/// which are refused for the same causes ([`requests_refusals`]). The facts are read where the turn
/// starts as well, so the description and the failure sentence name the two only where a call
/// would be accepted (SANDBOX-19).
pub(crate) fn requests_are_accepted(
    confine_runs: bool,
    mode: bravebot_sandbox::SandboxMode,
    workspace_trusted: bool,
) -> bool {
    requests_refusals(confine_runs, mode, workspace_trusted).is_empty()
}

/// Ask the person to let programs reach one more path for the session (SANDBOX-28).
///
/// `path` is routing: it decides what a program can touch, so it is read through the planner
/// argument gate and judged by the rules an `allowWrite` entry meets. `why` is content, drawn for
/// the person and recorded, and decides nothing: it is read through the display release alone, and a
/// call without one is asked about like any other. The reach is held by the workspace for the session
/// and written nowhere, and the directory is not marked trusted, unless the mode that answers every
/// question granted it: that opens the directory for the file tools too (PATHREQ-7).
fn request_path<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    use bravebot_sandbox::rules::{RequestRefusal, judged_request};

    let Some(named) = named_argument(arguments, "path") else {
        return Produced::problem(
            "error: 'path' is required and must be one absolute path, or one beginning ~/",
        );
    };
    let write = match arguments.get("write") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(write)) => *write,
        Some(_) => return Produced::problem("error: 'write' must be true or false"),
    };
    if !path_requests_accepted(policy, tools) {
        return Produced::problem(NOT_ACCEPTING_PATHS);
    }
    // A yes to a write would hand back what `read-only` withholds, and the mode that answers every
    // question would give it unasked (SANDBOX-22, SANDBOX-28).
    if write && tools.sandbox == bravebot_sandbox::SandboxMode::ReadOnly {
        return Produced::problem(NOT_ACCEPTING_WRITES);
    }
    let Some(confinement) = tools.confinement(workspace_is_trusted(policy, tools.workspace)) else {
        return Produced::problem(NOT_ACCEPTING_PATHS);
    };

    let path = match policy.read_planner_argument("request_path", "path", &named) {
        Ok(path) => path,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    if path.chars().any(char::is_control) {
        return Produced::problem(
            "error: 'path' holds a control character, which the person could not be shown; \
             nothing was asked.",
        );
    }
    // Drawn and recorded, and decided on by nothing: a call without a reason asks the person all the
    // same (TOOL-5), and the screen is the one to say whether there is anything to draw.
    let reason: String = why_of(policy, "request_path", arguments)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(LONGEST_PATH_REASON)
        .collect::<String>()
        .trim()
        .to_string();

    let judged = judged_request(
        std::path::Path::new(&path),
        write,
        tools.profile,
        tools.home,
        confinement.filesystem(),
    );
    let resolved = match judged {
        Ok(resolved) => resolved,
        Err(RequestRefusal::Missing) => {
            return Produced::problem(
                "error: nothing is at that path, so nothing was asked. Name a path that exists.",
            );
        }
        Err(RequestRefusal::NotAbsolute) => {
            return Produced::problem(
                "error: 'path' must be absolute or begin ~/; nothing was asked.",
            );
        }
        Err(RequestRefusal::NoHome) => {
            return Produced::problem(
                "error: this session has no home directory to judge the path against; nothing \
                 was asked.",
            );
        }
        Err(RequestRefusal::Wildcard) => {
            return Produced::problem(
                "error: 'path' names one path and takes no pattern; nothing was asked.",
            );
        }
        Err(refusal) => {
            // Said, because a refusal that names nothing leaves the planner asking for the
            // directory above, then the file, then trying another tool. Only what is true of the
            // path, never which of the person's settings decided it.
            let why = match refusal {
                RequestRefusal::CredentialLocation => {
                    "it is at or inside a place that holds credentials"
                }
                RequestRefusal::PersistenceLocation => {
                    "a program that could write there could leave code that runs later with no \
                     sandbox"
                }
                RequestRefusal::Denied => "the user's own sandbox deny list covers it",
                _ => {
                    "it is too broad or holds what no program is lent: a filesystem root, the \
                     home directory or a directory above it, ~/.ssh or ~/.bravebot"
                }
            };
            let way_out = if write {
                unconfined_way_out(policy, tools)
            } else {
                ""
            };
            return Produced::problem(format!(
                "refused: that path cannot be lent to a program, whatever the person answers, \
                 because {why}; nothing was asked{way_out}. Say which path the work needs, and \
                 what for, so the person can arrange it."
            ));
        }
    };

    let request = crate::confirm::PathRequest {
        path: resolved.clone(),
        write,
        why: reason.clone(),
    };
    let mode = tools.permission_mode.get();
    let answered_by = mode.answers_a_run_unasked().then(|| mode.name());
    match confirmer.confirm_path(&request) {
        crate::confirm::Decision::Approve => {
            let shown = resolved.display().to_string();
            let opened = answered_by.map(|mode| {
                (
                    mode,
                    open_for_the_file_tools(policy, tools.workspace, &resolved, mode),
                )
            });
            tools.workspace.grant_path_reach(resolved, write, reason);
            policy.record_path_reach(&shown, write, answered_by);
            let access = if write { "read and write" } else { "read" };
            let file_tools = match opened {
                None => String::new(),
                Some((_, Ok(true))) => " The file tools (read_file, write_file, edit_file, \
                     list_files, search) reach it too, and no delegate is given a checkout while \
                     it is open, so do not ask the person to run /add-dir for it."
                    .to_string(),
                Some((_, Ok(false))) => " The file tools (read_file, write_file, edit_file, \
                     list_files, search) reach it too, so do not ask the person to run /add-dir \
                     for it."
                    .to_string(),
                Some((_, Err(reason))) => format!(
                    " The file tools do not reach it: /add-dir would refuse it, because it {reason}."
                ),
            };
            Produced::new(
                // The path as the planner typed it: where a link landed is a name whoever made the
                // link chose, and this sentence is the driver's own words (PATHREQ-2).
                Labelled::trusted(format!(
                    "approved: programs you start with run may {access} {path} for the rest of \
                     this session. Run the command again.{file_tools}"
                )),
                String::new(),
                format!("programs may {access} {shown} this session"),
            )
        }
        crate::confirm::Decision::Reject => Produced::problem(
            "refused: the person declined, or nobody was there to ask. Nothing was granted.",
        ),
    }
}

/// Open `path` for the file tools in the way `/add-dir` does, for a mode that answers a
/// `request_path` in the person's place (PATHREQ-7, TRUST-9).
///
/// `Ok` says whether the directory holds the working directory, which ends checkouts (CHECKOUT-7).
/// `Err` is why `/add-dir` would refuse it, in words the driver wrote. A refusal leaves the grant
/// for programs standing, and opens nothing.
fn open_for_the_file_tools<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    path: &std::path::Path,
    mode: &'static str,
) -> Result<bool, &'static str> {
    // A lossy rendering of a path that is not text would name a different directory than the one
    // the grant gave programs (PATH-001).
    let text = path.to_str().ok_or("is not text")?;
    let opened = workspace
        .open_directory(text)
        .map_err(|error| match error {
            crate::WorkspaceError::Invalid { reason, .. } => reason,
            _ => "cannot be opened",
        })?;
    policy.vouch_for_opened_directory(&crate::workspace::key_of(&opened), mode);
    Ok(workspace.ends_checkouts(&opened))
}

/// What a refused request says, whatever refused it: the planner learns that no credential was
/// added and not which setting withheld it.
const NOT_ACCEPTING_REQUESTS: &str = "refused: this session does not accept a request for a \
     credential scope or toolchain list. Run the line without 'scopes', and say what it needs \
     for the person to add.";

/// What a request says that would be added to no stage, because every stage of the line has an
/// assignment in front of its program (SANDBOX-26). Fixed words: nothing of the line is quoted, so
/// the planner is told the cause and the working forms and reads no name or value it wrote back as
/// the driver's. `env` in front of the program is not named.
const REQUEST_REACHES_NO_STAGE: &str = "refused: a name in 'scopes' is not added to a stage that \
     has a NAME=value assignment in front of its program, and every stage of this line has one, \
     so the request would add nothing and the line did not run. Pass git configuration as \
     `git -c name=value` instead of GIT_CONFIG_* assignments, or drop the assignment, and run the \
     line again.";

/// What a refused request to run one line with no sandbox says, whatever refused it: the planner
/// learns that the line did not run and not which setting withheld it. It does not name the
/// argument: the planner learns that from the `run` description (SANDBOX-29).
const NOT_ACCEPTING_UNCONFINED: &str = "refused: this session does not accept a request to run a \
     line with no sandbox. Say what the line needs and why, so the person can arrange it.";

/// What a request to run a line with no sandbox says when no line has failed with one in this turn.
/// Unlike [`NOT_ACCEPTING_UNCONFINED`] it says what to do, since the session would accept the
/// request after a line the sandbox refused. It names what earns nothing, a line in the background,
/// a line stopped at its deadline and a failure a delegate reported, so a planner that has seen one
/// of those fail runs the same line in the foreground, with a deadline that does not stop it.
const UNCONFINED_BEFORE_A_FAILURE: &str = "refused: run the same line with the sandbox first, in \
     the foreground. A request to run a line with no sandbox is accepted only after a line in this \
     turn ran in the foreground with the sandbox and exited non-zero, or a line was refused \
     because its program could not be confined, and neither has happened yet this turn. A line \
     run in the background, whether started with 'background' or moved there, earns nothing by \
     exiting non-zero. A line stopped at its deadline or by the person earns nothing, and neither \
     does a failure a delegate reported. Run the line without 'background' and without \
     'unconfined', with a 'deadline_seconds' long enough for it to end on its own. If it then \
     fails because the sandbox refused it access, or is refused because it could not be \
     confined, ask again and say in 'why' what was refused.";

/// Whether this turn may be asked to start one line with no profile (SANDBOX-29).
///
/// Only in the mode `workspace-write`: `strict` and `read-only` are the person asking for the profile to be
/// held whatever a planner would rather, and `off` has no profile to remove. Not in a delegate, whose sub-task is
/// reach the person never set up. Not in a workspace the person has not trusted, and not under a
/// managed `run.network` of `closed`, for the reason [`bravebot_config::sandbox::resolve`] floors
/// `off` there: a line with no profile is not held to a closed network. A turn nobody is at to ask
/// is refused by the confirmer, and the mode that asks nothing does not approve it
/// ([`crate::permission_mode`]).
fn unconfined_accepted<S: Sink>(
    policy: &Policy<'_, S>,
    tools: &Tools<'_>,
) -> Result<(), Vec<RunRequestCause>> {
    let causes = unconfined_refusals(
        tools.confine_runs,
        tools.sandbox,
        workspace_is_trusted(policy, tools.workspace),
        network_pinned_closed(bravebot_config::settled_run_network()),
        tools.delegated,
    );
    match causes.is_empty() {
        true => Ok(()),
        false => Err(causes),
    }
}

/// Whether the planner may be pointed at `'unconfined': true`: where [`unconfined_accepted`] holds
/// and the front end can ask about the line.
fn unconfined_is_offered<S: Sink>(policy: &Policy<'_, S>, tools: &Tools<'_>) -> bool {
    tools.asks_about_unconfined && unconfined_accepted(policy, tools).is_ok()
}

/// What a planner whose write was refused for leaving the workspace is told it can do instead, or
/// nothing where the session would not accept the request (SANDBOX-29).
///
/// Said only where [`unconfined_accepted`] holds and the front end can ask about the line, so the
/// planner is never pointed at an argument the session or its window would refuse. The person is
/// still asked, and the prompt shows the file. Worded to follow a refusal from
/// [`crate::workspace::WorkspaceError::describe`], which ends without a full stop, and to end
/// without one so a caller can go on.
fn unconfined_way_out<S: Sink>(policy: &Policy<'_, S>, tools: &Tools<'_>) -> &'static str {
    match unconfined_is_offered(policy, tools) {
        true => {
            ". A run line that redirects its output to a file there, started with \
             'unconfined': true and the refusal named in 'why', can write it: the person is asked \
             to approve that one line, which shows the file and runs with no sandbox"
        }
        false => "",
    }
}

/// Whether the redirection targets of a `run` line may leave the workspace (CMDLINE-5).
///
/// The confinement of a target exists because this process opens the file and not the stage, so a
/// destination outside the workspace would reach what the stage's profile holds its programs back
/// from. A line whose programs start with no profile has nothing held back: one the person
/// approved with no sandbox (SANDBOX-29), or any line under the mode `off`. Never a delegate,
/// whose files are the ones its definition lets it write (DELEGATE-28).
fn redirections_may_leave_the_workspace(unconfined: bool, tools: &Tools<'_>) -> bool {
    redirections_may_leave(
        unconfined,
        tools.delegated,
        tools.confine_runs,
        tools.sandbox,
    )
}

/// The rule [`redirections_may_leave_the_workspace`] applies, over the facts it reads, so each row
/// is one a test can walk.
fn redirections_may_leave(
    unconfined: bool,
    delegated: bool,
    confine_runs: bool,
    mode: bravebot_sandbox::SandboxMode,
) -> bool {
    unconfined || (!delegated && confine_runs && mode == bravebot_sandbox::SandboxMode::Off)
}

/// The rule [`unconfined_accepted`] applies, over the facts it reads, so each refusal is a row of a
/// table a test can walk. Every condition that fails is a cause, in the order SANDBOX-29 lists
/// them, and a request is accepted exactly where none does. The mode is read only where the turn
/// confines its runs, as it is everywhere else.
fn unconfined_refusals(
    confine_runs: bool,
    mode: bravebot_sandbox::SandboxMode,
    workspace_trusted: bool,
    network_pinned_closed: bool,
    delegated: bool,
) -> Vec<RunRequestCause> {
    let mut causes = Vec::new();
    if !confine_runs {
        causes.push(RunRequestCause::NoConfinement);
    } else if mode != bravebot_sandbox::SandboxMode::WorkspaceWrite {
        causes.push(RunRequestCause::SandboxMode(mode.name()));
    }
    if !workspace_trusted {
        causes.push(RunRequestCause::UntrustedWorkspace);
    }
    if network_pinned_closed {
        causes.push(RunRequestCause::ManagedClosedNetwork);
    }
    if delegated {
        causes.push(RunRequestCause::Delegate);
    }
    causes
}

/// Whether the administrator's file closed the network, which only a managed pin does here: a person
/// who closed it in their own settings is the one asked about the line.
pub(crate) fn network_pinned_closed(settled: Option<&bravebot_config::RunNetwork>) -> bool {
    settled.is_some_and(|settled| {
        settled.network == bravebot_sandbox::network::Network::Closed
            && matches!(settled.decided, bravebot_config::Decided::Managed(_))
    })
}

/// The names `scopes` holds, each a word of the fixed menu and compared exactly, once each.
///
/// Absent, null and an empty array ask for nothing. Anything else that is not an array of menu
/// words is an error rather than a request for less: a typo that quietly dropped `aws` would run
/// the line without the credential and report the failure as the program's.
fn requested_from(arguments: &Value) -> Result<Vec<bravebot_sandbox::scope::Requested>, String> {
    use bravebot_sandbox::scope::Requested;
    let menu = Requested::MENU.join(", ");
    let named = match arguments.get("scopes") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(named)) => named,
        Some(_) => {
            return Err(format!(
                "error: 'scopes' must be an array of names from this list: {menu}"
            ));
        }
    };
    let mut requested: Vec<Requested> = Vec::new();
    for name in named {
        let Some(request) = name.as_str().and_then(Requested::named) else {
            return Err(format!(
                "error: 'scopes' holds a name that is not on this list: {menu}"
            ));
        };
        if !requested.contains(&request) {
            requested.push(request);
        }
    }
    Ok(requested)
}

/// Whether the stages of a line start with no profile whatever the planner asks: the mode is `off`,
/// or the turn does not confine its runs. A `scopes` or `unconfined` argument has nothing to add
/// or remove there, so it is accepted and ignored (SANDBOX-26, SANDBOX-29).
fn starts_without_a_profile(confine_runs: bool, mode: bravebot_sandbox::SandboxMode) -> bool {
    !confine_runs || mode == bravebot_sandbox::SandboxMode::Off
}

/// Whether this turn may be asked for a credential scope at all.
///
/// In `strict`, `read-only` and `workspace-write`, where a name from the menu is the one way to widen what a program
/// reaches past the credential table. Not in `off`, which has no profile to add to. Not in a
/// workspace the person has not trusted, where the files the line names are content nobody vouched
/// for and a credential lent to a script a checkout wrote is the case SANDBOX-16 keeps scopes
/// from. A turn nobody is at to ask is refused by the confirmer the prompt goes to, since a
/// request is always asked about.
fn requests_accepted<S: Sink>(
    policy: &Policy<'_, S>,
    tools: &Tools<'_>,
) -> Result<(), Vec<RunRequestCause>> {
    let causes = requests_refusals(
        tools.confine_runs,
        tools.sandbox,
        workspace_is_trusted(policy, tools.workspace),
    );
    match causes.is_empty() {
        true => Ok(()),
        false => Err(causes),
    }
}

/// The rule [`requests_accepted`] applies, over the facts it reads, with every condition that fails
/// as a cause for the same reason [`unconfined_refusals`] gives.
fn requests_refusals(
    confine_runs: bool,
    mode: bravebot_sandbox::SandboxMode,
    workspace_trusted: bool,
) -> Vec<RunRequestCause> {
    let mut causes = Vec::new();
    if !confine_runs {
        causes.push(RunRequestCause::NoConfinement);
    } else if mode == bravebot_sandbox::SandboxMode::Off {
        causes.push(RunRequestCause::SandboxMode(mode.name()));
    }
    if !workspace_trusted {
        causes.push(RunRequestCause::UntrustedWorkspace);
    }
    causes
}

/// The checks a compiled `run` plan meets before the person is asked about it, in the order a
/// refusal is owed: a name Windows reads as another file, a redirection leaving the workspace, a
/// write the sandbox mode forbids, the rules written in advance, and then those rules again under
/// the names a redirection's files are held by (PERM-7).
///
/// Hands back whether a redirection may leave the workspace on this line, which the run checks
/// again at each open. A refusal comes back as the result of the call. Everything checked is the
/// plan's own structure and the session's settings; no output of a program is read.
///
/// A refusal names a redirection's file under `told_directory`, the directory as the planner typed
/// it, and never under where the workspace landed that directory (LABEL-3, TOOL-4).
fn check_the_plan_before_asking<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    plan: &bravebot_core::command::Plan,
    unconfined: bool,
    told_directory: Option<&str>,
) -> Result<Checked, Box<Produced>> {
    let told = |path: &std::path::Path| {
        told_destination(tools.workspace, path, &plan.directory, told_directory)
    };
    // A read as well as a write. The rules below are matched against the spelling, and a name
    // Windows reads as another file would have them judge one file while the run opens the other.
    // Before the confinement, which cannot say where a stream or a device lands. `> NUL` is how
    // output is discarded on Windows, so a refused write names the one spelling a line may use for
    // that; a read has none, since `/dev/null` discards only output.
    let discard = ". If this was meant to discard output, redirect it to /dev/null instead";
    for (paths, hint) in [(&plan.writes, discard), (&plan.reads, "")] {
        for path in paths {
            if let Err(misleading) = tools.workspace.refuse_misleading(path) {
                let target = told(path);
                return Err(Box::new(Produced::problem(format!(
                    "refused: {}{hint}",
                    workspace_failure(policy, "run", "command", &misleading, &target)
                ))));
            }
        }
    }

    // A read from the person's fact or rule stores, their home rules file or the session records
    // is refused wherever the line may reach (RECALL-8). The run opens a redirection's source
    // itself, outside any profile, so the sandbox that holds programs out of the stores never sees
    // it.
    for path in &plan.reads {
        let lands = crate::workspace::destination(path);
        if lands.is_some_and(|lands| tools.workspace.in_the_stores(&lands)) {
            let target = told(path);
            let escape = crate::workspace::WorkspaceError::Escapes {
                path: target.clone(),
                remedy: crate::workspace::Remedy::Nothing,
            };
            return Err(Box::new(Produced::problem(format!(
                "refused: {}",
                workspace_failure(policy, "run", "command", &escape, &target)
            ))));
        }
    }

    // A redirection names a file the run opens itself, so the confinement every other write goes
    // through is applied here to the path, and again immediately before the run opens it. Not for a
    // line whose programs start with no profile: there is nothing for the target to get past, and
    // the person is shown the file before the line runs (CMDLINE-5).
    let reaches_anywhere = redirections_may_leave_the_workspace(unconfined, tools);
    for path in &plan.writes {
        if !reaches_anywhere && let Err(escape) = tools.workspace.confines_writing(path) {
            let target = told(path);
            let way_out = match escape {
                crate::workspace::WorkspaceError::Escapes { .. } => {
                    unconfined_way_out(policy, tools)
                }
                _ => "",
            };
            return Err(Box::new(Produced::problem(format!(
                "refused: {}{way_out}",
                workspace_failure(policy, "run", "command", &escape, &target)
            ))));
        }
    }
    // Every path the loop above lets through is in a directory the session was opened on, and this
    // process opens it rather than the confined stage, so the profile `read-only` starts the stage
    // under does not hold it back (SANDBOX-22). Refused here, before anybody is asked, as a file
    // tool's write is. The null device is not in the write set.
    if !plan.writes.is_empty() && sandbox_refuses_writes(tools.confine_runs, tools.sandbox) {
        policy.refuse_write_in_sandbox("run", tools.sandbox.name());
        return Err(Box::new(Produced::problem(
            "refused: the sandbox mode is read-only, and this line redirects output into a file, \
             which would write the session's files; nothing ran and nothing was asked. Leave the \
             redirection out, redirect to /dev/null to discard the output, or have a program open \
             the file itself, where the sandbox profile decides whether it may.",
        )));
    }

    // Before the person is asked. A rule refusing something is a statement that it does not run,
    // and there is nothing to show or approve once it has been made.
    if let Err(denial) = policy.before_plan_rules(plan) {
        return Err(Box::new(refused_by_a_rule(&denial)));
    }

    // The plan holds a redirection's target as an absolute path, and a project-relative rule such
    // as `Edit(.env)` does not cover one. Asked again under the name the file tools hold the file
    // by, and under the name it lands on, so the rule refuses the redirection as it refuses the
    // same file named in a call (PERM-7).
    for (paths, purpose) in [
        (&plan.writes, Purpose::Effect),
        (&plan.reads, Purpose::Read),
    ] {
        for path in paths {
            // The rules are asked under the landing-based name, and the refusal says the name
            // under the directory as typed (LABEL-3, TOOL-4).
            let shown = told(path);
            if let Err(refusal) =
                refuse_redirection_target_by_rules(policy, tools.workspace, purpose, path, &shown)
            {
                return Err(Box::new(Produced::problem(refusal)));
            }
        }
    }

    // The destinations that are protected paths (PERM-9), as the line spelled them and as they
    // land. Taken from the file system and the names alone. A mode other than bypassing does not
    // answer a line with one, and the classifier is not asked about it.
    let protected = plan
        .writes
        .iter()
        .filter(|path| redirection_is_protected(tools.workspace, path))
        .cloned()
        .collect();

    // Where each destination lands, for the prompt to draw beside the name (CMDLINE-5). Read off
    // the file system, never a byte of a file. A destination that lands where it is named is
    // listed with no landing.
    let destinations = plan
        .writes
        .iter()
        .map(|path| crate::confirm::RunDestination {
            written: path.clone(),
            lands_on: landing_other_than_the_name(path),
            protected: redirection_is_protected(tools.workspace, path),
        })
        .collect();

    Ok(Checked {
        reaches_anywhere,
        protected,
        destinations,
    })
}

/// Where `path` lands once every link is followed, when that is not the path itself. The two are
/// compared as paths and not as the text shown for them, because two names that differ only in
/// bytes that are not UTF-8 show the same (PATH-001).
fn landing_other_than_the_name(path: &std::path::Path) -> Option<std::path::PathBuf> {
    crate::workspace::destination(path).filter(|lands| lands != path)
}

/// What [`check_the_plan_before_asking`] found out about a line that it did not refuse.
struct Checked {
    /// Whether the line's redirections may leave the workspace (see
    /// [`redirections_may_leave_the_workspace`]).
    reaches_anywhere: bool,
    /// The redirection destinations that were protected paths when the line was checked. The open
    /// of any other destination is held to the same test again.
    protected: Vec<std::path::PathBuf>,
    /// Each redirection destination with where it lands and whether it is protected, for the
    /// question put to the person.
    destinations: Vec<crate::confirm::RunDestination>,
}

/// Ask the rules written in advance about a redirection's file, as the plan names it and under the
/// other names it reaches. Asked when the line is compiled and again immediately before the run
/// opens the file, so a link or hard link an earlier part of the line left at the name is judged
/// by the file it reaches (CMDLINE-5, PERM-7, DELEGATE-28). Both asks are this one function, so
/// they cannot differ in what they cover or in what they say.
///
/// `shown` is the name the refusal gives, which is the destination under the directory as the
/// planner typed it and never under where the workspace landed it (LABEL-3, TOOL-4). Only rules,
/// settings and file-system names decide: no byte of a file is read.
fn refuse_redirection_target_by_rules<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    purpose: Purpose,
    path: &std::path::Path,
    shown: &str,
) -> Result<(), String> {
    let named = workspace.relative_display(path);
    refuse_denied_path_as(policy, workspace, purpose, &named, shown)?;
    // The name keeps a `..` the line spelled (`config/sub/../app.env`, or `../app.env` from a
    // `directory`), which no rule's segments match across and which the landing asked about above
    // does not resolve, so the file's other names are asked about too.
    for reached in workspace.names_reached(path) {
        if reached != named {
            refuse_denied_path_as(policy, workspace, purpose, &reached, shown)?;
        }
    }
    Ok(())
}

/// What `run` decides about how a line is held before the person is asked: the confinement it
/// starts under, whether it spends a request for no profile, the sandbox mode and unasked writes
/// to record once it starts, whether a prompt is owed, and whether the remembered record is what
/// stopped it.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate answer the run line's decision hands to a different effect"
)]
struct Settled {
    /// None for a line run with no profile. Replaced by the network-closed one where the person's
    /// trust of the directory is what let the line through.
    confinement: Option<crate::confine::Confinement>,
    spends_a_request: bool,
    sandbox_mode: Option<&'static str>,
    unasked_writes: bool,
    asking: bool,
    covered_by_record: bool,
    /// A stage is lent a credential scope a person kept at an earlier prompt, so the line is
    /// labelled as one that asked for the scope is.
    lent_a_kept_scope: bool,
}

/// The other names each file a `run` line's audited steps read has, by the operand as the line
/// spelled it (TRUST-2, CMDLINE-8): what [`Policy::learn_operand_names`] is given.
///
/// Asked of the file system about the paths the line itself named, as an attachment is
/// ([`Workspace::other_trust_keys`]): where a symbolic link lands, the hard links of a file and its
/// spelling under another open directory that is the same directory. The name is looked up
/// relative to the root where it is inside it, because [`Workspace::resolve`] refuses an absolute
/// name inside the root and the file would then be found to have no other name. An operand the
/// lookup could not finish for is recorded as incomplete, which the policy reads as untrusted.
/// Empty for a line that is not read-proven and names no argv word, which has no operands to label.
/// The words of a line's argv are looked up as well as the read set, because a held line is asked
/// whether it names a path the map holds untrusted (RUN-5), and a line the table does not account
/// for, such as `cp`, has no read set.
///
/// A directory named as an operand is looked up as the directory; the names of the files beneath
/// it are not, which TRUST-2 lists as a cost.
fn operand_names_of<S: Sink>(
    policy: &Policy<'_, S>,
    workspace: &crate::Workspace,
    plan: &bravebot_core::command::Plan,
) -> std::collections::HashMap<String, (Vec<String>, bool)> {
    let mut found = std::collections::HashMap::new();
    let operands = policy
        .operands_read_by(plan)
        .unwrap_or_default()
        .into_iter()
        .chain(policy.words_named_by(plan));
    for operand in operands {
        if found.contains_key(&operand) {
            continue;
        }
        let looked_up = workspace
            .below_root(&operand)
            .unwrap_or_else(|| operand.clone());
        let others = workspace.other_trust_keys(&looked_up);
        found.insert(operand, (others.keys, others.complete));
    }
    found
}

/// Work out the confinement a `run` line starts under and whether the person is to be asked,
/// refusing a request for a scope or for no profile that this session cannot honour.
///
/// Decided from the compiled plan, the exit codes of earlier lines and the session's settings,
/// never from what a program printed. A refusal comes back as the result of the call.
fn settle_the_confinement<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    plan: &bravebot_core::command::Plan,
    requested: &[bravebot_sandbox::scope::Requested],
    unconfined: bool,
    recalled: Option<&bravebot_core::remembered::Remembered>,
) -> Result<Settled, Box<Produced>> {
    // No confinement at all for a line the planner asked to run with none: its stages start as
    // they do under the mode `off`, for this line only, and the next line is built from the
    // session's mode again.
    let mut confinement = tools
        .confinement(workspace_is_trusted(policy, tools.workspace))
        .filter(|_| !unconfined)
        .map(|confinement| {
            confinement
                .with_requested(requested)
                .with_requests_offered(
                    requests_accepted(policy, tools).is_ok()
                        && tools.permission_mode.answers_requests(),
                )
                .with_request_path_listed(tools.beside.holds(Named::RequestPath))
                .with_nobody_to_ask(tools.permission_mode.nobody_to_ask())
        });
    if !requested.is_empty() && !confinement.as_ref().is_some_and(|c| c.accepts_requests()) {
        let cause = match confinement {
            Some(_) => RunRequestCause::NoProfileDirectory,
            None => RunRequestCause::NoPlatformSandbox,
        };
        let causes = policy.refuse_run_request("scopes", &[cause]);
        return Err(Box::new(Produced::refused_with_a_note(
            NOT_ACCEPTING_REQUESTS,
            format!("refused, 'scopes' is not accepted: {causes}"),
        )));
    }
    // A request no stage would be given, because every stage has an assignment in front of its
    // program, is refused before the person is asked, since the person would be asked about a
    // request that adds nothing. Decided from the compiled steps and the menu words (SANDBOX-26).
    if confinement
        .as_ref()
        .is_some_and(|confinement| confinement.request_reaches_no_stage(&plan.steps()))
    {
        let causes = policy.refuse_run_request("scopes", &[RunRequestCause::EveryStageAssigned]);
        return Err(Box::new(Produced::refused_with_a_note(
            REQUEST_REACHES_NO_STAGE,
            format!("refused, 'scopes' adds nothing to this line: {causes}"),
        )));
    }
    // Accepted after a line has failed with the sandbox on, or for a line whose own redirection
    // the sandbox refuses before anything runs, which no run can fail at. Decided from exit codes
    // and the compiled plan, never from what a program printed. The request is spent once the
    // person has approved it and the policy has passed the plan, so a refusal on the way leaves it.
    let mut spends_a_request = false;
    if unconfined {
        let leaves_the_workspace = plan
            .writes
            .iter()
            .any(|path| tools.workspace.confines_writing(path).is_err());
        if !leaves_the_workspace && !tools.confined_failures.has_one() {
            let causes =
                policy.refuse_run_request("unconfined", &[RunRequestCause::NoConfinedAttempt]);
            return Err(Box::new(Produced::refused_with_a_note(
                UNCONFINED_BEFORE_A_FAILURE,
                format!("refused, 'unconfined' is not accepted: {causes}"),
            )));
        }
        spends_a_request = !leaves_the_workspace;
    }
    // Written once the line is about to start rather than here, so a line the person does not
    // approve is not recorded as having run in a mode. The writes are read from the confinement as
    // it stands now, before a directory's trust replaces it below.
    let sandbox_mode = match (unconfined, tools.confine_runs) {
        (true, _) => Some("unconfined"),
        (false, true) => Some(tools.sandbox.name()),
        (false, false) => None,
    };
    let unasked_writes = !unconfined
        && tools.confine_runs
        && confinement
            .as_ref()
            .is_some_and(|confinement| confinement.writes_without_a_request());

    let names: Vec<&'static str> = requested.iter().map(|request| request.name()).collect();
    // The credential scopes the person's own record attaches to the stages of this line, from the
    // grants and the compiled steps and nothing a program printed. Asked of the confinement as
    // built here, before a directory's trust replaces it below, and empty for a line run with no
    // profile, which carries no grant.
    let kept: Vec<&'static str> = confinement
        .as_ref()
        .map(|confinement| confinement.remembered_scopes(&plan.steps()))
        .unwrap_or_default()
        .into_iter()
        .map(bravebot_sandbox::scope::Scope::name)
        .collect();
    // How the sandbox holds the line, said from the compiled steps and the session's own settings:
    // the person's trust of the directory answers only for a line it holds (RUN-5).
    //
    // Under an open network no stage is held as the session stands, so the line is judged as it
    // would run with the network closed for this line only. It runs that way only if the person's
    // trust of the directory is what lets it through, below.
    let closed_for_the_line = confinement
        .as_ref()
        .filter(|confinement| !confinement.network_is_closed())
        .map(crate::confine::Confinement::closed_for_the_line);
    let containment = closed_for_the_line
        .as_ref()
        .or(confinement.as_ref())
        .map_or(Containment::Unknown, |confinement| {
            confinement.holds(&plan.steps(), &plan.directory)
        });
    let asking = match unconfined {
        true => policy.plan_needs_approval_unconfined(),
        false => policy.plan_needs_approval_lent(plan, &names, &kept, containment),
    };
    // A line the person's trust of the directory let through runs as it was judged: with the
    // network closed for this line, where the session's is open. A line any other road answered
    // keeps the network it was approved with.
    if !asking
        && !unconfined
        && policy.answered_by_directory_trust()
        && let Some(closed) = closed_for_the_line
    {
        confinement = Some(closed);
    }
    // Whether the record is what stopped the question. Read where the result is quarantined: the
    // advice about vouching is advice about a prompt, and no prompt will return here for this line
    // until somebody deletes the entry.
    let covered_by_record = !asking && recalled.is_some_and(|lines| lines.covers(plan));

    Ok(Settled {
        confinement,
        spends_a_request,
        sandbox_mode,
        unasked_writes,
        asking,
        covered_by_record,
        lent_a_kept_scope: !kept.is_empty(),
    })
}

/// What a `run` line that stops being waited for is kept as: the plan and the words it is shown
/// and told under, the label the kernel fixed before anything started, and the file authority's
/// revision from the same moment.
struct JobStart<'a> {
    spends: &'a [bravebot_core::ambient::Spent],
    plan: &'a bravebot_core::command::Plan,
    displayed: &'a str,
    said: &'a str,
    /// The directory the line ran in, spelled as the planner typed it.
    told_directory: &'a str,
    label: Label,
    authority: &'a bravebot_core::file_authority::FileAuthority,
    started_revision: u64,
    covered_by_record: bool,
    confinement: Option<&'a crate::confine::Confinement>,
    /// How long the line was waited for first, for a line that was moved to the background.
    moved_after: Option<std::time::Duration>,
}

/// Everything `run` does at the point a line becomes a job, whether it was started in the
/// background or moved there: spend what the line reaches, carry the directory over, keep the
/// job under a name, record what it was confined to, and tell the reporter it started. Hands back
/// the name and the token that stops it.
fn start_the_job<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    reporter: &mut R,
    running: crate::exec::Background,
    job: &JobStart<'_>,
    tree: TreeBefore,
) -> (String, bravebot_core::cancel::JobStop) {
    policy.record_ambient(job.spends);
    // The directory is carried over only once pre-flight checks and launch succeed.
    *tools.run_directory = RunDirectory {
        landing: job.plan.directory.clone(),
        told: job.told_directory.to_string(),
    };
    let (name, stop) = tools.jobs.keep(
        running,
        JobLines::new(job.displayed, job.said),
        job.label,
        job.authority.clone(),
        job.started_revision,
        job.covered_by_record,
    );
    tools.jobs.confined_as(
        &name,
        job.confinement.map(|confinement| {
            confinement.background_profile_offering(
                &job.plan.steps(),
                unconfined_is_offered(policy, tools),
            )
        }),
    );
    tools.jobs.tree_found(&name, tree);
    if let Some(after) = job.moved_after {
        policy.record_handoff(&name, after);
    }
    reporter.job(crate::report::JobEvent::Started {
        name: name.clone(),
        line: job.displayed.to_string(),
        moved_after: job.moved_after,
        stop: stop.clone(),
    });
    (name, stop)
}

/// What a `run` call asks for beyond the command line, read from its arguments.
struct RunCall {
    /// The deadline, clamped to the bounds the turn holds.
    limit: std::time::Duration,
    in_the_background: bool,
    /// The reference whose contents go to the first program's standard input.
    named_stdin: Option<Labelled<String>>,
    read_asked: bool,
    /// The credential scopes asked for by name from the fixed menu.
    requested: Vec<bravebot_sandbox::scope::Requested>,
    unconfined: bool,
}

/// Read and check the fields of a `run` call other than `command` and `directory`.
///
/// Shape and combination checks only, plus the two questions of whether this turn may be asked
/// for a scope or for no profile at all. Nothing here reads the planner's words as content or
/// holds a label: the fields are booleans, a list of menu words and the name of a reference.
fn run_call<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    arguments: &Value,
) -> Result<RunCall, Box<Produced>> {
    // The deadline the caller asked for, clamped to bounds the turn cannot exceed.
    // Not a safety property: a program that finishes in time is no safer than one that
    // does not. Absent a value, the short default is generous enough for an ordinary
    // build step and short enough that a hung program is noticed.
    let limit = match deadline_from(arguments, tools.deadlines) {
        Ok(limit) => limit,
        Err(diagnostic) => return Err(Box::new(Produced::problem(diagnostic))),
    };

    // Absent or non-boolean means the foreground, which is the reading that waits for the program
    // and hands back what it printed.
    let in_the_background = arguments
        .get("background")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // The reference whose contents go to the first program's standard input, where the call named
    // one. Present but not a string is refused rather than dropped, for the reason a directory is:
    // a field the driver quietly ignored would run a line over nothing, and a planner that asked
    // for a document to be filtered would be handed the filter's answer about an empty one. A
    // blank string names no document, so it is the field left out, as for every other reference.
    let named_stdin = match arguments.get("stdin_ref") {
        None | Some(Value::Null) => None,
        Some(Value::String(_)) => named_argument(arguments, "stdin_ref"),
        Some(_) => {
            return Err(Box::new(Produced::problem(
                "error: 'stdin_ref' must be a string naming a reference, e.g. \"ref:1\"",
            )));
        }
    };

    // Nothing waits for a background job, and nothing writes to one either: `start_steps_within`
    // gives its first step an empty stdin and has nowhere to put anything else. Refused rather
    // than ignored, so a call asking for both is told which of the two it cannot have.
    if named_stdin.is_some() && in_the_background {
        return Err(Box::new(Produced::problem(
            "error: a background command cannot be fed a reference. Run it in the foreground, \
             which waits for the program and hands back what it printed.",
        )));
    }

    // Absent means no, which leaves the result exactly as it would be without the field. Present but
    // not a boolean is refused, as a mistyped `stdin_ref` is: a planner that believed it had asked
    // would be handed a reference instead, and the way it would ask again is to run the line again.
    // Refused beside a background line for the reason a reference is: nothing is waited for there,
    // so this result holds nothing the line printed to read.
    let read_asked = read_asked_of(arguments)?;
    if read_asked && in_the_background {
        return Err(Box::new(Produced::problem(
            "error: a background command has printed nothing into this result, so there is \
             nothing in it to read. Leave 'read' out, or run it in the foreground.",
        )));
    }

    // What the planner asks every stage of this line to carry, by name from a fixed menu. Refused
    // rather than dropped where it cannot be honoured, so a planner that believed it had a
    // credential is told it has none and not handed a failure it would chase as a fault.
    let requested = match requested_from(arguments) {
        Ok(requested) => requested,
        Err(diagnostic) => return Err(Box::new(Produced::problem(diagnostic))),
    };

    // Whether the planner asks for this one line to start with no profile. Refused rather than
    // dropped where it is mistyped, as `read` is: a planner that believed it had asked would be
    // handed the profile's failure to chase as a fault.
    let unconfined = match arguments.get("unconfined") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(asked)) => *asked,
        Some(_) => {
            return Err(Box::new(Produced::problem(
                "error: 'unconfined' must be true or false",
            )));
        }
    };
    if unconfined && !requested.is_empty() {
        return Err(Box::new(Produced::problem(
            "error: give 'unconfined' or 'scopes', not both. A line with no sandbox has no \
             profile for a scope to be added to.",
        )));
    }

    // A line whose stages start with no profile, under the mode `off` or in a turn that does not
    // confine, in a workspace the person trusts, already runs with the person's own access. A scope has nothing to lend and
    // `unconfined` nothing to remove, so both are accepted and dropped: the line is asked about and
    // its output quarantined as an ordinary line's is, and nothing is refused, recorded as
    // requested or spent (SANDBOX-26, SANDBOX-29). The shape checks above still apply, so a
    // mistyped argument is an error in every mode. An untrusted workspace falls through to the
    // refusals below, as it did before (SANDBOX-16).
    if starts_without_a_profile(tools.confine_runs, tools.sandbox)
        && workspace_is_trusted(policy, tools.workspace)
    {
        return Ok(RunCall {
            limit,
            in_the_background,
            named_stdin,
            read_asked,
            requested: Vec::new(),
            unconfined: false,
        });
    }

    // Refused rather than dropped where the session cannot honour it, so a planner that believed
    // it had a credential is told it has none and not handed a failure it would chase as a fault.
    if !requested.is_empty()
        && let Err(causes) = requests_accepted(policy, tools)
    {
        let detail = policy.refuse_run_request("scopes", &causes);
        return Err(Box::new(Produced::refused_with_a_note(
            NOT_ACCEPTING_REQUESTS,
            format!("refused, 'scopes' is not accepted: {detail}"),
        )));
    }
    if unconfined && let Err(causes) = unconfined_accepted(policy, tools) {
        let detail = policy.refuse_run_request("unconfined", &causes);
        return Err(Box::new(Produced::refused_with_a_note(
            NOT_ACCEPTING_UNCONFINED,
            format!("refused, 'unconfined' is not accepted: {detail}"),
        )));
    }
    Ok(RunCall {
        limit,
        in_the_background,
        named_stdin,
        read_asked,
        requested,
        unconfined,
    })
}

/// Whether a stage of `plan` is lent a credential scope the person's own record holds, worked out
/// from the confinement the run is started with and the compiled steps, never from the line's
/// words (SANDBOX-27).
fn is_lent_a_kept_scope(
    confinement: Option<&crate::confine::Confinement>,
    plan: &bravebot_core::command::Plan,
) -> bool {
    confinement.is_some_and(|confinement| !confinement.remembered_scopes(&plan.steps()).is_empty())
}

/// What the sandbox does to the stages of a line, as far as the auto mode's classifier is
/// concerned (AUTO-3).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Profile {
    /// No confinement: the line starts without a profile.
    Absent,
    /// Every stage is confined and none lifts the `.git` hold-back.
    Holding,
    /// Some stage is confined by a profile that lifts the `.git` hold-back, which is the profile of
    /// a stage whose program is git (SANDBOX-18). This is not
    /// [`crate::confine::Confinement`]'s `writes_git_directories`, which is a fact about one
    /// profile's write rows.
    LiftingTheGitHoldBack,
}

/// The [`Profile`] the confinement a run is started with gives `plan`, from the profile each stage
/// is built with and never from a name in the line.
fn profile_of(
    confinement: Option<&crate::confine::Confinement>,
    plan: &bravebot_core::command::Plan,
) -> Profile {
    match confinement {
        None => Profile::Absent,
        Some(confinement) if confinement.lifts_the_git_hold_back(&plan.steps()) => {
            Profile::LiftingTheGitHoldBack
        }
        Some(_) => Profile::Holding,
    }
}

/// What the auto mode's classifier says of a line the policy is about to ask about, or that it was
/// not consulted (AUTO-3).
///
/// Consulted only in auto, and only for a line the sandbox confines that did not ask to start with
/// no profile and has no git stage: the classifier is shown the words of the line and not the
/// binary they resolve to, so the confinement is what holds the line to those words, and a git
/// stage's profile lifts the `.git` hold-back, so it writes a hook or a configuration that runs
/// later with no profile around it ([`Profile::LiftingTheGitHoldBack`], SANDBOX-18). The plan and
/// the directory are the driver's own structure and no labelled value is built or released here.
///
/// `told_in` is the directory as the planner typed it, and is `None` when the line runs at the
/// workspace root. The classifier is a model, so a name a link in the tree chose is not put in its
/// request any more than in the planner's: it is shown the typed spelling, or the root, which is
/// the person's own directory (AUTO-5).
#[expect(
    clippy::too_many_arguments,
    reason = "the plan and its scopes are the driver's own structure and `policy` and `tools` are exclusive borrows, so none of them can share a bundle without the borrows aliasing"
)]
fn consult_about_a_run<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    plan: &bravebot_core::command::Plan,
    told_in: Option<&str>,
    requested: &[bravebot_sandbox::scope::Requested],
    stdin: bool,
    unconfined: bool,
    confinement: Option<&crate::confine::Confinement>,
    protected: bool,
) -> crate::confirm::Classified {
    if unconfined
        || profile_of(confinement, plan) != Profile::Holding
        || tools.permission_mode.get() != crate::PermissionMode::Auto
    {
        return crate::confirm::Classified::NotAsked;
    }
    // A line that redirects into a protected path is the person's to answer: the file runs code
    // later with their rights, so a model's word is not asked for (AUTO-3, PERM-9).
    if protected {
        return crate::confirm::Classified::NotAsked;
    }
    // Only a line it can be shown whole: nothing fed to it from a reference, no name in `scopes`
    // (a credential scope, a toolchain list, `loopback` or `ssh-key`), nothing private released,
    // and no stage lent a credential scope a person kept at an earlier prompt: the classifier is
    // shown the words of the line, which do not say what the stage is lent (SANDBOX-27).
    if !requested.is_empty()
        || is_lent_a_kept_scope(confinement, plan)
        || stdin
        || plan.releases_private()
    {
        return crate::confirm::Classified::NotAsked;
    }
    let Some(classifying) = tools.classifying.as_ref() else {
        return crate::confirm::Classified::NotAsked;
    };
    let directory = told_in.map_or_else(
        || plan.directory.display().to_string(),
        std::string::ToString::to_string,
    );
    crate::classifier::consult(
        policy,
        &mut tools.chat,
        classifying,
        &crate::classifier::Question::Run {
            line: &plan.line,
            directory: &directory,
        },
    )
}

/// What the auto mode's classifier says of fetching a URL the policy is about to ask about, or that
/// it was not consulted. Never asked for a download, which puts bytes into a file (AUTO-3).
fn consult_about_a_fetch<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    url: &str,
    host: &str,
) -> crate::confirm::Classified {
    if tools.permission_mode.get() != crate::PermissionMode::Auto {
        return crate::confirm::Classified::NotAsked;
    }
    let Some(classifying) = tools.classifying.as_ref() else {
        return crate::confirm::Classified::NotAsked;
    };
    crate::classifier::consult(
        policy,
        &mut tools.chat,
        classifying,
        &crate::classifier::Question::Fetch { url, host },
    )
}

/// Put a plan to the person and act on what they answer.
///
/// Called only when the policy says this line is to be asked about. The answer is recorded, the
/// line remembered or its scopes and key kept where the person asked for that, and a refusal comes
/// back as the result of the call. No labelled value is built or released here: the plan, the
/// reference's name and the confinement are the driver's own structure.
#[expect(
    clippy::too_many_arguments,
    reason = "the plan and its scopes are the driver's own structure and `policy` and `confirmer` are exclusive borrows, so none of them can share a bundle without the borrows aliasing"
)]
#[expect(
    clippy::too_many_lines,
    reason = "it builds the one request the person answers, field by field, and each field is a separate decision about what the prompt shows"
)]
fn ask_about_the_run<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    plan: &bravebot_core::command::Plan,
    told_in: Option<&str>,
    record: Option<&crate::remembered::Store>,
    requested: &[bravebot_sandbox::scope::Requested],
    unconfined: bool,
    stdin: Option<String>,
    protected: bool,
    destinations: Vec<crate::confirm::RunDestination>,
    confinement: &mut Option<crate::confine::Confinement>,
) -> Result<(), Box<Produced>> {
    // Whether this person has already read a prompt for this binary under other arguments,
    // asked before the line on screen joins that list. An identical line is not a different
    // argument list, so the order is a matter of reading rather than of correctness.
    let varied = policy.arguments_have_varied(plan);
    policy.asked_about(plan);
    // Whether a stage is lent a credential scope the person's own record holds, worked out from
    // the confinement the prompt is drawn from and the compiled steps, as the policy was told.
    let lent_a_kept_scope = is_lent_a_kept_scope(confinement.as_ref(), plan);
    let classified = consult_about_a_run(
        policy,
        tools,
        plan,
        told_in,
        requested,
        stdin.is_some(),
        unconfined,
        confinement.as_ref(),
        protected,
    );
    let request = crate::confirm::RunRequest {
        plan: plan.clone(),
        // Offered only where it would stop a later prompt, which the policy decides: not for a
        // line releasing private data, not for one naming a file to write, not for one running
        // outside the workspace root, and not where a rule the person wrote in advance already
        // says to ask. A remembered line holds no tree, unlike a vouched entry, so the root is
        // still one of its refusals. The path is what the prompt shows, since a person cannot
        // endorse a record they were not shown.
        record: record
            .filter(|_| {
                policy.may_remember(plan)
                    && requested.is_empty()
                    && !lent_a_kept_scope
                    && !unconfined
            })
            .filter(|_| crate::remembered::may_be_added_to())
            .map(|store| store.path().to_path_buf()),
        // Said only where a key at this prompt will not finish the asking, only where a rule
        // in that file would decide the line at all, and only where there is a file to name:
        // a line that writes, releases private data, runs outside the root or carries an
        // assignment is asked about before any rule is read, so a pattern for one would stop
        // no prompt, and advice on a machine that names no home directory would send somebody
        // to a path nothing reads. A session in the mode that adds nothing to `~/.bravebot`
        // still gets the advice, because writing that file is the person's own act rather
        // than this session's.
        pattern: tools
            .home
            .filter(|_| {
                varied
                    && policy.a_rule_could_answer(plan)
                    && requested.is_empty()
                    && !lent_a_kept_scope
                    && !unconfined
            })
            .map(bravebot_config::user_settings_file),
        // The reference, so the person reads what is going in as well as that something is.
        // The driver's own name for a slot, never a byte of what the slot holds.
        stdin,
        // Described by the confinement the executor starts these steps under, the same value,
        // so the prompt names no confinement exactly where the steps start without one.
        confined: confinement
            .as_ref()
            .map(|confinement| confinement.describe(&plan.steps())),
        // Offered only for a line that asked for something to remember, in a session with a
        // state directory, a session to key the answer to and a mode that may add to that
        // directory. The path is what the prompt shows, for the reason `record` is.
        reach_record: tools
            .home
            .zip(tools.remembering)
            .filter(|_| !requested.is_empty() && crate::reach::may_be_added_to())
            .map(|(home, _)| crate::reach::Store::new(home).path().to_path_buf()),
        unconfined,
        // Offered where the answer can be written down, for the reason `reach_record` is: a
        // choice the next session forgets is a question asked again on every push.
        key_choice: tools
            .home
            .zip(tools.remembering)
            .filter(|_| crate::reach::may_be_added_to())
            .zip(confinement.as_ref())
            .and_then(|((home, _), confinement)| {
                confinement.key_choice(&plan.steps(), crate::reach::Store::new(home).path())
            })
            .map(Box::new),
        classified,
        protected,
        destinations,
    };
    // Read before the question is put, for the reason a write's is.
    let mode = tools.permission_mode.get();
    let answer = confirmer.confirm_run(&request);
    policy.record_answer(mode.answers_this_run_unasked(&request).then(|| mode.name()));
    keep_the_note(policy, "run", answer.approved(), answer.note());
    if !answer.approved() {
        policy.refuse_unapproved_run();
        return Err(Box::new(Produced::problem(format!(
            "refused: the user did not approve running this. Do not retry the same line{}",
            what_next(answer.note().is_some())
        ))));
    }
    // Recorded before the run, so a repeat of the same command later in this turn is not
    // asked about again. The policy carries it out of the turn and the session records it.
    //
    // Not for a line an entry could not record: one that feeds a file to a program, one that
    // feeds it a reference, one that writes an assignment in front of a program, one asking for a
    // scope or to run with no profile, and one redirecting into a protected path. The prompt
    // offers `a` for none of them, and this is the same refusal at the layer that would act on
    // it: `Plan::vouch` asks the predicate the prompt asks, which Verus proves, and the policy
    // records an entry only from the `Vouch` it returns. What an entry records is a
    // program, its exact argv and the tree the line runs in, and a `<` redirection, a
    // `stdin_ref` and an assignment are in none of the three, so an entry made here would cover
    // the same program fed any other file or reference, or run under no assignment at all. A
    // front end answering `always` anyway must not be able to widen the list that way.
    //
    // The tree is not among the refusals, and that is RUN-8's own answer rather than an
    // omission: an entry names the directory it was given in, so a line outside the workspace
    // root is one an entry can hold as written, and it grants there and nowhere else.
    if answer.remember
        && let Some(vouch) = plan.vouch(request.asked())
    {
        policy.remember_vouch(vouch);
    }
    // The credential scopes the planner asked for, kept for the stages they were added to
    // (SANDBOX-27). The shapes are worked out again from the plan and the closed table here,
    // rather than read back from what the prompt drew, so a front end answering with a key the
    // prompt never offered cannot attach a scope to a stage it was not shown on. The scopes
    // are the request's, which a person read in the plan, and no output byte is an input.
    if let (Some(lasting), Some(home), Some(session)) =
        (answer.remember_reach, tools.home, tools.remembering)
        && !requested.is_empty()
    {
        let scopes = request.requested_credential_scopes();
        crate::reach::keep_requested(
            &crate::reach::Store::new(home),
            &plan.steps(),
            &scopes,
            lasting,
            session,
            tools.workspace.root(),
            &crate::reach::today(),
        );
    }
    // The key the person chose for the owner this line's push leaves to ssh (SANDBOX-16). Read
    // against the candidates this layer listed, so a front end answering with a key the prompt
    // never offered, or answering where it offered none, chooses nothing. It applies to this
    // line's stages and is remembered for the lines after it.
    if let (Some(choice), Some(answer), Some(home)) = (&request.key_choice, answer.key, tools.home)
    {
        let key = match answer {
            crate::confirm::KeyAnswer::Candidate(at) => {
                choice.candidates.get(at).cloned().map(Some)
            }
            crate::confirm::KeyAnswer::SshDefault => Some(None),
        };
        if let Some(key) = key {
            let chosen = crate::reach::ChosenKey {
                address: choice.address.clone(),
                key,
                allowed: crate::reach::today(),
            };
            crate::reach::Store::new(home).choose_key(&chosen);
            *confinement = confinement
                .take()
                .map(|confinement| confinement.with_chosen_key(chosen));
        }
    }
    remember_the_answer(
        policy,
        tools,
        plan,
        record,
        requested,
        lent_a_kept_scope,
        unconfined,
        &answer,
    );
    Ok(())
}

/// Write down what a run's answer asked to have remembered, where the policy allows it.
#[expect(
    clippy::too_many_arguments,
    reason = "the policy is an exclusive borrow and the rest is the line's own structure and the answer, so none of them can share a bundle"
)]
fn remember_the_answer<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    plan: &bravebot_core::command::Plan,
    record: Option<&crate::remembered::Store>,
    requested: &[bravebot_sandbox::scope::Requested],
    lent_a_kept_scope: bool,
    unconfined: bool,
    answer: &crate::confirm::RunDecision,
) {
    // The second of the two refusals RUN-19 makes, at the layer that acts on the answer. The
    // policy is asked again rather than the drawing being read back: a front end answering
    // with a key the prompt never offered must not be able to put a line into a record that
    // outlives the session, and a guard that consulted what was drawn would be resting on the
    // very thing it is there to check. `may_be_added_to` is asked a second time too, inside
    // the store, for the mode that adds nothing to the state directory. The scope answer of
    // RUN-20 is asked of the table again by `scope_to_record`.
    if policy.may_remember(plan)
        && requested.is_empty()
        && !lent_a_kept_scope
        && !unconfined
        && let (Some(store), Some(session)) = (record, tools.remembering)
    {
        if let Some(line) = answer.line_to_record(plan) {
            store.remember(&line, session);
        } else if let Some(scope) = answer.scope_to_record(plan) {
            store.remember_scope(&scope, session);
        }
    }
}

/// What the planner is told when a run line failed before it finished.
///
/// A line is not known to have done nothing: a later pipeline stage fails after an earlier one was
/// spawned, a redirection's open fails after its program started (`true | cat > missing/out`), and
/// the right side of `a && b` fails after `a` has finished. So a failure says the line may have run
/// in part and the planner is asked to check before repeating it, except where the failure is one
/// that comes before its program starts.
///
/// A program refused for want of confinement did not start, and repeating the line is refused the
/// same way until a route the text names is taken or, where it names none, until the session's
/// settings or the platform change, since those decide the refusal (SANDBOX-17). A program
/// that could not be started did not start either. What else of the line ran is what exec recorded
/// on the error from the processes it spawned ([`crate::exec::StartedBefore`]): nothing, a part
/// joined before this program's pipeline, or the stages before it in that pipeline, which were
/// stopped. Only the last two say the line may have run in part, and only the first says the line
/// was not run: the others say the program was not started. `unconfined_offered` is
/// [`unconfined_is_offered`], so the planner is pointed at `'unconfined': true` and the mode `off`
/// only where the session would accept the request. Where the refusal is a closed network the
/// platform cannot deny, which the driver recorded on the error, those routes say the line then
/// runs with the network open (SANDBOX-29).
fn failed_line_text(
    said: &str,
    error: &crate::exec::ExecError,
    unconfined_offered: bool,
) -> String {
    use crate::exec::StartedBefore;
    let ran = |before: &StartedBefore| match before {
        StartedBefore::Nothing => "Nothing of the line started.",
        StartedBefore::EarlierPart => {
            "No stage of its pipeline started, so the line may have run in part only where a part \
             before it, joined with `&&`, `||` or `;`, or a `$(...)` it read, ran; check what that \
             part did before running the line again."
        }
        StartedBefore::EarlierStages => {
            "The stages before it in its pipeline had started and were stopped, so the line may \
             have run in part only in those stages and in a part before them; check what they \
             did before running the line again."
        }
    };
    // The line was not run only where nothing of it started; otherwise it is the program that
    // was not started, which the error's own text says.
    let head = |before: &StartedBefore| match before {
        StartedBefore::Nothing => format!("error: `{said}` was not run: {error}."),
        StartedBefore::EarlierPart | StartedBefore::EarlierStages => {
            format!("error: `{said}` stopped at a program that was not started: {error}.")
        }
    };
    match error {
        crate::exec::ExecError::NotConfined {
            before,
            network_closed,
            ..
        } => {
            let ran = ran(before);
            let head = head(before);
            let until = match unconfined_offered {
                true => "until the person makes one of the changes that follow.",
                false => "until the session's settings or the platform change.",
            };
            let routes = match unconfined_offered {
                true => {
                    " The person can let it start: run the same line with 'unconfined': true and \
                     say in 'why' that the sandbox could not confine it, and the person is asked \
                     to approve that one line, every time, to run with no sandbox; or the person \
                     can choose the sandbox mode off with /sandbox off or --sandbox off, where the \
                     administrator allows it."
                }
                false => "",
            };
            let open = match (unconfined_offered, network_closed) {
                (true, true) => {
                    " Either way the line runs with the network open, since only the sandbox \
                     holds it closed."
                }
                _ => "",
            };
            format!(
                "{head} {ran} Running the same line again is refused the same way \
                 {until}{routes}{open}"
            )
        }
        crate::exec::ExecError::NotStarted { before, .. } => {
            format!("{} {}", head(before), ran(before))
        }
        crate::exec::ExecError::Cancelled => format!(
            "error: `{said}` was started and then {error}; it may have run in part, so check \
             what it did before running it again"
        ),
        // Nothing of this pipeline started, so only a command earlier in a line joined with `&&`,
        // `||` or `;` can have run.
        crate::exec::ExecError::DirectoryGone => format!(
            "error: `{said}` was not run: {error}. Name a directory that \
             exists with the 'directory' argument. A command earlier in the line may already \
             have run"
        ),
        _ => format!(
            "error: `{said}` may have run in part: {error}; check what it did before running it \
             again"
        ),
    }
}

/// [`failed_line_text`] for a line `run` started, in the foreground or the background, after
/// noting a program refused for want of confinement. That refusal is a line the sandbox refused,
/// so it earns one request for a line with none, as a confined line that failed does
/// (SANDBOX-29). The note is read off the error's kind, which the driver chose.
fn noted_failure_text<S: Sink>(
    policy: &Policy<'_, S>,
    tools: &mut Tools<'_>,
    said: &str,
    error: &crate::exec::ExecError,
) -> String {
    if matches!(error, crate::exec::ExecError::NotConfined { .. }) {
        tools.confined_failures.note();
    }
    failed_line_text(said, error, unconfined_is_offered(policy, tools))
}

/// The entry a refusal to confine kept apart from its sentence, with control characters made
/// visible, as the person is told it (SANDBOX-24).
///
/// A `sandbox.network.deniedHosts` entry that is not a host name is a string a checkout wrote, so
/// it is never in the sentence the planner reads ([`failed_line_text`]). The error holds it beside
/// that sentence, and this is the only place it is read: the note goes to the person's screen and
/// the trail, and nothing branches on it.
fn entry_note(error: &crate::exec::ExecError) -> Option<String> {
    use crate::exec::{ExecError, SubstitutionFailure};
    match error {
        ExecError::NotConfined {
            entry: Some(entry), ..
        } => Some(format!(
            "sandbox.network.deniedHosts has an entry that is not a host name, so the line was \
             refused: {}",
            crate::confirm::printable(entry)
        )),
        ExecError::Substitution {
            failure: SubstitutionFailure::Inside(inner),
            ..
        } => entry_note(inner),
        _ => None,
    }
}

/// A failed line's result: the sentence for the planner, and where the error holds an entry for
/// the person alone, that entry in the note ([`entry_note`]).
fn failed_line_result(text: String, error: &crate::exec::ExecError) -> Produced {
    match entry_note(error) {
        Some(note) => Produced::refused_with_a_note(text, note),
        None => Produced::problem(text),
    }
}

/// Run a program, after a person approves the exact arguments.
///
/// The order is the whole of the safety argument, and it is the same order a write goes through:
///
/// 1. The plan is compiled from the planner's command line, which is untrusted.
/// 2. Each step's argv meets the rules a person wrote in advance, before its program name is
///    looked for, so a denied line is refused by the rule and not by what `$PATH` had to say
///    about it (PERM-7).
/// 3. Every program name is resolved **once**, to an absolute path.
/// 4. The person is shown that exact argv and that exact binary, and answers.
/// 5. The approval mints an endorsement bound to that exact plan, which is the steps, the join
///    shape, the directory and the files it writes, not the argv alone.
/// 6. `before_plan` consumes it, and only then does anything execute, by the resolved path.
///
/// Nothing here branches on untrusted content. The argv is the planner's own words, read through
/// the gate that says so and records it; what comes back from the program is never read by the
/// driver or the planner, and goes into a slot at the label the kernel fixed before it ran.
fn run<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let mut may_change_files = false;
    let mut produced = run_line(
        policy,
        tools,
        confirmer,
        reporter,
        arguments,
        &mut may_change_files,
    );
    produced.run_may_change_files = may_change_files;
    produced
}

/// The body of [`run`]. `may_change_files` is set once the line has compiled and the rules have
/// let it through, to whether nothing establishes that the plan leaves every file alone
/// ([`Policy::plan_may_change_files`]), and stays false for a line refused before that.
fn run_line<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    arguments: &Value,
    may_change_files: &mut bool,
) -> Produced {
    let Some(line) = argument(arguments, "command") else {
        return Produced::problem(
            "error: 'command' is required and must be a string holding one command line, \
             e.g. \"git log --oneline -50\"",
        );
    };

    let RunCall {
        limit,
        in_the_background,
        named_stdin,
        read_asked,
        requested,
        unconfined,
    } = match run_call(policy, tools, arguments) {
        Ok(call) => call,
        Err(produced) => return *produced,
    };

    // Assembled from the planner's own words, which are untrusted. A person reading the line at
    // the approval prompt is one destination for it, but it is not the only thing that happens to
    // it: `cmdline::compile` below searches these bytes and its refusal is an early return. A
    // witness saying the bytes may reach a screen does not say they may be examined on the way
    // (LABEL-6), so the gate here is the one that says the planner's own words may be read, and
    // records the read.
    let line = match policy.read_planner_argument("run", "command", &line) {
        Ok(line) => line,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    // Before the refusals below, so a line that is then refused still names what the planner wrote
    // (PANEL-16). Only the list the person reads is added to; nothing here changes the call.
    tools.note_command(&line);

    // A value that declared itself a credential, in the line itself. Refused here, before the
    // line is compiled, before anybody is shown it and before any of it runs: a line is the one
    // thing a turn writes that reaches all of what the clause names at once, since it goes into
    // the file a redirection opens, onto the prompt the person reads, onto the terminal the
    // program echoes it to, and into whatever the program is being asked to do with it.
    //
    // Only the declared half, and nobody is asked. A guess would need the run prompt to carry a
    // finding, which it does not, and a prompt that can be answered "run it anyway" is a prompt
    // a turn eventually gets past.
    let carried_in_the_line = policy.scan_a_command_line("run", &line);
    // Written down before the refusal below, for the reason a write tool writes one down before
    // its own: a finding is a record of where a secret went, and what was refused is as much a
    // thing the person's own machine saw as what went ahead (CRED-19). The line is not a file, so
    // the entry names it as the place rather than a path in the tree.
    tools
        .recording()
        .record(tools.workspace.root(), &carried_in_the_line.all());
    if !carried_in_the_line.refused().is_empty() {
        return credential_refusal_in_a_line(&carried_in_the_line);
    }

    // Present but not a string is refused rather than dropped. A field the driver quietly ignored
    // would run the line wherever the last call left off, which is the one place a planner that
    // bothered to name a directory cannot have meant. `null` is the exception and reads as absent,
    // because that is what filling an optional field in with nothing says.
    let named = match arguments.get("directory") {
        None | Some(Value::Null) => None,
        Some(Value::String(_)) => argument(arguments, "directory"),
        Some(_) => {
            return Produced::problem(
                "error: 'directory' must be a string naming a directory, relative to the \
                 workspace or inside a directory the user added",
            );
        }
    };

    // Whether the directory below is one this call named or the one the last call left, which a
    // refusal of a program named by a relative path says (CMDLINE-12). The planner's own argument.
    let carried = named.is_none();
    let (directory, told_directory) = match named {
        Some(proposed) => {
            // Read through the same gate the command line is, for the same reason: this is
            // resolved against the workspace and the driver branches on whether what it found is
            // a directory, which is a read rather than a delivery (LABEL-6). That it is also a
            // routing field shown in the approval prompt and endorsed with the plan (CMDLINE-12)
            // is what the prompt is for, not what authorises the inspection here.
            let dir = match policy.read_planner_argument("run", "directory", &proposed) {
                Ok(dir) => dir,
                Err(denial) => return Produced::problem(format!("refused: {denial}")),
            };
            // An effect, not a read. A program's relative writes land in the directory it runs in,
            // so a tree an `Edit` rule protects is not protected by a check that consults only the
            // `Read` rules: `npm install` in `vendor` writes throughout it without naming a file.
            if let Err(refusal) = refuse_denied_path(policy, tools.workspace, Purpose::Effect, &dir)
            {
                return Produced::problem(refusal);
            }
            let resolved = match tools.workspace.resolve(&dir) {
                Ok(path) => path,
                Err(escape) => {
                    return Produced::problem(format!(
                        "refused: {}",
                        workspace_failure(policy, "run", "directory", &escape, &dir)
                    ));
                }
            };
            if !resolved.is_dir() {
                return Produced::problem(format!("error: '{dir}' is not a directory"));
            }
            // What the planner is told the directory was: its own released argument, or as the
            // workspace spells a `..` that lands in an open directory, as the file tools do. The
            // landing is where a link led, and a link in a tree nobody vouched for chooses that name.
            let told = tools
                .workspace
                .climbed_to(&dir)
                .unwrap_or_else(|| dir.clone());
            (resolved, told)
        }
        None => {
            // The directory carried from the last call is checked as a named one is, before the
            // line is compiled and before anybody is asked: it may have been removed since.
            // Falling back to the workspace root means the next call is not refused the same way.
            if !tools.run_directory.landing.is_dir() {
                let gone = std::mem::replace(
                    tools.run_directory,
                    RunDirectory::at(tools.workspace.root()),
                );
                return Produced::problem(format!(
                    "error: the directory this line would run in, '{}', no longer exists. Name \
                     a directory with the 'directory' argument; without one the next call runs \
                     at the workspace root",
                    gone.told
                ));
            }
            let carried = tools.run_directory.clone();
            (carried.landing, carried.told)
        }
    };
    // The rules are handed to the compiler rather than consulted on the plan it hands back,
    // because a rule has to answer before the program is looked for (PERM-7): a denied line that
    // names something this machine does not have would otherwise be refused for not being found,
    // which is an answer about the machine in place of the person's own decision, and it arrives
    // without the "do not retry" a rule's refusal owes the planner.
    let mut rules = |program: &str, args: &[String]| policy.before_command_rules(program, args);
    // Only bypass mode may write `NAME=$VAR` or `NAME="$(cmd)"` in front of a program; every other
    // mode refuses `$` as it always has (CMDLINE-2).
    let expansions = if tools.permission_mode.get() == crate::PermissionMode::Bypass {
        crate::cmdline::Expansions::Assignments
    } else {
        crate::cmdline::Expansions::Refused
    };
    let mut plan = match crate::cmdline::compile_with(
        &line,
        &directory,
        tools.profile,
        &mut rules,
        expansions,
    ) {
        Ok(plan) => plan,
        // The refusal names the span that caused it, so the planner can rewrite that part rather
        // than guessing at the whole line. There is no degraded mode to fall back to.
        Err(crate::cmdline::Stopped::Compile(refused)) => {
            // Chosen from the planner's spelling, not from where the directory landed, because a
            // link in a tree nobody vouched for may lead to the root (LABEL-3). `told` is empty
            // at the root a turn starts at and where the planner named the directory as "".
            let looked_in = looked_for_in(
                &refused,
                (!told_directory.is_empty()).then_some(told_directory.as_str()),
                carried,
            );
            return Produced::problem(format!(
                "error: {}{looked_in}",
                refused.said(tools.beside.holds(Named::EditFile))
            ));
        }
        Err(crate::cmdline::Stopped::Rule(denial)) => return refused_by_a_rule(&denial),
    };

    // Asked of the plan with its patterns left as the line wrote them: the names a pattern took out
    // of a directory nobody may have vouched for do not decide it (CMDLINE-4). A plan that cannot
    // be paired with the line is one nothing is known about.
    *may_change_files = crate::cmdline::plan_as_written(&plan, tools.profile)
        .is_none_or(|written| policy.plan_may_change_files(&written));

    let told_in =
        (plan.directory.as_path() != tools.workspace.root()).then_some(told_directory.as_str());
    let Checked {
        reaches_anywhere,
        protected: protected_writes,
        destinations,
    } = match check_the_plan_before_asking(policy, tools, &plan, unconfined, told_in) {
        Ok(checked) => checked,
        Err(produced) => return *produced,
    };

    // What the planner named for standard input, turned into bytes and a label before anybody is
    // asked. Three gates, as a write's `contents_ref` has: the name is accepted as a reference
    // rather than read as content, the file behind it is opened if the slot was still deferring
    // it, and the kernel produces the bytes. They stay wrapped until the endorsement is consumed,
    // and nothing here looks at them.
    //
    // Resolved before the prompt because the *label* is what decides whether there is a prompt at
    // all: it goes on the plan below, [`Plan::releases_private`] reads it, and a private one is
    // put to a person every time (RUN-6). After the rules, though: a rule refusing the line is a
    // statement that it does not run, and a file opened for a run nobody is going to make is a
    // read of somebody's workspace that bought nothing. Read after the file is opened rather than
    // before, since a slot still holding only a path takes its label from the trust map as it
    // stands when it is read, and a plan built from the earlier label would be endorsed under a
    // label that no longer holds.
    let fed = match &named_stdin {
        None => None,
        Some(named) => {
            // The two routes RUN-4 names reach one descriptor, and one of them would lose
            // silently. Refused here, where the call is read, and again in exec, where the bytes
            // would otherwise be dropped into a run that looked like it had worked.
            if plan
                .steps()
                .iter()
                .flat_map(|step| &step.routes)
                .any(|route| matches!(route, bravebot_core::command::Route::Stdin { .. }))
            {
                return Produced::problem(
                    "error: give 'stdin_ref' or a '<' redirection, not both. They are two ways \
                     to fill the same standard input, and only one of them can be honoured.",
                );
            }
            let slot = match accept_named_reference(
                policy,
                tools.slots,
                "run",
                "stdin_ref",
                named,
                NamesToList::Text,
            ) {
                Ok(slot) => slot,
                Err(refusal) => return Produced::problem(refusal),
            };
            let opened = match materialise(
                policy,
                tools.workspace,
                tools.slots,
                "run",
                std::slice::from_ref(&slot),
            ) {
                Ok(opened) => opened,
                Err(refusal) => return Produced::problem(refusal),
            };
            let content = match policy.resolve("run", &slot, tools.slots) {
                Ok(content) => content,
                Err(denial) => return Produced::problem(format!("refused: {denial}")),
            };
            plan.stdin = Some(content.label());
            Some((slot, content, opened))
        }
    };

    // The record of lines somebody asked to be remembered past the session, where this session has
    // somewhere to keep one and somebody to have pressed the key. Read here rather than once at the
    // start of the session: the file belongs to every session begun in this directory, so a line
    // recorded a minute ago in another one is covered by this run, and one deleted a minute ago is
    // not. A session with nobody to put a prompt to reads nothing at all.
    let record = remembered_record(tools);
    let recalled = record.as_ref().map(|store| store.read());
    if let Some(lines) = &recalled {
        policy.recall(lines.clone());
    }

    // The other names of each file the line's audited steps read, so the trust map is asked about
    // the file and not only about the spelling the line used (TRUST-2, CMDLINE-8). Metadata of the
    // paths the line itself named, taken here because the policy crate reaches no file system.
    policy.learn_operand_names(operand_names_of(policy, tools.workspace, &plan));
    // The directories a file that a bare program name resolves to may not lie in, said before the
    // plan is judged by either road that reads them: whether the person is asked (inside
    // `settle_the_confinement`) and the label the output carries (`measure_a_run`). Taken now and
    // not when the turn began, because a directory can be added in the middle of one (CMDLINE-8).
    policy.set_unproven_directories(&unproven_directories(tools));

    let Settled {
        mut confinement,
        spends_a_request,
        sandbox_mode,
        unasked_writes,
        asking,
        covered_by_record,
        lent_a_kept_scope,
    } = match settle_the_confinement(
        policy,
        tools,
        &plan,
        &requested,
        unconfined,
        recalled.as_ref(),
    ) {
        Ok(settled) => settled,
        Err(produced) => return *produced,
    };

    if asking
        && let Err(produced) = ask_about_the_run(
            policy,
            tools,
            confirmer,
            &plan,
            told_in,
            record.as_ref(),
            &requested,
            unconfined,
            fed.as_ref().map(|(slot, _, _)| slot.to_string()),
            !protected_writes.is_empty(),
            destinations,
            &mut confinement,
        )
    {
        return *produced;
    }

    // The approval is what makes this plan trustworthy, and it is bound to this exact plan.
    policy.endorse_plan(&plan);

    let authority = policy.file_authority();
    // A vouch was for the programs as they ran under their profile with nothing added, so what a
    // line prints that was lent a credential, or run with no sandbox, is not trusted on its account.
    // That includes a credential a person kept at an earlier prompt (SANDBOX-27).
    let requesting = !requested.is_empty() || unconfined || lent_a_kept_scope;

    // Shared: the revision and the label `before_plan` reads from the map have to be of one state,
    // which an effect entering would break, and nothing here enters one. The gates (capability,
    // a plan with steps, the single-use grant) read no file and run before the boundary, and the
    // trail line is written after it, so no sink is called while a writer waits. Inside it go the
    // revision and the map lookup, which open no file and wait for nothing.
    if let Err(denial) = policy.gate_a_run(&plan) {
        return Produced::problem(format!("refused: {denial}"));
    }
    let (started_revision, measured) = policy.capture_files_shared(|policy| {
        let revision = authority.revision();
        (revision, policy.measure_a_run(&plan, requesting))
    });
    let label = match measured {
        Ok(measured) => policy.record_a_run(measured),
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    if spends_a_request {
        tools.confined_failures.spend();
    }
    if let Some(mode) = sandbox_mode {
        policy.record_sandbox_mode(mode);
    }
    if unasked_writes {
        policy.record_sandbox_unasked_writes();
    }

    // The tree comes with the line wherever the line is said, and only where it is not the root.
    // The directory persists across calls, so a planner whose earlier call has been summarised away
    // by a compaction has nothing else left in its context saying where the next one lands, and a
    // person asked to release what this printed would otherwise not be told which tree it came out
    // of. The planner is told the directory as it typed it: where a link in the tree led is a name
    // whoever wrote the tree chose, and a sentence the driver writes is read as the driver's own
    // (LABEL-3, TOOL-4). The person is shown the landing, which is what the line runs in. The
    // test below is of the landing against the workspace root, two paths the driver resolved, and
    // reads nothing of the directory's name.
    let at_the_root = plan.directory.as_path() == tools.workspace.root();
    let in_directory = |line: String, spelled: &str| match at_the_root {
        true => line,
        false => format!("{line} (in {spelled})"),
    };
    let displayed = in_directory(
        plan.display(),
        &tools.workspace.relative_display(&plan.directory),
    );
    // What the planner is told the line was: the plan with its operands as the line wrote them. A
    // glob in the line has been replaced by the names it matched, which came out of a directory
    // nobody may have vouched for, and the planner is not shown those (CMDLINE-4).
    let said = in_directory(
        crate::cmdline::display_as_written(
            &plan,
            (!at_the_root).then_some(told_directory.as_str()),
        ),
        &told_directory,
    );

    // What this line reaches that nothing here holds, which the person approving it was shown at
    // the prompt. Worked out from the same plan the prompt drew, so the record and the grant
    // cannot name different things, and recorded below at each point the line actually starts: a
    // grant is a sentence somebody answered and a use is a thing that happened.
    let spends = bravebot_core::ambient::spent_by(&plan);
    let started = JobStart {
        spends: &spends,
        plan: &plan,
        displayed: &displayed,
        said: &said,
        told_directory: &told_directory,
        label,
        authority: &authority,
        started_revision,
        covered_by_record,
        confinement: confinement.as_ref(),
        moved_after: None,
    };

    // Recorded per line, before anything starts, so a line a stage of which was refused still says
    // what the network was for it. Nothing for an open network, which is what every line had.
    if let Some(detail) = confinement
        .as_ref()
        .and_then(|confinement| confinement.network_for_the_trail(&plan.steps()))
    {
        policy.record_run_network(detail);
    }
    if let Some(detail) = confinement
        .as_ref()
        .and_then(|confinement| confinement.requested_for_the_trail(&plan.steps()))
    {
        policy.record_requested_scopes(detail);
    }
    if let Some(detail) = confinement
        .as_ref()
        .and_then(|confinement| confinement.kept_for_the_trail(&plan.steps()))
    {
        policy.record_kept_scopes(detail);
    }

    // A program opens files the line never names, so nothing tells the authority which it rewrites.
    // Counted for every line that gets this far, started or not, so a slot read before it is no
    // longer taken for a copy of whatever file it was read from. A number, never what ran.
    authority.note_program_started();

    if in_the_background {
        // What has to be refused is what start_steps_within cannot honour, and it honours no
        // route at all, including `2>&1`, which names nothing for anybody to endorse. A route on
        // a step inside a `$(...)` of the line is one as well, since start_steps_within opens it
        // with none of the checks a foreground line makes when it opens a route.
        let Some(steps) = plan.steps.unrouted_pipeline() else {
            return Produced::problem(
                "error: a background command must be one pipeline with no redirection, \
                 including one that names no file. Run the parts separately, or run this one \
                 in the foreground.",
            );
        };

        tools
            .workspace
            .mark_rewind_gap(crate::rewind::CoverageGap::Command);
        // The tree as the line finds it, compared when the job's end is learned (GROUND-1).
        let tree_before = TreeBefore::take(tools.workspace);
        return match crate::exec::start_steps_within(
            steps,
            &plan.directory,
            tools.workspace.own_scratch(),
            confinement.as_ref(),
            tools.cancel,
            limit,
        ) {
            Ok(running) => {
                // Spent at the moment the programs start, which for a background line is here:
                // it outlives this call, and nothing later in the turn knows what it reached.
                let (name, _stop) =
                    start_the_job(policy, tools, reporter, running, &started, tree_before);
                Produced::new(
                    // Nothing has been printed yet, and the label is the one the kernel fixed
                    // before anything started: leaving it running does not make it trustworthier.
                    Labelled::new(String::new(), label),
                    format!("`{said}` started in the background"),
                    format!("started as {name}"),
                )
                .started_in_the_background(name, tools.jobs.is_kept(), tools.beside)
                .having_run_a_program()
            }
            // A program refused for want of confinement is told as a foreground one is and earns
            // the same request (SANDBOX-17, SANDBOX-29). Read off the error's kind, which the
            // driver chose.
            Err(error) => {
                let text = match &error {
                    crate::exec::ExecError::NotConfined { .. } => {
                        noted_failure_text(policy, tools, &said, &error)
                    }
                    _ => format!("error: `{said}` did not start: {error}"),
                };
                failed_line_result(text, &error).having_started_a_program()
            }
        };
    }

    // Unwrapped here and nowhere earlier: the endorsement has been consumed, so the line about to
    // read these bytes is one a person approved. The witness is what licenses the unwrapping, and
    // what it licenses is carrying them to a descriptor: no branch below reads them, and exec
    // writes them to a pipe without looking either.
    let supplied = fed.map(|(slot, content, read)| {
        let proof = policy.authorise_program_input("run", &slot, content.label());
        (content.declassify(&proof), read)
    });
    // A redirection is a write, so the destination is reserved and marked untrusted before the
    // line may open it, and what it holds afterwards is decided when the line has stopped. Entered
    // from what the run is about to open, never from the plan's write set: the set names every
    // branch, so a destination a line decided against is in it, and a rule about a file nothing
    // wrote would quarantine a file the planner can read today.
    //
    // Keyed the way the authority keys it, so the two spellings of one destination in
    // `> out 2> ./out` are one effect rather than a second entry refused by the first.
    let mut effects = std::collections::BTreeMap::new();
    // Every key an effect of this line holds, a destination's other names included, and how many
    // there are: each one advances the authority's revision once when it is entered.
    let mut entered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // Entered for names a destination gained after its first open, because a part of the line
    // replaced it with a link in between. Completed untrusted, and by the drop of an effect that
    // was not completed (TRUST-4).
    let mut gained: Vec<(bravebot_core::file_authority::FileEffect, Vec<String>)> = Vec::new();
    // What each of those destinations held before the line opened it, taken at the same moment
    // and for the credential scan below: there is no later moment to ask, because afterwards
    // every answer is the line's own.
    let mut standing: Vec<Standing> = Vec::new();
    let folds = crate::workspace::volume_case_folding(tools.workspace.root()).restricts();
    tools
        .workspace
        .mark_rewind_gap(crate::rewind::CoverageGap::Command);
    // Offered to the person only for a line a job can hold, and only where they are watching it.
    // A line fed a reference would have its bytes written into a job nobody waits for, and a
    // delegate's line is not the one the screen shows running. A line that asked to read what it
    // printed is offered: the moved result says where its output is, so the planner reads it with
    // `job_output`.
    let movable =
        !tools.delegated && supplied.is_none() && plan.steps.unrouted_pipeline().is_some();
    // The tree as the line finds it, for the comparison that says whether it changed a file
    // (GROUND-1). A measurement of names, lengths and times: no file is read. Taken before the
    // line starts in every case, since whether it moves to the background is known only after it
    // has started. A line that stops in the foreground is compared below; a moved line carries
    // this mark into its job, which compares it when the job's end is learned.
    let tree_before = TreeBefore::take(tools.workspace);
    let ran = if movable {
        let handoff = bravebot_core::cancel::Handoff::new();
        reporter.movable(handoff.clone());
        match crate::exec::run_plan_movable(
            &plan,
            tools.cancel,
            &handoff,
            limit,
            tools.workspace.own_scratch(),
            confinement.as_ref(),
        ) {
            Ok(crate::exec::Waited::Moved(moved)) => {
                // Everything the background branch above does once its line has started, at the
                // moment this one stopped being waited for. The label goes with it unchanged: a
                // person pressing a key says nothing about what the line printed.
                let started = JobStart {
                    moved_after: Some(moved.after),
                    ..started
                };
                let (name, _stop) = start_the_job(
                    policy,
                    tools,
                    reporter,
                    moved.running,
                    &started,
                    tree_before,
                );
                return Produced::new(
                    Labelled::new(String::new(), label),
                    format!("`{said}` moved to the background"),
                    format!(
                        "moved to the background after {:.1}s, as {name}",
                        moved.after.as_secs_f64()
                    ),
                )
                .moved_to_the_background(name, moved.after, tools.jobs.is_kept(), tools.beside)
                .having_run_a_program();
            }
            Ok(crate::exec::Waited::Ran(ran)) => Ok(ran),
            Err(error) => Err(error),
        }
    } else {
        // Both open-time hooks ask the policy, and each is called between the other's calls.
        let asked = std::cell::RefCell::new(&mut *policy);
        crate::exec::run_plan_observed_reading(
            &plan,
            tools.cancel,
            limit,
            tools.workspace.own_scratch(),
            supplied.as_ref().map(|(bytes, _)| bytes.as_str()),
            confinement.as_ref(),
            &mut |path| {
                let mut asking = asked.borrow_mut();
                let policy = &mut **asking;
                // Confined again where it lands now, since the parts of the line before this one
                // have run since the check above and may have left a link at this name. At every
                // open and not only the first, because a part run between two opens of one
                // destination can have replaced it with a link (CMDLINE-5). Not for a line whose
                // programs have no profile, for the reason the check above is not.
                if !reaches_anywhere && let Err(escape) = tools.workspace.confines_writing(path) {
                    let target = told_destination(
                        tools.workspace,
                        path,
                        &plan.directory,
                        (!at_the_root).then_some(told_directory.as_str()),
                    );
                    let detail = workspace_failure(policy, "run", "command", &escape, &target);
                    return Err(crate::exec::ExecError::Redirection {
                        path: target,
                        detail,
                    });
                }
                // The rules written in advance, asked again about the file the destination lands
                // on now. A link or hard link an earlier part of the line left at this name
                // reaches a file the check before the line did not see, and a rule covers the
                // file and not the name it was approved under. At every open, a destination
                // already entered included, and whether or not the line has a profile: the
                // person's deny rules and a delegate's `writes` limit refuse a path either way
                // (CMDLINE-5, PERM-7, DELEGATE-28).
                let target = told_destination(
                    tools.workspace,
                    path,
                    &plan.directory,
                    (!at_the_root).then_some(told_directory.as_str()),
                );
                if let Err(refusal) = refuse_redirection_target_by_rules(
                    policy,
                    tools.workspace,
                    Purpose::Effect,
                    path,
                    &target,
                ) {
                    return Err(crate::exec::ExecError::Redirection {
                        path: target,
                        detail: refusal,
                    });
                }
                // A destination that is a protected path now and was not when the line was
                // checked: an earlier part of the line left a link or hard link at the name, and
                // the person was never shown the file it reaches (PERM-9). At every open, and
                // whether or not the line has a profile. After the rules above, so a deny rule
                // or a delegate's limit still names itself as the reason. Not under bypassing,
                // which approves a run redirecting into a protected path unasked (MODE-4), so the
                // same write is not refused for having been reached through a link.
                if tools.permission_mode.get() != crate::PermissionMode::Bypass
                    && !protected_writes.iter().any(|checked| checked == path)
                    && redirection_is_protected(tools.workspace, path)
                {
                    return Err(crate::exec::ExecError::Redirection {
                        detail: format!(
                            "refused: {target} now reaches a protected path, which the person was not shown when this line was approved. Nothing was written"
                        ),
                        path: target,
                    });
                }
                let own = tools.workspace.trust_key(&path.to_string_lossy());
                let key = authority.key(&own);
                // The file's other names: where a link lands, its hard links and a second path to
                // the tree. Looked up at every open, since a part run between two opens of one
                // destination can have made it a link, and before the boundary is taken, since the
                // walk reads directories (TRUST-4, CMDLINE-5).
                let others = tools
                    .workspace
                    .other_trust_keys(&tools.workspace.relative_display(path));
                // And each name a read of the file is asked under (`Workspace::names_reached`): the
                // spelling with each `..` taken out, which the file tools refuse to read the file
                // under as typed, and the place the path lands. Built from the redirection's own
                // text and the file system, never from a byte of any file.
                let mut named: Vec<String> = others.keys.clone();
                named.extend(
                    tools
                        .workspace
                        .names_reached(path)
                        .iter()
                        .map(|reached| tools.workspace.trust_key(reached)),
                );
                named.retain(|name| *name != own);
                named.sort();
                named.dedup();
                // `every` is each other name the file has. `fresh`, below, is the subset of them
                // this line has not entered yet: a name an earlier destination entered is held by
                // that effect and would be refused as a second writer.
                let mut every: Vec<String> = named
                    .iter()
                    .map(|other| authority.key(other))
                    .filter(|other| *other != key)
                    .collect();
                every.sort();
                every.dedup();
                // A destination this line already entered under this spelling gains only the names
                // it was not entered under. A new one reserves every name, so another spelling of
                // a file the line holds, or a second path to it, is refused as a second writer
                // (`> d/../dd.txt` then `> dd.txt`). Held by the effects and not by `entered`,
                // which also holds the other names of an earlier destination.
                let fresh: Vec<String> = match effects.contains_key(&key) {
                    true => every
                        .iter()
                        .filter(|other| !entered.contains(*other))
                        .cloned()
                        .collect(),
                    false => every.clone(),
                };
                // Whatever the line's label: a line that stops short leaves every destination
                // untrusted, and whether it will is not known until it has (MEMORY-5).
                for name in std::iter::once(&key).chain(&fresh) {
                    crate::memory::record_before_write(tools.workspace.memories(), name, folds)
                        .map_err(|e| crate::exec::ExecError::Io(e.to_string()))?;
                }
                if effects.contains_key(&key) {
                    // A destination this line entered under this spelling. It gains a name only
                    // where a part of the line left a link at it since the first open, and that
                    // name is entered now and left untrusted: what the line writes through the link
                    // is not what the destination was approved for (TRUST-4).
                    if fresh.is_empty() {
                        return Ok(());
                    }
                    return policy.capture_files(|_, capture| {
                        let names: Vec<&str> = fresh.iter().map(String::as_str).collect();
                        let effect = capture.begin_all(&names).ok_or_else(|| {
                            crate::exec::ExecError::Io(
                                "another file effect is still writing this destination".to_string(),
                            )
                        })?;
                        entered.extend(fresh.iter().cloned());
                        gained.push((effect, fresh.clone()));
                        Ok(())
                    });
                }
                policy.capture_files(|policy, capture| {
                    let prior =
                        if !policy.read_is_quarantined_under_names(&key, &named, others.complete) {
                            bravebot_core::label::Integrity::Trusted
                        } else {
                            bravebot_core::label::Integrity::Untrusted
                        };
                    let names: Vec<&str> = std::iter::once(key.as_str())
                        .chain(fresh.iter().map(String::as_str))
                        .collect();
                    let effect = capture.begin_all(&names).ok_or_else(|| {
                        crate::exec::ExecError::Io(
                            "another file effect is still writing this destination".to_string(),
                        )
                    })?;
                    entered.insert(key.clone());
                    entered.extend(fresh.iter().cloned());
                    // Kept only where the scan below could reach this destination, which is where
                    // what the line leaves in it will be at an integrity the driver may read.
                    // Reading a destination a line nobody vouched for is about to truncate buys
                    // nothing and costs the file twice over, and the label here can only fall
                    // further when the line has stopped, so a destination ruled out now stays ruled
                    // out. Why it was is in the trail already, beside the line's own label.
                    let held = match prior.meet(label.integrity) {
                        bravebot_core::label::Integrity::Trusted => {
                            crate::workspace::kept(path, MAX_SCANNED_BYTES)
                        }
                        bravebot_core::label::Integrity::Untrusted => {
                            crate::workspace::Before::NotKept
                        }
                    };
                    standing.push(Standing {
                        key: key.clone(),
                        others: every.clone(),
                        shown: tools.workspace.relative_display(path),
                        told: told_destination(
                            tools.workspace,
                            path,
                            &plan.directory,
                            (!at_the_root).then_some(told_directory.as_str()),
                        ),
                        held,
                        resolved: path.to_path_buf(),
                        prior,
                    });
                    effects.insert(key, (effect, prior));
                    Ok(())
                })
            },
            &mut |path| {
                // Read again where it lands now, for the reason a destination is confined again
                // above: an earlier part of the line may have left a link at this name that leads
                // into the stores, which the check before the line could not see (RECALL-8).
                let target = told_destination(
                    tools.workspace,
                    path,
                    &plan.directory,
                    (!at_the_root).then_some(told_directory.as_str()),
                );
                let lands = crate::workspace::destination(path);
                if lands.is_some_and(|lands| tools.workspace.in_the_stores(&lands)) {
                    return Err(crate::exec::ExecError::Redirection {
                        path: target,
                        detail: "refused: kept facts, rules and session records are not read by \
                                 a command line"
                            .to_string(),
                    });
                }
                // And the rules written in advance, asked about the file the source lands on now,
                // as for a destination: a `Read` deny rule covers the file and not the name the
                // line was approved under (CMDLINE-5, PERM-7).
                let mut asking = asked.borrow_mut();
                let policy = &mut **asking;
                if let Err(refusal) = refuse_redirection_target_by_rules(
                    policy,
                    tools.workspace,
                    Purpose::Read,
                    path,
                    &target,
                ) {
                    return Err(crate::exec::ExecError::Redirection {
                        path: target,
                        detail: refusal,
                    });
                }
                Ok(())
            },
        )
    };
    // After the line has stopped, so every request its programs made has been decided. Taken
    // whether or not the line ended well: a program that asked for a host and then failed has
    // still asked.
    if let Some((confinement, seen)) = confinement.as_ref().and_then(|confinement| {
        confinement
            .hosts_seen(&plan.steps())
            .map(|seen| (confinement, seen))
    }) {
        policy.record_hosts(seen.detail);
        if confinement.asks_about_unlisted_hosts() && !tools.delegated {
            ask_about_hosts(policy, tools, confirmer, &seen.unlisted);
        }
    }
    // A proof about inputs before execution cannot label output captured beside a write.
    // Our own effect entries each advance the revision once and are accounted for separately.
    let label = if authority.is_current(started_revision.wrapping_add(entered.len() as u64)) {
        label
    } else {
        Label::new(
            bravebot_core::label::Integrity::Untrusted,
            label.confidentiality,
        )
    };
    // Before the effects are completed, so every destination is still reserved while it is read
    // and while a refused one is put back: the reservation is what stops anything else reading a
    // file in the state the line left it in. Asked whether or not the line ended well, since a
    // line that failed halfway has still opened and written whatever it got to.
    let left = what_the_line_left(policy, &standing, label);
    // And written down, wherever the line put it. A destination put back below held the value
    // while the line ran, so it is a thing the person's tree has had in it whatever the refusal
    // then did about it, and a line drawn while nobody was looking is gone when the turn ends
    // (CRED-19).
    tools
        .recording()
        .record(tools.workspace.root(), &left.scanned.all());
    let mut stuck: Vec<String> = Vec::new();
    for destination in &left.refused_at {
        if tools
            .workspace
            .put_back(&destination.resolved, &destination.held, &mut *confirmer)
            .is_err()
        {
            stuck.push(destination.told.clone());
        }
    }
    // After a destination a scan refused has been put back. One the line created is removed again,
    // so that write is not counted; one that existed is written back with its old bytes and a new
    // modification time, so the mark reads it as changed and it is counted. Whether the line ended well does not enter into it: a line that failed halfway has
    // still left what it got to.
    let changed_a_file = tree_before.differs_now();
    let gained_names: Vec<String> = gained
        .iter()
        .flat_map(|(_, names)| names.iter().cloned())
        .collect();
    if ran
        .as_ref()
        .is_ok_and(|ran| ran.ended_well && ran.succeeded())
    {
        let refused: std::collections::BTreeSet<&str> = left
            .refused_at
            .iter()
            .map(|destination| destination.key.as_str())
            .collect();
        for (key, (effect, prior)) in effects {
            // A destination put back holds what it held before the line ran, so what the map
            // records about it is what it recorded before the line ran. Taking the meet there
            // would quarantine a file on the strength of a write that is no longer in it.
            let integrity = match refused.contains(key.as_str()) {
                true => prior,
                false => prior.meet(label.integrity),
            };
            effect.complete(integrity);
        }
        // A name a destination gained after its first open is left untrusted: the yes to the line
        // was about the destination as it stood, and what the line wrote through the link is not
        // known to be what that approval covered (TRUST-4).
        for (effect, _) in gained.drain(..) {
            effect.complete(bravebot_core::label::Integrity::Untrusted);
        }
    }
    for name in &gained_names {
        crate::memory::after_write(policy, tools.workspace.memories(), name, folds);
    }
    for destination in &standing {
        crate::memory::after_write(policy, tools.workspace.memories(), &destination.key, folds);
        for other in &destination.others {
            crate::memory::after_write(policy, tools.workspace.memories(), other, folds);
        }
        // The line is the planner's own words, so the name in it is one it typed. Recorded by
        // `told`, the directory spelled as typed, and not by `shown`, which holds the place a link
        // in the tree led (CHECKOUT-13, LABEL-3). Recorded only where the line left a file, so a
        // destination it failed to open, or one the scan took back out, is not reported as
        // written.
        let refused = left
            .refused_at
            .iter()
            .any(|refused| refused.key == destination.key);
        if !refused && std::fs::symlink_metadata(&destination.resolved).is_ok() {
            tools.workspace.record_write(Some(&destination.told));
        }
    }
    // Spent, on the reading that the line ran. What it exited with does not enter into it, because
    // a program that reached a daemon and then failed has still reached it, and neither does what
    // the scan below makes of what it left: a refusal is said after the line, and does not undo the
    // use. So it comes before that return rather than inside the arm below (CRED-5).
    if ran.is_ok() {
        policy.record_ambient(&spends);
    }
    if !left.scanned.refused().is_empty() {
        // A program in the line put the credential there, so one ran whatever the line ended in
        // (TURN-4).
        let refusal = credential_refusal_after_a_line(&said, &left, &stuck).having_run_a_program();
        return match changed_a_file {
            true => refusal.having_changed_a_file(),
            false => refusal,
        };
    }

    match ran {
        Ok(ran) => {
            // Carried over only once the line has actually run, which is where the background
            // branch carries it too: a line whose stages never started moved nothing, and a turn
            // whose working directory had followed a run that did not happen would land the next
            // line somewhere nobody chose.
            *tools.run_directory = RunDirectory {
                landing: plan.directory.clone(),
                told: told_directory.clone(),
            };

            // Both streams carry the revalidated plan label; their bytes decide nothing.
            let text = crate::exec::both_streams(&ran.stdout, &ran.stderr);

            // A line with a sandbox that did not exit zero earns one request for a line with none.
            // Read off the exit codes and whether the deadline stopped it, not off what it printed.
            if confinement.is_some() && ran.stopped.is_none() && !ran.stages_ended_well() {
                tools.confined_failures.note();
            }

            // Capped only where the planner may read it. Output it may not read is quarantined
            // whole, so nothing of it enters the conversation and there is nothing to bound.
            let sample = if label.is_trusted() {
                bounded(&text, tools.output_cap)
            } else {
                None
            };
            let (text, whole) = match sample {
                // The cap bounds the conversation, not the run. What was printed is kept whole
                // beside the sample, so the middle is still there to read a page at a time, hand to
                // a processor or write to a file, and nothing has to be run twice to see it.
                Some(sample) => (sample, Some(Labelled::new(text, label))),
                None => (text, None),
            };

            // Said in the driver's own words, from the exit codes and the clock, which are
            // structure rather than content: nothing here reads a byte of what the program
            // printed.
            let outcome = if let Some(signal) = ran.stopped_by_signal {
                crate::report::Outcome::StoppedBySignal {
                    ran_for: ran.stopped.unwrap_or_default(),
                    signal: signal.name(),
                }
            } else if let Some(after) = ran.stopped {
                // Told apart by the deadline the run was given and the ceiling in force, two
                // figures a person set, and not by anything the program printed. At the ceiling a
                // longer deadline cannot be asked for, so the planner is sent to the background.
                if limit >= tools.deadlines.ceiling {
                    crate::report::Outcome::StoppedAtTheCeiling(after)
                } else {
                    crate::report::Outcome::Stopped(after)
                }
            } else {
                // The codes and not `ended_well`: that is the verdict of the part that ran last, so
                // `make test ; echo done` would be told it exited 0 with the failed stage unnamed
                // (RUN-13).
                let confinement = confinement.as_ref().map(|confinement| {
                    confinement
                        .profile_offering(&plan.steps(), unconfined_is_offered(policy, tools))
                });
                how_it_ended(
                    &ran.codes,
                    &ran.signals,
                    &ran.closed,
                    confinement.as_deref(),
                )
            };
            let lines = text.lines().count();
            let mut note = format!("{}, {}", outcome.summary(), tally(lines, "line", "lines"));
            // A read the planner's own call did not make and nothing else on the screen would show:
            // the reference was standing in for a file the slot had not opened yet, and filling
            // standard input is what opened it.
            if let Some((_, read)) = supplied.as_ref().filter(|(_, read)| !read.is_empty()) {
                note.push_str(&format!(", fed by reading {}", read.join(", ")));
            }
            // A value the destination already held is reported and not refused, exactly as it is
            // for a write tool: a line that reformats or moves a file holding a key carries it
            // without having written it, and the person is the one who can decide what to do
            // about a secret that was in their tree before this session started.
            let note = inferred_note(carried_note(note, &left.scanned), &left.scanned);

            let mut produced = Produced::new(
                Labelled::new(text, label),
                format!("what `{said}` printed"),
                note,
            )
            .of_content()
            .capped(whole.is_some());
            produced.whole = whole;
            // Marks the block the person is shown as content nobody vouched for, which is what a
            // program's output is: it may include bytes an earlier step read out of a file an
            // attacker wrote.
            produced.untrusted = !label.is_trusted();
            // What the slot will be told it came from, so the user can be asked to read it later
            // and can see which command they are reading, beside how it went.
            produced.printed_by = Some(crate::report::Command {
                line: displayed.clone(),
                outcome,
                job: None,
            });
            produced.covered_by_record = covered_by_record;
            produced.read_asked = read_asked;
            produced.ran_a_program = true;
            produced.changed_a_file = changed_a_file;
            produced.command_failed = !ran.succeeded();
            // What the line created, from its own words and only where it ran and exited cleanly
            // (PANEL-18). The line is the one the planner wrote, which the gate above released.
            if ran.succeeded() && tools.keeping_links {
                produced.created = crate::links::creation_in_command(&line);
            }
            produced.started_a_program = true;
            produced.ran_git_in = plan
                .steps()
                .into_iter()
                .filter(|step| runs_git(step))
                .map(|step| git_directory(step, &plan.directory))
                .collect();
            produced
        }
        // A run that produced nothing still says what happened. The plan is safe to repeat back:
        // a person endorsed it, so it is not something an attacker chose.
        // A stage may have spawned before the one that failed, so the checkout is marked. A line is
        // stopped only once its stages have been spawned, so one the turn stopped has run (TURN-4).
        // The file mark is carried in every arm: a line that failed halfway has still left what it
        // got to (GROUND-1).
        Err(error) => {
            let stopped = matches!(error, crate::exec::ExecError::Cancelled);
            let error = told_failure(
                tools.workspace,
                error,
                &plan.directory,
                (!at_the_root).then_some(told_directory.as_str()),
            );
            let text = noted_failure_text(policy, tools, &said, &error);
            let mut produced = failed_line_result(text, &error);
            produced.changed_a_file = changed_a_file;
            match stopped {
                true => {
                    let mut produced = produced.having_run_a_program();
                    produced.command_failed = true;
                    produced
                }
                false => produced.having_started_a_program(),
            }
        }
    }
}

/// Put the hosts a line's programs were refused for want of an entry to the person, once the line
/// has stopped (SANDBOX-24, `onUnlisted: ask`).
///
/// Nothing waits on the answer: the programs were refused already, and a yes lets the lines that
/// follow reach the hosts. The hosts are a program's own bytes, checked to be host names before
/// they get here, and they go to the person's screen and the trail and no further, so the planner
/// is told nothing about them and nothing here branches on one. A host already answered this
/// session is not asked again, and no more than [`crate::confine::MOST_HOSTS_ASKED`] are asked at
/// once. The mode that asks nothing refuses without remembering it, so a later switch to a mode
/// that asks still asks, and a delegate is not offered the question.
fn ask_about_hosts<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &Tools<'_>,
    confirmer: &mut C,
    unlisted: &[String],
) {
    let mut hosts = tools.workspace.unanswered_hosts(unlisted);
    hosts.truncate(crate::confine::MOST_HOSTS_ASKED);
    if hosts.is_empty() {
        return;
    }
    if tools.permission_mode.get() == crate::PermissionMode::Bypass {
        policy.record_host_answer(&hosts, false);
        return;
    }
    let request = crate::confirm::HostRequest {
        hosts: hosts.clone(),
    };
    let granted = confirmer.confirm_host(&request) == Decision::Approve;
    if granted {
        tools.workspace.grant_hosts(&hosts);
    } else {
        tools.workspace.decline_hosts(&hosts);
    }
    policy.record_host_answer(&hosts, granted);
}

fn runs_git(step: &bravebot_core::command::Step) -> bool {
    step.resolved.file_stem().is_some_and(|stem| stem == "git")
}

/// The directory a git stage works in: where the line ran, moved by each `-C` that leads its
/// arguments, as git moves it. `-c name=value` before them is passed over. `--git-dir`,
/// `--work-tree` and `GIT_DIR` are not followed.
fn git_directory(
    step: &bravebot_core::command::Step,
    line: &std::path::Path,
) -> std::path::PathBuf {
    let mut directory = line.to_path_buf();
    let mut args = step.args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-C" => match args.next() {
                Some(path) => directory = directory.join(path),
                None => break,
            },
            "-c" => {
                args.next();
            }
            _ => break,
        }
    }
    directory
}

/// What every fetch asks for, in the order it prefers.
///
/// A fixed string in this program's own source. The planner's `url` argument takes no part in it,
/// no reply can change it, and every hop of every fetch sends the same bytes, so nothing a server
/// writes decides what the next request asks for.
///
/// Markdown first because the body is read by a processor and nothing else: the same page as
/// Markdown is a fraction of the tokens its HTML is, and a server that can serve both otherwise
/// serves HTML.
///
/// Nothing else is ranked. `*/*` carries one weight below Markdown, so a server holding no
/// Markdown answers with whatever it would have answered before this header existed. Naming
/// `text/html` above `*/*` would rank the rest, and an endpoint that negotiates would then read
/// this as a request for its HTML page where it used to answer with the compact JSON a processor
/// reads for fewer tokens.
const FETCH_ACCEPT: &str = "text/markdown, */*;q=0.9";

/// The refusal a `deny` rule gives a URL (FETCH-2), shared by `fetch_url` and `download_url`.
///
/// Before the person is asked. A rule refusing something is a statement that it does not happen,
/// and there is nothing to show or approve once it has been made. The answer is the policy's, so
/// no labelled content is read here.
fn fetch_rules_refusal<S: Sink>(policy: &mut Policy<'_, S>, url: &str) -> Option<Produced> {
    match policy.before_fetch_rules(url) {
        Ok(()) => None,
        Err(denial) => Some(Produced::problem(format!(
            "refused: {denial}. Do not retry this URL and do not look for another route to that \
             host: say in your reply what you needed from it."
        ))),
    }
}

/// [`fetch_not_approved`] for `fetch_url`, which is also the one fetch the classifier is put the
/// first question about a URL of (AUTO-3). Asked only where the policy would ask the person.
fn fetch_url_not_approved<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    url: &str,
    host: &str,
) -> Option<Produced> {
    let classified = match policy.fetch_needs_approval(url) {
        true => consult_about_a_fetch(policy, tools, url, host),
        false => crate::confirm::Classified::NotAsked,
    };
    fetch_not_approved(
        policy,
        confirmer,
        "fetch_url",
        url,
        host,
        classified,
        tools.permission_mode.get(),
    )
}

/// Puts a host that needs approval to the person (FETCH-2, FETCH-4) and returns the refusal when
/// they reject it, shared by `fetch_url` and `download_url`, which `call` names for the trail
/// (TRACE-1). The answer is the person's, so no labelled content is read here.
fn fetch_not_approved<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    confirmer: &mut C,
    call: &str,
    url: &str,
    host: &str,
    classified: crate::confirm::Classified,
    mode: crate::PermissionMode,
) -> Option<Produced> {
    if !policy.fetch_needs_approval(url) {
        return None;
    }
    let request = crate::confirm::FetchRequest {
        url: url.to_string(),
        host: host.to_string(),
        redirected_from: None,
        classified,
    };
    let answer = confirmer.confirm_fetch(&request);
    // Recorded for auto alone: no other mode's fetch answer has ever been, and its trail is not
    // changed here (AUTO-8).
    if mode == crate::PermissionMode::Auto {
        policy.record_answer(
            mode.answers_this_fetch_unasked(&request)
                .then(|| mode.name()),
        );
    }
    if answer != Decision::Reject {
        return None;
    }
    policy.refuse_unapproved_fetch(call);
    Some(Produced::problem(
        "refused: the user did not approve fetching this. Do not retry the same URL; \
         ask what they would prefer."
            .to_string(),
    ))
}

fn fetch_url<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let Some(proposed) = argument(arguments, "url") else {
        return Produced::problem(
            "error: 'url' is required and must be a string holding an http or https URL",
        );
    };

    // The planner's own words, so untrusted. Read rather than released for a screen: `host_of`
    // below searches these bytes and the else arm is an early return, and LABEL-6 says minting a
    // display witness is not permission to inspect. The person still sees the URL at the approval
    // prompt; that is a destination, and this is the read.
    let url = match policy.read_planner_argument("fetch_url", "url", &proposed) {
        Ok(url) => url,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    tools.note_address(&url);

    // Worked out here rather than in the prompt, so what a person is asked about is the host the
    // request will reach and not whatever the string looks like it names.
    let Some(host) = bravebot_core::url::host_of(&url) else {
        return Produced::problem(format!(
            "error: '{url}' names no host to fetch from; give an absolute http or https URL"
        ));
    };

    if let Some(refusal) = fetch_rules_refusal(policy, &url) {
        return refusal;
    }
    if let Some(refusal) = fetch_url_not_approved(policy, tools, confirmer, &url, &host) {
        return refusal;
    }

    // Bound to this exact URL, so an approval cannot be spent on another.
    policy.endorse_fetch(&url);
    let label = match policy.before_fetch(&url) {
        Ok(label) => label,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // Recorded where the request goes out rather than where a response comes back. The metadata
    // service hands out a role's credentials to whatever opens the socket, so the authority is
    // spent by asking; whether the answer arrives is a fact about the network.
    if let Some(spent) = bravebot_core::ambient::at_host(&host) {
        policy.record_ambient(&[spent]);
    }

    // The header is this program's own, fixed at compile time: a preference for the cheapest form
    // of the page a processor will read. Nothing of the planner's or of a server's reaches it, and
    // every hop of the chain re-sends it, because the egress crate re-sends the whole request.
    let request = bravebot_net::Request::get(&url).header("accept", FETCH_ACCEPT);
    let mut fetched =
        tools
            .chat
            .egress
            .fetch_watching(policy, request.clone(), label, Some(tools.cancel));
    // Before anything returns, so a failure does not leave the rest of the turn's egress being
    // checked against the host this one call was approved for.
    policy.fetch_finished();

    // A hop the gate stopped for leaving the approved host is put to the person (FETCH-4). A no
    // leaves `fetched` the refusal it was, and a yes sends the one request again with that host
    // allowed for it alone.
    if fetched.is_err()
        && let Some(hop) = policy.take_fetch_hop()
        && let Some(followed) =
            fetch_followed_to(policy, tools, confirmer, &url, &host, &hop, request)
    {
        fetched = followed;
    }

    match fetched {
        Ok(response) => {
            // Decoded lossily rather than refused for not being text. What a server sends is
            // untrusted either way, and a page with one bad byte is still the page that was asked
            // for: nothing here reads it, so there is nothing for a decoding failure to protect.
            // Reshaped inside the kernel, so the page never becomes a bare String here: it
            // comes back still labelled, and the count is taken where the bytes are released.
            let text = policy.render_in_place("fetch_url", &response.body, |bytes| {
                String::from_utf8_lossy(&bytes).into_owned()
            });
            let status = response.status;
            let note = note_for(policy, "fetch_url", &text, |text| {
                format!("{status}, {}", tally(text.lines().count(), "line", "lines"))
            });

            // The host as it was requested, and not the URL: this origin is written into the trail,
            // which holds no userinfo, path, query or fragment (TRACE-2). A redirect chain ends
            // somewhere the person approving never saw, and naming that here would present a host
            // nobody agreed to as though they had, in the planner's context, the trace and the
            // transcript alike. Nothing here can name it in any case: where a chain went does not
            // leave the crate that followed it.
            let mut produced = Produced::new(text, format!("what {host} returned"), note)
                .of_content()
                .capped(response.truncated);
            // Always. A fetched body is never trusted, so the block a person is shown is always
            // marked as content nobody vouched for.
            produced.untrusted = true;
            produced
        }
        // The URL is safe to repeat: a person approved it, so it is not something an attacker
        // chose. Nothing of the response is, and none of it is read to build this: a failure
        // names the URL that was asked for and never the hop a redirect took the request to,
        // which is the one thing of a server's that could otherwise reach this sentence.
        Err(error) => Produced::problem(format!("error: fetching {url} failed: {error}")),
    }
}

/// Put a host a fetch was redirected to to the person, and on a yes send the request again with
/// that host allowed for it alone (FETCH-4). `None` for a no, which leaves the refusal the gate gave.
///
/// The host is a server's own bytes, so it is drawn and decides nothing until the person says the
/// fetch may go there: the answer is bound to the host as it was drawn, spent once, and gone when
/// the request reports back. Nothing about the answer reaches the planner, whose sentence is the
/// same on both (FETCH-6).
fn fetch_followed_to<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    url: &str,
    approved: &str,
    hop: &Labelled<String>,
    request: bravebot_net::Request,
) -> Option<Result<bravebot_net::Response, bravebot_net::EgressError>> {
    let shown = policy.render_in_place("fetch_url", hop, |host| host);
    let host = {
        let proof = policy.authorise_display_release("where a fetch was redirected to");
        shown.declassify(&proof)
    };
    let question = crate::confirm::FetchRequest {
        url: url.to_string(),
        host: host.clone(),
        redirected_from: Some(approved.to_string()),
        classified: crate::confirm::Classified::NotAsked,
    };
    if confirmer.confirm_fetch(&question) == Decision::Reject {
        policy.refuse_unapproved_fetch("fetch_url");
        return None;
    }

    policy.endorse_fetch(url);
    policy.endorse_fetch_hop(&host);
    let label = policy.before_fetch_via(url, &host).ok()?;
    if let Some(spent) = bravebot_core::ambient::at_host(&host) {
        policy.record_ambient(&[spent]);
    }
    let followed = tools
        .chat
        .egress
        .fetch_watching(policy, request, label, Some(tools.cancel));
    policy.fetch_finished();
    Some(followed)
}

/// How long a download may run from the moment the reply begins.
///
/// Longer than a fetch's reply, since a body worth saving to a file is one that takes a while, and
/// still a bound: nothing else ends a server that sends a byte a minute.
const DOWNLOAD_WITHIN: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// What a download that did not finish says about why, kept apart from the text it is reported in
/// so the sentence is chosen once the write has returned.
enum DownloadStopped {
    TooLarge,
    Cancelled,
    Failed(String),
}

/// A server's content type as a person may be shown it: printable ASCII only and short.
///
/// The header is the server's own words, and this goes to a screen. A control character or an
/// escape sequence in it would be drawn as an instruction to the terminal rather than as text, so
/// only the characters a media type is written in are kept, and a long one is cut.
fn shown_content_type(raw: Option<&str>) -> String {
    const LONGEST: usize = 100;
    match raw {
        None => "no content type".to_string(),
        Some(raw) => raw
            .chars()
            .filter(|c| c.is_ascii_graphic() || *c == ' ')
            .take(LONGEST)
            .collect(),
    }
}

/// Save what a URL serves to a file, which neither the driver nor the planner reads.
///
/// Two routing arguments, each of which a person can read and answer for: the URL, through the
/// questions and rules `fetch_url` puts to a host (FETCH-2, FETCH-4), and the path, through the
/// destination gate `write_file` applies (WRITE-3). The bytes go from the socket to the file in
/// pieces, labelled as a fetched body is, and nothing here holds or looks at them (DOWNLOAD-1).
fn download_url<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let workspace = tools.workspace;
    let Some(proposed) = argument(arguments, "url") else {
        return Produced::problem(
            "error: 'url' is required and must be a string holding an http or https URL",
        );
    };
    if named_argument(arguments, "path_ref").is_some() {
        return Produced::problem(
            "error: download_url takes 'path' and no reference: name the file to save to",
        );
    }

    // Read for the reason `fetch_url` reads it: the host is taken out of these bytes below.
    let url = match policy.read_planner_argument("download_url", "url", &proposed) {
        Ok(url) => url,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    tools.note_address(&url);
    let Some(host) = bravebot_core::url::host_of(&url) else {
        return Produced::problem(format!(
            "error: '{url}' names no host to download from; give an absolute http or https URL"
        ));
    };

    // Before anybody is asked, for the reason it is in `fetch_url`: a rule is a statement that
    // something does not happen.
    if let Some(refusal) = fetch_rules_refusal(policy, &url) {
        return refusal;
    }

    let found = match path_argument(
        policy,
        workspace,
        "download_url",
        Purpose::Effect,
        tools.slots,
        arguments,
    ) {
        Ok(found) => found,
        Err(refusal) => return Produced::problem(refusal),
    };
    let (path, shown_path, proposed_path) = (found.path, found.shown, found.released);

    // Before anybody is asked about the host or the file, as a write's is (TRUST-10).
    if let Err(unreachable) = tools.workspace.resolve(&proposed_path) {
        return download_refused(policy, &unreachable, &shown_path);
    }

    // After the destination is known to be admissible, so the person is not asked about a host
    // for a path that would have been refused.
    if let Some(refusal) = fetch_not_approved(
        policy,
        confirmer,
        "download_url",
        &url,
        &host,
        crate::confirm::Classified::NotAsked,
        tools.permission_mode.get(),
    ) {
        return refusal;
    }

    // What the file holds now is not read: it is replaced whole, and the person is told so.
    let (replaces, approved_at) = policy.capture_files(|_, capture| {
        (
            workspace.names_a_file(&proposed_path),
            capture.stamp_of(&workspace.trust_key(&proposed_path)),
        )
    });
    let intent = Intent::of_a_write(replaces);
    // What the destination reaches as the question is drawn, which the write compares with what it
    // reaches when the bytes are released.
    let reach = workspace.reach_of(&proposed_path);

    // Always put to the person, whatever the trust map would have said about the path: the bytes
    // are nobody's and the file is somebody's, so the pair is the one thing they can answer for.
    let request = WriteRequest {
        written_since_checkout: false,
        intent,
        existing: replaces.then(String::new),
        // The pair, in the one field every front end draws as the destination. No file is read and
        // no body is held, so there is nothing to put in `contents` and nothing for a diff to say.
        path: format!("{url} -> {proposed_path}"),
        contents: String::new(),
        diff: Diff::compute("", ""),
        untrusted: true,
        remark: None,
        protected: write_is_protected(workspace, &proposed_path),
        credentials: Vec::new(),
        may_always: false,
        record: None,
        may_edit: false,
    };
    let mode = tools.permission_mode.get();
    let (answer, who) = confirmer.confirm_write_noting_the_mode(&request);
    let answered_by = crate::confirm::answered_by(who, || {
        mode.answers_a_write_unasked(false, request.protected)
            .then(|| mode.name())
    });
    let person_asked = answered_by.is_none();
    policy.record_answer(answered_by);
    keep_the_note(policy, "download_url", answer.approved(), answer.note());
    if !answer.approved() {
        policy.refuse_unapproved_write("download_url");
        return Produced::problem(format!(
            "refused: the user did not approve saving this there. Do not retry the same \
             download{}",
            what_next(answer.note().is_some())
        ));
    }

    // Bound to this exact URL, so an approval cannot be spent on another.
    policy.endorse_fetch(&url);
    let label = match policy.before_fetch(&url) {
        Ok(label) => label,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    if let Some(spent) = bravebot_core::ambient::at_host(&host) {
        policy.record_ambient(&[spent]);
    }

    // No `Accept` of this program's own: a download wants the file as the server holds it, and
    // the Markdown preference `fetch_url` states is for a page a processor reads.
    let request = bravebot_net::Request::get(&url).stream_within(DOWNLOAD_WITHIN);
    let opened = tools
        .chat
        .egress
        .fetch_streaming(policy, request, label, Some(tools.cancel));
    // Once the request is open, so a failure does not leave the turn's other egress being checked
    // against the host this one call was approved for. The body is read below with no hop left.
    policy.fetch_finished();
    let mut stream = match opened {
        Ok(stream) => stream.capped_at(tools.download_cap),
        Err(error) => {
            return Produced::problem(format!("error: downloading {url} failed: {error}"));
        }
    };
    let status = stream.status;
    let content_type = shown_content_type(stream.content_type.as_deref());

    // The approval is the path's authority, and it is bound to this exact value.
    policy.issue_grant("file_write", "path", proposed_path.clone());

    let cancel = tools.cancel;
    let mut stopped = None;
    let outcome = workspace.write_streamed_endorsed(
        policy,
        &path,
        label,
        Some(crate::workspace::Prepared::new(approved_at, person_asked)),
        Some(&reach),
        |file, proof| {
            use std::io::Write;
            loop {
                if cancel.is_cancelled() {
                    stopped = Some(DownloadStopped::Cancelled);
                    return Err(std::io::Error::other("cancelled"));
                }
                match stream.next_chunk() {
                    Ok(Some(piece)) => {
                        let bytes = piece.declassify(proof);
                        file.write_all(&bytes)?;
                    }
                    Ok(None) => break,
                    Err(error) => {
                        stopped = Some(DownloadStopped::Failed(error.to_string()));
                        return Err(std::io::Error::other("the body stopped"));
                    }
                }
            }
            if stream.truncated() {
                stopped = Some(DownloadStopped::TooLarge);
                return Err(std::io::Error::other("over the cap"));
            }
            Ok(())
        },
    );

    match (outcome, stopped) {
        (Ok(landed), _) => {
            workspace.record_write(Some(&shown_path));
            // The stream's own count of what it moved: nothing here measured the released chunks.
            let written = stream.handed_back();
            let verb = if replaces { "replaced" } else { "created" };
            confirmed(
                format!(
                    "{verb} {shown_path} with {written} bytes downloaded from {url} (HTTP \
                     {status}). It is written and quarantined: you cannot read it, and there is \
                     nothing further to do for it."
                ),
                format!("{status}, {written} bytes, {content_type}"),
            )
            .marked_untrusted(true)
            .having_changed_a_file_at(workspace, &landed)
        }
        (Err(_), Some(DownloadStopped::TooLarge)) => Produced::problem(format!(
            "error: {url} is larger than the {} bytes a download may write, so nothing was \
             saved. Do not retry; say that the limit is download.maxBytes in settings.json.",
            tools.download_cap
        )),
        (Err(_), Some(DownloadStopped::Cancelled)) => {
            Produced::problem("error: the download was stopped, so nothing was saved")
        }
        (Err(_), Some(DownloadStopped::Failed(error))) => Produced::problem(format!(
            "error: downloading {url} failed part-way: {error}. Nothing was saved."
        )),
        (Err(e), None) => download_refused(policy, &e, &shown_path),
    }
}

/// A call to a tool of an MCP server whose list somebody vouched for (SERVERS-7).
///
/// What the server answered is content nobody vouched for, as a fetched page is, and so is what
/// it said about a call that failed: both go to the planner quarantined and are marked on the
/// screen. The sentences this process writes name the tool by the alias the person gave the
/// server and the word they read on its list, and nothing the server answered.
#[expect(
    clippy::too_many_arguments,
    reason = "every argument is forwarded to `mcp::call`, which takes the same list, and the dispatch holds each apart"
)]
fn call_server_tool<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    offer: &crate::mcp::Offer,
    egress: &bravebot_net::Egress,
    confirmer: &mut C,
    reporter: &mut R,
    alias: &str,
    tool: &str,
    arguments: &Value,
    slots: &mut SlotStore,
) -> Produced {
    let name = format!("{alias}:{tool}");
    let (text, pictures, left_out, origin, note, failed) = match crate::mcp::call(
        policy,
        offer,
        egress,
        confirmer,
        reporter,
        alias,
        tool,
        arguments,
        Some(slots),
        false,
    ) {
        crate::mcp::Called::Answered(output) => (
            output.text,
            output.pictures,
            output.left_out,
            crate::mcp::origin_of_reply(&name, false),
            "answered",
            false,
        ),
        crate::mcp::Called::Failed(text) => (
            text,
            Vec::new(),
            0,
            crate::mcp::origin_of_reply(&name, true),
            "the tool reported a failure",
            true,
        ),
        crate::mcp::Called::Unusable(problem) | crate::mcp::Called::Problem(problem) => {
            return Produced::problem(problem);
        }
    };
    let mut produced = Produced::new(text, origin, note).of_content();
    produced.untrusted = true;
    produced.failed = failed;
    produced.pictures = pictures;
    produced.left_out = left_out;
    produced
}

/// What a background pipeline has printed since it was last looked at.
///
/// The job name is routing, and it is the driver's own: a name this module minted and looked up in
/// its own map, so nothing the planner writes reaches anything but that lookup. A name that is no
/// job is compared with the delegates the kernel numbered too, so a planner waiting on one is told
/// where its report comes from. The output is content and carries the label the kernel fixed
/// before the pipeline started.
fn job_output<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let Some(named) = argument(arguments, "job") else {
        return Produced::problem(
            "error: 'job' is required and must be a job name, e.g. \"job:1\"",
        );
    };

    // Looked up against names the driver handed out, which is the same treatment a reference
    // gets. Nothing is decided from it beyond whether it is one of ours, but that lookup is a
    // comparison against the released bytes and an early return on the answer, so it is a read and
    // goes through the gate that records one (LABEL-6).
    let name = match policy.read_planner_argument("job_output", "job", &named) {
        Ok(name) => name,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    let kill = arguments
        .get("kill")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Refused before anything happens, so a planner that believed it had asked is not handed a
    // reference instead, and a kill in the same call is not carried out (RUN-22).
    let read_asked = match read_asked_of(arguments) {
        Ok(read_asked) => read_asked,
        Err(refusal) => return *refusal,
    };

    let (wait, until_exit) = match wait_from(arguments, tools.deadlines).and_then(|wait| {
        until_exit_from(arguments, wait, kill)
            .map(|until_exit| (wait, until_exit))
            .map_err(String::from)
    }) {
        Ok(asked) => asked,
        Err(refusal) => return Produced::problem(refusal),
    };

    // Every job's token and not only this one's: another job's stop is carried out at the next
    // round, and a look here that sat out its wait would hold that round back (RUN-27).
    let stops: Vec<bravebot_core::cancel::JobStop> = tools
        .jobs
        .running
        .values()
        .filter(|job| !job.reported)
        .map(|job| job.stop.clone())
        .collect();

    let Some(job) = tools.jobs.running.get_mut(&name) else {
        // Started, not still running: the kernel does not know which have been collected, so
        // the answer has to hold whether the report is still to come or already above.
        if policy.delegates_started().any(|id| id.to_string() == name) {
            return Produced::problem(format!(
                "error: '{name}' is a delegate, not a background job, so job_output neither reads \
                 it nor stops it. How it ended reaches you on its own, in a message saying {name} \
                 has finished or did not finish. When a delegate is all that is left, end the round \
                 with one line naming what is outstanding. The turn asks you again no later than \
                 when as many of the delegates of the spawn_agent call that started {name} have \
                 reported as its wake_after asked for: one where it gave no count, and all of \
                 them where it asked for more than it started. Delegates still working keep \
                 working."
            ));
        }
        // `run` is named only where the turn's list holds it (TOOL-6).
        let handed_back = if tools.running.offered() {
            "run handed back"
        } else {
            "handed back"
        };
        return Produced::problem(format!(
            "error: there is no background job called '{name}'. Only a job name {handed_back} \
             in this turn can be read, and they do not outlive the turn."
        ));
    };

    // The latest look's answer is the one the account of the job's end follows (RUN-22).
    job.read_asked = read_asked;

    // None where there was nothing to wait for: a caller with unseen output already waiting for it
    // asked to be told about output, and it is here. Whether there is any is byte counts the driver
    // kept per pipe, never a comparison of what was printed. A caller that asked for the exit is
    // not waiting to be told about output, so unseen output is no reason not to wait for the exit.
    let waited = wait
        .filter(|_| until_exit || !job.running.has_more(&job.seen))
        .map(|bound| {
            // Timed, so the answer can say which window it watched. A wait that ends early because the
            // job printed or exited is otherwise indistinguishable from one that sat out its whole
            // bound, and the difference is the whole of what a caller learns from silence.
            let began = std::time::Instant::now();
            if until_exit {
                job.running
                    .wait_for_exit(bound, tools.cancel, &stops, &tools.arrival);
            } else {
                job.running
                    .wait_for_more(&job.seen, bound, tools.cancel, &stops, &tools.arrival);
            }
            began.elapsed()
        });

    let ended = job.running.ended();
    // A stage found stopped by a signal is ended before the output is taken, as the person's stop
    // is, and said so at this look and at every later one (RUN-17).
    let held = !ended && !job.reported && job.stopped_by_a_signal();
    let ended = ended || held;
    // The person's stop, read at this step as at a round's (RUN-27), and carried out before the
    // output is taken and the finish reported. One already reported has been given its account
    // and killed.
    let asked = !ended && !job.reported && job.stop.is_requested();
    let stopped_now = asked.then(|| job.stop_for_the_person());
    // Per pipe, so output arriving on one stream cannot shift where the other sits in the composed
    // text and hand back bytes this caller was already shown.
    let (fresh, look) = job.take();

    let ran_for = job.running.ran_for();
    let line = job.line.clone();
    let said = job.said.clone();
    let label = job.label_now();

    // Said from the clock and the exit codes, which are structure: nothing here reads a byte of
    // what the pipeline printed. Worked out before the kill below, so a job that had already ended
    // is reported as what it did rather than as what the kill would have done to it. A job the
    // person stopped is said to be theirs at every look, since its exit codes are the kill's.
    let outcome = if let Some(after) = job.stopped_by_the_person {
        crate::report::Outcome::StoppedByTheUser(after)
    } else if job.stopped_by_a_signal.is_some() {
        job.signal_outcome()
    } else if ended {
        how_it_ended(
            job.running.codes(),
            job.running.signals(),
            &job.running.closed(),
            job.confinement.as_deref(),
        )
    } else if kill {
        crate::report::Outcome::Stopped(ran_for)
    } else {
        // Running rather than Stopped: nothing stopped it, and a planner told a job it is waiting on
        // was stopped stops asking about a program that is still printing.
        crate::report::Outcome::Running { ran_for, waited }
    };

    // This answer is the account of the finish, so the turn's own look between rounds does not give
    // it a second time (CMDLINE-14). A killed job is finished too: the planner asked for the end of
    // it and was told what it had done, and news of it exiting afterwards is news of nothing.
    let finishing = (ended || kill || asked) && !job.reported;
    if finishing {
        job.reported = true;
        reporter.job(crate::report::JobEvent::Ended {
            name: name.clone(),
            outcome: outcome.clone(),
        });
    }

    if let Some(after) = stopped_now {
        policy.record_job_stop(&name, after);
    }
    if kill {
        job.running.kill();
    }
    // Compared at the look that gives the account of the end and at no other, so a job still
    // running keeps its mark for the end, and one already reported counts nothing again
    // (GROUND-1). After the kill, so what a killed job left is in it.
    let changed_a_file = finishing && job.wrote();

    // Composed from the outcome the planner is given rather than written out again here, so the
    // screen and the planner cannot come to disagree about the same job. The window a wait watched
    // is part of that outcome, which is why nothing adds it separately.
    let note = format!(
        "{}, {}",
        outcome.summary(),
        tally(look.lines, "new line", "new lines")
    );

    // Capped only where the planner may read it, exactly as a foreground run is: output it may not
    // read is quarantined whole, and there is nothing of it in the conversation to bound.
    let sample = if label.is_trusted() {
        bounded(&fresh, tools.output_cap)
    } else {
        None
    };
    let (fresh, whole) = match sample {
        Some(sample) => (sample, Some(Labelled::new(fresh, label))),
        None => (fresh, None),
    };

    let origin = look.origin(&said);
    let mut produced = Produced::new(Labelled::new(fresh, label), origin, note)
        .of_content()
        .capped(whole.is_some());
    produced.whole = whole;
    produced.untrusted = !label.is_trusted();
    produced.read_asked = read_asked;
    produced.look = Some(look);
    // A write and no command: the command was counted when the line started.
    produced.changed_a_file = changed_a_file;
    // So a person can be asked to read it later, and can see which command they are reading. This is
    // also the one place the window a wait watched is said to the planner, which the note above is
    // not: that one goes to a screen.
    produced.printed_by = Some(crate::report::Command {
        line,
        outcome,
        job: Some(name),
    });
    produced
}

/// How much of a command's output may enter the conversation where nobody named a figure.
///
/// A context-budget decision and not a safety one: a single tool result must never be able to
/// spend a large fraction of a conversation, however useful what it printed was. Because it is a
/// budget rather than a boundary, `run.maxOutput` may name another (RUN-21), and this is what
/// stands where nothing did.
pub(crate) const OUTPUT_CAP: usize = 16 * 1024;

/// The most one `download_url` call may write to a file where nobody named a figure.
///
/// A disk-budget decision: the bytes go to a file and never into the conversation, so this bounds
/// what one call can leave on the person's disk. `download.maxBytes` may name another.
pub(crate) const DOWNLOAD_CAP: usize = 100 * 1024 * 1024;

/// `text` cut to `cap` bytes, keeping the head and the tail, or `None` where it fits.
///
/// Head and tail rather than head alone, because a build log's verdict is at the end and its first
/// error is near the beginning: keeping only the front of one answers neither question a reader
/// has. What went is said in between, in the driver's own words, so a planner knows it is looking
/// at a sample rather than at a short result. Where the rest of it went is said by the turn,
/// which is the only thing that knows the slot it went to.
///
/// The cap is a byte count, which is what a configured one is documented as: cutting on characters
/// would make the figure a person wrote mean a different amount of context per language.
fn bounded(text: &str, cap: usize) -> Option<String> {
    if text.len() <= cap {
        return None;
    }
    let half = cap / 2;
    // Cut on a character boundary, or a multi-byte character straddling the cut would panic the
    // turn on output nobody chose.
    let head_end = text
        .char_indices()
        .map(|(at, _)| at)
        .take_while(|at| *at <= half)
        .last()
        .unwrap_or(0);
    let tail_start = text
        .char_indices()
        .map(|(at, _)| at)
        .find(|at| *at >= text.len() - half)
        .unwrap_or(text.len());

    let dropped_bytes = tail_start - head_end;
    let dropped_lines = text[head_end..tail_start].lines().count();
    Some(format!(
        "{}\n\n(the middle of this output was dropped: {dropped_bytes} bytes, \
         about {dropped_lines} lines, from byte {head_end}.)\n\n{}",
        &text[..head_end],
        &text[tail_start..]
    ))
}

/// Why a processor call's `reads` could not be taken, naming the mistake in it.
///
/// Chosen from the argument's name and JSON type and nothing else, so each answer is a fixed
/// sentence. Told only that `reads` is required, a planner that had sent `read` holding a string
/// sent the same call again word for word.
fn unreadable_reads(arguments: &Value) -> &'static str {
    match (arguments.get("reads"), arguments.get("read")) {
        (None, Some(sent)) if sent.is_string() => {
            "error: the argument is 'reads', not 'read', and it takes an array rather than a \
             string holding one, e.g. \"reads\": [\"ref:0\"]"
        }
        (None, Some(_)) => {
            "error: the argument is 'reads', not 'read', e.g. \"reads\": [\"ref:0\"]"
        }
        (Some(sent), _) if sent.is_string() => {
            "error: 'reads' takes an array rather than a string holding one, e.g. \"reads\": \
             [\"ref:0\"]"
        }
        _ => {
            "error: 'reads' is required and must be an array of reference names, e.g. \
             \"reads\": [\"ref:0\"]"
        }
    }
}

/// Put the planner's question to the advisor and bring back what it said.
///
/// The answer comes back as the result of the call, labelled from the context the advisor was
/// shown, so the kernel decides in the turn whether the planner reads it or is given a reference.
/// Every failure is the driver's own words: what a backend says about a failed request can carry
/// a credential, and the planner is told the category only (TOOL-4).
fn advise<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    arguments: &Value,
) -> Produced {
    let Some(question) = named_argument(arguments, "question") else {
        return Produced::problem("error: 'question' is required and must be a string");
    };
    let Some(advising) = tools.advising.as_mut() else {
        return Produced::problem("error: no such tool 'advisor'");
    };
    if *advising.asked >= crate::advisor::CALLS_PER_TURN {
        return Produced::problem(format!(
            "refused: the advisor has been asked {} times this turn, which is as often as a turn \
             may. Decide from the advice you already have.",
            crate::advisor::CALLS_PER_TURN
        ));
    }
    let question = match policy.before_advice(&question) {
        Ok(question) => question,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // Counted before the call, so one that fails or is cancelled still counts: a planner that
    // keeps asking a model that keeps failing is the loop the bound is for.
    *advising.asked += 1;
    let call = *advising.asked;
    let model = advising.model;

    let asked_at = std::time::Instant::now();
    let answer =
        crate::advisor::consult(policy, &mut tools.chat, model, advising.context, &question);
    let waited = Some(crate::timing::Interval::since(asked_at));
    match answer {
        Ok(advice) => {
            policy.record_advice(model, call, advice.usage.total(), question.chars().count());
            Produced::new(
                advice.answer,
                format!("the advisor {model}"),
                format!("the advisor {model} answered"),
            )
            .costing(advice.usage)
            .waiting(waited)
            .of_content()
        }
        Err(crate::advisor::AdviceError::Chat(error)) => {
            model_request_failed("advisor", &error, waited)
        }
        Err(crate::advisor::AdviceError::Refused(why)) => Produced::problem(match why {
            bravebot_config::ModelRefusal::Retrieves => {
                "refused: the advisor model searches the web before it replies, so it is not \
                 asked. Carry on without it."
            }
            _ => {
                "refused: the settings on this machine do not allow the advisor model. Carry on \
                 without it."
            }
        })
        .waiting(waited),
        Err(crate::advisor::AdviceError::NotServed) => Produced::problem(
            "refused: no configured service answers for the advisor model, so it is not asked. \
             Carry on without it.",
        )
        .waiting(waited),
        Err(crate::advisor::AdviceError::Denied(error)) => request_denied(&error, waited),
    }
}

/// What the planner is told when a request to a model the driver chose (`what` is `advisor` or
/// `processor`) failed, with the cost of the attempts and the wait.
///
/// Tool problems are trusted driver text. Backend display strings can contain secrets, so the
/// planner is told the category only, or that the request was cancelled, and a cancelled request
/// is carried separately so the turn ends as stopped rather than failed (TOOL-4).
fn model_request_failed(
    what: &str,
    error: &crate::backend::BackendError,
    waited: Option<crate::timing::Interval>,
) -> Produced {
    let cancelled = error.is_cancelled();
    let diagnosis = error.diagnosis();
    let reason = if cancelled {
        "cancelled"
    } else {
        diagnosis.category.name()
    };
    let mut produced = Produced::problem(format!("error: {what} request {reason}"))
        .costing(error.completed_usage().unwrap_or_default())
        .waiting(waited);
    if cancelled {
        produced.cancelled = Some(crate::outcome::Cancellation {
            attempts: diagnosis.attempts,
        });
    }
    produced
}

/// What the planner is told when the policy denied a request to a model.
fn request_denied(
    error: &impl std::fmt::Display,
    waited: Option<crate::timing::Interval>,
) -> Produced {
    Produced::problem(format!("error: {error}")).waiting(waited)
}

/// Hand quarantined content to an isolated model and quarantine what comes back.
///
/// The tool that makes an untrusted workspace workable. Everything the planner cannot read, a
/// processor can, and everything a processor produces the planner still cannot read: what comes
/// back is a reference, exactly as a read of an untrusted file is.
///
/// Nothing here decides anything from content. The references are names the driver handed out,
/// the instruction is the planner's, and the label on the result is computed by the kernel from
/// the inputs before the processor runs.
fn spawn_processor<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    arguments: &Value,
) -> Produced {
    let Some(instruction) = argument(arguments, "instruction") else {
        return Produced::problem("error: 'instruction' is required and must be a string");
    };
    let Some(entries) = arguments.get("reads").and_then(Value::as_array) else {
        return Produced::problem(unreadable_reads(arguments));
    };
    // Before any reference is looked at, because an empty list is a call that took the tool for
    // something it is not. The kernel refuses it too (SPAWN-1), but in words about the kernel.
    if entries.is_empty() {
        return Produced::problem(
            "error: 'reads' names no references. A processor only works on quarantined content \
             you were not shown, so it needs at least one reference to some."
                .to_string()
                + write_it_yourself(tools.beside),
        );
    }

    let mut reads = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(name) = entry.as_str() else {
            return Produced::problem(
                "error: every entry in 'reads' must be a reference name, e.g. \"ref:0\"",
            );
        };
        let named = Labelled::new(
            name.to_string(),
            bravebot_core::label::Label::untrusted_public(),
        );
        match accept_named_reference(
            policy,
            tools.slots,
            "spawn_processor",
            "reads",
            &named,
            NamesToList::Text,
        ) {
            Ok(slot) => reads.push(slot),
            Err(refusal) => return Produced::problem(refusal),
        }
    }

    // The planner's own number: how many entries of a list it wants a processor to choose among.
    // A call that gives one asks for a number and no document, so it names none the answer is
    // about. Both are the planner's arguments, checked before a reference is opened so that a
    // call that cannot be made reads nothing. The kernel holds the same range when it builds the
    // spec.
    let largest = bravebot_core::processor::ProcessorSpec::LARGEST_CHOICE;
    let choose_upto = match arguments.get("choose_upto") {
        None | Some(Value::Null) => None,
        Some(asked) => match asked
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| (2..=largest).contains(n))
        {
            Some(upto) => Some(upto),
            None => {
                return Produced::problem(format!(
                    "error: 'choose_upto' must be a whole number from 2 to {largest}"
                ));
            }
        },
    };
    if choose_upto.is_some() && argument(arguments, "about").is_some() {
        return Produced::problem(
            "error: 'about' and 'choose_upto' cannot be given together: a processor asked for a \
             number writes no document, so its answer is about none",
        );
    }

    // Named for the audit trail from the slots it reads, which are the driver's own names for
    // things. Two processors reading the same references in one turn share a name, and that is
    // the honest description of them.
    //
    // "Isolated" is said every time because it is true every time: the request carries no tool
    // list, no history and no second round, and nothing about a call can change that. The word
    // is not "sandboxed": there is no operating-system boundary here, and there is no untrusted
    // *code* for one to hold. What confines a processor is that it has no capabilities at all.
    // Reference names, not filenames: this is the planner's copy, and it is the one thing it
    // may not have. The person's copy of the same line is resolved where it is drawn.
    let over = {
        let proof = policy.authorise_display_release("which references a processor was given");
        references_in(arguments).declassify(&proof)
    };
    let origin = format!("an isolated processor over {over}");

    // Before the spec, not after: `before_processor` computes the output's label by taint over
    // the inputs, and a slot that reads its file here may come back untrusted where the trust
    // map fell after the slot was reserved. A spec built first would carry the label the inputs
    // used to have.
    let opened = match materialise(
        policy,
        tools.workspace,
        tools.slots,
        "spawn_processor",
        &reads,
    ) {
        Ok(opened) => opened,
        Err(refusal) => return Produced::problem(refusal),
    };

    // Which document the call is about: the answer replaces that one, and an answer that marks
    // no document leaves it standing. The planner's own choice, out of the references it named,
    // fixed before the processor exists.
    let about = match argument(arguments, "about") {
        Some(named) => match accept_named_reference(
            policy,
            tools.slots,
            "spawn_processor",
            "about",
            &named,
            NamesToList::Text,
        ) {
            Ok(slot) => Some(slot),
            Err(refusal) => return Produced::problem(refusal),
        },
        None => None,
    };

    // The planner's words, at the label of the context it wrote them in, so an instruction
    // written after private content was let out for the planner is private, and so is the answer.
    let instruction = match policy.adopt_model_output("spawn_processor", instruction) {
        Ok(instruction) => instruction,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    let spec = match choose_upto {
        Some(upto) => policy.before_chooser(&origin, &reads, &instruction, upto, tools.slots),
        None => policy.before_processor(&origin, &reads, &instruction, about, tools.slots),
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    let asked_at = std::time::Instant::now();
    // A call that asked for a number is answered with one, whatever the processor said: the number
    // is read outside the driver and carried here as a labelled byte, and what the planner is
    // told is the same words for every value of it (PROC-16).
    if let Some(upto) = choose_upto {
        let answer = processor::choose(policy, &mut tools.chat, tools.slots, &spec);
        let waited = Some(crate::timing::Interval::since(asked_at));
        return match answer {
            Ok(done) => {
                let mut produced = Produced::chosen(
                    ChosenNumber {
                        number: done.number,
                        upto,
                    },
                    origin,
                    format!("an isolated processor chose one entry of {over}"),
                )
                .processing(done.usage)
                .waiting(waited);
                produced.substituted = done.substituted;
                produced
            }
            Err(error) => processor_failed(error, waited),
        };
    }
    let answer = processor::run(policy, &mut tools.chat, tools.slots, &spec);
    // Taken around the call rather than inside it, so a processor that failed still reports the
    // time it spent failing: a request that errored kept the turn waiting just as long.
    let waited = Some(crate::timing::Interval::since(asked_at));
    match answer {
        Ok(done) => {
            // Nothing to write, and nothing minted for it. An answer that never said which part
            // of itself was a file cannot become one: everything a processor writes is for a
            // person to read unless it declares where the document begins, and this one
            // declared nothing. The document the call was about stands as it was, and no slot
            // is minted for a copy of it: a slot is written once and read by whatever the
            // planner points at it, and one holding a copy of a document already in a slot has
            // nothing for anyone to point at.
            //
            // A processor with nothing to change and one that forgot the line hand back the
            // same answer, and what would tell them apart is in bytes the driver may not read.
            // So this says what is true of both and asks for the line, rather than reporting a
            // verdict the reply is not entitled to give.
            let Some(document) = done.document else {
                let stands = match spec.about() {
                    Some(from) => format!("nothing was written and {from} is as it was"),
                    None => "there is nothing to write".to_string(),
                };
                let substituted = done.substituted;
                let mut produced = confirmed(
                    format!(
                        "that answer marked no document, so {stands}. What it said is on the \
                         screen. If the change is still needed, process it again and say that \
                         the whole file must follow the line that marks where the document \
                         begins."
                    ),
                    "produced no document",
                )
                .processing(done.usage)
                .waiting(waited);
                produced.said = done.note;
                produced.substituted = substituted;
                return produced;
            };

            let wrote = note_for(policy, "spawn_processor", &document, |text: String| {
                tally(text.lines().count(), "line", "lines")
            });
            // Says who did what. The planner's own reads read nothing, since a reference to a
            // file already is the file; the processor is what opens them, and until this said
            // so the only reads on the screen were the ones that did not happen.
            let note = if opened.is_empty() {
                format!("an isolated processor wrote {wrote}")
            } else {
                format!(
                    "an isolated processor read {} and wrote {wrote}",
                    opened.join(", ")
                )
            };
            let mut produced = Produced::new(document, origin, note)
                .processing(done.usage)
                .waiting(waited)
                .of_content();
            produced.answers_for = Some(spec.clone());
            produced.said = done.note;
            produced.substituted = done.substituted;
            produced
        }
        Err(error) => processor_failed(error, waited),
    }
}

/// What the planner is told when a processor did not answer.
fn processor_failed(
    error: crate::processor::ProcessorError,
    waited: Option<crate::timing::Interval>,
) -> Produced {
    match error {
        crate::processor::ProcessorError::Model(refusal) => {
            // The planner is told the call did not run and nothing more. The sentence naming the
            // key to correct is the person's, and goes to the note.
            let mut produced = Produced::problem(
                "error: the configured processor model cannot be used, so nothing was sent",
            )
            .waiting(waited);
            produced.note = crate::report::processor_model_refusal(&refusal);
            produced
        }
        crate::processor::ProcessorError::Chat(error) => {
            // What a failed request completed is a processor's usage as well, and is kept out of
            // the figure a delegate's budget counts for the same reason.
            let mut produced = model_request_failed("processor", &error, waited);
            produced.usage_is_unbudgeted = true;
            produced
        }
        crate::processor::ProcessorError::Denied(error) => request_denied(&error, waited),
    }
}

/// Hand a sub-task to a second planner and bring back one report.
///
/// Everything about the delegate is fixed by the kernel before it exists: which capabilities it
/// holds, which prompt it reads, and how many rounds it may take. What arrives here is a name out
/// of an enumerated set and a paragraph, and neither can widen anything.
///
/// The report comes back as a labelled value and is presented like any other tool result, so the
/// kernel decides whether the planner is shown the words or a reference to them. Nothing here
/// reads it, and nothing here could: a delegate's answer is model output, and which of the two
/// shapes it takes is settled by the label its own context gave it.
fn spawn_agent<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let Some(kind) = argument(arguments, "kind") else {
        return Produced::problem(format!(
            "error: 'kind' is required and must be one of {}",
            policy.delegates().names().join(", ")
        ));
    };
    let tasks = match tasks_in(arguments) {
        Ok(tasks) => tasks,
        Err(refusal) => return Produced::problem(refusal),
    };
    let keeping = match servers_kept(arguments) {
        Ok(keeping) => keeping,
        Err(refusal) => return Produced::problem(refusal),
    };
    let budget = match budget_asked(arguments) {
        Ok(budget) => budget,
        Err(refusal) => return Produced::problem(refusal),
    };
    let wake_after = match wake_after_asked(arguments) {
        Ok(wake_after) => wake_after,
        Err(refusal) => return Produced::problem(refusal),
    };
    // What the run has left is shared by the delegates of one call, so a fan-out divides it
    // rather than giving each the whole of it (DELEGATE-29). Fewer tokens than delegates leaves a
    // share of zero, which the kernel refuses like a run with nothing left, so the delegates
    // never carry more between them than the parent had.
    let parent_left = tools
        .budget_left
        .map(|left| left / tasks.len().max(1) as u64);

    let mut produced = Produced::new(
        Labelled::trusted(String::new()),
        String::new(),
        String::new(),
    );
    produced.wake_after = wake_after;
    let mut started = Vec::new();
    // Each checkout's number, the commit it holds and what was carried into it, in the order they
    // were made.
    let mut checkouts: Vec<(String, String, Option<crate::workspace::Carried>)> = Vec::new();
    // Read once for the call and put into each checkout it makes (CHECKOUT-23).
    let mut plan: Option<crate::workspace::CarryPlan> = None;
    let mut kind_name = String::new();
    let mut without: Vec<bravebot_core::capability::Capability> = Vec::new();
    let mut rest_refused: Option<String> = None;

    for task in &tasks {
        // Numbered by the kernel, beneath this run's own number in the order this run started
        // them, when it approves the delegate. Everything recorded or reported about this
        // delegate carries the number, which is the only thing saying whose a line is: the
        // alternative is reading the line, which is prose a model wrote. A fan-out is exactly
        // where two of them read alike. A refusal takes no number, so the trail says which run
        // was asked and the number only ever names a delegate that was approved.
        //
        // The trail's name for it is that number, not the task: a task is a paragraph, and it
        // would be in every line of the trail that mentions this run.
        //
        // Gated once per delegate rather than once per call. A fan-out is several runs, and a
        // gate that saw one of them would be approving the others on the strength of a sibling.
        let spec = match policy.before_delegate_within(
            &kind,
            task,
            keeping.as_ref(),
            budget.as_ref(),
            parent_left,
        ) {
            Ok(spec) => spec,
            Err(denial) if started.is_empty() => {
                return Produced::problem(format!("refused: {denial}"));
            }
            // The tree filling up is the one refusal that can land partway through a fan-out.
            // The ones before it are started and on the screen, so they run and the planner is
            // told the rest did not.
            Err(denial) => {
                rest_refused = Some(denial.to_string());
                break;
            }
        };
        let id = spec.id();

        // Compared only once the gate above has passed, which is when this run has met nothing a
        // name could be steered by (CHECKOUT-1).
        let wanted = match wants_checkout(arguments, &spec, tools) {
            Ok(wanted) => wanted,
            // Approved by the gate and refused here, so it starts nothing and takes no place
            // under the turn's ceiling (DELEGATE-7).
            Err((cause, refusal)) => {
                policy.withdraw_delegate(spec, &cause, tasks.len() - started.len());
                if started.is_empty() {
                    return Produced::problem(refusal);
                }
                rest_refused = Some(refusal);
                break;
            }
        };
        let with_changes = wanted.as_ref().is_some_and(|wanted| wanted.with_changes);
        let made = match wanted.map(|wanted| wanted.place) {
            None => None,
            Some(place) => match match place {
                CheckoutPlace::State(state) => tools.workspace.checkout_for_cause_as(
                    policy,
                    state,
                    id,
                    checkout_reader(tools.beside),
                ),
                CheckoutPlace::Temporary => {
                    tools.workspace.checkout_in_temporary_directory_cause_as(
                        policy,
                        id,
                        checkout_reader(tools.beside),
                    )
                }
            } {
                Ok(made) => {
                    if let Some(checkout) = made.checkout() {
                        crate::workspace::record_checkout(
                            policy.sink(),
                            crate::workspace::Happened::Made,
                            checkout.path(),
                        );
                    }
                    if with_changes {
                        let plan = plan.get_or_insert_with(|| tools.workspace.plan_carry(policy));
                        tools.workspace.carry_into(
                            policy,
                            plan,
                            &made,
                            crate::workspace::CarryBound::FIXED,
                        );
                    }
                    Some(made)
                }
                Err((cause, refusal)) => {
                    // Said as the definition's, since the call that met the refusal may not have
                    // asked.
                    let refusal = match spec.asks_for_checkout() {
                        true => format!(
                            "The {} definition asks for a checkout of its own. {refusal}",
                            spec.definition()
                        ),
                        false => refusal,
                    };
                    policy.withdraw_delegate(spec, &cause, tasks.len() - started.len());
                    if started.is_empty() {
                        return Produced::problem(refusal);
                    }
                    rest_refused = Some(refusal);
                    break;
                }
            },
        };

        // The task is released for a screen the way the target of any other call is. A person
        // watching several delegates has nothing else to tell them apart by.
        let asked = {
            let proof = policy.authorise_display_release("what a delegate was asked to do");
            task.clone().declassify(&proof)
        };
        let stop = bravebot_core::cancel::DelegateStop::new();
        reporter.delegate_started(crate::report::Delegation {
            stop: stop.clone(),
            id,
            // The definition's name rather than its kind's, because "a reader" stops telling the
            // person watching anything the moment two definitions are readers. Printable for the
            // one reason a skill's name is: a name from a source nobody vouched for never
            // reached the set this was selected out of.
            kind: spec.definition().to_string(),
            task: asked,
        });

        kind_name = spec.definition().to_string();
        started.push(id.to_string());
        // Every delegate of one call names one kind and starts from this run's one set, so what
        // one was started without is what each was (DELEGATE-33).
        without = spec.without().to_vec();

        // Everything the kernel settled, taken off the policy here on the turn's own thread. From
        // this point the delegate needs nothing further from the run that spawned it, which is
        // what lets the two run at the same time.
        let mut seeded =
            crate::delegate::seed(policy, spec, tools.remembering.filter(|_| made.is_none()));
        seeded.stop = stop;
        if let Some(made) = made {
            if let Some(checkout) = made.checkout() {
                seeded.file_authority = seeded.file_authority.rooted_at(checkout.key());
                // A rule written about a full path under the working directory is about the
                // same place in the checkout (CHECKOUT-9). These rules are the delegate's own
                // copy, so they are withdrawn with it.
                seeded.permissions.copy_beneath(
                    &tools.workspace.root().to_string_lossy(),
                    &made.root().to_string_lossy(),
                );
                checkouts.push((
                    checkout.id().to_string(),
                    checkout.commit().to_string(),
                    checkout.carried().cloned(),
                ));
            }
            seeded.workspace = Some(made);
        }
        produced = produced.delegating(id, seeded);
    }

    // Started rather than finished. The turn hears about the report when there is one, and in the
    // meantime the planner has its round back: a turn that had to sit still until a delegate
    // answered could only ever have one working.
    let named = started.join(", ");
    let asked_again = when_asked_again(started.len(), produced.wake_after, !tools.delegated);
    let body = if started.len() == 1 {
        format!(
            "the {kind_name} delegate {named} has started. Its report will reach you when it is \
             ready, and you do not have to wait for it: carry on, or spawn another. When it is all \
             that is left, end the round with one line naming what is outstanding: {asked_again}"
        )
    } else {
        format!(
            "{} {kind_name} delegates have started: {named}. Each reports on its own and you do \
             not have to wait for any of them: carry on, or spawn another. When they are \
             all that is left, end the round with one line naming what is outstanding: \
             {asked_again}",
            started.len()
        )
    };
    let body = match started_without_sentence(&without, started.len()) {
        Some(said) => format!("{body} {said}"),
        None => body,
    };
    let body = match checkouts.first() {
        Some((_, commit, None)) if started.len() == 1 => format!(
            "{body} It works in a checkout of commit {commit}, which has no changes that are not \
             committed and is not your working directory. It keeps no memory between \
             conversations{}. {}",
            lsp_clause(tools.beside, false),
            checkouts_share_refs(tools.running)
        ),
        Some((_, commit, None)) => format!(
            "{body} Each works in a checkout of its own of commit {commit}, which has no changes \
             that are not committed and is not your working directory. None keeps memory between \
             conversations{}. {}",
            lsp_clause(tools.beside, true),
            checkouts_share_refs(tools.running)
        ),
        Some((_, commit, Some(_))) if started.len() == 1 => format!(
            "{body} It works in a checkout of commit {commit}, which is not your working \
             directory. The driver tried to carry your working directory's uncommitted changes \
             into it as they were when it was made. {} It keeps no memory between conversations\
             {}. {}",
            carried_summaries(&checkouts),
            lsp_clause(tools.beside, false),
            checkouts_share_refs(tools.running)
        ),
        Some((_, commit, Some(_))) => format!(
            "{body} Each works in a checkout of its own of commit {commit}, none of which is your \
             working directory. The driver tried to carry your working directory's uncommitted \
             changes into each as they were when it was made. {} None keeps memory between \
             conversations{}. {}",
            carried_summaries(&checkouts),
            lsp_clause(tools.beside, true),
            checkouts_share_refs(tools.running)
        ),
        None => body,
    };
    let body = match (rest_refused, tasks.len() - started.len()) {
        (Some(denial), 1) => format!("{body} The last one was not started, refused: {denial}"),
        (Some(denial), left) => {
            format!("{body} The other {left} were not started, refused: {denial}")
        }
        (None, _) => body,
    };
    let note = if started.len() == 1 {
        format!("a {kind_name} delegate started")
    } else {
        format!("{} {kind_name} delegates started", started.len())
    };

    produced.text = Labelled::trusted(body);
    produced.origin = format!("a {kind_name} delegate");
    produced.note = note;
    produced
}

/// One sentence naming what the delegates of a call were started without, or nothing where they
/// were started with all their definition asked for (DELEGATE-33).
///
/// Built from the capabilities the kernel compared when it approved them, and from nothing a
/// delegate, a definition file or the planner wrote.
fn started_without_sentence(
    without: &[bravebot_core::capability::Capability],
    started: usize,
) -> Option<String> {
    use bravebot_core::capability::Capability;
    if without.is_empty() {
        return None;
    }
    let names: Vec<String> = without
        .iter()
        .map(|capability| match capability {
            Capability::FileWrite => "writing files".to_string(),
            Capability::ShellExec => "running programs".to_string(),
            Capability::LanguageServer => "asking a language server".to_string(),
            other => other.to_string(),
        })
        .collect();
    let subject = match started {
        1 => "It was",
        _ => "Each was",
    };
    Some(format!(
        "{subject} started without {}, because the run that started it does not hold {}.",
        names.join(", "),
        match without.len() {
            1 => "it",
            _ => "them",
        }
    ))
}

/// What the planner is told was carried into the checkouts a call made (CHECKOUT-23): one account
/// where every checkout was given the same, and each checkout's own where they differ.
fn carried_summaries(checkouts: &[(String, String, Option<crate::workspace::Carried>)]) -> String {
    let all: Vec<&crate::workspace::Carried> = checkouts
        .iter()
        .filter_map(|(_, _, carried)| carried.as_ref())
        .collect();
    match all.split_first() {
        Some((first, rest)) if rest.iter().all(|carried| carried == first) => {
            crate::delegate::carried_summary(first)
        }
        _ => checkouts
            .iter()
            .filter_map(|(id, _, carried)| {
                carried
                    .as_ref()
                    .map(|carried| format!("{id}: {}", crate::delegate::carried_summary(carried)))
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Whether a delegate is given a checkout, because the call or its definition asks, or why it may
/// not have one (CHECKOUT-1, CHECKOUT-2, CHECKOUT-3, CHECKOUT-6, CHECKOUT-23).
///
/// A definition's request is refused where the call's would be, rather than dropped, so the
/// planner cannot start that definition's delegate in the working directory.
fn wants_checkout<'a>(
    arguments: &Value,
    spec: &bravebot_core::delegate::DelegateSpec,
    tools: &Tools<'a>,
) -> Result<Option<Wanted<'a>>, (CheckoutRefusal, String)> {
    let (called, with_changes) = match arguments.get("isolation") {
        None | Some(Value::Null) => (false, false),
        Some(Value::String(value)) if value == "checkout" => (true, false),
        Some(Value::String(value)) if value == "checkout_with_changes" => (true, true),
        Some(_) => {
            return Err((
                CheckoutRefusal::UnknownIsolation,
                "error: 'isolation' may only be \"checkout\" or \"checkout_with_changes\"; leave \
                 it out for a delegate that works in your working directory, unless its \
                 definition asks for one"
                    .to_string(),
            ));
        }
    };
    if !called && !spec.asks_for_checkout() {
        return Ok(None);
    }
    if spec.kind() == bravebot_core::delegate::Kind::Reader {
        return Err((
            CheckoutRefusal::Reader,
            "refused: a reader writes nothing, so a checkout separates it from nobody, and a \
             reader started without one reads your working tree as it is"
                .to_string(),
        ));
    }
    // Said as the definition's, since the call that met the refusal may not have asked.
    let whose = match spec.asks_for_checkout() {
        true => format!(
            "the {} definition asks for a checkout of its own, and ",
            spec.definition()
        ),
        false => String::new(),
    };
    if tools.workspace.checkout().is_some() {
        return Err((
            CheckoutRefusal::AlreadyInCheckout,
            format!(
                "refused: {whose}you already work in a checkout, which the delegates you start \
                 share"
            ),
        ));
    }
    // Making one writes `worktrees/<id>/` under the repository's `.git` (CHECKOUT-5), which is part
    // of the project that `read-only` holds programs and the file tools back from writing
    // (SANDBOX-22).
    if sandbox_refuses_writes(tools.confine_runs, tools.sandbox) {
        return Err((
            CheckoutRefusal::ReadOnlySandbox,
            format!(
                "refused: {whose}the sandbox mode is read-only, and making a checkout writes into \
                 the repository's .git, so no checkout was made; only the user can choose another \
                 sandbox mode"
            ),
        ));
    }
    let place = match tools.home {
        Some(state) if !bravebot_core::incognito::engaged() => CheckoutPlace::State(state),
        _ => CheckoutPlace::Temporary,
    };
    Ok(Some(Wanted {
        place,
        with_changes,
    }))
}

/// A checkout a delegate is to be given.
struct Wanted<'a> {
    place: CheckoutPlace<'a>,
    /// Whether the working directory's uncommitted changes are carried into it (CHECKOUT-23).
    with_changes: bool,
}

/// Where a checkout is made (CHECKOUT-6).
enum CheckoutPlace<'a> {
    /// Under the state directory, where it outlasts the session.
    State(&'a std::path::Path),
    /// Under the system temporary directory, for a session that keeps no record or has no state
    /// directory, and gone when it ends.
    Temporary,
}

/// The most delegates one call may fan a task out over.
///
/// A bound on futility rather than on authority: the planner could always start this many with
/// this many calls, and each is gated on its own either way. What changes is the cost of asking,
/// and a field that turns one sentence into forty runs is a field worth a ceiling.
const MAX_FANOUT: usize = 8;

/// The tasks one `spawn_agent` call asks for, one per delegate.
///
/// Without `each` that is the task itself. With it, the task is what they share and each entry is
/// what one of them is additionally told, composed here before anything is labelled: both halves
/// are the same model output out of the same call, so joining them settles nothing about either.
fn tasks_in(arguments: &Value) -> Result<Vec<Labelled<String>>, String> {
    let Some(task) = arguments.get("task").and_then(Value::as_str) else {
        return Err(
            "error: 'task' is required and must be a string saying what the delegate has to do"
                .to_string(),
        );
    };

    let Some(each) = arguments.get("each") else {
        return Ok(vec![Labelled::new(
            task.to_string(),
            bravebot_core::label::Label::untrusted_public(),
        )]);
    };

    let Some(entries) = each.as_array() else {
        return Err("error: 'each' must be an array of strings, one per delegate".to_string());
    };
    if entries.is_empty() {
        return Err(
            "error: 'each' was empty, so it named no delegates; leave it out to start one"
                .to_string(),
        );
    }
    if entries.len() > MAX_FANOUT {
        return Err(format!(
            "error: 'each' named {} delegates and at most {MAX_FANOUT} may be started by one \
             call; split the work or narrow it",
            entries.len()
        ));
    }

    entries
        .iter()
        .map(|entry| {
            let Some(entry) = entry.as_str() else {
                return Err(
                    "error: every entry in 'each' must be a string saying what that one \
                     delegate is additionally told"
                        .to_string(),
                );
            };
            Ok(Labelled::new(
                format!("{task}\n\n{entry}"),
                bravebot_core::label::Label::untrusted_public(),
            ))
        })
        .collect()
}

/// The MCP servers a `spawn_agent` call's delegates keep, where it names them (AGENT-6).
///
/// Only the shape is checked here. Whether each name is a server this run holds is the kernel's
/// to compare, after the gate that says this run has met nothing a name could be steered by.
fn servers_kept(arguments: &Value) -> Result<Option<Labelled<Vec<String>>>, String> {
    let refusal = || {
        "error: 'mcp_servers' must be an array of the names of MCP servers you may call; leave it \
         out to hand on every one"
            .to_string()
    };
    let entries = match arguments.get("mcp_servers") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(entries)) => entries,
        Some(_) => return Err(refusal()),
    };
    let names = entries
        .iter()
        .map(|entry| entry.as_str().map(str::to_string).ok_or_else(refusal))
        .collect::<Result<Vec<String>, String>>()?;
    Ok(Some(Labelled::new(
        names,
        bravebot_core::label::Label::untrusted_public(),
    )))
}

/// The token budget a `spawn_agent` call gives each of its delegates, where it names one
/// (DELEGATE-29).
///
/// A whole number of tokens above zero, or the words `/limit` takes for one such as `500k` or
/// `2m`. Only the shape is checked here: the kernel takes the least of this, the definition's and
/// what the run has left, once the integrity gate has passed.
fn budget_asked(arguments: &Value) -> Result<Option<Labelled<u64>>, String> {
    Ok(budget_figure(arguments)?
        .map(|figure| Labelled::new(figure, bravebot_core::label::Label::untrusted_public())))
}

/// When the planner is asked again about a call that started `started` delegates and asked to be
/// woken after `wake_after` of them (DELEGATE-17), as the call's result says it.
///
/// A figure at or above `started` is all of them, as the turn counts it. One delegate is asked
/// about when its report is in whatever the figure. `typed_lines_wake` is false for a delegate's
/// own call, whose wait no typed line ends, so its result does not say one does.
fn when_asked_again(started: usize, wake_after: usize, typed_lines_wake: bool) -> String {
    if started <= 1 {
        "the turn asks you again when the report is in.".to_string()
    } else if wake_after >= started {
        let sooner = match typed_lines_wake {
            true => "a line from the user arrives or another call's delegates wake it",
            false => "another call's delegates wake it",
        };
        format!(
            "the turn asks you again when all {started} have reported, and not before unless \
             {sooner}."
        )
    } else if wake_after > 1 {
        format!(
            "the turn asks you again when {wake_after} of them have reported, while the others \
             keep working, and again as each of the rest reports."
        )
    } else {
        "the turn asks you again when the first report is in, while the others keep working, and \
         again as each of the rest reports."
            .to_string()
    }
}

/// How many of a `spawn_agent` call's delegates have to end before the planner is asked again
/// (DELEGATE-17): the call's `wake_after`, or 1 where it names none.
///
/// A whole number above zero. A figure at or above the number of delegates the call starts asks
/// for all of them, which the turn settles when it counts, so it is not compared with that number
/// here. The figure is the planner's own argument and selects no destination, so nobody is asked
/// about it.
fn wake_after_asked(arguments: &Value) -> Result<usize, String> {
    let figure = match arguments.get("wake_after") {
        None | Some(Value::Null) => return Ok(1),
        Some(Value::Number(number)) => number.as_u64().filter(|figure| *figure > 0),
        Some(_) => None,
    };
    figure
        .map(|figure| usize::try_from(figure).unwrap_or(usize::MAX))
        .ok_or_else(|| {
            "error: 'wake_after' must be a whole number of delegates above zero, such as 3; leave \
             it out to be asked again as soon as one of this call's delegates has ended"
                .to_string()
        })
}

/// The figure a call's `budget` field names, where it names one.
fn budget_figure(arguments: &Value) -> Result<Option<u64>, String> {
    let figure = match arguments.get("budget") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(number)) => number.as_u64().filter(|figure| *figure > 0),
        Some(Value::String(written)) => bravebot_config::limit::parse_count(written),
        Some(_) => None,
    };
    figure.map(Some).ok_or_else(|| {
        "error: 'budget' must be a count of tokens above zero, such as 500000 or \"500k\"; leave \
         it out to set none"
            .to_string()
    })
}

/// Read a skill the planner was listed.
///
/// The name arrives as model output, so it is promoted the way a read path is: the operation
/// changes nothing and is confined to a boundary the user established. It is in fact more
/// confined than a read. The promoted name never becomes a path component; it only **selects**
/// from the set the driver enumerated before the turn began, so a name naming a traversal, an
/// absolute path, or anything else at all matches nothing and the call is refused. There is no
/// filesystem lookup for it to reach.
///
/// The body keeps the label it was read with and goes back like any other tool result, so
/// `Policy::present` is what decides whether the planner sees it.
fn load_skill<S: Sink>(
    policy: &mut Policy<'_, S>,
    skills: &crate::skills::Catalogue,
    beside: Beside,
    arguments: &Value,
) -> Produced {
    let Some(proposed) = argument(arguments, "name") else {
        return Produced::problem("error: 'name' is required and must be a string");
    };

    let name = match policy.promote_confined_read("load_skill", "name", &proposed) {
        Ok(name) => name,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    // Safe to read: promotion just proved this is (T,pub), and comparing trusted text decides
    // nothing an attacker steers.
    let Ok(name) = name.into_trusted() else {
        return Produced::problem("error: the skill name was not usable");
    };

    // A page of the documentation compiled into this program (SKILL-17): selected from a fixed table
    // by an exact match, and offered only while the built-in that lists the pages is the one in the
    // catalogue, so a user's skill of that name takes the pages with it.
    if skills
        .get(crate::help::SKILL)
        .is_some_and(|skill| skill.source == crate::skills::Source::BuiltIn)
        && let Some(text) = crate::help::load(&name)
    {
        let note = tally(text.lines().count(), "line", "lines");
        let mut produced = confirmed(text.as_ref(), note);
        produced.origin = "built-in".to_string();
        return produced;
    }

    // A skill kept from the planner is answered as a name that matches nothing, whether or not the
    // person named it: they are supplied for the line that names them, so there is nothing for a
    // call to load and nothing it may learn about the skill (SKILL-20).
    let Some(skill) = skills.get(&name).filter(|skill| !skill.person_only) else {
        // A built-in is listed in the system prompt; the rest are found with search_skills
        // (SKILL-18). Either way the name has to come from one of those two, so this is a
        // mistake worth naming rather than a refusal worth explaining.
        return Produced::problem(format!(
            "error: no skill named '{name}'. Call search_skills to find one, or use a name you \
             were listed or that search_skills already returned."
        ));
    };

    let note = note_for(policy, "load_skill", skill.body(), |text: String| {
        tally(text.lines().count(), "line", "lines")
    });
    let body = opened_skill(policy, skill, beside);
    let mut produced = Produced::new(body, skill.origin.clone(), note);
    produced.skill = Some(skill.name.clone());
    // Carried out rather than acted on here. The rounds after this one are the turn's to ask, and a
    // skill that named neither carries nothing, so a turn holding no answer is a turn with nothing
    // to change.
    if skill.runs_as.names_anything() {
        produced.loaded = Some((skill.name.clone(), skill.runs_as.clone()));
    }
    produced
}

/// Find a skill by a word or phrase, among the ones a built-in's listing left out (SKILL-18).
///
/// `query` is routing, promoted the way `search`'s own `pattern` is: it only selects what to
/// return out of a catalogue every entry of which already passed the trust gate at discovery,
/// so nothing here reads untrusted content and nothing a directory wrote decides what matches.
///
/// A name and description a match returns are the words [`Catalogue::describe_for_prompt`]
/// would have shown had the skill been a built-in, never cut, so this and the default listing
/// read the planner the same way (SKILL-18).
fn search_skills<S: Sink>(
    policy: &mut Policy<'_, S>,
    skills: &crate::skills::Catalogue,
    arguments: &Value,
) -> Produced {
    let Some(proposed) = argument(arguments, "query") else {
        return Produced::problem("error: 'query' is required and must be a string");
    };
    let query = match policy.promote_confined_read("search_skills", "query", &proposed) {
        Ok(query) => query,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    // Safe to read: promotion just proved this is (T,pub), and matching trusted text against
    // trusted names and descriptions decides nothing an attacker steers.
    let Ok(query) = query.into_trusted() else {
        return Produced::problem("error: the query was not usable");
    };
    if query.trim().is_empty() {
        return Produced::problem("error: 'query' must not be blank");
    }

    let (matches, more) = skills.search(&query);
    if matches.is_empty() {
        return Produced::new(
            Labelled::trusted("No skill matched that search.".to_string()),
            String::new(),
            "no match".to_string(),
        );
    }
    let mut text = String::new();
    for skill in &matches {
        text.push_str(&crate::skills::prompt_line(&skill.name, &skill.description));
    }
    let note = tally(matches.len(), "skill", "skills");
    if more > 0 {
        text.push_str(&format!(
            "...and {more} more matched; a narrower query would show them.\n"
        ));
    }
    Produced::new(Labelled::trusted(text), String::new(), note)
}

/// A skill's instructions as the planner is given them, and the reach into its directory that
/// being given them brings.
///
/// Shared by `load_skill` and by a turn supplying a skill the person named (SKILL-20), so the two
/// give the planner the same text and the same reach.
///
/// The files beside the skill are named as the driver found them before the turn began (LOAD-4). A
/// name is written into the result in the kernel, after the body, so the body keeps the label it was
/// read with and the names are the driver's own list and never a lookup of the planner's.
///
/// The reach is the way into those files for a skill of the user's own: a read of one, which no read
/// reaches otherwise. For a project's skill the files are inside the workspace already and the trust
/// map decides each, as it does any other read.
pub(crate) fn opened_skill<S: Sink>(
    policy: &mut Policy<'_, S>,
    skill: &crate::skills::Skill,
    beside: Beside,
) -> Labelled<String> {
    let body = match (&skill.directory, skill.files.is_empty()) {
        (Some(directory), false) => {
            let listing =
                beside_the_skill(directory, skill.landed.as_deref(), &skill.files, beside);
            policy.render_in_place("load_skill", skill.body(), |text| {
                format!("{text}{listing}")
            })
        }
        _ => skill.body().clone(),
    };
    if let Some(reach) = skill.reach() {
        policy.reach_skill_directory(reach);
    }
    body
}

/// What follows a skill's body when it keeps other files beside its `SKILL.md`.
///
/// This process's own words around names the driver listed. The files are read with `read_file`,
/// naming the directory and then the file, and nothing here is a path the planner could not have
/// typed already.
///
/// Where the directory is a link to another directory in the workspace, the sentence says where it
/// lands, because a listing of the link's parent shows nothing and a listing of the landed
/// directory shows the files (LOAD-4).
fn beside_the_skill(
    directory: &str,
    landed: Option<&str>,
    files: &[String],
    beside: Beside,
) -> String {
    // How a file is named, with read_file named only where the list holds it. A list without it
    // is given the path to use for another tool and no instruction about which.
    let how = if beside.holds(Named::ReadFile) {
        "Read one with read_file, naming the directory and the file, for example"
    } else {
        "For example"
    };
    let mut listing = format!(
        "\n\n---\nFiles kept beside this skill in {directory}. {how} {directory}/{}:\n",
        files[0]
    );
    if let Some(landed) = landed {
        listing = format!(
            "\n\n---\nFiles kept beside this skill in {directory}, a link to {landed}, which is where \
             list_files shows them. {how} {landed}/{}:\n",
            files[0]
        );
    }
    for file in files {
        listing.push_str("- ");
        listing.push_str(file);
        listing.push('\n');
    }
    listing
}

/// Load a tool of an MCP server out of the ones a person vouched for (SERVERS-16).
///
/// The name is routing, promoted as a skill's is, and it only **selects** from the offer the turn
/// holds: matched exactly against `alias:tool` of a vouched tool, so a name one character out
/// loads nothing. The result is a sentence of this process's own and holds no byte a server sent.
fn load_tool<S: Sink>(
    policy: &mut Policy<'_, S>,
    offer: Option<&crate::mcp::Offer>,
    arguments: &Value,
) -> Produced {
    let Some(proposed) = argument(arguments, "name") else {
        return Produced::problem("error: 'name' is required and must be a string");
    };
    let name = match policy.promote_confined_read("load_tool", "name", &proposed) {
        Ok(name) => name,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let Ok(name) = name.into_trusted() else {
        return Produced::problem("error: the tool name was not usable");
    };
    let Some(offer) = offer else {
        return Produced::problem("error: no tools are waiting to be loaded");
    };
    let Some((wire, loaded)) = offer.named(&name) else {
        return Produced::problem(format!(
            "error: no tool named '{name}'. The tools you can load are listed in the description \
             of load_tool; there are no others."
        ));
    };
    let note = if loaded { "already loaded" } else { "loaded" };
    let mut produced = confirmed(
        format!(
            "{name} is loaded and is offered to you from your next request on. Calling it still \
             asks the user first."
        ),
        note,
    );
    if !loaded {
        produced.tool_loaded = Some(wire.to_string());
    }
    produced
}

/// Read one option the model offered.
///
/// Two shapes, because a model that writes a bare string meant an option with no explanation and
/// refusing it would cost the person a choice over punctuation.
fn choice_from(option: &Value) -> Option<Choice> {
    if let Value::String(label) = option {
        return Some(Choice::new(label.clone(), None));
    }
    let label = option.get("label")?.as_str()?.to_string();
    let detail = option
        .get("detail")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(Choice::new(label, detail))
}

/// Read one question the model asked.
///
/// Total in the way `choice_from` is: it yields `None` for a question there is nothing to draw,
/// and the count check at the call site turns that into a refusal of the whole call. Nothing
/// here decides which questions exist, only whether the call as a whole is answerable.
fn question_from(entry: &Value) -> Option<Question> {
    let header = entry.get("header")?.as_str()?.to_string();
    let prompt = entry.get("question")?.as_str()?.to_string();

    let offered = match entry.get("options") {
        None | Some(Value::Null) => None,
        Some(Value::Array(options)) => Some(options),
        // Present but not a list. Falling through to a bare text field here would throw away
        // options the model meant to offer and leave the user staring at a question that names
        // choices it does not show.
        Some(_) => return None,
    };

    let choices: Vec<Choice> = offered
        .map(|options| options.iter().filter_map(choice_from).collect())
        .unwrap_or_default();
    // An option with nothing to draw is the one thing skipped, and this is what says so rather
    // than asking a question with a hole in it.
    if choices.len() != offered.map_or(0, Vec::len) {
        return None;
    }

    // Never silently falls back to one answer. A model that asked for several and was quietly
    // given a single-answer picker leaves the user unable to say what they were asked for, with
    // nothing on screen to suggest anything went wrong.
    let multiple = match entry.get("multiple") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        // Tool arguments arrive as JSON text, so the quoted spelling is common enough to accept.
        Some(Value::String(text)) if text.eq_ignore_ascii_case("true") => true,
        Some(Value::String(text)) if text.eq_ignore_ascii_case("false") => false,
        Some(_) => return None,
    };

    Some(Question::new(header, prompt, choices, multiple))
}

/// Put the planner's questions to the person and hand back what they said.
///
/// The only tool whose result comes from a person rather than from the workspace, and the only
/// one with no effect at all. It still has a destination, the user's screen, and that is what
/// makes the questions and their options **routing**: they decide what the person is shown and
/// therefore what they can answer. The routing field here is approved by being read, since what
/// is drawn is exactly the bytes the gate checked and nothing re-parses them afterwards.
///
/// The gate runs once for the whole series. A series is asked whole or refused whole, because
/// asking some of it would mean deciding which of the questions the person sees, and that
/// decision would be taken from what is in them.
///
/// Note there is no hand-written check on the context here. The refusal is the ordinary routing
/// gate doing its job, which is the point: relocating that decision into the driver would be the
/// violation, not the safeguard.
fn ask_user<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    confirmer: &mut C,
    arguments: &Value,
    vouched_channel: bool,
) -> Produced {
    let Some(entries) = arguments.get("questions").and_then(Value::as_array) else {
        return Produced::problem(
            "error: 'questions' is required and must be an array of one to four questions",
        );
    };

    if entries.is_empty() {
        return Produced::problem("error: 'questions' must hold at least one question.");
    }
    // Refused rather than trimmed. A question dropped here is one the model is told the person
    // was asked and the person never saw, which is worse than being made to send the call again.
    if entries.len() > ask::MOST_AT_ONCE {
        return Produced::problem(format!(
            "error: at most {} questions can be asked at once; nobody was asked anything. Send \
             the ones the work turns on.",
            ask::MOST_AT_ONCE
        ));
    }

    let asked: Vec<Question> = entries.iter().filter_map(question_from).collect();
    if asked.len() != entries.len() {
        return Produced::problem(
            "error: every question needs a 'header' tag and a 'question' sentence, and every \
             option needs a label; nobody was asked anything. Send the whole set again.",
        );
    }

    let series = policy.label_model_output("ask_user", Series::new(asked));

    // One string standing for every question, so the gate checks everything the person will be
    // shown rather than the first question or the sentences alone.
    let canonical = policy.render_in_place("ask_user", &series, |s| ask::canonical_series(&s));
    if let Err(denial) = policy.before_asking("ask_user", &canonical) {
        return Produced::problem(format!(
            "refused: {denial}. Questions can only be put to the user before anything untrusted \
             has reached your context. Continue without an answer, or say in your reply what you \
             need to know."
        ));
    }

    // Shaped inside the kernel for the same reason a task list is: laying out options means
    // reading them. Every question yields a prompt and every choice a row, so nothing in the
    // text decides what the person is shown the existence of.
    let shaped = policy.render_in_place("ask_user", &series, |s| ask::asking(&s));
    let proof = policy.authorise_display_release("questions for the user");
    let mut asking = shaped.declassify(&proof);
    // The one kind of question an interface may close on a clock: this is the planner asking for
    // something only the person knows, where a permission or an approval is the person's to give.
    // Set here, by the driver, so nothing a question says decides it (ASK-9).
    asking.closes_when_idle = true;
    let answers = confirmer.ask_user(&asking);

    // The kernel puts the replies into words and lines them up against the questions, so nothing
    // here branches on what the person said or counts what they answered. The line a person
    // reads counts the questions asked for the same reason: a count of replies would be a second
    // reading of them, and a skipped question has no reply to count.
    //
    // Words that arrived on a channel the person vouched for are recorded as that, beside the
    // reply, and the count is the number of answers that are words, which is a fact about their
    // kind and not about what they say (CLI-36).
    if vouched_channel {
        let words = answers
            .iter()
            .filter(|answer| matches!(answer, bravebot_core::ask::Answer::Typed(_)))
            .count();
        if words > 0 {
            policy.admit_vouched_answer(words);
        }
    }
    match policy.record_answers("ask_user", &series, &answers) {
        Ok(text) => Produced::new(
            text,
            "",
            format!("{} asked", tally(entries.len(), "question", "questions")),
        ),
        Err(denial) => Produced::problem(format!("refused: {denial}")),
    }
}

/// Put a finished plan to the person and, on approval, leave plan mode for the mode they chose
/// (MODE-14).
///
/// The plan is the planner's own text and goes through the same routing gate as a question: a
/// planner whose context has met something untrusted cannot ask, and is told so. What the person
/// is offered is fixed by the driver ([`crate::plan_approval`]), so the mode the session moves to
/// is picked from a list they see in full and never named by the planner. Nothing here branches
/// on the plan's bytes: the branch is on which of the fixed choices the person picked.
fn propose_plan<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    use crate::plan_approval::{Approval, approval, question};

    let Some(plan) = arguments.get("plan").and_then(Value::as_str) else {
        return Produced::problem("error: 'plan' is required and must be the text of the plan.");
    };
    if tools.permission_mode.get() != crate::PermissionMode::Plan {
        return Produced::problem(
            "error: the session is not in plan mode, so there is no plan to approve. Carry out \
             the work.",
        );
    }

    let series = policy.label_model_output("propose_plan", Series::new(vec![question(plan)]));
    let canonical = policy.render_in_place("propose_plan", &series, |s| ask::canonical_series(&s));
    if let Err(denial) = policy.before_asking("propose_plan", &canonical) {
        return Produced::problem(format!(
            "refused: {denial}. A plan can only be put to the user before anything untrusted has \
             reached your context. Say the plan in your reply instead."
        ));
    }
    let shaped = policy.render_in_place("propose_plan", &series, |s| ask::asking(&s));
    let proof = policy.authorise_display_release("the plan for the user");
    let mut asking = shaped.declassify(&proof);
    crate::plan_approval::asked_afresh(&mut asking);
    let answers = confirmer.ask_user(&asking);

    let said = match policy.record_answers("propose_plan", &series, &answers) {
        Ok(text) => text,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let outcome = match approval(answers.first()) {
        Approval::LeaveFor(mode) => {
            // Applies to the rest of this turn, as any change of mode does (MODE-8).
            tools
                .permission_mode
                .set_by(mode, crate::ModeCause::PlanApproval);
            "The user approved the plan and plan mode has ended. Carry the plan out now."
        }
        Approval::KeepPlanning => {
            "The user did not approve the plan, and the session is still in plan mode. Revise \
             the plan from what they said, if anything, and do not write."
        }
    };
    let text = policy.render_in_place("propose_plan", &said, |said| format!("{said}\n{outcome}"));
    Produced::new(text, "", String::new())
}

/// Whether an `include` glob leans on syntax the matcher does not have.
///
/// A pattern the matcher cannot read selects no files, and a search over no files reports no
/// matches, which is the same sentence a search that read the whole tree and found nothing
/// prints. A real turn took that for proof and answered the question wrong: the glob it wanted
/// was `**/*.{cc,h,mm}`, brace groups were not supported at the time, and it retreated to
/// `**/*.cc`, dropping the two extensions the answer was actually in.
///
/// Braces are supported now. This is for what is still missing, and for the next thing to be
/// added: the point is that an unreadable pattern must never come back looking like a fact
/// about the tree.
fn reads_as_an_unsupported_glob(pattern: &str) -> Option<&'static str> {
    // Extended globs. Checked before the bracket test, which their contents would otherwise
    // trip with a less useful message.
    for tell in ["?(", "*(", "+(", "@(", "!("] {
        if pattern.contains(tell) {
            return Some(
                "extended globs like !(…) and +(…) are not supported; name the files with * \
                 and ** instead",
            );
        }
    }
    if pattern.starts_with('!') {
        return Some(
            "a leading ! does not negate a pattern here; search without an include instead",
        );
    }
    // A character class. Both halves are required, in order, so a filename holding a stray
    // bracket is not lectured at.
    if let Some(open) = pattern.find('[')
        && pattern[open..].contains(']')
    {
        return Some(
            "character classes like [abc] are not supported; write the alternatives as a \
             brace group, e.g. {a,b,c}",
        );
    }
    None
}

/// The patterns a search was asked for.
///
/// One string or a list of them, because the model writes both and the difference is not
/// worth a failed call. A list entry that is not a string is dropped here and the emptiness
/// is refused below, which is the same shape `references_in` uses.
fn patterns_in(arguments: &Value) -> Vec<Labelled<String>> {
    let label = bravebot_core::label::Label::untrusted_public();
    match arguments.get("pattern") {
        Some(Value::Array(entries)) => entries
            .iter()
            .filter_map(Value::as_str)
            .map(|p| Labelled::new(p.to_string(), label))
            .collect(),
        Some(Value::String(one)) => vec![Labelled::new(one.clone(), label)],
        _ => Vec::new(),
    }
}

/// Ask a language server about a symbol.
///
/// The two halves of the answer take different roads out of here, which is [LSP-3]:
///
/// - The **locations** are a path and two integers read off the server's index. The two integers
///   are structure. The path is a name out of a tree nobody need have vouched for, so the kernel
///   labels the answer from the trust map's entry for each path it names and for the file it was
///   asked at: read as written where every one is vouched for, and a reference where any is not.
/// - The **text**, where an operation reports any, is bytes a file chose, and no answer says which
///   file chose them: a hover response carries a position and no file. So it is untrusted and comes
///   back as a reference, in a vouched-for tree as much as in `vendor/`.
///
/// [LSP-3]: ../../../docs/specs/tools/lsp.md
fn lsp<S: Sink, C: Confirmer + ?Sized>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let Some(named) = argument(arguments, "operation") else {
        return Produced::problem("error: 'operation' is required");
    };

    // The operation is routing: it decides what the server is asked. Promoted like any other
    // proposal, then matched against the closed set, so a name off the list is refused here rather
    // than forwarded on the chance a server understands it.
    let operation = match policy.promote_confined_read("lsp", "operation", &named) {
        Ok(promoted) => match promoted.clone().into_trusted() {
            Ok(name) => match bravebot_lsp::Operation::parse(&name) {
                Some(operation) => operation,
                None => {
                    return Produced::problem(format!(
                        "refused: {}",
                        bravebot_lsp::LspError::UnknownOperation { named: name }
                    ));
                }
            },
            Err(_) => return Produced::problem("refused: the operation was not trusted"),
        },
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // A position is two integers the planner states. Nothing reads the file to work one out, which
    // is what keeps this tool clear of LABEL-5: a position derived by scanning bytes would be a
    // decision taken from content.
    let line = arguments
        .get("line")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;
    let character = arguments
        .get("character")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;

    // `workspaceSymbol` ranges over the tree instead of starting from a position, so it has no path
    // and the query stands where the path stands: it is the whole of what the server is asked to
    // look for. That makes it routing, and it takes the same road the operation took, so the trail
    // says the planner chose it. Read only for the operation that sends one, since a promotion
    // recorded for a call that carries no query is a choice the planner never made.
    let query = if operation.needs_position() {
        None
    } else {
        match argument(arguments, "query") {
            Some(proposed) => match policy.promote_confined_read("lsp", "query", &proposed) {
                Ok(promoted) => match promoted.into_trusted() {
                    Ok(query) => Some(query),
                    Err(_) => return Produced::problem("refused: the query was not trusted"),
                },
                Err(denial) => return Produced::problem(format!("refused: {denial}")),
            },
            None => None,
        }
    };

    let (relative, landed) = if operation.needs_position() {
        let Some(proposed) = argument(arguments, "path") else {
            return Produced::problem(format!(
                "error: 'path' is required for {}",
                operation.as_str()
            ));
        };
        let promoted = match policy.promote_confined_read("lsp", "path", &proposed) {
            Ok(promoted) => promoted,
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        };
        let named = match promoted.clone().into_trusted() {
            Ok(named) => named,
            Err(_) => return Produced::problem("refused: the path was not trusted"),
        };
        // A deny rule covering the file covers asking a server about it too: the answer quotes
        // where things are in it, so this is a read.
        if let Err(refusal) = refuse_denied_path(policy, tools.workspace, Purpose::Read, &named) {
            return Produced::problem(refusal);
        }
        // The path is asked about as a read is: a path that lands outside the workspace is refused
        // here, before a server is started or the person is asked about one, and the refusal is
        // recorded in the trail (LSP-4, TRACE-1). The promotion above says only that the planner
        // chose it. Worded about the name typed, since the landing can come from a link.
        let landed = match tools.workspace.resolve(&named) {
            Ok(landed) => landed,
            Err(e) => return file_tool_problem(policy, "lsp", "path", &e, &named),
        };
        (named, Some(landed))
    } else {
        (String::new(), None)
    };

    let Some(servers) = tools.servers.as_deref_mut() else {
        // LSP-5, and it is not a fault: a host that cannot confine a subprocess does not get to run
        // one, and saying so is better than an answer nobody could trust.
        return Produced::problem(
            "refused: no language server can be started here, because this platform offers no \
             way to confine one."
                .to_string()
                + if tools.beside.holds(Named::ReadFile) && tools.beside.holds(Named::Search) {
                    " Use search and read_file instead."
                } else {
                    ""
                },
        );
    };

    // The server is given the path it can open, which is the one the workspace resolved. What the
    // planner is shown is rendered back against the root by the lsp module.
    let root = servers.root().to_path_buf();
    let file = landed.unwrap_or_else(|| root.join(&relative));
    let absolute = if relative.is_empty() {
        String::new()
    } else {
        file.to_string_lossy().into_owned()
    };

    // A file that is not there has nothing to check, and the person should not be asked to start a
    // server to be told so. Whether a path is a file is a fact about the filesystem, not a byte of
    // anything in it.
    if operation == bravebot_lsp::Operation::Diagnostics && !file.is_file() {
        return Produced::problem(format!(
            "error: {relative} is not a file, so there is nothing for a language server to check"
        ));
    }

    let answer = match servers.ask(
        policy,
        confirmer,
        &bravebot_lsp::Question {
            operation,
            path: &absolute,
            line,
            character,
            query: query.as_deref(),
        },
        tools.workspace,
    ) {
        Ok(answer) => answer,
        // LSP-6: every one of these says which failure it was, and none of them reads as a
        // statement about the code. A planner told "no references" deletes a function.
        Err(error) => return Produced::problem(format!("refused: {error}")),
    };

    let answer = crate::lsp::without_denied(answer, &root, tools.workspace, policy);
    let described = crate::lsp::describe(operation, &answer, &root);
    let found = answer.locations.len();

    // The text is the half that is content, and nothing here labels it by the path this call
    // named: a doc comment is written where the symbol is defined, which is a different file. The
    // kernel decides whether the planner sees it. Where there is no text, the result is the
    // driver's own words about where things are, which is trusted.
    let text = match crate::lsp::text_of(policy, &answer) {
        Some(Ok(text)) => Some(text),
        Some(Err(denial)) => return Produced::problem(format!("refused: {denial}")),
        None => None,
    };

    // LSP-13: a `diagnostics` answer is a sentence of the driver's own, built from severities and
    // line numbers, and messages the server wrote. The messages are content nobody can attribute,
    // so they do not join the sentence: they travel beside it and are handed over as a reference.
    let (text, messages) = if operation == bravebot_lsp::Operation::Diagnostics {
        (None, text)
    } else {
        (text, None)
    };

    let still = if answer.partial {
        ", still indexing"
    } else {
        ""
    };
    let note = match &answer.diagnostics {
        Some(reported) => format!(
            "{}, {}{still}",
            tally(reported.errors.len(), "error", "errors"),
            tally(reported.warnings, "warning", "warnings"),
        ),
        None => format!("{}{still}", tally(found, "location", "locations")),
    };

    match text {
        // Hover text exists, so the result carries it and is labelled by its file. The locations go
        // in front of it: they are structure, and a reader who cannot see the text can still act on
        // them.
        Some(text) => {
            let untrusted = !text.label().is_trusted();
            let combined =
                policy.render_in_place("lsp", &text, |text| format!("{described}\n\n{text}"));
            let mut produced = Produced::new(combined, relative, note);
            produced.content = true;
            produced.untrusted = untrusted;
            produced.incomplete = answer.partial;
            produced
        }
        // Locations only, which is every operation but hover. The names in it are bytes out of the
        // tree the server indexed, so the kernel labels it from the trust map's entry for each path
        // and for the file it was asked at, and an answer naming or asked at a file nobody vouched
        // for is quarantined, as a listing is. A
        // `diagnostics` answer names no path the server reported, so it carries no taint.
        None => {
            let asked = crate::lsp::asked_file(operation, &relative);
            let label =
                match crate::lsp::label_for_locations(policy, tools.workspace, &answer, asked) {
                    Ok(label) => label,
                    Err(denial) => return Produced::problem(format!("refused: {denial}")),
                };
            let origin = format!("what the language server said about {relative}");
            let mut produced = Produced::new(Labelled::new(described, label), relative, note)
                .marked_untrusted(!label.is_trusted());
            produced.content = !label.is_trusted();
            produced.incomplete = answer.partial;
            produced.apart = messages.map(|text| Apart {
                origin,
                text,
                introduced_by: "The messages the language server wrote about this file are \
                                not shown to you. They are a reference:",
            });
            produced
        }
    }
}

/// The lines a search matched, one `path:line:text` each and nothing of the driver's. The text
/// follows the separator byte for byte, as `grep -n` prints it.
///
/// With context, the lines around them follow grep's shape: `path-line-text` for a line that did
/// not match, and a `--` line between groups that are not adjacent in the file, so a gap is not
/// read as the lines having been next to each other.
fn match_lines(found: &crate::workspace::Matches) -> String {
    use crate::workspace::SearchOutput;
    match found.output {
        SearchOutput::Lines => {}
        SearchOutput::Files => {
            return found
                .tallies
                .iter()
                .map(|t| t.path.as_str())
                .collect::<Vec<_>>()
                .join("\n");
        }
        SearchOutput::Count => {
            if found.tallies.is_empty() {
                return String::new();
            }
            let mut rows: Vec<String> = found
                .tallies
                .iter()
                .map(|t| format!("{}: {}", t.path, t.lines))
                .collect();
            rows.push(format!(
                "total: {}",
                tally(found.matched, "matching line", "matching lines")
            ));
            return rows.join("\n");
        }
    }
    if found.context.is_empty() {
        return found
            .matches
            .iter()
            .map(|m| format!("{}:{}:{}", m.path, m.line, m.text))
            .collect::<Vec<_>>()
            .join("\n");
    }
    // Both lists are in file and line order, so a merge keeps that order.
    let mut rows: Vec<(&str, usize, String)> = found
        .matches
        .iter()
        .map(|m| {
            (
                m.path.as_str(),
                m.line,
                format!("{}:{}:{}", m.path, m.line, m.text),
            )
        })
        .chain(found.context.iter().map(|c| {
            (
                c.path.as_str(),
                c.line,
                format!("{}-{}-{}", c.path, c.line, c.text),
            )
        }))
        .collect();
    rows.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let mut out = Vec::with_capacity(rows.len());
    let mut previous: Option<(&str, usize)> = None;
    for (path, line, text) in &rows {
        if let Some((before, at)) = previous
            && (before != *path || at + 1 != *line)
        {
            out.push("--".to_string());
        }
        out.push(text.clone());
        previous = Some((path, *line));
    }
    out.join("\n")
}

fn search<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    arguments: &Value,
) -> Produced {
    let proposed_patterns = patterns_in(arguments);
    if proposed_patterns.is_empty() {
        return Produced::problem(
            "error: 'pattern' is required and must be a string or a list of strings",
        );
    }

    let mut patterns = Vec::with_capacity(proposed_patterns.len());
    for proposed in &proposed_patterns {
        match policy.promote_confined_read("search", "pattern", proposed) {
            Ok(p) => patterns.push(p),
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        }
    }

    let proposed_dir = argument(arguments, "directory").unwrap_or_else(|| {
        Labelled::new(
            ".".to_string(),
            bravebot_core::label::Label::untrusted_public(),
        )
    });

    let directory = match policy.promote_confined_read("search", "directory", &proposed_dir) {
        Ok(d) => d,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // The directory a search would walk, on the same footing as a listing: a match quotes the
    // line it was found on, so searching a tree is reading it.
    let proposed_where = match policy.read_planner_argument("search", "directory", &proposed_dir) {
        Ok(directory) => directory,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    if let Err(refusal) = refuse_denied_path(policy, workspace, Purpose::Read, &proposed_where) {
        return Produced::problem(refusal);
    }
    let proposed_where = workspace
        .climbed_to(&proposed_where)
        .unwrap_or(proposed_where);

    let include = match argument(arguments, "include") {
        Some(proposed) => match policy.promote_confined_read("search", "include", &proposed) {
            Ok(p) => Some(p),
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
        None => None,
    };

    // Routing, like the pattern beside it, but a literal rather than a name: there is nothing in a
    // flag to promote, so it is taken from the arguments directly. Absent means sensitive: a search
    // that quietly widened itself would report matches the caller cannot see the reason for.
    let case_sensitive = arguments
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // Read as a plain number, like the offset on a read: it names nothing, so there is no
    // destination for it to decide. A model that omits it gets the first page.
    let offset = arguments
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1)
        .min(usize::MAX as u64) as usize;

    // A plain number as well, and for the same reason: it names nothing. Capped by the workspace.
    let context = arguments
        .get("context")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(usize::MAX as u64) as usize;

    // Routing as well: a closed set of names for how the result is shaped, matched the way
    // read_git's query is. A name off the list is refused rather than guessed at.
    let output = match argument(arguments, "output") {
        None => crate::workspace::SearchOutput::Lines,
        Some(proposed) => match policy.promote_confined_read("search", "output", &proposed) {
            Ok(promoted) => match promoted
                .into_trusted()
                .ok()
                .and_then(|name| crate::workspace::SearchOutput::named(name.trim()))
            {
                Some(output) => output,
                None => {
                    return Produced::problem(
                        "error: 'output' must be one of lines, files or count",
                    );
                }
            },
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        },
    };

    match workspace.grep_shaped(
        policy,
        &patterns,
        &directory,
        include.as_ref(),
        case_sensitive,
        offset,
        context,
        output,
    ) {
        Ok(found) => {
            let note = note_for(policy, "search", &found, |found| {
                format!(
                    "{} in {}",
                    if found.output.summarises() {
                        tally(found.matched, "matching line", "matching lines")
                    } else {
                        tally(found.matches.len(), "match", "matches")
                    },
                    tally(found.searched, "file", "files")
                )
            });

            // Either cap leaves the answer partial, and the distinction between them matters to
            // whoever reads the body. To the turn it does not: a sample is a sample.
            //
            // What the turn does need beside it is where to go next, so both come out of one look
            // at the result. A second one would copy the matches again to read a single number and
            // would write a second entry in the trail for it.
            //
            // Releasing where to continue tells the planner which cap stopped the answer, since
            // only the cap on matches can be asked past, and that is the point of saying it. The
            // offset itself is the caller's own argument plus the cap, and the count of matches is
            // no more than the line count the reference already carries.
            let (incomplete, paging, links) = {
                let shaped = policy.render_in_place("search", &found, |found| {
                    (
                        found.truncated
                            || found.unvisited
                            || found.timed_out
                            || found.context_truncated
                            || found.size_truncated
                            || found.tallies_truncated,
                        found.paging(),
                        found.links,
                    )
                });
                let proof = policy.authorise_display_release(
                    "whether a search hit a cap, where it continues, and how many symbolic links \
                     it did not enter",
                );
                shaped.declassify(&proof)
            };
            // Asked of the glob alone, and only where the promote gate left it trusted, so this
            // reads no content and needs no witness. Nothing here looks at the result: whether to
            // say it is decided below, inside the render gate, from whether anything was found.
            let bad_glob = include.as_ref().and_then(|include| {
                include
                    .clone()
                    .into_trusted()
                    .ok()
                    .and_then(|g| reads_as_an_unsupported_glob(&g))
            });
            let had_include = include.is_some();

            let rendered = policy.render_in_place("search", &found, |found| {
                let mut body = if found.is_empty() {
                    // The three ways a search comes back empty, which used to print the same
                    // sentence. Files were read and the needle was not in them, which is an
                    // answer. Or the include glob selected nothing, so nothing was read and
                    // the tree was never asked. Or a rule covers what was selected, which no
                    // query can get around. Told apart here because a reader who cannot tell
                    // them apart takes a broken query for proof of absence, and rewrites a glob
                    // that was never the problem.
                    if found.withheld && found.considered == 0 {
                        "(a deny rule in the user's settings covers what this search would \
                         have read, so nothing was searched. Do not retry with another \
                         pattern or glob: work without it, or say in your reply what you \
                         needed it for)"
                            .to_string()
                    } else if had_include && found.considered == 0 {
                        let mut text = "(the include glob matched no files, so nothing was \
                                        searched; this says nothing about whether the pattern \
                                        is in the tree)"
                            .to_string();
                        if let Some(advice) = bad_glob {
                            text.push_str(&format!("\n\n({advice})"));
                        }
                        text
                    } else if let Some(Paging::PastTheEnd { found: total }) = found.paging() {
                        // A fourth empty answer, and it arrives with a page in hand: asking to
                        // continue past the last match reads as the pattern having gone away
                        // between two calls. Saying how many there were says where the end is.
                        format!(
                            "(this search found {}, so there is nothing at offset {}; the earlier \
                             matches are still there)",
                            tally(total, "match", "matches"),
                            found.first_match
                        )
                    } else {
                        "(no matches)".to_string()
                    }
                } else {
                    match_lines(&found)
                };
                // The empty result is the one that most needs this. A search that stopped before
                // it reached the file holding the needle reports nothing, and nothing reads as an
                // answer: the model concludes the string does not occur in the tree.
                if found.unvisited {
                    // The count the walk actually stopped at, not the constant it usually is.
                    // A host may have lowered the cap, and a notice naming a number the search
                    // did not reach is worse than one naming none.
                    body.push_str(&format!(
                        "\n\n(this search stopped after {} files and did not reach the whole \
                         tree; search a subdirectory to cover the rest)",
                        found.considered
                    ));
                }
                if found.links > 0 {
                    // Links are not entered, so files behind them were not read. Said as a
                    // number and without names, and with every result: an empty answer would
                    // otherwise read as covering them, and a partial one as the whole.
                    body.push_str(&format!("\n\n({})", search_links_clause(found.links)));
                }
                if found.timed_out {
                    body.push_str(&search_time_note(found.searched));
                }
                if found.output.summarises() && (found.unvisited || found.timed_out) {
                    // Nothing in a list of paths or counts looks short, which is why it is said:
                    // what the walk never read cannot be in it.
                    body.push_str(
                        "\n\n(the files and counts above are a lower bound: the search did not \
                         read the whole tree)",
                    );
                }
                if found.tallies_truncated {
                    body.push_str(&format!(
                        "\n\n(this lists the first {} files with a match and the result is \
                         incomplete; {}narrow the pattern or search a subdirectory to see the \
                         rest)",
                        found.tallies.len(),
                        if found.output == crate::workspace::SearchOutput::Count {
                            "the total above covers every match that was read; "
                        } else {
                            ""
                        }
                    ));
                }
                if found.context_truncated {
                    body.push_str(
                        "\n\n(the cap on context lines was reached, so the matches after the \
                         last context line shown come without theirs and the result is \
                         incomplete; ask for less context or search a narrower tree)",
                    );
                }
                if found.size_truncated {
                    // The budget bit, so what follows the last line shown is missing, whichever cap
                    // would have come next. Where a match was dropped the offset reaches it; where
                    // only context was, the matches are all there and the offset is not offered.
                    let continuing = match found.paging() {
                        Some(Paging::Continue(next)) => {
                            format!("ask again with offset {next} for the matches after these, or ")
                        }
                        _ => String::new(),
                    };
                    body.push_str(&format!(
                        "\n\n(this search reached the limit of {} characters on a result and is \
                         incomplete; {continuing}use output files or count to survey the matches, \
                         ask for less context, or search a narrower directory)",
                        crate::workspace::MAX_SEARCH_CHARS
                    ));
                } else if found.truncated {
                    // Without this a model that gets exactly the cap concludes it has
                    // every occurrence, which is how a rename misses call sites.
                    let rest = match found.paging() {
                        // The offset is what makes the rest reachable: narrowing the pattern is a
                        // guess, and a guess that misses drops the matches it was meant to find.
                        Some(Paging::Continue(next)) => {
                            format!("ask again with offset {next} for the rest")
                        }
                        // A walk that stopped short of the tree cannot be paged past its own cap,
                        // because every later page stops at the same place. The notice above says
                        // what to do instead.
                        _ => "narrow the pattern or search a subdirectory".to_string(),
                    };
                    body.push_str(&format!(
                        "\n\n(this search stopped at {} matches and is incomplete; {rest})",
                        found.matches.len()
                    ));
                }
                body
            });
            // The match lines alone, which are empty for a search that found nothing: the
            // sentence saying so, the count past the end and the notices after the matches are the
            // driver's words and are not glimpsed (VIEW-24). Built in the same gate as the body, so
            // whether there is anything to glimpse is not decided here.
            let glimpsed = policy.render_in_place("search", &found, |found| match_lines(&found));
            Produced {
                glimpsed: Some(glimpsed),
                ..Produced::new(rendered, proposed_where, note)
                    .of_content()
                    .capped(incomplete)
                    .linked(links)
                    .paging(paging)
            }
        }
        // Worded about the name typed. The error carries where it landed, which a link can choose.
        Err(e) => file_tool_problem(policy, "search", "directory", &e, &proposed_where),
    }
}

/// A `read_git` argument the planner wrote as text, promoted as routing: `None` when it was not
/// given, and the refusal to hand back when it could not be promoted.
fn git_argument<S: Sink>(
    policy: &mut Policy<'_, S>,
    arguments: &Value,
    field: &'static str,
) -> Result<Option<Labelled<String>>, String> {
    match argument(arguments, field) {
        Some(proposed) => match policy.promote_confined_read("read_git", field, &proposed) {
            Ok(promoted) => Ok(Some(promoted)),
            Err(denial) => Err(format!("refused: {denial}")),
        },
        None => Ok(None),
    }
}

/// A `since` or `until` day, as the seconds since the epoch its start or its end falls at.
fn git_day(
    value: Option<&Labelled<String>>,
    field: &str,
    end: bool,
) -> Result<Option<i64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Ok(text) = value.clone().into_trusted() else {
        return Err(format!("refused: the {field} was not trusted"));
    };
    match crate::git::parse_day(text.trim()) {
        Some(start) => Ok(Some(if end { start + 86_399 } else { start })),
        None => Err(format!(
            "error: '{field}' must be a day written YYYY-MM-DD, such as 2026-01-31"
        )),
    }
}

/// What the planner is told about a `read_git` call that was not answered: the driver's sentence for
/// the failure, and what to do instead where this turn is offered the tool that would do it.
///
/// The two are built separately because only one of them depends on the turn. The sentence is the
/// same for everybody, and `run` is the only other way to read a repository this reader declines, so
/// a turn offered no `run` is told what happened and nothing more. Composed here rather than in the
/// workspace, which words a failure for a log and for a person as well as for the planner and has no
/// turn's tool list to read.
///
/// `said` is that sentence, from [`workspace_failure`].
fn git_failure(mut said: String, e: &crate::workspace::WorkspaceError, running: Running) -> String {
    if let crate::workspace::WorkspaceError::Git { declined, .. } = e
        && running.offered()
        && let Some(instead) = declined.instead()
    {
        said.push(' ');
        said.push_str(instead);
    }
    said
}

/// What a search that ran out of time tells the planner, written inside the body. It says how many
/// files were opened, which the workspace counted, and names the setting that sets the time, as
/// [`read_git_time_note`] does for a status. A directory or an include glob gives the search fewer
/// files to open, and it is not offered as the whole remedy: the clock is wall time, so a search on a
/// busy machine can stop again on the narrowed tree. Fixed words, the count and no name from the tree
/// (SEARCH-3).
fn search_time_note(searched: usize) -> String {
    format!(
        "\n\n(this search ran out of time after {searched} files and did not read the rest; \
         a directory or an include glob gives it fewer files to read, but a busy machine can \
         stop the narrowed search again, so raise search.maxSeconds in the settings to give a \
         search more time)"
    )
}

/// What a read_git answer that ran out of time tells the planner can narrow it. A status takes
/// only a path, so it is not told about a range or a count, and it is told the setting that sets
/// the time and, where the turn holds `run`, that `git status` through it is not held to that time.
fn read_git_time_note(query: crate::git::Query, run_held: bool) -> String {
    if !matches!(query, crate::git::Query::Status) {
        return "\n\n(read_git ran out of time and this answer is partial; narrow it with a \
                path, a range or a smaller count)"
            .to_owned();
    }
    let instead = if run_held {
        ", or use run with git status"
    } else {
        ""
    };
    format!(
        "\n\n(read_git ran out of time and this status is partial; narrow it with a path, or \
         raise search.maxSeconds in the settings{instead})"
    )
}

fn read_git<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let workspace = tools.workspace;
    // Whether anything this tool refuses may point at `run`, which is how git answers what this
    // reader will not. Read once here because every refusal below is worded against it.
    let running = tools.running;
    let Some(named) = argument(arguments, "query") else {
        return Produced::problem(
            "error: 'query' is required: one of log, show, diff, status, tags or search",
        );
    };
    // The question is routing, promoted like any other proposal and then matched against the
    // closed set, so a name off the list is refused rather than guessed at.
    let query = match policy.promote_confined_read("read_git", "query", &named) {
        Ok(promoted) => match promoted.into_trusted() {
            Ok(name) => match crate::git::Query::named(name.trim()) {
                Some(query) => query,
                None => {
                    return Produced::problem(format!(
                        "error: read_git answers log, show, diff, status, tags and search, not \
                         {}.{}",
                        name.trim(),
                        if running.offered() {
                            " Use run to ask git for anything else."
                        } else {
                            ""
                        }
                    ));
                }
            },
            Err(_) => return Produced::problem("refused: the query was not trusted"),
        },
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    let proposed = argument(arguments, "repository").unwrap_or_else(|| {
        Labelled::new(
            ".".to_string(),
            bravebot_core::label::Label::untrusted_public(),
        )
    });
    let repository = match policy.promote_confined_read("read_git", "repository", &proposed) {
        Ok(repository) => repository,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let shown = match policy.read_planner_argument("read_git", "repository", &proposed) {
        Ok(shown) => shown,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let revision = match git_argument(policy, arguments, "revision") {
        Ok(revision) => revision,
        Err(refused) => return Produced::problem(refused),
    };
    let path = match git_argument(policy, arguments, "path") {
        Ok(path) => path,
        Err(refused) => return Produced::problem(refused),
    };
    let pattern = match git_argument(policy, arguments, "pattern") {
        Ok(pattern) => pattern,
        Err(refused) => return Produced::problem(refused),
    };
    let since = match git_argument(policy, arguments, "since")
        .and_then(|since| git_day(since.as_ref(), "since", false))
    {
        Ok(since) => since,
        Err(refused) => return Produced::problem(refused),
    };
    let until = match git_argument(policy, arguments, "until")
        .and_then(|until| git_day(until.as_ref(), "until", true))
    {
        Ok(until) => until,
        Err(refused) => return Produced::problem(refused),
    };

    // A literal, like a search's offset: it names nothing, so there is no destination for it to
    // decide.
    let count = arguments
        .get("count")
        .and_then(Value::as_u64)
        .map_or(crate::git::DEFAULT_COUNT, |n| {
            n.clamp(1, crate::git::MAX_COUNT as u64) as usize
        });
    let skip = arguments
        .get("skip")
        .and_then(Value::as_u64)
        .map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
    let messages = arguments
        .get("messages")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // A deny rule over the file a question names covers asking about its history too, and is said
    // the way every other read says it, before anything under `.git` is opened.
    let inside = |relative: &str| crate::workspace::in_repository(&shown, relative);
    let git_shown = inside(".git");
    let mut named_paths = vec![shown.clone(), git_shown.clone()];
    for (field, value) in [("path", path.as_ref()), ("revision", revision.as_ref())] {
        let Some(value) = value else { continue };
        let written = match policy.read_planner_argument("read_git", field, value) {
            Ok(written) => written,
            Err(denial) => return Produced::problem(format!("refused: {denial}")),
        };
        let relative = match field {
            "path" => Some(written.as_str()),
            _ => crate::git::path_in(&written),
        };
        if let Some(relative) = relative.filter(|r| !r.is_empty() && *r != ".") {
            named_paths.push(inside(relative));
        }
    }
    for named in &named_paths {
        if let Err(refusal) = refuse_denied_path(policy, tools.workspace, Purpose::Read, named) {
            return Produced::problem(refusal);
        }
    }

    let question = crate::workspace::GitQuestion {
        repository: &repository,
        revision: revision.as_ref(),
        path: path.as_ref(),
        pattern: pattern.as_ref(),
        query,
        count,
        skip,
        messages,
        since,
        until,
        beside: tools.beside,
    };
    let answer = match workspace.read_git(policy, &question) {
        Ok(answer) => answer,
        Err(e) => {
            let said = workspace_failure(policy, "read_git", "repository", &e, &shown);
            return Produced::problem(format!("error: {}", git_failure(said, &e, running)));
        }
    };

    // Scanned before the planner is given it, as a file read is (CRED-15): a commit that added a
    // key puts the key in the diff. A file's lines are scanned as a read of that file and keyed
    // by it, so agreeing to one file's key agrees to nothing about another's; everything else the
    // answer holds is scanned as a read of `.git`.
    let mut keyed_findings: Vec<(String, bravebot_core::credentials::Finding)> = Vec::new();
    if answer.label().is_trusted() {
        let read = match examine_git_answer(policy, &answer, running) {
            Ok(read) => read,
            Err(refusal) => return Produced::problem(refusal),
        };
        let pieces = read
            .printed
            .into_iter()
            .map(|p| (inside(&p.path), p.first_line, p.text))
            .chain([(git_shown.clone(), 1, read.around)]);
        for (path, first_line, text) in pieces {
            let key = workspace.trust_key(&path);
            let text = Labelled::new(text, answer.label());
            for finding in policy.scan_a_read("read_git", &path, first_line, &text) {
                let again = keyed_findings
                    .iter()
                    .any(|(k, f)| *k == key && f.fingerprint == finding.fingerprint);
                if !again {
                    keyed_findings.push((key.clone(), finding));
                }
            }
        }
    } else {
        let body = policy.render_in_place("read_git", &answer, |a| a.text);
        policy.scan_a_read("read_git", &shown, 1, &body);
    }
    let (asked, allowed): (Vec<_>, Vec<_>) = keyed_findings
        .into_iter()
        .partition(|(key, _)| !policy.read_exposure_is_allowed(key));
    if !asked.is_empty() {
        let pending: Vec<_> = asked.iter().map(|(_, finding)| finding).collect();
        tools.recording().record(workspace.root(), &pending);
        let request = crate::confirm::ExposureRequest {
            path: git_shown.clone(),
            credentials: describe_all(&pending),
        };
        let (decision, answered_by) = confirmer.confirm_exposing_read_noting_the_mode(&request);
        if decision != Decision::Approve {
            policy.refuse_unapproved_exposure("read_git");
            return Produced::refused_with_a_note(
                format!(
                    "refused: what read_git would show from {git_shown} holds what looks like a \
                     credential, and the user did not agree to you being shown it. Do not try to \
                     read it another way: work without it, or say in your reply what you needed \
                     from it."
                ),
                format!("not shown, it holds {}", describe_all(&pending).join("; ")),
            );
        }
        for (key, _) in &asked {
            policy.allow_exposing_read(key, answered_by);
        }
    }
    let found: Vec<_> = allowed
        .into_iter()
        .chain(asked)
        .map(|(_, finding)| finding)
        .collect();

    let incomplete = {
        let shaped = policy.render_in_place("read_git", &answer, |a| a.cut || a.timed_out);
        let proof = policy.authorise_display_release("whether a read of history hit a cap");
        shaped.declassify(&proof)
    };
    let note = note_for(policy, "read_git", &answer, |a| {
        tally(a.text.lines().count(), "line", "lines")
    });
    let rendered = policy.render_in_place("read_git", &answer, |a| {
        let mut body = a.text.trim_end_matches('\n').to_owned();
        if a.withheld {
            body.push_str(
                "\n\n(a path a deny rule covers, or one whose name is not UTF-8, was left out of \
                 this answer)",
            );
        }
        if a.timed_out {
            body.push_str(&read_git_time_note(query, tools.beside.holds(Named::Run)));
        } else if let Some(next) = a.next {
            let (answer, more) = match query {
                crate::git::Query::Tags => ("list of tags", "tags"),
                crate::git::Query::Search => ("search", "matching lines"),
                _ => ("log", "commits"),
            };
            body.push_str(&format!(
                "\n\n(this {answer} stopped with more {more} to list; ask again with skip {next} \
                 for the next of them)"
            ));
        } else if a.cut {
            body.push_str(
                "\n\n(this answer stopped at read_git's cap and is incomplete; narrow it with a \
                 path, a range or a smaller count)",
            );
        }
        body
    });
    // Without the notices appended above, which are the driver's words (VIEW-24).
    let glimpsed = policy.render_in_place("read_git", &answer, |a| {
        a.text.trim_end_matches('\n').to_owned()
    });
    Produced {
        glimpsed: Some(glimpsed),
        ..Produced::new(rendered, shown, exposed_note(note, &found))
            .of_content()
            .capped(incomplete)
    }
}

#[cfg(test)]
mod tests {
    use crate::exec::Deadlines;
    use crate::watch::Arming;
    /// Where a gate shows up in the trail, so a test can say which of two reads happened first.
    fn gate_at(sink: &bravebot_core::event::RecordingSink, gate: &str, detail: &str) -> usize {
        sink.events()
            .iter()
            .position(|event| match event {
                bravebot_core::event::Event::GatePassed {
                    gate: passed,
                    detail: said,
                } => *passed == gate && said.contains(detail),
                _ => false,
            })
            .unwrap_or_else(|| {
                panic!(
                    "no {gate} gate saying {detail:?} in the trail: {:?}",
                    sink.events()
                )
            })
    }

    use super::*;

    mod activity;

    mod questions;

    mod scheduling;

    /// TOOL-4, ADVISOR-7, SPAWN-5: a failed or cancelled request to the advisor or a processor is
    /// worded by the driver, carries the cost of the attempts that completed and the wait, and
    /// marks a cancelled one separately so the turn ends as a stop.
    mod model_requests;

    mod todos;

    /// Arming a watch is the one thing on this surface that outlives the turn asking for it, so
    /// what it refuses is the interesting half.
    mod watching;

    /// Editing compares the planner's `old_text` against the file to find the passage to
    /// replace, and that comparison decides whether the write happens at all.
    mod editing;

    /// TOOL-6 for the refusals of `read_file` (the saved outputs of a notebook) and `read_git`
    /// when the driver may not examine what was read. A run does not reach either refusal today,
    /// because a file or an answer nobody vouched for is held back before it is examined, so the
    /// helpers are called directly. The wording is the driver's own and names a way out only where
    /// the turn holds it.
    mod untrusted_refusals;

    /// TRACE-1 at `run`, which resolves two kinds of path without a promotion in front of them:
    /// the directory it runs in and the file a redirection opens. Either one outside the workspace
    /// refuses the call before anything is approved or run, and the trail records the refusal
    /// under the name the planner is told.
    mod confinement;

    /// PERM-9 for the settings file named with `--settings`, which a session reads as a layer that
    /// grants and which a write must therefore ask about as it does for `.bravebot`.
    mod named_settings;

    /// LABEL-5 over a tool's arguments, at the three tools that take a decision from one and had
    /// been releasing it through a display witness instead.
    ///
    /// The clause holds an argument by the context the planner wrote it in rather than by the
    /// wrapper it arrives in: readable while that context has met nothing untrusted, refused once
    /// it has. `Policy::authorise_display_release` asks neither question, so a tool that compiled
    /// a command line, resolved a host or looked up a job name from a value released that way was
    /// deciding things from an argument the clause says it may no longer read.
    ///
    /// Each tool gets a pair. The first says the argument is still read from a trusted context,
    /// and says it through the trail: a value released for a screen and a value read as the
    /// planner's own words are the same bytes, so the gate a tool went through is the only thing
    /// that tells the two apart. The second is the refusal the clause requires.
    mod arguments;
    /// RECALL-4 to RECALL-8: the `remember` tool, called against a state directory of its own.
    #[cfg(unix)]
    mod remember;

    /// TURN-10: reads of one round that run beside each other.
    mod reading_together;
    /// RECALL-7 and RECALL-10: the `recall` tool, called against a state
    /// directory of their own.
    #[cfg(unix)]
    mod recalling;

    /// `message_session` at dispatch: the refusals that come before a person is asked, what the
    /// person is asked, the bound and the standing answer, and what the planner and the screen are
    /// told (MSG-1 to MSG-5, MSG-10, MSG-12).
    mod session_messages;

    /// TRUST-4 at the `read_file` call: the stamp is taken beside the preview and handed to the
    /// vouch, so a yes given while a `run` line started is not spent on the file.
    mod vouching_after_a_preview;

    /// FSWATCH-7: the one look both front ends take at a watch.
    mod looking_at_a_watch;

    /// SETLINK-1: `set_link` records an address only when it is a whole word of a line the person
    /// typed and is one web address, on any host.
    mod setting_a_link;

    /// PANEL-16: the issues and pull requests the planner's own arguments name, read after the gate
    /// that reads them and only in a context that has met nothing untrusted.
    mod references;

    /// The failure sentences a `run` line gets, and how each failed step is named.
    mod failed_run_lines;

    /// Looks at a background job: the count of lines, the notes, and the turn's end.
    mod job_looks;

    /// What `search` and `list_files` say of a glob argument.
    mod glob_arguments;

    /// A tool name as dispatched, and the typed lines handed to `set_link`.
    mod tool_names_at_dispatch;

    /// The tools a turn is offered, the reach each is given and the lists scanned for them.
    mod the_tool_table;

    /// How a call held for a person is drawn.
    mod held_calls;

    /// What a delegate is offered and what `spawn_agent` accepts.
    mod delegate_offers;

    /// The descriptions of the tools, which name only the tools the list holds.
    mod tool_descriptions;

    /// A `Content-Type` header as shown on a screen.
    mod content_types;

    /// `run`: its arguments, the unconfined and scope requests and its deadlines.
    mod run_arguments;

    /// `job_output`: the bound of a wait and what it offers.
    mod job_output_waits;

    /// The text of `run` and `read_file`, as the planner reads it.
    mod run_and_read_file_text;

    /// What a read of a file fills a slot with.
    mod reading_files;

    /// The description of `ask_user`.
    mod asking_the_user;

    /// The schema of each tool and the approval the mutating tools state.
    mod tool_schemas;

    /// Output cut to a cap, and a page of it.
    mod output_caps;

    /// The files beside a skill, `search_skills` and the help pages, which need a unix home.
    #[cfg(unix)]
    mod loading_skills;
}
