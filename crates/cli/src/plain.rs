//! A session in lines, with the terminal left as it was found.
//!
//! The interface that draws takes the terminal for its length: raw mode, a screen of its own,
//! mouse reporting, bracketed paste (INPUT-33). What that buys is a transcript repainted in place,
//! and what it costs is everything that reads a terminal the ordinary way. A viewport rewritten in
//! place is not a document a screen reader can follow, and what leaves the top of it is in this
//! program's own scroller rather than in the scrollback a person's own tooling knows how to read.
//! Without a session that takes none of it, the alternative to that viewport is not using the
//! program.
//!
//! So this is the same session with nothing drawn. A prompt is a line read from stdin, the reply
//! is written to stdout when the turn ends, and progress and every question go to stderr, which
//! is the one-shot run's own division of the two streams (CLI-5) with a person in front of it.
//! Nothing is repainted, nothing is animated, and no mode is ever asked of the terminal.
//!
//! It is an option on this program rather than a second one because the machinery is the
//! machinery already here: the turn loop, the confirmer, the reporter and the trust question are
//! the ones the other two surfaces use, and the whole of what is new is where a question is
//! written and where its answer is read from. What it does not have is everything that was a
//! drawing: the scroller, the key list, the slash commands, `@` naming a file, a picture on the
//! clipboard. CLI-14 is what says so.

use crate::exit::{Ending, fail};
use bravebot_agent::confirm::{
    CallDecision, Confirmer, Decision, ExposureRequest, FetchRequest, ManifestRequest,
    McpCallRequest, MoveRequest, OutputRequest, RunDecision, RunRequest, ServerRequest,
    ToolListRequest, VetRequest, VouchRequest, WriteDecision, WriteRequest,
};
use bravebot_agent::diff::Change;
use bravebot_agent::turn::{self, Task};
use bravebot_agent::{PermissionMode, Workspace};
use bravebot_config::Config;
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::permissions::Permissions;
use bravebot_core::programs::{AskedAbout, TrustedPrograms};
use bravebot_core::trust::TrustStore;
use bravebot_core::vetting::Verdict;
use bravebot_i18n::t;
use bravebot_session::jobs::Held;
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

/// What the person types a prompt after.
///
/// Two characters, no colour and no glyph. A marker is the only thing a session in lines draws at
/// all, and it is drawn for a reader that may be speaking it rather than looking at it.
const MARKER: &str = "> ";

/// How many lines of what a question is about are shown before the rest is counted instead.
///
/// A write is approved from the change it would make and a read from the bytes it would release,
/// so the content is what the question is; and the whole of a generated file is not readable as
/// one. A person scrolled past a thousand lines is answering whatever was in front of them at the
/// end of it, which is not the question that was asked.
const MOST_CONTENT_LINES: usize = 40;

/// Lines of unchanged text kept either side of a change, so a hunk can be placed in its file.
const CONTEXT: usize = 3;

/// Start a session in lines.
///
/// `skip_permissions` is the same flag it is everywhere: the one way to stop being asked. It is
/// read here rather than deeper in for the reason [`crate::take_skip_permissions`] takes it out of
/// the arguments before anything dispatches.
///
/// `agent` is the definition `--agent` named, which every prompt of the session is addressed to
/// once the directory's trust is settled (CLI-17).
///
/// `prompts` is what `--system-prompt` and `--append-system-prompt` named, carried by every prompt
/// of the session (CLI-19).
pub fn session(
    skip_permissions: bool,
    agent: Option<String>,
    prompts: bravebot_agent::turn::SystemPrompts,
) -> ExitCode {
    run(skip_permissions, agent, prompts, None)
}

/// A session in lines for a process no terminal owns, reading its prompts and its answers from
/// the channel and writing everything to it (BG-1).
///
/// The mode is the one every session opens in, bypass is not offered, and an allow rule in a
/// settings file answers nothing: a rule is a decision about a session somebody is sitting in
/// front of (BG-8). A question about starting a server is held for whoever attaches, so nothing a
/// person did not see is started (BG-7).
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn hosted(hosting: crate::host::Hosting) -> ExitCode {
    run(
        false,
        None,
        bravebot_agent::turn::SystemPrompts::default(),
        Some(hosting),
    )
}

/// The prompt line, the answers, what is said beside the work, and where a background session's
/// turns are kept.
type Streams = (
    Prompting<Box<dyn BufRead + Send>, Box<dyn Write + Send>>,
    Box<dyn Write + Send>,
    Box<dyn Write + Send>,
    Option<crate::host::Broadcast>,
);

fn run(
    skip_permissions: bool,
    agent: Option<String>,
    prompts: bravebot_agent::turn::SystemPrompts,
    hosting: Option<crate::host::Hosting>,
) -> ExitCode {
    let hosted_as = hosting.as_ref().map(|hosting| hosting.id.clone());
    // Refused rather than read. The lines this reads are the person's own prompts, and a pipe has
    // nothing vouching for what it carries: CLI-3 quarantines piped bytes for exactly that reason,
    // so a session taking its prompts from one would be taking instruction from whatever fed it,
    // and answering its own approval questions out of the same bytes. `-p` is the invocation that
    // reads a pipe, and it reads it as the untrusted context it is.
    if hosting.is_none() && !std::io::stdin().is_terminal() {
        return fail(Ending::Argument, t!(cli_plain_needs_a_terminal));
    }

    let mut config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            return fail(
                Ending::Configuration,
                t!(cli_configuration_problem, problem = err),
            );
        }
    };

    // Before the session opens, which is where the other three ways of starting one ask it: a
    // transcript that began with nothing configured to answer reads as the agent rather than as
    // the configuration, and this is the one moment somebody is looking for what to do next
    // (BACKEND-39).
    //
    // Asked of the model this session will request, which is the recorded one or the configured
    // default: no model can be named on the command line here, since the flag composes with
    // everything except this one.
    //
    // Where nothing is configured at all, what Claude Code or opencode configured is offered first,
    // in lines, the way this session asks everything (IMPORT-1).
    if hosting.is_none()
        && let Some(ended) = crate::import::before_the_session(&mut config)
    {
        return ended;
    }

    let settings = bravebot_config::Settings::load();
    let mut workspace = match crate::current_workspace(&settings, &bravebot_config::Managed::load())
    {
        Ok(workspace) => workspace,
        Err(err) => return fail(Ending::Failed, t!(cli_workspace_problem, problem = err)),
    };

    // Somewhere of its own to write what is not part of the project, for the length of the
    // session. Held in a binding because dropping it is what removes it.
    let _scratch = crate::scratch_for_this_run(&mut workspace);

    // There is somebody here, so the rules a settings file wrote hold as they do in a session: an
    // allow rule says which prompts to stop raising, and this is a surface that raises them. The
    // unattended reading, which drops every allow rule, is `-p`'s and belongs to a run nobody is
    // watching (CLI-1).
    let home = bravebot_agent::home::directory();
    // The home directory rather than the state directory inside it, which is what a `~/` rule in
    // the file is anchored at (PERM-3).
    let profile = bravebot_agent::home::profile();
    let (permissions, rejected) = match hosting {
        Some(_) => bravebot_agent::permissions::for_an_unattended_run(
            &settings,
            profile.as_deref(),
            workspace.root(),
        ),
        None => bravebot_agent::permissions::from_settings(
            &settings,
            profile.as_deref(),
            workspace.root(),
        ),
    };

    let mode = match skip_permissions {
        true => PermissionMode::Bypass,
        false => PermissionMode::Ask,
    };

    let pick = bravebot_agent::backend::pick(
        &config,
        bravebot_session::store::model(bravebot_session::store::load_model(), &settings),
    );
    let set_aside = match &pick {
        bravebot_agent::backend::Pick::SetAside(recorded) => Some((recorded.clone(), None)),
        bravebot_agent::backend::Pick::Refused { recorded, reason } => {
            Some((recorded.clone(), Some(reason.clone())))
        }
        bravebot_agent::backend::Pick::Absent | bravebot_agent::backend::Pick::InForce(_) => None,
    };
    let model = pick.into_model();
    let (mut asking, mut reply, mut beside, beside_turns): Streams = match hosting {
        Some(hosting) => (
            Prompting::watched(
                Box::new(hosting.input),
                Box::new(hosting.output.clone()),
                hosting.watch,
            ),
            Box::new(hosting.output.clone()),
            Box::new(hosting.output.clone()),
            Some(hosting.output),
        ),
        None => (
            Prompting::new(
                Box::new(std::io::BufReader::new(std::io::stdin())),
                Box::new(std::io::stderr()),
            ),
            Box::new(std::io::stdout()),
            Box::new(std::io::stderr()),
            None,
        ),
    };
    let hosted = beside_turns.is_some();

    asking.say(&t!(
        cli_plain_opening,
        version = crate::VERSION,
        model = model
            .clone()
            .unwrap_or_else(|| config.default_model.clone())
    ));
    if let Some((recorded, refused)) = set_aside {
        asking.say(&match refused {
            Some(reason) => t!(
                session_model_pick_refused,
                model = recorded,
                reason = reason
            ),
            None => t!(session_model_pick_set_aside, model = recorded),
        });
    }
    for problem in &rejected {
        asking.say(&t!(
            session_permission_rule_ignored,
            problem = bravebot_agent::permissions::describe(problem)
        ));
    }
    // And the `allow` entries a checkout wrote, which are dropped wherever the file is read
    // (PERM-14) and which this session grants none of: it puts no question, so there is nowhere
    // an answer to one could have been collected (PERM-15). Said for the reason the rejects above
    // are: the prompt the entry was written to answer still appears, and somebody told nothing
    // reads that as a second fault rather than as the rule not being in force.
    for rule in bravebot_agent::permissions::proposed(&settings, profile.as_deref()) {
        asking.say(&t!(
            session_permission_allow_ignored,
            rule = &rule.rule,
            path = rule.path.display().to_string()
        ));
    }

    // The startup question (TRUST-7), put as a line. The map a yes writes is the one the panel's
    // yes writes, because both go through the same function; the end of the input is the third
    // answer the panel has, and it starts no session.
    //
    // Not put again to a session that is started again: the map in its record is the answer it was
    // given, as it is for a resumed foreground session.
    let resumed = hosted_as
        .as_deref()
        .and_then(|id| bravebot_session::sessions::load(workspace.root(), id));
    let Some(trust) = resumed
        .as_ref()
        .and_then(|record| record.trust_map(workspace.root()))
        .or_else(|| {
            opening_trust(&mut asking, mode, workspace.root(), |root| {
                bravebot_agent::trusted::record_for(home.as_deref(), profile.as_deref(), root)
            })
        })
    else {
        return ExitCode::SUCCESS;
    };
    if let Some(record) = resumed.as_ref().filter(|record| record.turns > 0) {
        asking.say(&t!(
            cli_plain_resumed,
            count = t!(count_turns, count = record.turns)
        ));
    }

    // Matched after that question, because its answer decides whether the checkout's definitions
    // are in the set, and before any server is reached, so a name matching nothing starts nothing
    // (ADDRESS-5).
    let agent = match agent
        .map(|name| {
            let definitions = bravebot_agent::agents::resolved(
                &workspace,
                home.as_deref(),
                trust.clone(),
                permissions.clone(),
                &mut RecordingSink::new(),
            );
            bravebot_tui::app::definition_named(&config, &definitions, &name)
        })
        .transpose()
    {
        Ok(agent) => agent,
        Err(refused) => return fail(Ending::Argument, refused.into_message()),
    };
    // With the definition's model, because the opening line named the session's before the name
    // was matched, and every prompt asks for the definition's.
    if let Some(agent) = &agent {
        let notice = match &agent.model {
            Some(model) => t!(
                cli_plain_working_under_model,
                definition = &agent.name,
                model = model
            ),
            None => t!(cli_plain_working_under, definition = &agent.name),
        };
        asking.say(&t!(cli_notice, notice = notice));
    }

    // After that question, and put on the same two streams every other question here is. Held for
    // the length of the session, since dropping one stops its server.
    let mut terminal;
    let mut attached;
    let asker: &mut dyn bravebot_agent::servers::Asker = if hosted {
        attached = Attached(&mut asking);
        &mut attached
    } else {
        terminal = crate::mcp::Person {
            answers: &mut asking.input,
            screen: &mut asking.output,
            present: true,
        };
        &mut terminal
    };
    let mut reached = bravebot_agent::servers::for_this_session(
        &settings,
        workspace.root(),
        match skip_permissions {
            true => bravebot_agent::servers::Asking::Bypass,
            false => bravebot_agent::servers::Asking::Person,
        },
        asker,
        // Where this session's own lines go, since nothing draws over them.
        bravebot_sandbox::Stream::Inherited,
    );
    for note in std::mem::take(&mut reached.notes) {
        asking.say(&note);
    }

    // What compaction measures the conversation against, and whether the model in force reads an
    // effort level. A session in lines opens no picker, so the model in force here is the stored
    // one or the configured one, and this is the only place either can be looked up.
    //
    let under = agent.as_ref().map(|definition| definition.name.clone());
    // The definition's model where it names one, since that is the one every turn will ask for.
    let named = agent
        .as_ref()
        .and_then(|agent| agent.model.clone())
        .or_else(|| model.clone())
        .unwrap_or_else(|| config.default_model.clone());
    let reads_effort = bravebot_tui::app::adopt_listing_for_model(&mut config, &named);

    // The level the turns below ask for: the recorded pick and the settings layers, ranked as
    // BACKEND-43 ranks them. Resolved once, here, beside the other answers this session reads out
    // of a file, because a file edited mid-session describes the next one and this mode has no
    // command that changes the level.
    let effort = bravebot_session::store::effort(bravebot_session::store::load_effort(), &settings);

    // Said where a level was chosen and the model in force reads none, because a level charged for
    // and discarded at the far end answers exactly like one that was honoured, so silence would
    // leave somebody believing every turn of the session thought harder than it did. Said once,
    // here, since neither the model nor the roster's answer about it can change while this runs.
    //
    // Nothing is said where no level was chosen: nothing was withheld from somebody who asked for
    // none. What is recorded stays recorded either way, so the choice applies again the moment a
    // model that reads one is in force (BACKEND-22).
    if !reads_effort && effort.is_some() {
        asking.say(&t!(cli_notice, notice = t!(session_effort_not_read)));
    }

    // Before the first prompt, so a browser opening and a code to type are not interleaved with a
    // turn. Not fatal: the turn goes ahead and fails with the backend's own account, which says
    // more than this could guess.
    let signed_in = bravebot_agent::backend::Backend::sign_in_if_needed(&config, &named, |line| {
        match &beside_turns {
            Some(hosted) => hosted.clone().line(&line),
            None => eprintln!("{line}"),
        }
    });
    if let Err(failure) = signed_in {
        asking.say(&t!(cli_notice, notice = failure.to_string()));
    }
    if skip_permissions {
        asking.say(&t!(cli_notice, notice = t!(session_permissions_skipped)));
    }

    let mut running = Running {
        config: &config,
        egress: bravebot_net::Egress::new(),
        workspace: &workspace,
        permissions,
        mode,
        attribution: settings.attribution().clone(),
        prompts,
        output_cap: settings.run_output_cap(),
        deadlines: bravebot_agent::exec::Deadlines::resolve(settings.run_deadlines()),
        model,
        in_force: named,
        agent,
        reads_effort,
        effort,
        complained: None,
        home,
        profile,
        conversation: match &resumed {
            Some(record) => {
                bravebot_agent::conversation::Conversation::restored(record.conversation.clone())
            }
            None => bravebot_agent::conversation::Conversation::new(),
        },
        trust,
        programs: match &resumed {
            Some(record) => record.trusted_programs(workspace.root()),
            None => TrustedPrograms::new(),
        },
        kept: hosted_as
            .as_deref()
            .map(|id| Kept::opening(workspace.root(), id, resumed.as_ref(), under.as_deref())),
        servers: None,
        mcp: reached.session(),
        beside: beside_turns,
        asked_about: AskedAbout::new(),
        exposed: bravebot_core::credentials::Exposed::new(),
        auto_vetting: bravebot_core::vetting::auto(
            bravebot_core::vetting::asked_for(),
            bravebot_session::store::load_vetting(),
            settings.auto_vetting(),
        ),
    };

    // The streams themselves rather than a lock on each. A lock held for the length of the session
    // is a lock a delegate's thread waits on for ever: a turn lends its reporter to the delegates
    // it spawns, they write progress from threads of their own, and the first of those writes would
    // block on a lock this thread does not give back until the session ends. Each write takes the
    // lock for its own line, which is what the one-shot run does with the same two streams.
    lines(&mut asking, &mut reply, &mut running, &mut beside);
    ExitCode::SUCCESS
}

