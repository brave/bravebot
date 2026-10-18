//! What a turn is asked to do: the [`Task`] a caller builds, and what it carries.

use base64::Engine;
use bravebot_aichat::protocol::{Effort, ImageUrl, Part};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::permissions::Permissions;
use std::path::PathBuf;

use super::MAX_TOOL_ROUNDS;
use crate::conversation::Composed;
use crate::tools;

/// A file the user attached, to be carried rather than read as text.
///
/// Separate from [`Task::files`] because the two differ in what reaches the planner: a context
/// file arrives as text in a message, an attachment as bytes in a part. Trusted for the same
/// reason though, which is that the user named it.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Workspace-relative, and routing: it decides which file is opened.
    pub path: String,
    /// The media type to name in the URI, chosen by the interface from a closed table of
    /// extensions.
    ///
    /// Not routing. It cannot redirect anything, since the path alone decides what is opened, and
    /// it is one of a handful of constants rather than anything a user or a model composed.
    pub media: String,
}

/// What `--system-prompt` and `--append-system-prompt` named (CLI-19).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemPrompts {
    /// Stands in for [`OPENING`] alone. Everything after it in the prompt stays, so the planner
    /// still has the guidance its refusals and its quarantine rest on.
    ///
    /// [`OPENING`]: super::OPENING
    pub replacing: Option<String>,
    /// Added to the standing instructions as the last source, after the project's `AGENTS.md`
    /// (INSTR-10). A delegate is given this one and not the other.
    pub appending: Option<String>,
}