/// Read a prompt, run it, write what it left, until the input ends.
///
/// The turn is behind [`Turns`] rather than called here, so what this decides is a thing a test can
/// read back without a backend: that the end of the input ends the session, that an empty line is
/// not a turn, and that the reply goes on one stream with everything else on the other.
fn lines<R: BufRead + Send, W: Write + Send, T: Turns<Prompting<R, W>>>(
    asking: &mut Prompting<R, W>,
    reply: &mut impl Write,
    turns: &mut T,
    beside: &mut impl Write,
) {
    while let Some(prompt) = asking.prompt() {
        // A blank line is somebody pressing Enter at the marker, which is not a prompt. Sending it
        // would spend a turn asking the model to answer nothing.
        if prompt.trim().is_empty() {
            continue;
        }

        let said = turns.take(&prompt, asking);

        // A turn that could not run is said beside rather than on the reply stream, which carries
        // the reply and nothing else (CLI-5). The session goes on: a backend that was unreachable
        // a moment ago is worth another prompt, and the conversation is still here.
        if let Some(failure) = &said.failure {
            let _ = writeln!(beside, "{failure}");
            crate::say_notices(beside, &said.notices);
            continue;
        }

        // The same division a one-shot run's ending goes through, and the same function, so the
        // two surfaces cannot come to disagree about which stream a notice belongs on.
        crate::report(
            reply,
            beside,
            &crate::Finished {
                reply: &said.reply,
                notices: &said.notices,
                attempt: None,
                trail: None,
                clean: said.clean,
                ending: crate::ending_of_a_turn(said.clean, false, said.not_served.is_some()),
                not_served: said.not_served.as_deref(),
            },
        );
    }
}

/// What a turn left for the person, once it has ended.
///
/// The turn's own [`turn::Outcome`] holds a great deal more, and every field of it belongs to
/// somebody: the tokens to a status bar, the watches to a session that keeps them. What is here is
/// what a session in lines has anywhere to put.
#[derive(Debug, Default)]
struct Said {
    /// The reply, released for display. Empty where the turn could not run.
    reply: String,
    /// A turn that could not run at all, in the words the failure gave.
    failure: Option<String>,
    /// The driver's own words about what loaded and what did not, never anything read out of a
    /// file.
    notices: Vec<String>,
    /// Whether no gate refused anything during the turn.
    clean: bool,
    /// What to say where one model was asked for and another one answered (CLI-10).
    not_served: Option<String>,
}

/// How one prompt becomes a turn.
///
/// A trait because the loop above does not need to know: the implementation below runs a turn
/// against the configured backend, and a test answers with a canned [`Said`] and no backend at
/// all. Generic over the confirmer rather than taking one of its own, because the thing that reads
/// what the person types is the thing that asks them questions, and there is exactly one of it.
trait Turns<C: Confirmer + Send> {
    fn take(&mut self, prompt: &str, asking: &mut C) -> Said;
}

/// A session's turns, against the configured backend.
///
/// Everything here outlives a turn and is lent to it, which is what makes this a session rather
/// than a sequence of one-shot runs: the conversation is what "try that again" refers to, the
/// trust map carries what a write recorded, the vouched programs carry what the person approved
/// once, and the language servers are started on the first question that needs one and kept
/// (LSP-8).
struct Running<'a> {
    config: &'a Config,
    egress: bravebot_net::Egress,
    workspace: &'a Workspace,
    permissions: Permissions,
    mode: PermissionMode,
    /// The model the person chose, or `None` to leave the configured one in force.
    model: Option<String>,
    /// The name of the model in force, whichever of the two it came from, which is what a
    /// substitution is measured against (CLI-10).
    in_force: String,
    /// The definition every prompt is addressed to, where `--agent` named one.
    agent: Option<bravebot_tui::state::Addressed>,
    /// The words the command line put in the system prompt of every prompt (CLI-19).
    prompts: bravebot_agent::turn::SystemPrompts,
    /// Whether the model in force still reads an effort level.
    ///
    /// The listing answers it where the session is assembled, because that is where the listing is
    /// fetched and this mode has no command that puts another model in force. A service that
    /// refuses the field answers the same question later, so this is taken down again after every
    /// turn: a session that asked once went on sending the level and reporting it as in force for
    /// the rest of its life after the requests carrying one had stopped. A turn carries the
    /// recorded level only where this is true (BACKEND-22).
    reads_effort: bool,
    /// How hard the turns of this session ask the model to think, where anything asked.
    ///
    /// The pick `/effort` recorded, and otherwise the level a settings file named (BACKEND-43).
    /// Resolved once where the session is assembled, for the reason the attribution is: a file
    /// edited mid-session describes the next one, and this mode has no command that changes it.
    effort: Option<bravebot_session::store::Effort>,
    /// The last substitution said, so the same complaint is not repeated every turn.
    ///
    /// A session asks the same model over and over, so a substitution said once per turn is one
    /// sentence of noise between every prompt and its reply. Said again where the answer changes,
    /// which is a different fact about a different turn.
    complained: Option<String>,
    /// The person's own directory, holding standing instructions and skills.
    home: Option<std::path::PathBuf>,
    /// The directory that one sits inside, which is what a leading `~` stands for (CMDLINE-4).
    profile: Option<std::path::PathBuf>,
    conversation: bravebot_agent::conversation::Conversation,
    trust: TrustStore,
    programs: TrustedPrograms,
    servers: Option<bravebot_agent::lsp::LanguageServers>,
    asked_about: AskedAbout,
    /// The files this session has agreed the planner may be given despite what the scan found.
    exposed: bravebot_core::credentials::Exposed,
    /// What the settings say a commit message and a pull request this session writes may carry.
    ///
    /// Read once, where the session is assembled, for the reason the permission rules are: a file
    /// edited mid-session describes the next one.
    attribution: bravebot_config::Attribution,
    /// What the settings say a command's output may spend of the conversation, where they said
    /// anything.
    ///
    /// Read once beside the attribution and for the same reason: a file edited mid-session
    /// describes the next one.
    output_cap: Option<usize>,
    /// How long a command may run, and the most one call may ask for, as the settings leave them.
    ///
    /// Resolved once beside the cap and for the same reason: a file edited mid-session describes
    /// the next one.
    deadlines: bravebot_agent::exec::Deadlines,
    /// Whether a check that finds nothing may promote a slot without the person being asked.
    ///
    /// Resolved once, where the session is assembled, out of the three routes
    /// [`bravebot_core::vetting::auto`] takes. There is somebody here, so the flag and the two
    /// standing answers mean what they mean in a session that draws; what this mode does not have
    /// is the key that turns the mode on, since it offers one answer per question (CLI-14).
    auto_vetting: bool,
    /// The MCP servers this session started, where it started any, with their lists (SERVERS-9).
    mcp: Option<bravebot_agent::mcp::Session>,
    /// Where a turn's progress goes where this session has no terminal of its own.
    beside: Option<crate::host::Broadcast>,
    /// The record a background session writes after each turn, which is what starting it again
    /// resumes from (BG-1).
    kept: Option<Kept>,
}

/// The record of a background session, written down after each turn that ended.
///
/// Named by the session's id in the roster, so the entry and the record are found from one
/// another. Only what a session in lines produces is written: no history of its own, no asides, no
/// rewind points and no checkouts.
struct Kept {
    handle: bravebot_session::sessions::Handle,
    turns: usize,
    tokens: u64,
    spend: std::collections::BTreeMap<usize, u64>,
}

impl Kept {
    fn opening(
        root: &std::path::Path,
        id: &str,
        resumed: Option<&bravebot_session::sessions::Record>,
        under: Option<&str>,
    ) -> Self {
        use bravebot_session::sessions::{Front, Handle};
        let mut kept = match resumed {
            Some(record) => Self {
                handle: Handle::resuming(root, record, Front::Terminal, bravebot_stamp::BUILD),
                turns: record.turns,
                tokens: record.tokens,
                spend: record.spend.clone(),
            },
            None => Self {
                handle: Handle::begin_as(
                    id.to_string(),
                    root,
                    Front::Terminal,
                    bravebot_stamp::BUILD,
                ),
                turns: 0,
                tokens: 0,
                spend: std::collections::BTreeMap::new(),
            },
        };
        // What this session's turns are addressed to, not what the record it resumed was under:
        // a session that does not work under that definition must not claim it.
        kept.handle.set_agent(under.map(str::to_string));
        kept
    }

    fn after_a_turn(
        &mut self,
        prompt: &str,
        outcome: &turn::Outcome,
        conversation: &bravebot_agent::conversation::Conversation,
        trust: &TrustStore,
        programs: &TrustedPrograms,
    ) {
        self.turns += 1;
        self.tokens += outcome.tokens;
        *self.spend.entry(self.turns).or_insert(0) += outcome.tokens;
        self.handle.save(
            prompt,
            bravebot_session::sessions::Standing {
                conversation: &conversation.snapshot(),
                history: None,
                turns: self.turns,
                tokens: self.tokens,
                spend: &self.spend,
                timing: &std::collections::BTreeMap::new(),
                model: Some(&outcome.model),
                todos: &std::collections::BTreeMap::new(),
                asides: &[],
                trust,
                programs,
                directories: &[],
                manifest: None,
                rewind: &[],
                checkouts: &[],
            },
        );
    }
}

impl<C: Confirmer + Send> Turns<C> for Running<'_> {
    fn take(&mut self, prompt: &str, asking: &mut C) -> Said {
        let task = Task::new(prompt.to_string())
            .with_mcp(self.mcp.clone())
            .with_home(self.home.clone())
            .with_profile(self.profile.clone())
            .with_cache(bravebot_agent::home::cache())
            // No bound on the rounds, as a session passes: there is a person watching, and they
            // are a better bound than any number. The terminal's own interrupt is how they use
            // it, this session having taken none of the keyboard.
            .with_rounds(None)
            .with_model(self.model.clone())
            // Only where the roster describing the model in force says it is read. The choice
            // itself is left on disk, so it applies again under a model that reads one
            // (BACKEND-22).
            .with_effort(self.effort.filter(|_| self.reads_effort))
            .with_permissions(self.permissions.clone())
            .with_permission_mode(self.mode)
            .with_attribution(self.attribution.clone())
            .with_system_prompts(self.prompts.clone())
            .with_output_cap(self.output_cap)
            .with_deadlines(self.deadlines)
            .with_confined_runs(true)
            .with_auto_vetting(self.auto_vetting)
            .already_asked_about(self.asked_about.clone())
            .already_exposed(self.exposed.clone())
            .addressing(self.agent.as_ref().map(|agent| agent.name.clone()));

        // The mode as the session holds it. The confirmer below is what enforces it, and the task
        // above is the half the planner is told about; both are set from the one value, and
        // screening is threaded the same way for the reason the mode is.
        let mut confirmer = bravebot_agent::Confining::new(asking, self.mode, task.auto_vetting);
        let mut sink = RecordingSink::new();
        // One per turn. It holds every call the turn made, for a result object a session has no
        // way of asking for, so one kept for the session would be a list nothing reads growing for
        // as long as the session lasts.
        let mut reporter = crate::progress::Progress::new(match &self.beside {
            Some(beside) => Box::new(beside.clone()) as Box<dyn Write + Send>,
            None => Box::new(std::io::stderr()),
        });
        let mut servers = self.servers.take().unwrap_or_else(|| {
            bravebot_agent::lsp::LanguageServers::new(
                self.workspace.root().to_path_buf(),
                self.home.clone(),
            )
        });

        let completed = turn::resume(
            self.config,
            &self.egress,
            self.workspace,
            &task,
            &mut self.conversation,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            self.trust.clone(),
            self.programs.clone(),
            Some(&mut servers),
            // Nothing here cancels a turn: the session holds none of the keyboard, so Ctrl-C is
            // the terminal's own interrupt and ends the process rather than reaching this.
            &Cancel::new(),
        );
        // Kept whether the turn succeeded or not. A server started during a turn that then failed
        // is still running, and a set dropped here would leave the next prompt paying for a second
        // index of the same tree.
        self.servers = Some(servers);

        // Taken apart with no `..`, so an answer a turn learns to remember does not build until
        // this session says what becomes of it.
        let turn::Decisions {
            trust,
            programs,
            asked_about,
            exposed,
        } = completed.decisions;
        self.trust = trust;
        self.programs = programs;
        self.asked_about = asked_about;
        self.exposed = exposed;
        if let (Ok(outcome), Some(kept)) = (&completed.outcome, &mut self.kept) {
            kept.after_a_turn(
                prompt,
                outcome,
                &self.conversation,
                &self.trust,
                &self.programs,
            );
        }
        let mut said = match completed.outcome {
            Ok(outcome) => Said {
                reply: outcome.reply_for_display().to_string(),
                failure: None,
                notices: outcome.notices.clone(),
                clean: outcome.clean,
                // Skipped for a definition's model, because the turn compared that model itself
                // and reported it in the definition's words (ADDRESS-11). The same for a model a
                // skill moved the turn onto (SKILL-15).
                not_served: match self.agent.as_ref().and_then(|agent| agent.model.as_ref()) {
                    Some(_) => None,
                    None => outcome
                        .ran_on_the_sessions_model()
                        .then(|| self.substituted(&outcome.model))
                        .flatten(),
                },
            },
            // From the reporter rather than the outcome, there being no outcome: a turn that could
            // not run still said what its hooks did, and those sentences are the person's own to
            // hear (HOOK-7).
            Err(failure) => Said {
                failure: Some(crate::exit::ending_of(&failure).told(&failure)),
                notices: reporter.notices().to_vec(),
                ..Said::default()
            },
        };
        // After either ending, because either can have learned it: the level goes out on the first
        // request a turn makes, and a turn that then failed refused nothing about the field.
        said.notices.extend(self.a_level_refused_this_turn());
        said
    }
}

impl Running<'_> {
    /// Take down a level the service refused during the turn, and what to say about it.
    ///
    /// The roster answered this question before the first prompt, and a service that refuses the
    /// field answers it again mid-turn: the listing is what a gateway says it would do, and where
    /// there is no listing at all a refusal is the only answer there has ever been. Asked after
    /// every turn because that is the only moment the answer can have changed, and the session goes
    /// on to send another request and to report what it carries.
    ///
    /// Said once, on the turn that learned it, for the reason the same sentence is said once at
    /// startup: the condition holds for the rest of the session and repeating it between every
    /// prompt and its reply would bury the work. Nothing is said where no level was chosen, nothing
    /// having been withheld from somebody who asked for none, and the choice is kept either way so
    /// it applies again under a model that reads one (BACKEND-22).
    fn a_level_refused_this_turn(&mut self) -> Option<String> {
        if !self.reads_effort
            || !bravebot_agent::backend::refused_a_level(self.config, &self.in_force)
        {
            return None;
        }
        self.reads_effort = false;
        self.effort.map(|_| t!(session_effort_not_read).to_string())
    }

    /// What to say where the endpoint answered with a model other than the one in force, and
    /// nothing where it answered as asked or where the two cannot be compared.
    ///
    /// A model that cannot be served is substituted rather than refused, so the name the server
    /// reports is the only trace of it, and the questions of which name the service was asked and
    /// whether it reports one at all are the backend's. There is no flag here for a run to fail
    /// over: a session names its model in a picker or a settings file, and the person is reading
    /// the answer.
    fn substituted(&mut self, answered: &str) -> Option<String> {
        let asked = bravebot_agent::backend::Backend::name_as_asked(self.config, &self.in_force);
        let comparable = bravebot_agent::backend::Backend::reports_the_model_it_was_asked_for(
            self.config,
            &self.in_force,
        );
        let complaint = crate::model_not_served(&asked, comparable, answered);
        match complaint == self.complained {
            true => None,
            false => {
                self.complained = complaint.clone();
                complaint
            }
        }
    }
}

/// The startup question (TRUST-7) as a line, or nothing where the person left at it.
///
/// The three answers the panel has, in the one shape a line has: the affirmative trusts the
/// working directory, any other line declines and trusts nothing, and the end of the input is
/// somebody leaving, which starts no session. The map itself is built by the interface's own
/// function, so what a yes grants here is what a yes grants there.
///
/// An answer another front end was told to remember about this directory answers here too
/// (TRUST-23), after the mode and before the question. `remembered` finds the record about the
/// directory it is handed, which is `root`, and is called only where the mode has not answered.
/// Nothing here writes it: a line has one answer per question, and none of them outlives the
/// session.
fn opening_trust<R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    mode: PermissionMode,
    root: &std::path::Path,
    remembered: impl FnOnce(
        &std::path::Path,
    ) -> Option<(
        bravebot_agent::trusted::Store,
        bravebot_agent::trusted::Identity,
    )>,
) -> Option<TrustStore> {
    if let Some(answered) = bravebot_tui::trust_prompt::answered_by(mode, root) {
        return Some(answered);
    }

    if let Some((store, kept)) = remembered(root)
        .and_then(|(store, identity)| store.kept(&identity).map(|kept| (store, kept)))
    {
        asking.say(&t!(
            cli_plain_trusting_kept,
            directory = root.display().to_string(),
            when = bravebot_session::sessions::how_long_ago(kept.at),
            path = store.path().display().to_string()
        ));
        return Some(bravebot_tui::trust_prompt::trusting_the_workspace(root));
    }

    // What is being asked about first and the question last, as every question here is put: a
    // person reading a line at a time, or hearing one, has read the detail by the time they are
    // asked to answer it.
    let lines = [
        root.display().to_string(),
        t!(trust_directory_explained).to_string(),
        t!(trust_directory_regardless).to_string(),
    ];
    let answer = match asking.put(&lines, t!(trust_directory_title)) {
        Some(Decision::Approve) => bravebot_tui::trust_prompt::Answer::Trust,
        Some(Decision::Reject) => bravebot_tui::trust_prompt::Answer::Decline,
        None => bravebot_tui::trust_prompt::Answer::Leave,
    };
    let trust = bravebot_tui::trust_prompt::trust_for(answer, root)?;

    asking.say(&match trust.is_trusted(".") {
        true => t!(session_trusting, directory = root.display().to_string()),
        false => t!(session_not_trusting).to_string(),
    });
    Some(trust)
}