/// What a turn is asked to do.
#[derive(Debug, Clone)]
pub struct Task {
    pub(crate) file_authority: Option<bravebot_core::file_authority::FileAuthority>,
    /// The session's background jobs, where the session keeps them between turns (RUN-15).
    ///
    /// `None` for a one-shot run, an incognito session and every delegate, whose jobs end with
    /// the turn that started them.
    pub(crate) jobs: Option<tools::SessionJobs>,
    /// The user's instruction. The only trusted input.
    pub prompt: String,
    /// Why this prompt exists, where nobody typed it.
    ///
    /// `None` for a line a person wrote, which is nearly every turn. A watch firing submits an
    /// ordinary turn whose prompt the agent composed, and a transcript drawn later has nothing
    /// but the sentence to go on unless the record says so. See [`Composed`].
    pub composed: Option<Composed>,
    /// Whether the driver wrote this prompt and no person typed it, which the view of the request
    /// says of it. Set only by a caller that composed the line itself.
    pub driver_wrote_the_prompt: bool,
    /// Workspace-relative files to include as context. Trusted because the user named
    /// them, not the model.
    pub files: Vec<String>,
    /// Files the user attached, carried as bytes rather than read as text.
    pub attachments: Vec<Attachment>,
    /// Text files the user dropped on the window.
    ///
    /// Context, exactly as [`Task::files`] is and trusted for the same reason, and kept apart from
    /// them only because a drop may name a file outside the workspace: the path came from a
    /// gesture rather than from anything a model said. Nothing else in the directory it came from
    /// becomes reachable.
    pub dropped_text: Vec<String>,
    /// Parts of past sessions the person named with `@session:<id>`, as `(id, text)`.
    ///
    /// Part of the person's own message, which is what the person did: they typed the name and
    /// sent the line. The text is read out of a stored record by the caller, and a record holds
    /// only what a planner could hold (SESSION-2), so no untrusted byte comes in this way. The
    /// driver carries it and branches on none of it.
    pub session_excerpts: Vec<(String, String)>,
    /// Input piped into the process on stdin.
    ///
    /// Untrusted, unlike [`Task::files`]. Naming a file says which bytes the user meant; a pipe
    /// says only that some bytes arrived, and `gh pr diff` carries whatever the author of the
    /// pull request wrote. So the planner is shown a reference, never the bytes.
    pub piped: Option<String>,
    /// Images the user pasted, carried with the prompt they were pasted into.
    ///
    /// Trusted for the reason [`Task::prompt`] is, and by the same act: the user copied something
    /// and pressed a key. The caveat is shell mode's, and stated in
    /// [`Policy::admit_pasted_image`]: a screenshot of a hostile page carries a stranger's words
    /// into the context as though the user had written them.
    ///
    /// [`Policy::admit_pasted_image`]: bravebot_core::policy::Policy::admit_pasted_image
    pub images: Vec<PastedImage>,
    /// The user's own directory, holding standing instructions and skills.
    ///
    /// Supplied by the caller rather than read from the environment, and `None` by default. A
    /// library that reached for `$HOME` behind its callers' backs would make every test depend
    /// on whatever the developer happened to have installed, and a run would differ from the
    /// same run elsewhere for reasons nothing in the task described.
    pub home: Option<PathBuf>,
    /// The user's profile directory, which is the directory `home` sits inside.
    ///
    /// What a leading `~` in a command line the planner sends stands for (CMDLINE-4). Carried
    /// beside `home` rather than derived from it, because the two answer different questions and
    /// the four things read out of `home` all want the state directory: a `~` names a file of the
    /// user's, and resolving it against `~/.bravebot` would put every home-relative path the
    /// planner writes inside the directory this program keeps its own files in.
    ///
    /// Supplied by the caller for the reason `home` is, and `None` by default, which refuses a
    /// `~` for want of anything to stand for rather than guessing at one.
    pub profile: Option<PathBuf>,
    /// The directory the platform keeps this user's disposable files in, where the copy of a
    /// picture goes that a person opens before letting the planner see it (VET-4).
    ///
    /// Supplied by the caller for the reason `home` is, and `None` by default, which refuses that
    /// prompt rather than writing the copy anywhere this library chose.
    pub cache: Option<PathBuf>,
    /// The run prompts this session has already put to the person, by program and arguments.
    ///
    /// Empty by default and for a caller that keeps nothing between turns. It grants nothing and
    /// no gate reads it: what it decides is whether a run prompt says that this line's arguments
    /// have already differed and so that a pattern in a settings file is what ends the asking.
    ///
    /// Carried by the caller for the reason `home` and `remembering` are: a turn is where a prompt
    /// is drawn, and a session is where somebody answers the same shape of prompt all day.
    pub asked_about: bravebot_core::programs::AskedAbout,
    /// The files this session has been asked about, and agreed to the planner being given.
    ///
    /// Empty by default and for a caller that keeps nothing between turns, which costs one extra
    /// question about each such file per turn rather than anything a gate reads: the read was
    /// already allowed by the trust map before the scan ran.
    ///
    /// Carried by the caller for the reason [`Task::asked_about`] is: a turn is where a question
    /// is drawn, and a session is where somebody answers the same one all day.
    pub exposed: bravebot_core::credentials::Exposed,
    /// The session this turn belongs to, where a run prompt's answer may outlive it.
    ///
    /// `None` by default and for every turn with nobody to put a prompt to: a one-shot run, a
    /// session whose channel has closed. Such a turn reads no record of remembered lines and writes
    /// none, because what a record answers is a prompt, and where no prompt can be drawn it would
    /// be saying instead which effects may happen with nobody there to see them.
    ///
    /// The session's own identifier rather than a flag, because the reading back has to say which
    /// answers a person is still carrying from an earlier session and a flat list cannot.
    /// Supplied per turn for the reason `home` is: which session this is belongs to the caller.
    pub remembering: Option<String>,
    /// A model the planner may put a question to, when the session named one.
    ///
    /// `None` leaves it to the `advisorModel` setting, which may name none. Never offered on a
    /// delegate's turn: a delegate is not shown the tool and a call to it is answered as an
    /// unknown name.
    pub advisor: Option<String>,
    /// Where messages from other sessions wait for this session's planner (MSG-6).
    ///
    /// `None` for every session that is not a background one, and for every delegate: a delegate's
    /// task came from a planner, so a message aimed at the session is for the planner that started
    /// it and stays in the inbox for that planner's next request.
    ///
    /// A caller that sets one also implements [`Reporter::session_message`], which is the only thing
    /// that shows the person a message going in: the default says nothing, and a planner that
    /// changes course on a message nobody saw arrive reads as one that changed its mind unprompted.
    ///
    /// [`Reporter::session_message`]: crate::report::Reporter::session_message
    pub inbox: Option<std::sync::Arc<dyn crate::messages::Inbox>>,
    /// How this session reaches the other background sessions, or `None` where it cannot send
    /// (MSG-12).
    ///
    /// `None` for every session that is not a background one and for every delegate, so none of
    /// them is offered `message_session` and a call to it by name is answered as an unknown name.
    /// A caller that sets one also implements [`Reporter::message_sent`] and answers
    /// [`Confirmer::confirm_message`]: the first is the only thing that shows the person a message
    /// going out under a standing answer, and the second is where the person approves the target.
    ///
    /// [`Reporter::message_sent`]: crate::report::Reporter::message_sent
    /// [`Confirmer::confirm_message`]: crate::confirm::Confirmer::confirm_message
    pub courier: Option<std::sync::Arc<dyn crate::messages::Courier>>,
    /// The model to request, when the user has chosen one.
    ///
    /// `None` means the configured default applies. Supplied per turn rather than read here for
    /// the same reason as `home`: where the choice is stored is the caller's business, and a turn
    /// should not differ from the same turn elsewhere for reasons the task does not state.
    pub model: Option<String>,
    /// How hard to think, when the user has asked for a level.
    ///
    /// `None` leaves the service its own default, which is what a build nobody has asked sends.
    /// Supplied per turn for the reason `model` is: where the choice is kept is the caller's
    /// business.
    pub effort: Option<Effort>,
    /// The schema the person running this task supplied for its reply, sent with each of the
    /// planner's requests.
    ///
    /// The planner's own requests only: a delegate is built with none, and a compaction summary and
    /// a manifest step are not the reply the person asked a shape of. Held here rather than checked
    /// here, because whether the finished reply conforms is the caller's question to put once the
    /// turn has ended.
    pub output_schema: Option<crate::output_schema::OutputSchema>,
    /// How many tool-calling rounds this turn may make, or `None` for no bound.
    ///
    /// The caller's business, like `model` and `home`, because the right answer depends on who is
    /// there. A person watching a turn is a better bound than any number: they can see what it is
    /// doing, and a stop reaches it mid-round. A bound would only interrupt work that was going
    /// fine. So the interface passes `None`, and an unattended run passes
    /// [`MAX_TOOL_ROUNDS`], where nothing else can end a loop.
    pub rounds: Option<usize>,
    /// The session's spend limit, which the turn asks about before a request that would go past it
    /// (TURN-8). Empty unless a caller with a person in front of it sets one, for the reason
    /// `rounds` is the caller's: only that caller can put the question.
    pub spend_limit: crate::spend_limit::SpendLimit,
    /// What the session had spent before this turn, which the limit is read against together with
    /// what the turn has spent so far.
    pub spent_before: u64,
    /// The token a person sets to stop this one delegate, where the task is a delegate's.
    ///
    /// Read at a round boundary and treated as that round's limit: the delegate loses its tools
    /// and answers with what it has (DELEGATE-25). `None` for a turn, which has the stop keys.
    pub stop: Option<bravebot_core::cancel::DelegateStop>,
    /// How much of what a program printed may enter this turn's conversation, or `None` for the
    /// built-in cap.
    ///
    /// The caller's business for the reason `rounds` is: both are budgets on the same context, and
    /// which of a settings file's layers named this one is something only the caller that read them
    /// knows. `None` rather than the built-in number, so the default lives in one place
    /// ([`crate::tools::OUTPUT_CAP`]) and a caller saying "unchanged" is not a second copy of it.
    pub output_cap: Option<usize>,
    /// The most one `download_url` call this turn makes may write to a file, or `None` for the
    /// built-in cap ([`crate::tools::DOWNLOAD_CAP`]).
    ///
    /// The caller's business for the reason `output_cap` is: it is what `download.maxBytes` came to
    /// across the layers of a settings file only the caller read.
    pub download_cap: Option<usize>,
    /// How long a command this turn runs may take, and the most one call may ask for.
    ///
    /// The caller's business for the reason `output_cap` is, and resolved rather than optional: the
    /// two figures constrain each other, so a caller handing over what a file said one at a time
    /// would leave the arithmetic to be done again here. [`crate::exec::Deadlines::resolve`] is
    /// where it is done, and [`crate::exec::Deadlines::BUILT_IN`] is what a caller that read no
    /// settings file gets.
    pub deadlines: crate::exec::Deadlines,
    /// Whether a program `run` starts is confined to what its plan accounts for.
    ///
    /// Off unless the caller turns it on, which every front end does: a test that runs a program
    /// in a scratch directory outside the places a session is opened on would otherwise be
    /// refused. A delegate inherits the spawning turn's answer, since the person's answer about
    /// what a program may reach does not stop being theirs because the work moved.
    pub confine_runs: bool,
    /// Which profile those programs run under, or that none does (SANDBOX-22).
    ///
    /// Read only where `confine_runs` is true. A delegate inherits it with `confine_runs`.
    pub sandbox: bravebot_sandbox::SandboxMode,
    /// Whether the front end can put a line that starts with no sandbox to a person (SANDBOX-29).
    ///
    /// Off unless the caller turns it on. The terminal interface and a session in lines draw the
    /// question; a window refuses the request without asking, and a run with nobody to ask refuses
    /// every question. A refusal names the `unconfined` argument only where this is on, so a planner
    /// is not pointed at a request its front end would turn down. A delegate is never pointed at it.
    pub asks_about_unconfined: bool,
    /// Rules the user wrote in advance about which actions to ask them about.
    ///
    /// Supplied per turn for the reason `home` and `model` are: which file they came from is the
    /// caller's business. Empty by default, which is a session that behaves as it did before a
    /// settings file could say anything.
    pub permissions: Permissions,
    /// What the settings say a commit message and a pull request this turn writes may carry.
    ///
    /// Empty by default, which is a caller that read no settings file: neither destination is
    /// decided and the planner is told nothing about either, exactly as it was before the block
    /// existed. Supplied per turn for the reason `permissions` is: which files it was resolved
    /// from is the caller's business, and a turn should not differ from the same turn elsewhere
    /// for reasons the task does not state.
    ///
    /// Nothing here writes a commit message. What this decides is what the planner is told
    /// (BACKEND-30), which is where the answer has to be for it to survive a long turn.
    pub attribution: bravebot_config::Attribution,
    /// The words the command line put in the planner's system prompt, where it did (CLI-19).
    ///
    /// Plain strings, without a label, because a person typed them as an argument of the command
    /// they ran, which is the footing their own message has (LABEL-8).
    /// They grant nothing: the mode, the permission prompts and every refusal are enforced below
    /// the prompt, whatever it says. Empty by default, which is every caller that read no flag.
    pub system_prompts: SystemPrompts,
    /// The built-in style `/style` chose, which stands in for the opening where `--system-prompt`
    /// did not (CLI-19). A person's own words win over it. Not given to a delegate.
    pub style: Option<crate::styles::Style>,
    /// How much this turn asks before it acts.
    ///
    /// Carried by the task because the planner has to be told about one of them: plan mode refuses
    /// writes however the person would have answered, and a planner reading an unexplained refusal
    /// retries. The others change who answers a question rather than anything about the work, and
    /// say nothing. See [`crate::PermissionMode::instruction`].
    ///
    /// The confirmer enforces it. This is the half the model is told, and the two are set from the
    /// same value by the caller.
    pub permission_mode: crate::LiveMode,
    /// Which tick of a loop this turn is, where a caller is running one.
    ///
    /// `None` for an ordinary turn, and for a prompt the person typed in the middle of a loop.
    /// It changes two things and nothing else: the planner is told it is being asked the same
    /// question again, and a self-paced tick is offered a way to say when the next is due. It
    /// cannot say what the next tick asks, because the line belongs to the person who typed it
    /// and the caller holds it.
    pub tick: Option<Tick>,
    /// Whether this turn may arm a standing watch, and why not where it may not.
    ///
    /// `Unavailable` by default, which is a caller that keeps no watches: the tool is not
    /// offered and a call to it is answered as an unknown name. A caller that does keep them
    /// says how many slots are free, because the bound is the session's and only the session can
    /// count it.
    pub arming: crate::watch::Arming,
    /// Whether this caller will send this turn's line again after a wait the turn asks for.
    ///
    /// `false` by default, which is every caller that runs a turn and stops: a one-shot run, the
    /// desktop bridge, anything with nothing holding the line afterwards. Such a turn is offered
    /// no way to arrange a later look and a call to that tool is answered as an unknown name,
    /// because the wait would be discarded and the confirmation would be telling somebody a
    /// watch exists that does not (SCHED-6).
    ///
    /// Supplied per turn rather than per session, because the answer differs turn by turn on a
    /// caller that keeps a line at all: a loop repeats a line somebody endorsed, so a turn
    /// running a sentence this program wrote has nothing a later look could repeat.
    ///
    /// Read only where `tick` is `None`. A tick is already the thing being asked again, and what
    /// it may say about the next one is the loop's own question.
    pub looking_again: bool,
    /// The condition this session is working towards, where a person set one.
    ///
    /// `None` for a turn with no goal. It changes one thing: the planner is told what the session
    /// is working towards, so the turn aims at it rather than being judged against a condition it
    /// was never shown. Whether it holds is decided after the turn, by the caller, and there is no
    /// tool here that reads or writes this.
    ///
    /// A turn cannot edit it, because the line belongs to the person who typed it and the caller
    /// holds it. A delegate carries none: it has a job of its own, given to it by the turn that
    /// spawned it.
    pub working_towards: Option<String>,
    /// Whether a check that finds nothing may promote a slot without anybody being asked.
    ///
    /// `false` by default, which is every caller that says nothing: what this turns off is a person
    /// being asked before content nobody vouched for reaches the planner, so a default that had it
    /// on would be a different product.
    ///
    /// Supplied per turn for the reason `home` and `permissions` are, and resolved by the caller
    /// rather than here: the three routes into it are a flag, a file in the person's own directory
    /// and a settings key, and which of them won is the caller's business.
    /// `bravebot_core::vetting::auto` is the rule they resolve it with.
    pub auto_vetting: bool,
    /// What this turn is a delegate of, where it is one rather than a person's.
    ///
    /// `None` for every turn somebody typed the prompt for. Where it is set, four things come
    /// from it and from nowhere else: the capabilities the turn holds, the tools it is offered,
    /// the prompt in front of it, and its round bound. Nothing widens it, because the kernel
    /// built it before this turn existed and there is no method here that could.
    pub delegate: Option<bravebot_core::delegate::DelegateSpec>,
    /// The definition the person's line addressed, by the name they typed, where it addressed
    /// one.
    ///
    /// Only a caller holding a line off the input box sets this (ADDRESS-3). It goes into the
    /// routing table, and the kernel matches it against the set this turn resolves, so a name
    /// matching nothing ends the turn before anything is sent.
    pub addressing: Option<String>,
    /// Whether [`Task::model`] outranks the model an addressed definition names.
    ///
    /// Set only by a caller whose model was named on its own command line. CLI-9 ranks that flag
    /// above everything else, and a definition is not one of the layers it ranks. The turn reports
    /// it on the outcome where it changed the model asked for (ADDRESS-11).
    pub model_outranks_a_definition: bool,
    /// The MCP servers this session reached at its start, each by the alias it was declared
    /// under.
    ///
    /// Empty unless a caller launched some. A turn holds a grant to call each server named here
    /// and no other, which is what SERVERS-9 asks of a grant: one per server rather than one for
    /// every server.
    pub servers: Vec<bravebot_core::capability::ServerAlias>,
    /// The servers themselves, whose tools a turn offers once somebody vouched for their lists.
    ///
    /// `None` for a caller that reached none. A delegate is handed its parent's and offered the
    /// tools of the servers its spec holds, which for a kind that holds no servers is none.
    pub mcp: Option<crate::mcp::Session>,
}