/// The one thing that reads what the person types, and the one thing that writes what is not the
/// reply.
///
/// One of it rather than two, because two readers of the same input each hold a buffer: a line
/// read ahead into the buffer of whichever was not asked is a prompt that went missing, or an
/// approval answered by the line after it. So the prompt and every answer come through here.
///
/// Generic over both streams so a test can hand it a script and read back every byte it wrote.
/// That is what pins the property this whole module exists for: what is written here is the whole
/// of what a session in lines puts on a terminal, so a test can say that none of it is an escape
/// sequence.
pub struct Prompting<R: BufRead, W: Write> {
    input: R,
    output: W,
    /// What the person answered, by the question's key, for the rest of the session (ASK-8).
    answers: Vec<(String, Answer)>,
    /// Told when a line is about to be waited for and when it has come, by a session whose process
    /// a terminal does not own.
    watch: Option<Box<dyn Watcher>>,
    /// The kind of question about to be put, for the watcher. Taken by the read that follows.
    kind: Option<Held>,
}

/// Puts the questions about starting a tool server to whoever attaches, and holds them until one
/// does (BG-7).
struct Attached<'a, R: BufRead, W: Write>(&'a mut Prompting<R, W>);

impl<R: BufRead, W: Write> bravebot_agent::servers::Asker for Attached<'_, R, W> {
    fn anybody_there(&self) -> bool {
        true
    }

    fn ask_to_start(
        &mut self,
        question: &bravebot_agent::servers::Question<'_>,
    ) -> bravebot_agent::servers::Answer {
        use bravebot_agent::servers::{Answer, Question};
        self.0.say("");
        for line in question.lines() {
            self.0.say(&line);
        }
        self.0.say("");
        for line in question.choices() {
            self.0.say(&line);
        }
        self.0.about(Held::Server);
        match self.0.answer(&Question::answer_prompt()) {
            Some(typed) => Answer::typed(&typed),
            None => Answer::No,
        }
    }

    fn ask_to_move(&mut self, request: &MoveRequest) -> bool {
        matches!(self.0.confirm_move(request), Decision::Approve)
    }
}

/// What a line is being waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waiting {
    /// The next prompt, with nothing asked.
    Prompt,
    /// The answer to a question, of the kind it is.
    Answer(Held),
}

/// Told what the session is waiting for, so that a process nobody is at can say so and take a
/// line only when one is being waited for (BG-7, BG-10).
pub(crate) trait Watcher: Send {
    /// Called before the read, which blocks.
    fn waiting(&mut self, on: Waiting);
    /// Called after it, with the line where one came.
    fn received(&mut self, on: Waiting, line: Option<&str>);
}

impl<R: BufRead, W: Write> Prompting<R, W> {
    pub(crate) fn new(input: R, output: W) -> Self {
        Self {
            input,
            output,
            answers: Vec::new(),
            watch: None,
            kind: None,
        }
    }

    /// The same, with a watcher told what each read is for.
    pub(crate) fn watched(input: R, output: W, watch: Box<dyn Watcher>) -> Self {
        Self {
            watch: Some(watch),
            ..Self::new(input, output)
        }
    }

    /// Say what kind of question the next read answers. A read that was not told is a question of
    /// the plainest kind, so that no question is ever taken for a prompt.
    fn about(&mut self, held: Held) {
        self.kind = Some(held);
    }

    /// What the person answered the last time this exact question was put to them, if it was.
    fn recall_answer(&self, key: &str) -> Option<Answer> {
        self.answers
            .iter()
            .find(|(asked, _)| asked == key)
            .map(|(_, answer)| answer.clone())
    }

    /// Remember an answer, replacing any earlier one for the same question.
    fn remember_answer(&mut self, key: String, answer: Answer) {
        match self.answers.iter_mut().find(|(asked, _)| *asked == key) {
            Some(slot) => slot.1 = answer,
            None => self.answers.push((key, answer)),
        }
    }

    /// Say something beside the work. A failed write is dropped: stderr closed means nobody is
    /// reading, not that the session should end holding what it was going to say.
    pub(crate) fn say(&mut self, line: &str) {
        let _ = writeln!(self.output, "{line}");
        let _ = self.output.flush();
    }

    /// The next prompt, or `None` where the input has ended.
    ///
    /// The end of the input is how a session in lines is left, this one having no key to press:
    /// the terminal is not in raw mode, so Ctrl-D is the terminal's own end of file and arrives
    /// here as one.
    fn prompt(&mut self) -> Option<String> {
        let _ = write!(self.output, "{MARKER}");
        let _ = self.output.flush();
        self.line(Waiting::Prompt)
    }

    /// What a read that is not the prompt is waiting for.
    fn answering(&mut self) -> Waiting {
        Waiting::Answer(self.kind.take().unwrap_or(Held::Question))
    }

    /// One line, with its newline taken off, or `None` at the end of the input.
    ///
    /// A newline is written either way, because everything that asks for a line wrote a marker or
    /// a question and left the cursor after it. On a terminal the person's own Enter is echoed
    /// there and this is the blank line between what they typed and the answer; where the stream is
    /// a file there is no echo, and without this the progress that follows would be appended to a
    /// question nobody can see the end of. The end of the input needs it most: there is no echo for
    /// Ctrl-D at all, so without this the shell's own prompt comes back on the marker's line.
    fn line(&mut self, on: Waiting) -> Option<String> {
        if let Some(watch) = &mut self.watch {
            watch.waiting(on);
        }
        let mut line = String::new();
        let read = self.input.read_line(&mut line);
        let _ = writeln!(self.output);
        let _ = self.output.flush();
        let line = match read {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end_matches(['\n', '\r']).to_string()),
        };
        if let Some(watch) = &mut self.watch {
            watch.received(on, line.as_deref());
        }
        line
    }

    /// Write what is being asked about, then the question, and read the answer back.
    ///
    /// `None` is the end of the input arriving in place of an answer, which every caller but the
    /// startup question reads as a refusal: a question nobody answered is not a question somebody
    /// said yes to.
    fn put(&mut self, lines: &[String], question: &str) -> Option<Decision> {
        for line in lines {
            self.say(line);
        }
        let _ = write!(self.output, "{question} {} ", t!(line_answer));
        let _ = self.output.flush();

        // Only the affirmative approves. Any other line is a person who typed something that was
        // not yes, and the end of the input is nobody answering at all.
        let on = self.answering();
        let typed = self.line(on)?;
        Some(match typed.trim().to_lowercase() == t!(line_answer_yes) {
            true => Decision::Approve,
            false => Decision::Reject,
        })
    }

    /// Ask, and read the end of the input as a refusal.
    pub(crate) fn ask(&mut self, lines: &[String], question: &str) -> Decision {
        self.put(lines, question).unwrap_or(Decision::Reject)
    }

    /// Ask a question answered with a word rather than a yes, and read the word back trimmed, or
    /// `None` at the end of the input.
    pub(crate) fn answer(&mut self, question: &str) -> Option<String> {
        let _ = write!(self.output, "{question} ");
        let _ = self.output.flush();
        let on = self.answering();
        self.line(on).map(|typed| typed.trim().to_string())
    }
}

/// One line of quarantined text, made safe to put on a terminal.
///
/// Every question below shows something nobody vouched for: the body of a write, the output of a
/// command, the first lines of a file. An escape sequence in any of it would move the cursor or
/// recolour the screen of a session whose whole claim is that it draws nothing, and could forge the
/// lines around itself. So control characters are pictured rather than sent, which is what
/// [`crate::progress`] does with the same content for the same reason.
fn shown(text: &str) -> String {
    crate::progress::printable(text)
}

/// What a check made of the same bytes, in one line, or nothing where it said nothing.
///
/// Advice beside the content and never in place of it: it decides nothing here, exactly as it
/// decides nothing in the panel. The check's own sentence is not carried: it is free text written
/// about content an attacker may own, and a line-oriented question has no margin to put it behind.
fn checked(verdict: Verdict) -> String {
    match verdict {
        Verdict::Safe => t!(check_safe),
        Verdict::Unsafe => t!(check_unsafe),
        Verdict::Inconclusive(_) => t!(check_inconclusive),
    }
    .to_string()
}

/// Quarantined content as rows, each behind a margin, capped.
///
/// The margin is on every row, for the reason [`crate::progress`] puts it on every row: a caption
/// above the block could be imitated by the block's own first line, and a margin cannot be. The cap
/// is because a question has to be readable as one: a person scrolled past a thousand lines of
/// output is answering whatever is in front of them at the end of it.
fn quarantined(content: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut counted = content.lines();
    for line in counted.by_ref().take(MOST_CONTENT_LINES) {
        rows.push(format!(
            "{} {}",
            crate::progress::QUARANTINE_BAR,
            shown(line)
        ));
    }
    let left_out = counted.count();
    if left_out > 0 {
        rows.push(t!(transcript_more_lines, count = left_out).to_string());
    }
    rows
}

/// The lines a proposed write is read before approving: what it would do, then the change itself.
fn change(request: &WriteRequest) -> Vec<String> {
    let mut lines = vec![shown(&request.summary())];
    if request.untrusted {
        lines.push(t!(write_untrusted).to_string());
    }
    // What the isolated processor that produced the body said about it, beside the diff rather than
    // somewhere up the scrollback: a remark saying a typo was fixed is only a claim worth anything
    // while the lines it describes are in front of the person reading it. It decides nothing, and
    // it is free text a processor authored, so it goes behind the margin with the content.
    if let Some(remark) = &request.remark {
        lines.push(t!(write_remark).to_string());
        lines.extend(quarantined(&remark.preview.join("\n")));
    }
    // What the scan inferred, beside the lines it read it from. These are the driver's own words
    // about its own findings, each already a kind, a location and a masked preview, so no part of
    // the value is repeated here and none of it needs the margin content sits behind.
    if !request.credentials.is_empty() {
        lines.push(t!(write_credentials).to_string());
        lines.extend(request.credentials.iter().map(|found| shown(found)));
    }

    if request.written_since_checkout {
        lines.push(t!(write_since_checkout).to_string());
    }

    let diff = &request.diff;
    // A change too large to diff says so rather than showing a guess at it, which is what the
    // panel does with the same diff. The summary above still counts the lines.
    if !diff.is_exact() {
        lines.push(t!(
            write_too_large_to_show,
            added = diff.added(),
            removed = diff.removed()
        ));
        return lines;
    }

    let mut changed = 0usize;
    let mut left_out = 0usize;
    for held in diff.condensed(CONTEXT) {
        if changed == MOST_CONTENT_LINES {
            left_out += 1;
            continue;
        }
        changed += 1;
        lines.push(match held {
            Change::Added(line) => format!("+ {}", shown(&line)),
            Change::Removed(line) => format!("- {}", shown(&line)),
            Change::Kept(line) => format!("  {}", shown(&line)),
            Change::Elided(count) => t!(write_unchanged, count = count).to_string(),
        });
    }
    if left_out > 0 {
        lines.push(t!(transcript_more_lines, count = left_out).to_string());
    }
    lines
}

/// What one ambient authority is, in the words a person reads.
///
/// A sentence per authority rather than one with a name substituted in, because what each of them
/// costs is different: a container daemon is root on this machine, a logged-in tool is an account
/// elsewhere, the agent is a signature, the metadata service is a role. The word that named it
/// comes from the table that recognised it, so no part of the command line reaches this sentence.
fn authority(spent: &bravebot_core::ambient::Spent) -> String {
    let named = spent.named;
    match spent.authority {
        bravebot_core::ambient::Authority::ContainerDaemon => {
            t!(run_authority_container, named = named)
        }
        bravebot_core::ambient::Authority::LoggedInTool => {
            t!(run_authority_logged_in, named = named)
        }
        bravebot_core::ambient::Authority::AgentSocket => t!(run_authority_agent, named = named),
        bravebot_core::ambient::Authority::MetadataService => {
            t!(run_authority_metadata, named = named)
        }
    }
    .to_string()
}

/// The lines a run is read before approving: every step as the line wrote it, the binary each name
/// resolved to, what it would write, and what it is not.
///
/// The same things the panel shows, in the same order. Each argument is quoted by
/// [`bravebot_core::command::Step::as_written`], so no two argument lists render alike and a space
/// inside an argument cannot read as the boundary between two.
fn program(request: &RunRequest) -> Vec<String> {
    let mut lines = vec![shown(&request.summary())];
    for step in request.plan.steps() {
        lines.push(shown(&step.as_written()));
        // The binary under the name, because a name is not a program: `$PATH` decides what `grep`
        // means, and this is what will run.
        lines.push(format!("  {}", shown(&step.binary())));
    }
    if !request.plan.writes.is_empty() {
        lines.push(t!(run_writes).to_string());
        for path in &request.plan.writes {
            lines.push(format!("  {}", shown(&path.to_string_lossy())));
        }
    }
    // Said every time, because it is true every time and is the thing a person is likeliest to
    // assume otherwise.
    lines.push(t!(run_not_sandboxed).to_string());
    // Which access in particular a yes hands over, where the line reaches one nothing here holds.
    // The line above says what confinement there is and is said every time; this says what is
    // being granted, and is said only where there is something to name.
    let spends = request.ambient_authority();
    if !spends.is_empty() {
        lines.push(t!(run_spends_authority).to_string());
        for spent in &spends {
            lines.push(format!("  {}", authority(spent)));
        }
    }
    if request.releases_private() {
        lines.push(t!(run_releases_private).to_string());
    }
    lines
}