/// One tick of a loop, as the turn running it needs to know about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Which tick this is, counting the first.
    pub number: usize,
    /// Whether nobody gave an interval, so this turn says when the next tick is due.
    pub self_paced: bool,
    /// Whether this turn is not offered the tool that sets the pace, though nobody gave an
    /// interval: it is addressed to a definition (ADDRESS-8), so the loop ends after it.
    pub unpaceable: bool,
}

/// The largest image a paste will carry.
///
/// Not a policy rule: nothing about a big picture is unsafe, it is that encoding one into a request
/// costs a third again in base64 and a screenshot of a large display already runs to several
/// megabytes. Every front end that takes a paste holds it to this one number, so a screenshot one
/// takes the other takes too.
pub const MAX_PASTED_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// The media types a front end that cannot see the clipboard may name for a pasted picture
/// (PASTE-3).
///
/// The type lands in a `data:` URL, where it is routing. A caller's string only selects an entry
/// here, and the entry is what is sent.
pub const PASTED_IMAGE_MEDIA: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// An image on its way into a prompt, before it has been encoded for the wire.
///
/// Raw bytes rather than the finished data URL, so the size that is recorded and reported is the
/// size of the picture rather than the size of its encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastedImage {
    /// The IANA type of the bytes, chosen by whichever clipboard flavour answered.
    ///
    /// From a fixed set the driver owns, never from a filename or anything else read: it lands in
    /// the data URL, where it is routing, and a media type taken from content would be one an
    /// attacker chose. Static, so the set stays the clipboard reader's own literals from where it
    /// is read to where it is sent, and a type derived from content does not compile.
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

impl PastedImage {
    /// The part of a message that carries the picture.
    ///
    /// One place, because three requests carry a paste now: a turn's prompt, a question asked
    /// beside the work, and the task a manifest run is planned from. The encoding happens here
    /// rather than in the interface because a data URI is the wire's business, and holding raw
    /// bytes until this point keeps the size the trail reports honest.
    ///
    /// The record `pasting.md` PASTE-8 asks for is the caller's, and each of the three takes it
    /// where it has the policy: [`Policy::admit_pasted_image`] is not on this path because a part
    /// can be built on a thread that holds no policy at all.
    ///
    /// [`Policy::admit_pasted_image`]: bravebot_core::policy::Policy::admit_pasted_image
    pub(crate) fn part(&self) -> Part {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        Part::ImageUrl {
            image_url: ImageUrl {
                url: format!("data:{};base64,{}", self.media_type, encoded),
            },
        }
    }
}

impl Task {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            file_authority: None,
            jobs: None,
            prompt: prompt.into(),
            // A line somebody typed until a caller says what composed it.
            composed: None,
            driver_wrote_the_prompt: false,
            files: Vec::new(),
            attachments: Vec::new(),
            dropped_text: Vec::new(),
            session_excerpts: Vec::new(),
            images: Vec::new(),
            piped: None,
            home: None,
            profile: None,
            cache: None,
            // Nothing has been asked about until a caller says so, which is what a caller keeping
            // nothing between turns is saying.
            asked_about: bravebot_core::programs::AskedAbout::new(),
            // Nothing agreed to until a caller says so, for the same reason.
            exposed: bravebot_core::credentials::Exposed::new(),
            // Nothing is remembered past the session unless a caller says which session this is,
            // which is the caller saying there is somebody a prompt could be put to.
            remembering: None,
            advisor: None,
            inbox: None,
            courier: None,
            model: None,
            effort: None,
            output_schema: None,
            tick: None,
            // No watches unless a caller says it keeps some, for the reason `rounds` is bounded
            // by default: a default cannot know whether anybody is there to read a fire.
            arming: crate::watch::Arming::Unavailable,
            // And nothing will ask again unless a caller says it will, for the same reason: a
            // default cannot know whether anything is still holding the line once this returns.
            looking_again: false,
            working_towards: None,
            // Bounded unless a caller says otherwise. The unbounded case needs somebody watching,
            // and a default cannot know whether anybody is, so the default is the one that is
            // wrong in the cheaper direction.
            rounds: Some(MAX_TOOL_ROUNDS),
            spend_limit: crate::spend_limit::SpendLimit::default(),
            spent_before: 0,
            stop: None,
            // The built-in cap, which is a caller that read no settings file saying nothing about
            // what a command's output may spend.
            output_cap: None,
            download_cap: None,
            // And the built-in figures for how long one may run, for the same reason.
            deadlines: crate::exec::Deadlines::BUILT_IN,
            // Unconfined, which is what a turn has always done.
            confine_runs: false,
            sandbox: bravebot_sandbox::SandboxMode::default(),
            asks_about_unconfined: false,
            permissions: Permissions::new(),
            // Asking, which is what a turn has always done.
            permission_mode: crate::LiveMode::default(),
            // Asking too: nobody has said a check's word may stand in for an answer.
            auto_vetting: false,
            // Nothing said about either destination, which is a caller that read no settings
            // file. Empty is a value the block can carry and this is not it.
            attribution: bravebot_config::Attribution::default(),
            system_prompts: SystemPrompts::default(),
            style: None,
            delegate: None,
            addressing: None,
            model_outranks_a_definition: false,
            servers: Vec::new(),
            mcp: None,
        }
    }

    /// The task one delegate was given.
    ///
    /// Its prompt is the task the kernel recorded on the spec, and its bound is the one its
    /// definition chose beneath its kind's ceiling: a caller cannot set either, which is the
    /// difference between a delegate and a turn. See [`crate::delegate`].
    pub fn delegated(spec: bravebot_core::delegate::DelegateSpec) -> Self {
        let rounds = spec.rounds();
        Self {
            rounds: Some(rounds),
            delegate: Some(spec.clone()),
            ..Self::new(spec.task())
        }
    }

    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.files.push(path.into());
        self
    }

    /// Say that this prompt was composed rather than typed, and what for.
    ///
    /// Only a caller that composed the prompt itself may say so, which is why this is a builder on
    /// the task rather than anything a request can carry: a caller able to set an arbitrary tag
    /// would be able to have a transcript draw the interface's own rows about a line a person
    /// typed. A front end reaching this through the bridge may name its own tag and none of the
    /// agent's.
    pub fn composed_rather_than_typed(mut self, composed: Composed) -> Self {
        self.composed = Some(composed);
        self
    }

    /// Say that the driver wrote this prompt, for a line it sends when nobody typed one.
    pub fn written_by_the_driver(mut self) -> Self {
        self.driver_wrote_the_prompt = true;
        self
    }

    /// Include a text file the user dropped, which may sit anywhere on the disk.
    pub fn with_dropped_text(mut self, path: impl Into<String>) -> Self {
        self.dropped_text.push(path.into());
        self
    }

    /// Add what a past session said to the message, as the person named it.
    pub fn with_session_excerpt(mut self, id: impl Into<String>, text: impl Into<String>) -> Self {
        self.session_excerpts.push((id.into(), text.into()));
        self
    }

    pub fn with_attachment(mut self, path: impl Into<String>, media: impl Into<String>) -> Self {
        self.attachments.push(Attachment {
            path: path.into(),
            media: media.into(),
        });
        self
    }

    /// Attach an image the user pasted into this prompt.
    pub fn with_image(mut self, image: PastedImage) -> Self {
        self.images.push(image);
        self
    }

    pub fn with_piped_input(mut self, text: impl Into<String>) -> Self {
        self.piped = Some(text.into());
        self
    }

    /// Name the user's own directory, usually [`crate::home::directory`].
    ///
    /// Without one, a turn has no global skills and no global standing instructions, which is
    /// the correct behaviour for a caller that has not said where those live.
    pub fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// Name the directory a leading `~` stands for, usually [`crate::home::profile`].
    ///
    /// Without one a command line that starts a path with `~` is refused, which is the correct
    /// answer for a caller that has not said where the user's home is: the alternative is showing
    /// somebody an approval prompt naming a directory this program invented.
    pub fn with_profile(mut self, profile: Option<PathBuf>) -> Self {
        self.profile = profile;
        self
    }

    /// Name the directory a copy of a picture is put in for a person to open, usually
    /// [`crate::home::cache`].
    ///
    /// Without one, `vet_content` over a picture refuses where it would have asked somebody.
    pub fn with_cache(mut self, cache: Option<PathBuf>) -> Self {
        self.cache = cache;
        self
    }

    /// Carry in the run prompts this session has already drawn.
    ///
    /// Said by a caller that holds a session together across turns. Without it every turn starts
    /// with nothing to compare a line against, so no prompt says a line's arguments have varied.
    pub fn already_asked_about(mut self, asked: bravebot_core::programs::AskedAbout) -> Self {
        self.asked_about = asked;
        self
    }

    /// Carry in the files this session has already agreed the planner may be given.
    ///
    /// Said by a caller that holds a session together across turns. Without it every turn asks
    /// again about each file the scan finds something in.
    pub fn already_exposed(mut self, exposed: bravebot_core::credentials::Exposed) -> Self {
        self.exposed = exposed;
        self
    }

    /// Name the session whose run prompts may have their answers remembered past it.
    ///
    /// Said only by a caller that can put a prompt to somebody. Without it a turn neither reads the
    /// record of remembered lines nor writes one, and every run asks.
    pub fn remembering(mut self, session: Option<String>) -> Self {
        self.remembering = session;
        self
    }

    /// Name the model the planner may consult, or `None` to leave it to the `advisorModel` setting.
    pub fn with_advisor(mut self, model: Option<String>) -> Self {
        self.advisor = model;
        self
    }

    /// Give the turn the inbox of the background session it runs in, or `None` for any other.
    pub fn with_inbox(mut self, inbox: Option<std::sync::Arc<dyn crate::messages::Inbox>>) -> Self {
        self.inbox = inbox;
        self
    }

    /// Give the turn the courier of the background session it runs in, or `None` for any other.
    pub fn with_courier(
        mut self,
        courier: Option<std::sync::Arc<dyn crate::messages::Courier>>,
    ) -> Self {
        self.courier = courier;
        self
    }

    /// Address a definition by the name a person typed, or `None` for the session's own planner.
    pub fn addressing(mut self, name: Option<String>) -> Self {
        self.addressing = name;
        self
    }

    /// Say that [`Task::model`] was named on the command line, so it outranks a definition's.
    pub fn model_outranks_a_definition(mut self, outranks: bool) -> Self {
        self.model_outranks_a_definition = outranks;
        self
    }

    /// Request a particular model rather than the configured default.
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    /// Ask for a particular amount of thinking rather than the service's own default.
    pub fn with_effort(mut self, effort: Option<Effort>) -> Self {
        self.effort = effort;
        self
    }

    /// Ask for a reply that matches a schema the person supplied.
    pub fn with_output_schema(
        mut self,
        schema: Option<crate::output_schema::OutputSchema>,
    ) -> Self {
        self.output_schema = schema;
        self
    }

    /// Bound how many tool-calling rounds this turn may make, or `None` to leave it unbounded.
    ///
    /// `None` is for a caller with a person in front of it, who is the better bound. See
    /// [`Task::rounds`].
    pub fn with_rounds(mut self, rounds: Option<usize>) -> Self {
        self.rounds = rounds;
        self
    }

    /// Hold the turn to the session's spend limit, given what the session had spent before it began.
    ///
    /// See [`Task::spend_limit`].
    pub fn with_spend_limit(
        mut self,
        limit: crate::spend_limit::SpendLimit,
        spent_before: u64,
    ) -> Self {
        self.spend_limit = limit;
        self.spent_before = spent_before;
        self
    }

    /// Let a person stop this delegate alone, without stopping the turn that started it.
    pub fn stoppable_by(mut self, stop: bravebot_core::cancel::DelegateStop) -> Self {
        self.stop = Some(stop);
        self
    }

    /// Bound how much of what a program printed may enter the conversation, or `None` for the
    /// built-in cap.
    ///
    /// What `run.maxOutput` comes to. A caller that read no settings file passes `None`, which is
    /// the cap every turn ran under before the key existed. See [`Task::output_cap`].
    pub fn with_output_cap(mut self, cap: Option<usize>) -> Self {
        self.output_cap = cap;
        self
    }

    /// Bound how much one `download_url` call may write to a file, or `None` for the built-in cap.
    ///
    /// What `download.maxBytes` comes to. See [`Task::download_cap`].
    pub fn with_download_cap(mut self, cap: Option<usize>) -> Self {
        self.download_cap = cap;
        self
    }

    /// State how long a command may run, and the most one call may ask for.
    ///
    /// What `run.defaultSeconds` and `run.maxSeconds` come to, which is
    /// [`crate::exec::Deadlines::resolve`]'s answer. A caller that read no settings file leaves this
    /// alone and runs under the figures every turn ran under before the keys existed. See
    /// [`Task::deadlines`].
    pub fn with_deadlines(mut self, deadlines: crate::exec::Deadlines) -> Self {
        self.deadlines = deadlines;
        self
    }

    /// Hold every program `run` starts to the profile its plan accounts for. See
    /// [`Task::confine_runs`].
    pub fn with_confined_runs(mut self, confine: bool) -> Self {
        self.confine_runs = confine;
        self
    }

    /// Hold those programs to `mode`'s profile. See [`Task::sandbox`].
    pub fn with_sandbox_mode(mut self, mode: bravebot_sandbox::SandboxMode) -> Self {
        self.sandbox = mode;
        self
    }

    /// Say the front end can ask about a line that starts with no sandbox. See
    /// [`Task::asks_about_unconfined`].
    pub fn asking_about_unconfined(mut self, asks: bool) -> Self {
        self.asks_about_unconfined = asks;
        self
    }

    /// Keep this turn's background jobs with the session, so they are running when the next turn
    /// begins and end when the session does (RUN-15).
    ///
    /// Only an interactive session says so. A delegate ignores it, since what it starts has to end
    /// before the turn that spawned it can.
    pub fn keeping_jobs(mut self, jobs: tools::SessionJobs) -> Self {
        self.jobs = Some(jobs);
        self
    }

    /// Retain live file decisions after success, failure or cancellation.
    /// The caller seeds this from its current map and reads it after the turn has joined
    /// its children. This authority replaces the file map passed to the run.
    pub fn with_file_authority(
        mut self,
        authority: bravebot_core::file_authority::FileAuthority,
    ) -> Self {
        self.file_authority = Some(authority);
        self
    }

    /// Apply the rules a person wrote in advance about what to ask them about.
    pub fn with_permissions(mut self, permissions: Permissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// State what the settings say a commit message and a pull request may carry.
    pub fn with_attribution(mut self, attribution: bravebot_config::Attribution) -> Self {
        self.attribution = attribution;
        self
    }

    /// Open the system prompt with this style's words, unless `--system-prompt` named its own.
    pub fn with_style(mut self, style: Option<crate::styles::Style>) -> Self {
        self.style = style;
        self
    }

    /// State the words the command line put in the system prompt (CLI-19).
    pub fn with_system_prompts(mut self, system_prompts: SystemPrompts) -> Self {
        self.system_prompts = system_prompts;
        self
    }

    /// Say which tick of a loop this turn is.
    pub fn ticking(mut self, tick: Option<Tick>) -> Self {
        self.tick = tick;
        self
    }

    /// Say whether this turn may arm a standing watch, and why not where it may not.
    pub fn arming(mut self, arming: crate::watch::Arming) -> Self {
        self.arming = arming;
        self
    }

    /// Say whether this caller will send this turn's line again after a wait the turn asks for.
    pub fn looking_again(mut self, looking_again: bool) -> Self {
        self.looking_again = looking_again;
        self
    }

    /// Say what condition the session is working towards, where a person set one.
    pub fn working_towards(mut self, condition: Option<String>) -> Self {
        self.working_towards = condition;
        self
    }

    /// Say how much this turn asks before it acts.
    ///
    /// The caller must give the same mode to [`crate::Confining`], which is what enforces it. This
    /// only decides what the planner is told.
    ///
    /// A [`crate::LiveMode`] is read again whenever the turn decides something, so a mode the
    /// person changes while the turn runs is the one the rest of the turn follows (MODE-8).
    pub fn with_permission_mode(mut self, mode: impl Into<crate::LiveMode>) -> Self {
        self.permission_mode = mode.into();
        self
    }

    /// Say whether a check that finds nothing may promote a slot without anybody being asked.
    ///
    /// The caller has already resolved the three routes into one answer with
    /// `bravebot_core::vetting::auto`. Nothing here reads a file, a flag or a setting, for the
    /// reason nothing here reads `$HOME`.
    pub fn with_auto_vetting(mut self, auto: bool) -> Self {
        self.auto_vetting = auto;
        self
    }

    /// Name the MCP servers this session reached, each by its alias.
    ///
    /// The turn holds a grant to call each one and no other (`mcp-servers.md` SERVERS-9). A
    /// delegate's grant comes from its spec, so this says nothing to one.
    pub fn with_servers(mut self, servers: Vec<bravebot_core::capability::ServerAlias>) -> Self {
        self.servers = servers;
        self
    }

    /// Give the turn the servers this session reached, where it reached any, and a grant to call
    /// each of them.
    pub fn with_mcp(mut self, session: Option<crate::mcp::Session>) -> Self {
        self.servers = session
            .as_ref()
            .map(crate::mcp::Session::grants)
            .unwrap_or_default();
        self.mcp = session;
        self
    }
}

/// The capabilities a turn begins with.
///
/// FileWrite and ShellExec are granted, but granting the capability is not what permits the
/// effect: both gates additionally require a single-use endorsement that only a user's approval
/// creates. Without one, a write or a run is refused even though the capability is present.
///
/// A delegate holds what the kernel worked out before it existed, which is its kind's set
/// narrowed by whatever the run that spawned it held. Taken from the spec rather than recomputed
/// here, because a second computation of the same thing is a second answer waiting to disagree
/// with the one the trail recorded. The servers it holds are there too, where its kind holds
/// servers, and are those its parent held that its definition selects.
pub(super) fn held(task: &Task) -> CapabilitySet {
    match &task.delegate {
        Some(spec) => spec.capabilities().clone(),
        None => CapabilitySet::from_iter(
            [
                Capability::WebFetch,
                Capability::FileRead,
                Capability::FileWrite,
                Capability::ShellExec,
                Capability::LanguageServer,
            ]
            .into_iter()
            .chain(task.servers.iter().cloned().map(Capability::McpCall)),
        ),
    }
}