impl<R: BufRead, W: Write> Confirmer for Prompting<R, W> {
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        let lines = change(request);
        self.about(Held::Write);
        self.ask(&lines, t!(write_title)).into()
    }

    /// Approves this once and nothing else.
    ///
    /// The session's key for "stop asking about these programs" and the settings file's key for
    /// "stop asking about this line" are both a second answer to the one question, and a line has
    /// room for one: an answer of `a` beside `y` is a standing permission somebody could grant by
    /// mistyping. Saying yes again next time costs a keystroke; a grant nobody meant cannot be
    /// taken back.
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        let lines = program(request);
        self.about(Held::Run);
        match self.ask(&lines, t!(run_title)) {
            Decision::Approve => RunDecision::approve(),
            Decision::Reject => RunDecision::reject(),
        }
    }

    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        let mut lines = vec![
            shown(&request.summary()),
            t!(output_unseen).to_string(),
            checked(request.verdict),
        ];
        lines.extend(quarantined(&request.output));
        self.about(Held::Read);
        self.ask(&lines, t!(output_title))
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        let mut lines = vec![
            shown(&request.summary()),
            // The planner's own words about what it expects, which is untrusted for the reason
            // everything the planner wrote is.
            t!(vet_expected, expects = shown(&request.expects)),
            t!(vet_covers_this_only).to_string(),
            t!(vet_unseen).to_string(),
            checked(request.verdict),
        ];
        match &request.picture {
            // A picture is not a thing this mode can print, so the person is given a copy to
            // open. The path is the driver's own, which is why it goes out as any line of this
            // program's does rather than inside the margin.
            Some(picture) => {
                lines.push(t!(vet_picture_open).to_string());
                lines.push(shown(&picture.path.display().to_string()));
                lines.push(t!(vet_picture_words).to_string());
                if picture.is_a_pdf() {
                    lines.push(t!(vet_pdf_hidden_text).to_string());
                }
                self.about(Held::Read);
                self.ask(&lines, t!(vet_picture_title))
            }
            None => {
                lines.extend(quarantined(&request.content));
                self.about(Held::Read);
                self.ask(&lines, t!(vet_title))
            }
        }
    }

    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        let mut lines = vec![
            // The host on its own row, because that is what the answer is about: a URL is easy to
            // misread, and `https://example.com@evil.test/` names one site and reaches another.
            t!(fetch_host, host = shown(&request.host)),
            shown(&request.url),
        ];
        // What the host is, where it is this machine's own metadata service. That service asks
        // nothing of whoever opens the socket and answers with the credentials of the role, so
        // the address alone does not say what the request reaches.
        if request.ambient_authority().is_some() {
            lines.push(t!(fetch_authority_metadata).to_string());
        }
        lines.push(t!(fetch_explained).to_string());
        self.about(Held::Fetch);
        self.ask(&lines, t!(fetch_title))
    }

    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        let lines = vec![
            shown(&request.summary()),
            shown(&request.program),
            t!(server_workspace, workspace = shown(&request.workspace)),
            match request.runs_build_tooling {
                true => t!(server_build_tooling).to_string(),
                false => t!(server_reads_only).to_string(),
            },
            t!(server_explained).to_string(),
        ];
        self.about(Held::Server);
        self.ask(&lines, t!(server_title))
    }

    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        let mut lines = vec![
            shown(&request.path),
            t!(vouch_explained).to_string(),
            checked(request.verdict),
        ];
        match request.preview.is_empty() {
            true => lines.push(t!(vouch_nothing).to_string()),
            false => lines.extend(quarantined(&request.preview)),
        }
        self.about(Held::Vouch);
        self.ask(&lines, t!(vouch_title))
    }

    /// The findings, and nothing of the file.
    ///
    /// Each line is already a kind, a place and a mask, so there is no content here to picture or
    /// to put behind a margin. `shown` is still applied, for the reason the manifest's steps get
    /// it: a control character reaching a terminal from any direction is a cursor somewhere else,
    /// and the path in a finding is a path the planner may have spelled.
    fn confirm_exposing_read(&mut self, request: &ExposureRequest) -> Decision {
        let mut lines = vec![shown(&request.path), t!(expose_explained).to_string()];
        lines.push(t!(expose_found).to_string());
        for finding in &request.credentials {
            lines.push(format!("  {}", shown(finding)));
        }
        self.about(Held::Read);
        self.ask(&lines, t!(expose_title))
    }

    /// The whole list, each description behind the margin and none of it cut.
    ///
    /// A yes puts exactly this text in front of the planner for every session the list stays the
    /// same, so what is read here has to be all of it. The description is behind the margin because
    /// it is the server's words, and the names and arguments are not because the client drew them
    /// from a fixed alphabet.
    fn confirm_tool_list(&mut self, request: &ToolListRequest) -> Decision {
        let mut lines = vec![match request.tools.is_empty() {
            true => t!(mcp_tools_none, alias = shown(&request.alias)),
            false => t!(
                mcp_tools_offered,
                alias = shown(&request.alias),
                count = request.tools.len()
            ),
        }];
        if request.changed {
            lines.push(t!(mcp_tools_changed).to_string());
        }
        lines.push(checked(request.verdict));
        for tool in &request.tools {
            lines.push(format!("  {}", shown(&tool.name)));
            if !tool.arguments.is_empty() {
                lines.push(format!("    {}", shown(&tool.arguments.join(", "))));
            }
            if let Some(description) = &tool.description {
                lines.extend(
                    description
                        .lines()
                        .map(|line| format!("{} {}", crate::progress::QUARANTINE_BAR, shown(line))),
                );
            }
        }
        if request.refused > 0 {
            lines.push(t!(mcp_tools_not_listed, count = request.refused));
        }
        lines.push(t!(mcp_tools_explained).to_string());
        self.about(Held::Tools);
        self.ask(&lines, t!(mcp_tools_title))
    }

    /// Approves this call once and nothing else, for the reason a run is approved once here: a
    /// line has room for one answer, and the one that stops asking outlives the session.
    fn confirm_mcp_call(&mut self, request: &McpCallRequest) -> CallDecision {
        let mut lines = vec![format!("{} {}", shown(&request.name()), t!(mcp_call_kind))];
        match request.arguments.is_empty() {
            true => lines.push(format!("  {}", t!(mcp_call_no_arguments))),
            false => lines.extend(
                request
                    .arguments
                    .iter()
                    .map(|(name, value)| format!("  {}: {}", shown(name), shown(value))),
            ),
        }
        if let Some(description) = &request.description {
            lines.extend(quarantined(description));
        }
        self.about(Held::Tools);
        match self.ask(&lines, t!(mcp_call_question)) {
            Decision::Approve => CallDecision::approve(),
            Decision::Reject => CallDecision::reject(),
        }
    }

    /// The url a yes declares, pictured like every other word a server wrote, under the one the
    /// declaration names now and above the host and port it reaches.
    fn confirm_move(&mut self, request: &MoveRequest) -> Decision {
        let mut lines = vec![
            t!(
                mcp_move_declared,
                alias = shown(&request.alias),
                url = shown(&request.declared)
            ),
            t!(mcp_move_destination, url = shown(&request.destination)),
            t!(mcp_move_reaching, authority = shown(&request.authority)),
            t!(mcp_move_explained).to_string(),
        ];
        if !request.may_record {
            lines.push(t!(mcp_move_this_session_only).to_string());
        }
        self.about(Held::Move);
        self.ask(&lines, t!(mcp_move_title))
    }

    /// The plan, before anything has run.
    ///
    /// The steps are the driver's own rendering rather than somebody else's bytes, and the task is
    /// the person's own words, so neither is behind a margin. They are still pictured: a control
    /// character reaching a terminal from any direction is a cursor somewhere else.
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        let mut lines = vec![shown(&request.task)];
        for step in &request.steps {
            lines.push(format!("  {}", shown(step)));
        }
        for sentence in [
            t!(plan_explained),
            t!(plan_not_its_writes),
            t!(plan_nothing_yet),
        ] {
            lines.push(sentence.to_string());
        }
        self.about(Held::Manifest);
        self.ask(&lines, t!(plan_title))
    }

    /// A question the planner posed, answered in the person's own words.
    ///
    /// The choices are listed and the answer is a line, whether or not there were any: a line
    /// naming one of them is an answer in the person's own words that happens to be one of the
    /// options, and there is nothing here to move a cursor between rows with. Nothing typed is
    /// declining, which is a first-class answer, and so is the end of the input.
    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        // A planner that loops back over the same decision does not make the person restate it
        // (ASK-8). Each settled question is said to have been answered already, so an answer
        // reused silently is not mistaken for a question that was never put.
        let known: Vec<Option<Answer>> = asking
            .prompts
            .iter()
            .map(|prompt| self.recall_answer(&prompt.key))
            .collect();
        for (prompt, earlier) in asking.prompts.iter().zip(&known) {
            if earlier.is_some() {
                self.say(&t!(
                    session_answered_already,
                    question = shown(&prompt.question)
                ));
            }
        }

        let mut fresh = Vec::new();
        for prompt in asking
            .prompts
            .iter()
            .zip(&known)
            .filter(|(_, earlier)| earlier.is_none())
            .map(|(prompt, _)| prompt)
        {
            self.say(&shown(&prompt.header));
            self.say(&shown(&prompt.question));
            for row in &prompt.rows {
                self.say(&format!("  {}", shown(&row.label)));
                if let Some(detail) = &row.detail {
                    self.say(&format!("    {}", shown(detail)));
                }
            }
            let _ = write!(self.output, "{} ", t!(ask_own_words));
            let _ = self.output.flush();
            self.about(Held::Question);
            let on = self.answering();
            fresh.push(match self.line(on) {
                Some(typed) if !typed.trim().is_empty() => Answer::Typed(typed),
                _ => Answer::Declined,
            });
        }

        let answers = bravebot_tui::ask::in_order(known, fresh);
        for (prompt, answer) in asking.prompts.iter().zip(&answers) {
            self.remember_answer(prompt.key.clone(), answer.clone());
        }
        answers
    }

    /// Nothing. The only line this reads is a prompt it asked for or an answer to a question it
    /// just put, so there is never a line waiting that somebody typed unprompted: the turn runs on
    /// this thread, and nothing is read while it does.
    fn interjection(&mut self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A confirmer that needs no backend: it answers with what the test wrote for it.
    struct Canned {
        said: Vec<Said>,
    }

    impl<C: Confirmer + Send> Turns<C> for Canned {
        fn take(&mut self, _prompt: &str, _asking: &mut C) -> Said {
            match self.said.is_empty() {
                true => Said::default(),
                false => self.said.remove(0),
            }
        }
    }

    /// Drive a whole session over a script, and read back what each stream carried.
    fn session_over(script: &str, said: Vec<Said>) -> (String, String) {
        let mut reply = Vec::new();
        let mut beside = Vec::new();
        let mut asking = Prompting::new(
            std::io::BufReader::new(std::io::Cursor::new(script.as_bytes().to_vec())),
            Vec::new(),
        );
        let mut turns = Canned { said };
        lines(&mut asking, &mut reply, &mut turns, &mut beside);

        let mut aside = String::from_utf8(asking.output).expect("what was written is text");
        aside.push_str(&String::from_utf8(beside).expect("what was written is text"));
        (
            String::from_utf8(reply).expect("what was written is text"),
            aside,
        )
    }

    /// The reply is what a pipe gets, and nothing else is (CLI-5). A session in lines is
    /// interactive and still pipeable, which is what puts the marker, the questions and the
    /// progress on the other stream.
    #[test]
    fn the_reply_is_the_only_thing_on_the_reply_stream() {
        let (reply, beside) = session_over(
            "summarise this\n",
            vec![Said {
                reply: "a summary".to_string(),
                notices: vec!["a skill would not load".to_string()],
                clean: true,
                ..Said::default()
            }],
        );

        assert_eq!(reply, "a summary\n");
        assert!(
            beside.contains("a skill would not load"),
            "the notice is not beside the reply: {beside}"
        );
        assert!(
            beside.contains(MARKER),
            "nothing asked for a prompt: {beside}"
        );
    }

    /// A model other than the one in force answering is said beside the reply, never in it
    /// (CLI-10): a model that cannot be served is substituted rather than refused, so the name the
    /// server reported is the only trace of it, and a pipe reading the reply must not pick the
    /// complaint up.
    #[test]
    fn a_substituted_model_is_said_beside_the_reply() {
        let (reply, beside) = session_over(
            "summarise this\n",
            vec![Said {
                reply: "a summary".to_string(),
                not_served: Some("asked for one model, answered by another".to_string()),
                clean: true,
                ..Said::default()
            }],
        );

        assert_eq!(reply, "a summary\n");
        assert!(
            beside.contains("asked for one model, answered by another"),
            "the substitution was not said: {beside}"
        );
    }

    /// The property the whole module exists for. What a session in lines writes is what is written
    /// through these two streams, so an escape sequence anywhere in it would be here: the
    /// alternate screen, mouse reporting and bracketed paste are all a `\x1b[?` away, and a
    /// session that sent one would have taken something it cannot give back.
    #[test]
    fn a_session_in_lines_asks_the_terminal_for_nothing() {
        let (reply, beside) = session_over(
            "summarise this\nand again\n",
            vec![
                Said {
                    reply: "a summary".to_string(),
                    clean: true,
                    ..Said::default()
                },
                Said {
                    reply: "another".to_string(),
                    clean: true,
                    ..Said::default()
                },
            ],
        );

        for written in [&reply, &beside] {
            assert!(
                !written.contains('\x1b'),
                "a session in lines wrote an escape sequence: {written:?}"
            );
        }
    }

    /// The end of the input is how this session is left, there being no key to press for it: the
    /// terminal is not in raw mode, so Ctrl-D arrives as the end of the input and nothing further
    /// is read.
    #[test]
    fn the_end_of_the_input_ends_the_session() {
        let (reply, _) = session_over(
            "one\n",
            vec![
                Said {
                    reply: "first".to_string(),
                    ..Said::default()
                },
                Said {
                    reply: "never asked for".to_string(),
                    ..Said::default()
                },
            ],
        );

        assert_eq!(reply, "first\n");
    }

    /// Enter at the marker is not a prompt. Sending it would spend a turn, and a model asked
    /// nothing answers something.
    #[test]
    fn a_blank_line_is_not_a_turn() {
        let (reply, _) = session_over(
            "\n   \nsomething\n",
            vec![Said {
                reply: "the one answer".to_string(),
                ..Said::default()
            }],
        );

        assert_eq!(reply, "the one answer\n");
    }

    /// A turn that could not run leaves the reply stream alone: a script reading it gets nothing
    /// rather than an empty line that looks like an answer. The session goes on, because a backend
    /// that was unreachable a moment ago is worth another prompt and the conversation is still
    /// here.
    #[test]
    fn a_failed_turn_says_so_beside_and_the_session_goes_on() {
        let (reply, beside) = session_over(
            "one\ntwo\n",
            vec![
                Said {
                    failure: Some("BB1001: nothing answered".to_string()),
                    ..Said::default()
                },
                Said {
                    reply: "the second answer".to_string(),
                    ..Said::default()
                },
            ],
        );

        assert_eq!(reply, "the second answer\n");
        assert!(
            beside.contains("BB1001: nothing answered"),
            "the failure was not said: {beside}"
        );
    }

    /// A turn that could not run still says what its hooks said (HOOK-7). It produced no account of
    /// itself for those sentences to arrive on, and a session that dropped them would leave somebody
    /// believing a formatter that has not run since they mistyped its path is still running.
    #[test]
    fn a_failed_turn_still_says_what_its_hooks_said() {
        let (reply, beside) = session_over(
            "one\n",
            vec![Said {
                failure: Some("BB1001: nothing answered".to_string()),
                notices: vec!["hook turn-finished: /usr/bin/fmt could not be started".to_string()],
                ..Said::default()
            }],
        );

        assert!(reply.is_empty(), "the reply stream carried it: {reply}");
        assert!(
            beside.contains("/usr/bin/fmt could not be started"),
            "the turn failed and the hook sentence went with it: {beside}"
        );
    }

    /// Only the affirmative approves, and the end of the input is nobody answering rather than a
    /// yes. Every question this module puts goes through the one function, so this is the answer to
    /// all of them.
    #[test]
    fn only_the_affirmative_approves_and_silence_refuses() {
        for (answer, expected) in [
            ("y\n", Some(Decision::Approve)),
            ("Y\n", Some(Decision::Approve)),
            ("yes\n", Some(Decision::Reject)),
            ("n\n", Some(Decision::Reject)),
            ("\n", Some(Decision::Reject)),
            ("", None),
        ] {
            let mut asking = Prompting::new(
                std::io::BufReader::new(std::io::Cursor::new(answer.as_bytes().to_vec())),
                Vec::new(),
            );
            assert_eq!(
                asking.put(&["something".to_string()], "do it?"),
                expected,
                "{answer:?} was not read as {expected:?}"
            );
        }
    }

    /// SERVERS-11 in lines: the declaration, the destination and what it reaches are put to the
    /// person, and only the affirmative moves the server.
    #[test]
    fn a_move_is_asked_in_lines_and_only_a_yes_moves_the_server() {
        let request = MoveRequest {
            alias: "news".to_string(),
            declared: "https://news.example/mcp".to_string(),
            destination: "https://elsewhere.example/mcp".to_string(),
            authority: "elsewhere.example:443".to_string(),
            may_record: false,
        };
        for (answer, expected) in [
            ("y\n", Decision::Approve),
            ("n\n", Decision::Reject),
            ("", Decision::Reject),
        ] {
            let mut asking = Prompting::new(
                std::io::BufReader::new(std::io::Cursor::new(answer.as_bytes().to_vec())),
                Vec::new(),
            );
            assert_eq!(asking.confirm_move(&request), expected, "{answer:?}");
            let drawn = String::from_utf8(asking.output).expect("text");
            for shown in [
                "news is declared at https://news.example/mcp",
                "and its reply points to https://elsewhere.example/mcp",
                "reaching elsewhere.example:443",
                t!(mcp_move_this_session_only),
                t!(mcp_move_title),
            ] {
                assert!(drawn.contains(shown), "{shown} is not drawn in {drawn}");
            }
        }
    }

    /// The startup question (TRUST-7) reaches a session in lines too, and the three answers the
    /// panel has are the three a line has. What a yes grants is not decided here: the map comes
    /// back from the interface's own function, so the two surfaces cannot come to disagree about
    /// what trusting a directory means.
    #[test]
    fn the_startup_question_is_asked_in_lines_and_answered_the_same_way() {
        let asked = |answer: &str| {
            let mut asking = Prompting::new(
                std::io::BufReader::new(std::io::Cursor::new(answer.as_bytes().to_vec())),
                Vec::new(),
            );
            opening_trust(
                &mut asking,
                PermissionMode::Ask,
                std::path::Path::new("/work"),
                |_| None,
            )
        };

        assert!(
            asked("y\n").is_some_and(|trust| trust.is_trusted(".")),
            "the affirmative did not trust the working directory"
        );
        assert!(
            asked("n\n").is_some_and(|trust| !trust.is_trusted(".")),
            "declining trusted something"
        );
        // The end of the input in place of an answer is somebody leaving, which starts no session:
        // a session begun behind that question is one nobody agreed to have.
        assert!(
            asked("").is_none(),
            "the end of the input started a session"
        );
    }

    /// The mode that asks about nothing answers this question along with the rest, and answers it
    /// yes, so the question is not put at all. The decision is the interface's; what is pinned here
    /// is that a session in lines consults it rather than asking anyway, and before any remembered
    /// answer is read (TRUST-23), so the flag is what the session starts from.
    #[test]
    fn the_mode_that_asks_about_nothing_is_not_asked_about_the_directory() {
        let mut asking = Prompting::new(
            std::io::BufReader::new(std::io::Cursor::new(Vec::new())),
            Vec::new(),
        );
        let trust = opening_trust(
            &mut asking,
            PermissionMode::Bypass,
            std::path::Path::new("/work"),
            |_| panic!("the record was read for a session that asks about nothing"),
        );

        assert!(
            trust.is_some_and(|trust| trust.is_trusted(".")),
            "bypassing did not take the map a yes would have written"
        );
        assert!(
            String::from_utf8(asking.output)
                .expect("what was written is text")
                .is_empty(),
            "the question was put to a session that asks about nothing"
        );
    }

    /// A state directory and a working directory of their own under the build directory, emptied
    /// first, resolved the way a session's working directory is, and removed however the test ends.
    struct Remembering {
        base: std::path::PathBuf,
        home: std::path::PathBuf,
        root: std::path::PathBuf,
    }

    impl Remembering {
        fn new(name: &str) -> Self {
            let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(name);
            let _ = std::fs::remove_dir_all(&base);
            let (home, root) = (base.join("state"), base.join("project"));
            std::fs::create_dir_all(&home).expect("create the state directory");
            std::fs::create_dir_all(&root).expect("create the working directory");
            Self {
                home: home.canonicalize().expect("canonical state directory"),
                root: root.canonicalize().expect("canonical working directory"),
                base,
            }
        }
    }

    impl Drop for Remembering {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// The startup question with this input, the record found where a session here finds it.
    fn opening_with_a_record(
        answer: &str,
        home: &std::path::Path,
        root: &std::path::Path,
    ) -> (Option<TrustStore>, String) {
        let mut asking = Prompting::new(
            std::io::BufReader::new(std::io::Cursor::new(answer.as_bytes().to_vec())),
            Vec::new(),
        );
        let trust = opening_trust(&mut asking, PermissionMode::Ask, root, |root| {
            bravebot_agent::trusted::record_for(Some(home), None, root)
        });
        let said = String::from_utf8(asking.output).expect("what was written is text");
        (trust, said)
    }

    /// An answer the terminal interface or the desktop was told to remember answers a session in
    /// lines started in that directory too (TRUST-23), with the rule a yes writes and nothing else.
    /// The input is empty, so a question put anyway would have been left and started no session.
    /// The line it opens with says where the grant came from and how to take it back, naming the
    /// file, since the command that does it is not one a session in lines has.
    #[test]
    fn a_remembered_answer_settles_a_session_in_lines_without_asking() {
        let scratch = Remembering::new("plain-remembered-answer");
        let (home, root) = (&scratch.home, &scratch.root);
        let Some(identity) = bravebot_agent::trusted::Identity::of(root) else {
            // A filesystem that cannot say when a directory was made keeps no answer to read.
            return;
        };
        let store = bravebot_agent::trusted::Store::new(home, root);
        assert!(store.keep(&identity, "1-2", 7), "the answer was not kept");

        let (trust, said) = opening_with_a_record("", home, root);

        let trust = trust.expect("the question was put and left instead of the record answering");
        assert!(
            trust.is_trusted("."),
            "the kept answer did not trust the directory"
        );
        assert!(
            trust.is_trusted("src/main.rs"),
            "the kept answer did not cover the tree below the directory"
        );
        assert!(
            !trust.is_trusted("/etc/passwd"),
            "the kept answer trusted something outside the directory"
        );
        assert!(
            !said.contains(t!(trust_directory_title)),
            "the question was put although an answer was kept: {said}"
        );
        assert!(
            said.contains(&root.display().to_string())
                && said.contains(&store.path().display().to_string()),
            "the session did not say which directory the kept answer trusts and where it is kept: \
             {said}"
        );
    }

    /// The record a background session writes names what its own turns are addressed to, and not
    /// the definition of the record it resumed: its turns are not addressed to that one.
    #[test]
    fn a_background_session_records_the_definition_it_works_under_not_the_one_it_resumed() {
        let record: bravebot_session::sessions::Record =
            serde_json::from_value(serde_json::json!({
                "id": "1-2",
                "directory": "/tmp/x",
                "title": "a session",
                "started": 1,
                "updated": 1,
                "turns": 0,
                "tokens": 0,
                "conversation": {
                    "messages": [],
                    "context": "trusted",
                    "references": 0,
                    "archive": [],
                    "measured": 0,
                    "asked_to_write": false,
                },
                "agent": "rule-reviewer",
            }))
            .expect("a record");
        let root = std::path::Path::new("/tmp/x");

        let unnamed = Kept::opening(root, "1-2", Some(&record), None);
        assert_eq!(unnamed.handle.agent(), None);
        let named = Kept::opening(root, "1-2", Some(&record), Some("another"));
        assert_eq!(named.handle.agent(), Some("another"));
        let fresh = Kept::opening(root, "3-4", None, Some("another"));
        assert_eq!(fresh.handle.agent(), Some("another"));
    }

    /// With no answer kept about this directory the question is put. A yes here, and the `r` the
    /// terminal interface takes as "remember", are answered as every line is: the first trusts the
    /// directory for this session, the second declines, and neither writes the record, since
    /// nothing answered in lines outlives the session (CLI-14).
    #[test]
    fn an_answer_in_lines_is_asked_for_and_never_kept() {
        let scratch = Remembering::new("plain-answer-not-kept");
        let (home, root) = (&scratch.home, &scratch.root);
        let Some(identity) = bravebot_agent::trusted::Identity::of(root) else {
            return;
        };
        let store = bravebot_agent::trusted::Store::new(home, root);

        let (trust, said) = opening_with_a_record("y\n", home, root);
        assert!(
            said.contains(t!(trust_directory_title)),
            "a directory with no kept answer was not asked about: {said}"
        );
        assert!(
            trust.is_some_and(|trust| trust.is_trusted(".")),
            "the affirmative did not trust the working directory"
        );
        assert_eq!(store.kept(&identity), None, "a yes in lines was remembered");

        let (trust, _) = opening_with_a_record("r\n", home, root);
        assert!(
            trust.is_some_and(|trust| !trust.is_trusted(".")),
            "r was taken as something other than a line that is not yes"
        );
        assert_eq!(store.kept(&identity), None, "r in lines was remembered");
    }

    /// A write is approved from the change it would make, so the change is what the question
    /// carries. The body is untrusted, so what it holds cannot be allowed to move the cursor or
    /// forge the lines around it.
    #[test]
    fn a_write_is_asked_about_with_the_change_it_would_make() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "notes.md".to_string(),
            contents: "kept\nwritten\x1b[2J".to_string(),
            existing: Some("kept\nreplaced".to_string()),
            diff: bravebot_agent::diff::Diff::compute("kept\nreplaced", "kept\nwritten\x1b[2J"),
            intent: bravebot_agent::confirm::Intent::Edit,
            untrusted: false,
            remark: None,
            credentials: Vec::new(),
            may_always: false,
            record: None,
        };

        let lines = change(&request).join("\n");
        assert!(
            lines.contains("notes.md"),
            "the question does not name the file: {lines}"
        );
        assert!(
            lines.contains("- replaced") && lines.contains("+ written"),
            "the question does not carry the change: {lines}"
        );
        assert!(
            !lines.contains('\x1b'),
            "an escape sequence in the body reached the screen: {lines:?}"
        );
    }

    /// CHECKOUT-14. The question for a file brought back from a checkout says the session wrote the
    /// path in the working directory since, and one for any other write does not.
    #[test]
    fn a_write_since_the_checkout_is_said_in_the_plain_question() {
        let asked = |since: bool| {
            change(&WriteRequest {
                written_since_checkout: since,
                path: "out.txt".to_string(),
                contents: "theirs\n".to_string(),
                existing: Some("mine\n".to_string()),
                diff: bravebot_agent::diff::Diff::compute("mine\n", "theirs\n"),
                intent: bravebot_agent::confirm::Intent::Overwrite,
                untrusted: false,
                remark: None,
                credentials: Vec::new(),
                may_always: false,
                record: None,
            })
            .join("\n")
        };
        assert!(asked(true).contains("after the checkout was made"));
        assert!(!asked(false).contains("after the checkout was made"));
    }

    /// A processor's claim about a body it produced belongs beside the lines it describes. Nothing
    /// checks a remark against the document, and it decides nothing, so a person reading the diff
    /// has to be able to read the claim against it rather than remember it from further up.
    #[test]
    fn a_write_a_processor_produced_carries_what_it_said_about_it() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "notes.md".to_string(),
            contents: "written\n".to_string(),
            existing: None,
            diff: bravebot_agent::diff::Diff::compute("", "written\n"),
            intent: bravebot_agent::confirm::Intent::Create,
            untrusted: true,
            remark: Some(bravebot_agent::confirm::Remark {
                preview: vec!["fixed the typo in the heading".to_string()],
                lines: 1,
                label: "untrusted".to_string(),
            }),
            credentials: Vec::new(),
            may_always: false,
            record: None,
        };

        let lines = change(&request).join("\n");
        assert!(
            lines.contains("fixed the typo in the heading"),
            "what the processor said is not in the question: {lines}"
        );
        // Behind the margin, because the remark is the processor's own words about content nobody
        // vouched for, and a margin is the one part of a block its own text cannot forge.
        assert!(
            lines.contains(&format!(
                "{} fixed the typo",
                crate::progress::QUARANTINE_BAR
            )),
            "the remark is not marked as what it is: {lines}"
        );
    }

    /// A pipeline is approved from its arguments, one to a row: the boundaries between them are
    /// the thing being read, and a rendering that joined them with spaces would show a space
    /// inside an argument as a boundary between two.
    #[test]
    fn a_run_is_asked_about_one_argument_to_a_row() {
        let pipeline =
            bravebot_core::command::Pipeline::new(vec![bravebot_core::command::Stage::new(
                "rm",
                vec!["-rf".to_string(), "two words".to_string()],
            )]);
        let request = RunRequest::from_pipeline(&pipeline, &["/usr/bin/rm".to_string()], "/work");

        let lines = program(&request).join("\n");
        // Quoted, which is what keeps two argument lists from rendering alike: the space inside
        // the argument is inside the quotes rather than reading as the boundary before another.
        assert!(
            lines.contains("rm -rf 'two words'"),
            "the arguments do not read as their own: {lines}"
        );
        assert!(
            lines.contains("/usr/bin/rm"),
            "the question does not say which binary the name resolved to: {lines}"
        );
        assert!(
            lines.contains(t!(run_not_sandboxed)),
            "the question does not say the program is not sandboxed: {lines}"
        );
        // The line above says what confinement there is, which is none. This line reaches nothing
        // that is on no tier at all, so nothing further is claimed about it.
        assert!(
            !lines.contains(t!(run_spends_authority)),
            "a line reaching no ambient authority was said to spend one: {lines}"
        );
    }

    /// A name that reached its file through a link is asked about with the link beside the file,
    /// since the link is what starts and an entry for the line is keyed on both.
    #[test]
    fn a_run_started_through_a_link_is_asked_about_with_the_link() {
        let pipeline =
            bravebot_core::command::Pipeline::new(vec![bravebot_core::command::Stage::new(
                "python",
                vec!["-V".to_string()],
            )]);
        let mut request =
            RunRequest::from_pipeline(&pipeline, &["/usr/bin/python3.12".to_string()], "/work");
        if let bravebot_core::command::Steps::Pipeline(steps) = &mut request.plan.steps {
            steps[0].started_as = "/work/.venv/bin/python".into();
        }

        let lines = program(&request).join("\n");
        assert!(
            lines.contains("/work/.venv/bin/python -> /usr/bin/python3.12"),
            "the question does not say the link is what starts: {lines}"
        );
    }

    /// Both front ends ask the same question, so both have to say what a yes hands over. The
    /// blanket line is about confinement and is said every time; this names the particular access
    /// a person cannot read off the argument list, and a `gh` line is an account elsewhere rather
    /// than a program in this tree.
    #[test]
    fn a_run_that_reaches_an_ambient_authority_says_which_one() {
        let pipeline =
            bravebot_core::command::Pipeline::new(vec![bravebot_core::command::Stage::new(
                "gh",
                vec!["pr".to_string(), "list".to_string()],
            )]);
        let request = RunRequest::from_pipeline(&pipeline, &["/usr/bin/gh".to_string()], "/work");

        let lines = program(&request).join("\n");
        assert!(
            lines.contains(t!(run_not_sandboxed)),
            "the blanket line was dropped in favour of the list: {lines}"
        );
        assert!(
            lines.contains("already logged in"),
            "the question does not say what the line spends: {lines}"
        );
    }

    fn series_of(questions: &[(&str, &str)]) -> Asking {
        bravebot_core::ask::asking(&bravebot_core::ask::Series::new(
            questions
                .iter()
                .map(|(tag, sentence)| {
                    bravebot_core::ask::Question::new(
                        *tag,
                        *sentence,
                        vec![bravebot_core::ask::Choice::new("yes", None)],
                        false,
                    )
                })
                .collect(),
        ))
    }

    /// A planner that loops back over the same decision does not make the person restate it
    /// (ASK-8). The script holds one line, so a second question put to the reader would be
    /// declined by the end of the input and the answers would differ.
    #[test]
    fn a_question_asked_again_in_a_session_in_lines_is_answered_from_memory() {
        let mut asking = Prompting::new(
            std::io::BufReader::new(std::io::Cursor::new(b"the first one\n".to_vec())),
            Vec::new(),
        );
        let series = series_of(&[("Region", "Which region?")]);

        let first = asking.ask_user(&series);
        let before = asking.output.len();
        let second = asking.ask_user(&series);

        assert_eq!(first, vec![Answer::Typed("the first one".to_string())]);
        assert_eq!(second, first, "the person was asked again");
        let later = String::from_utf8(asking.output[before..].to_vec()).expect("text");
        assert!(
            later.contains(&t!(session_answered_already, question = "Which region?")),
            "the reuse was not said: {later}"
        );
        assert!(
            !later.contains(t!(ask_own_words)),
            "the question was put to the person again: {later}"
        );
    }

    /// A set where some questions are settled shows only the rest, and the answers keep the
    /// places of the questions they belong to. Questions differing only in their tag are
    /// different questions.
    #[test]
    fn a_series_with_some_questions_settled_puts_only_the_rest() {
        let mut asking = Prompting::new(
            std::io::BufReader::new(std::io::Cursor::new(b"east\nblue\n".to_vec())),
            Vec::new(),
        );
        let first = asking.ask_user(&series_of(&[("Region", "Which?")]));
        assert_eq!(first, vec![Answer::Typed("east".to_string())]);

        let before = asking.output.len();
        let both = asking.ask_user(&series_of(&[("Colour", "Which?"), ("Region", "Which?")]));

        assert_eq!(
            both,
            vec![
                Answer::Typed("blue".to_string()),
                Answer::Typed("east".to_string())
            ],
            "the remembered answer lost its place or the tag was ignored"
        );
        let later = String::from_utf8(asking.output[before..].to_vec()).expect("text");
        assert!(
            later.contains("Colour"),
            "the fresh question was not put: {later}"
        );
        assert!(
            !later.contains("Region"),
            "the settled question was put again: {later}"
        );
    }

    /// A watcher that keeps what each read was for, so a test can read it back.
    struct Recorded(std::sync::Arc<std::sync::Mutex<Vec<Waiting>>>);

    impl Watcher for Recorded {
        fn waiting(&mut self, on: Waiting) {
            self.0.lock().expect("the record").push(on);
        }

        fn received(&mut self, _on: Waiting, _line: Option<&str>) {}
    }

    /// BG-7: a server question is put to whoever attaches as a held question of its own kind, and
    /// is read as the foreground reads it. A reader at the end of its input declines, so a session
    /// nobody answers for is never given a server.
    #[test]
    fn a_server_question_is_held_as_its_own_kind_and_read_as_the_foreground_reads_it() {
        use bravebot_agent::servers::{Answer, Asker, Question};

        let declaration = bravebot_config::mcp::Declaration::Http {
            url: "https://news.example/mcp".to_string(),
        };
        for (typed, expected) in [
            (&b"1\n"[..], Answer::Once),
            (b"2\n", Answer::Project),
            (b"3\n", Answer::No),
            (b"what\n", Answer::No),
            (b"", Answer::No),
        ] {
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut asking = Prompting::watched(
                std::io::BufReader::new(std::io::Cursor::new(typed.to_vec())),
                Vec::new(),
                Box::new(Recorded(seen.clone())),
            );
            let answer = Attached(&mut asking).ask_to_start(&Question {
                alias: "news",
                file: ".bravebot/settings.json",
                declaration: &declaration,
                program: None,
                changed: false,
            });

            assert_eq!(answer, expected, "{typed:?}");
            assert_eq!(
                *seen.lock().expect("the record"),
                vec![Waiting::Answer(Held::Server)],
                "{typed:?}"
            );
            let screen = String::from_utf8(asking.output).expect("text");
            assert!(screen.contains("news"), "{screen}");
            assert!(screen.contains("[1/2/3]"), "{screen}");
        }
    }

    /// BG-7: every question a session puts reaches its watcher as the kind it is, which is the
    /// word the roster stores and the list shows. A method that never said its kind would be
    /// listed as a plain question, and one that said another's would be listed as that.
    #[test]
    fn each_question_tells_the_watcher_its_kind() {
        let pipeline =
            bravebot_core::command::Pipeline::new(vec![bravebot_core::command::Stage::new(
                "rm",
                vec!["-rf".to_string()],
            )]);
        let verdict = Verdict::Safe;
        let asked = |put: &mut dyn FnMut(&mut Prompting<_, Vec<u8>>)| {
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut asking = Prompting::watched(
                std::io::BufReader::new(std::io::Cursor::new(b"n\n".to_vec())),
                Vec::new(),
                Box::new(Recorded(seen.clone())),
            );
            put(&mut asking);
            std::mem::take(&mut *seen.lock().expect("the record"))
        };
        let held = |held| vec![Waiting::Answer(held)];

        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_write(&WriteRequest {
                    written_since_checkout: false,
                    path: "notes.md".to_string(),
                    contents: "new".to_string(),
                    existing: None,
                    diff: bravebot_agent::diff::Diff::compute("", "new"),
                    intent: bravebot_agent::confirm::Intent::Create,
                    untrusted: false,
                    remark: None,
                    credentials: Vec::new(),
                    may_always: false,
                    record: None,
                });
            }),
            held(Held::Write)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_run(&RunRequest::from_pipeline(
                    &pipeline,
                    &["/usr/bin/rm".to_string()],
                    "/work",
                ));
            }),
            held(Held::Run)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_read_output(&OutputRequest {
                    command: "ls".to_string(),
                    output: "a".to_string(),
                    lines: 1,
                    reference: "ref:1".to_string(),
                    verdict,
                    reason: None,
                });
            }),
            held(Held::Read)
        );
        for picture in [
            None,
            Some(bravebot_agent::confirm::PictureShown {
                path: "a.png".into(),
                media: "image/png".to_string(),
                bytes: 3,
            }),
        ] {
            assert_eq!(
                asked(&mut |asking| {
                    asking.confirm_vetted_read(&bravebot_agent::confirm::VetRequest {
                        origin: "a.md".to_string(),
                        expects: "notes".to_string(),
                        content: "a".to_string(),
                        lines: 1,
                        verdict,
                        reason: None,
                        picture: picture.clone(),
                    });
                }),
                held(Held::Read),
                "{picture:?}"
            );
        }
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_fetch(&FetchRequest {
                    url: "https://example.com/a".to_string(),
                    host: "example.com".to_string(),
                });
            }),
            held(Held::Fetch)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_server(&ServerRequest {
                    language: "rust",
                    program: "rust-analyzer".to_string(),
                    workspace: "/work".to_string(),
                    runs_build_tooling: false,
                });
            }),
            held(Held::Server)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_vouch(&VouchRequest {
                    path: "scripts/a.sh".to_string(),
                    preview: "echo".to_string(),
                    truncated: false,
                    verdict,
                    reason: None,
                });
            }),
            held(Held::Vouch)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_exposing_read(&ExposureRequest {
                    path: ".env".to_string(),
                    credentials: vec!["a token".to_string()],
                });
            }),
            held(Held::Read)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_tool_list(&ToolListRequest {
                    alias: "weather".to_string(),
                    tools: Vec::new(),
                    refused: 0,
                    changed: false,
                    verdict,
                    reason: None,
                });
            }),
            held(Held::Tools)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_mcp_call(&McpCallRequest {
                    alias: "weather".to_string(),
                    tool: "forecast".to_string(),
                    arguments: Vec::new(),
                    description: None,
                    may_stand: false,
                });
            }),
            held(Held::Tools)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_move(&MoveRequest {
                    alias: "news".to_string(),
                    declared: "https://news.example/mcp".to_string(),
                    destination: "https://elsewhere.example/mcp".to_string(),
                    authority: "elsewhere.example:443".to_string(),
                    may_record: false,
                });
            }),
            held(Held::Move)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.confirm_manifest(&ManifestRequest {
                    task: "tidy".to_string(),
                    steps: vec!["read".to_string()],
                });
            }),
            held(Held::Manifest)
        );
        assert_eq!(
            asked(&mut |asking| {
                asking.ask_user(&Asking {
                    prompts: vec![bravebot_core::ask::Prompt {
                        header: "Scope".to_string(),
                        question: "Which one?".to_string(),
                        rows: Vec::new(),
                        multiple: false,
                        key: "which".to_string(),
                    }],
                });
            }),
            held(Held::Question)
        );
        // A read nobody said the kind of is a question of the plainest kind, never a prompt.
        assert_eq!(
            asked(&mut |asking| {
                asking.answer("Name?");
            }),
            held(Held::Question)
        );
    }
}

#[cfg(test)]
#[path = "plain_retention_tests.rs"]
mod retention_tests;
