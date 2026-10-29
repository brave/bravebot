//! What `/status` reports.
//!
//! Built as lines rather than printed, so what it says can be tested without a terminal and shown
//! in the transcript like any other note.
//!
//! # What it deliberately leaves out
//!
//! Not the endpoint host and not the key id, though `bravebot doctor` prints both. A status panel is the
//! thing people paste into an issue or a screenshot, and an internal hostname is the part worth not
//! spreading. Which environment is in use answers the question people actually have, which is
//! whether they are pointed at dev or prod.
//!
//! Nothing here is labelled content. The trust rules are the user's own decisions, the paths in them
//! are workspace-relative names shown to the person who owns the workspace, and the counts are this
//! program's own arithmetic. No model reads any of it.

use bravebot_agent::servers::EffectivePolicy;
use bravebot_agent::watch;
use bravebot_config::Config;
use bravebot_core::label::Integrity;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use std::path::Path;

/// How many remembered lines are listed before the rest become a count.
///
/// The two standing permissions above this one are listed in full, because a permission nobody can
/// read back is one whose effect has to be remembered instead. This record is the one that can be
/// read somewhere else: it is a file, and the line below the list says where it is, so the count
/// says how much more is in a file the reader has just been pointed at.
const MAX_REMEMBERED: usize = 6;

/// How much of a background job's line `/status` and `/jobs` give, so a long one does not push
/// every note in the report across the screen. The whole line is in the job's row, which the view
/// opens.
pub(crate) const JOB_LINE: usize = 48;

/// One line at most `most` columns wide, ending in an ellipsis where it was cut.
///
/// Only line breaks and tabs are folded, since neither can be drawn in one row of the report. The
/// spacing inside a line is the command as it was approved.
pub(crate) fn cut(text: &str, most: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let line = text
        .lines()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\t', " ");
    if crate::wrap::display_width(&line) <= most {
        return line;
    }
    let mut kept = String::new();
    let mut width = 0;
    for c in line.chars() {
        let wide = c.width().unwrap_or(0);
        if width + wide + 1 > most {
            break;
        }
        width += wide;
        kept.push(c);
    }
    kept + "…"
}

/// One line of the report: a label, a value, and an optional aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub value: String,
    /// Why the value is what it is, where that is not obvious.
    pub note: String,
}

impl Line {
    pub(crate) fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.to_string(),
            value: value.into(),
            note: String::new(),
        }
    }

    pub(crate) fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }
}

/// Everything `/status` has to say, in the order it says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<Line>,
}

/// What the session knows about itself, passed in rather than read here.
///
/// A struct rather than eight arguments, and borrowed rather than owned, because this only reads.
pub struct Facts<'a> {
    pub session_name: &'a str,
    pub session_id: &'a str,
    pub directory: &'a Path,
    pub added_directories: &'a [std::path::PathBuf],
    /// The session's own directory outside the project, or `None` where it has none.
    pub scratch: Option<&'a Path>,
    /// The checkouts the session made for its delegates and has not removed, oldest first.
    pub checkouts: &'a [bravebot_agent::workspace::SessionCheckout],
    pub model: Option<&'a str>,
    /// The definition every turn is addressed to, where `--agent` named one (CLI-17).
    pub agent: Option<&'a crate::state::Addressed>,
    /// Whether the `agent` setting chose it (ADDRESS-13).
    pub agent_by_setting: bool,
    /// How hard the model is asked to think, or `None` where nothing is asked and the service
    /// applies its own default.
    pub effort: Option<bravebot_aichat::protocol::Effort>,
    /// Whether the model in force reads that level, as its roster row stated.
    pub model_reads_effort: bool,
    /// The level the session began with, where the model in force is one of the ChatGPT account's:
    /// the account is sent that level for the whole session whatever is chosen afterwards, unless
    /// the model refuses a level, so it is what a request carries
    /// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)). `None` for every other
    /// model, and `Some(None)` for a session that began with none.
    pub account_effort: Option<Option<bravebot_aichat::protocol::Effort>>,
    /// What the server reported using on the last turn, or `None` before one has run.
    ///
    /// Observed rather than configured, which is the whole point: the endpoint answers a model name
    /// it will not serve by substituting a weaker one, so what was asked for does not establish what
    /// answered.
    pub served_model: Option<&'a str>,
    /// What that same turn asked the service for, where the server answered with something else.
    ///
    /// `None` on the ordinary path, and wherever the two names were never comparable. Recorded by
    /// the turn that ran rather than worked out here, because the model field above is the choice
    /// in force for the *next* turn: `/model` moves it and leaves the served name alone, so a panel
    /// that compares the two is comparing two different turns.
    pub substituted_model: Option<&'a str>,
    /// Whether the last turn actually presented the premium key.
    ///
    /// `None` before any turn has run. Three states rather than a bool because "not yet known" is
    /// not the same as "no", and reporting a guess as an observation is the bug this replaced.
    pub premium: Option<bool>,
    pub theme: &'a str,
    pub config: &'a Config,
    /// What the platform can confine, without the sandbox mode: the mode is [`Facts::sandbox_mode`]
    /// and is put beside it here, so the line follows `/sandbox`.
    pub confinement: &'a str,
    /// Whether this kernel leaves every unix socket that has a path reachable from a confined
    /// stage, as [`crate::state::Session::sockets_with_a_path_reachable`] holds it.
    pub sockets_with_a_path_reachable: bool,
    /// The sandbox mode programs `run` starts are held to now, which `/sandbox` can change.
    pub sandbox_mode: bravebot_sandbox::SandboxMode,
    /// The MCP servers the session started.
    pub servers: &'a crate::state::Servers,
    /// How much the session is asking before it acts, as the mode key last left it.
    pub permission_mode: bravebot_agent::PermissionMode,
    /// Whether the session was started with the flag that skips permissions.
    pub began_in_bypass: bool,
    /// Whether the programs `run` starts write every path a request for writing would be granted,
    /// as [`programs_write_unasked`] answers it, before the trust of the directory is read: a
    /// workspace the person has not trusted accepts no request, so [`report`] says it only where
    /// `trust` vouches for the directory.
    pub programs_write_unasked: bool,
    /// Whether a check that finds nothing promotes a slot without the person being asked.
    ///
    /// Reported for the reason the permission mode is: it is a standing answer that stops a prompt
    /// appearing, and a note at startup scrolls away. Only where it is on, since a line saying the
    /// ordinary thing on every session is a line people learn to skim.
    pub auto_vetting: bool,
    pub turns: usize,
    pub tokens: u64,
    /// What the turn in flight has spent so far, which `tokens` does not hold until the turn ends.
    /// Added to the goal's spend, since `/status` is answered while a turn runs.
    pub tokens_in_flight: u64,
    /// Where the session's wall clock went, every turn added together.
    ///
    /// Beside the token count because it is the other half of what a session cost. A person
    /// reading this wants to know which of the three things to do something about, and only the
    /// split can tell them: a faster model, a faster test suite, or fewer prompts.
    pub timing: bravebot_agent::timing::Timing,
    /// How much of the last turn's prompt the backend answered out of its own cache.
    ///
    /// `None` before a turn has run. Zero in both halves where the service reported nothing about a
    /// cache, which every backend but Bedrock currently does, and that is not the same answer as a
    /// turn whose cache missed.
    pub cached: Option<bravebot_aichat::protocol::Cached>,
    pub trust: &'a TrustStore,
    pub programs: &'a TrustedPrograms,
    /// Whether the turn in flight holds the two above, which it answers into as it runs. Then
    /// neither is read and the report says the rules are the turn's until it ends.
    pub turn_holds_rules: bool,
    /// The loops repeating a prompt, where the person started any, oldest first.
    ///
    /// Absent from the report entirely when there is none, rather than reported as "no loop": a
    /// line saying a thing is not happening is a line on every report for the sake of the few
    /// where it is.
    pub looping: &'a [crate::loops::Running],
    /// The condition the session is working towards, where the person set one.
    ///
    /// Left out when there is none, for the reason the loop is.
    pub goal: Option<&'a crate::goals::Running>,
    /// The standing watches this session holds, oldest first.
    ///
    /// Empty says nothing at all, for the reason the loop's absence says nothing. A live one is
    /// a line each, because the number is what a person ends it by and the turn that armed it is
    /// what makes a prompt arriving hours later have a cause.
    pub watches: &'a [watch::Watch],
    /// The background jobs of the turn in flight, or of the last one, in the order they started.
    ///
    /// A `/status` typed during a turn is answered at once, so a job here may still be running.
    ///
    /// Empty says nothing, for the reason no watch says nothing.
    pub jobs: Vec<(&'a crate::state::Output, &'a crate::state::JobView)>,
    /// The command lines somebody asked to be remembered past a session, for this directory.
    ///
    /// `None` where this session keeps no such record at all: no state directory, or a mode that
    /// adds nothing to one. Nothing is said in that case, for the reason the loop says nothing when
    /// there is none.
    pub remembered: Option<Remembered<'a>>,
    /// The reach remembered for commands that is in force here, one row each as `/reach` lists it.
    ///
    /// Empty says nothing, for the reason no remembered line says nothing.
    pub reach: &'a [String],
    /// The paths the person let programs reach for this session, one row each as `/reach paths`
    /// lists it (SANDBOX-28). Empty says nothing.
    pub requested_paths: &'a [String],
    /// The directories bypass mode took the file tools into, one line each (PATHREQ-7).
    pub reached_directories: &'a [String],
    /// The yes about this directory a session kept, where one is kept and would settle the next
    /// session started here (TRUST-23).
    pub kept_trust: Option<KeptTrust<'a>>,
}

/// A remembered yes about the working directory, as the report needs it: when it was given, and
/// the file that holds it.
#[derive(Debug, Clone, Copy)]
pub struct KeptTrust<'a> {
    /// How long ago, already worded.
    pub when: &'a str,
    pub path: &'a Path,
    /// The git worktree root the answer was kept about, where that is not the working directory.
    pub root: Option<&'a Path>,
}

/// The record of lines remembered past a session, as the report needs it.
///
/// The lines and where they are kept, because both are part of what a person has to be able to read
/// back: what they are still carrying, and the file to delete a line from to be asked again.
#[derive(Debug, Clone, Copy)]
pub struct Remembered<'a> {
    pub lines: &'a bravebot_core::remembered::Remembered,
    pub path: &'a Path,
}

/// What to call a permission mode, or `None` for the one that needs no name.
///
/// One definition, used by `/status` and by the line under the input box, so the two cannot come to
/// call the same mode different things. `None` for asking in a session that never had bypass, which
/// is what a session has always done: the modes worth drawing are the ones that changed something.
///
/// Asking is named where the session started with the flag that skips permissions. It began in
/// bypass, so leaving it is a change, and a line that goes blank is indistinguishable from a
/// session that never skipped anything.
pub fn named_mode(
    mode: bravebot_agent::PermissionMode,
    began_in_bypass: bool,
) -> Option<&'static str> {
    use bravebot_agent::PermissionMode;
    match mode {
        PermissionMode::Ask => began_in_bypass.then(|| t!(mode_ask)),
        PermissionMode::AcceptEdits => Some(t!(mode_accept_edits)),
        PermissionMode::Plan => Some(t!(mode_plan)),
        PermissionMode::Bypass => Some(t!(mode_bypass)),
    }
}

/// The confinement line with the sandbox mode beside it, where the mode is not `workspace-write`.
///
/// Said for `strict` and `read-only`, which change what a program can read or write, and for `off`,
/// the one a person has to be able to see without asking, since nothing else on the screen shows
/// that a program `run` starts is not confined at all (SANDBOX-22).
pub fn with_the_sandbox_mode(confinement: &str, mode: bravebot_sandbox::SandboxMode) -> String {
    use bravebot_sandbox::SandboxMode;
    match mode {
        SandboxMode::WorkspaceWrite => confinement.to_string(),
        SandboxMode::Strict | SandboxMode::ReadOnly => t!(
            confinement_with_mode,
            level = confinement,
            mode = mode.name()
        ),
        SandboxMode::Off => t!(confinement_with_mode_off, level = confinement),
    }
}

/// The line for the network the programs `run` starts have, where it is closed.
fn network_line(
    settled: Option<&bravebot_config::RunNetwork>,
    mode: bravebot_sandbox::SandboxMode,
) -> Option<Line> {
    // Not under `off`: nothing confines a program then, so a closed network would be a claim about
    // a boundary that is not there. The confinement line says the programs are unconfined.
    if mode == bravebot_sandbox::SandboxMode::Off {
        return None;
    }
    // `read-only` closes it whatever the setting says (SANDBOX-22); the confinement line names the
    // mode that did.
    let setting = settled.map_or(bravebot_sandbox::network::Network::Open, |s| s.network);
    if !mode.network_in_force(setting).is_closed() {
        return None;
    }
    let line = Line::new(t!(status_network), t!(status_network_closed));
    Some(
        match settled.filter(|settled| settled.network.is_closed()) {
            Some(settled) => line.with_note(run_network_source(&settled.decided)),
            None => line,
        },
    )
}

/// Whether a stage the next turn starts reaches every unix socket that has a path: where the kernel
/// does not govern them and the mode is `workspace-write`. `strict` and `read-only` refuse such a
/// stage, and under `off` the confinement line already says nothing is confined (SANDBOX-3).
pub fn sockets_with_a_path_open(reachable: bool, mode: bravebot_sandbox::SandboxMode) -> bool {
    reachable && mode == bravebot_sandbox::SandboxMode::WorkspaceWrite
}

/// The line for the unix sockets the programs `run` starts can reach, where every one that has a
/// path is open to them.
fn sockets_line(reachable: bool, mode: bravebot_sandbox::SandboxMode) -> Option<Line> {
    sockets_with_a_path_open(reachable, mode).then(|| {
        Line::new(t!(status_path_sockets), t!(status_path_sockets_reachable))
            .with_note(t!(status_path_sockets_why))
    })
}

/// Whether the stages this session's `run` starts write every path a request for writing would be
/// granted, as [`bravebot_agent::confine::Confinement::writes_without_a_request`] answers for the
/// home, sandbox mode and permission mode a turn builds its confinement from. This session is the
/// lead session, so bypass alone decides whether the confinement is told to. It leaves out the
/// trust of the directory, which a turn reads when a line runs and [`report`] reads from the map.
pub fn programs_write_unasked(
    mode: bravebot_sandbox::SandboxMode,
    permission: bravebot_agent::PermissionMode,
    home: Option<&Path>,
) -> bool {
    bravebot_agent::confine::Confinement::here(Vec::new(), None, home).is_some_and(|confinement| {
        confinement
            .with_mode(mode)
            .with_unasked_writes(permission == bravebot_agent::PermissionMode::Bypass)
            .writes_without_a_request()
    })
}

/// The files that wrote a list's entries, each once, in the order they are first seen. An entry no
/// file wrote is named `unnamed`.
fn sources<'a>(
    by: impl Iterator<Item = &'a Option<std::path::PathBuf>>,
    unnamed: &str,
) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    for by in by {
        let source = match by {
            Some(path) => path.display().to_string(),
            None => unnamed.to_string(),
        };
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    sources
}

/// The line for the person's own filesystem lists, where any has an entry: how many each holds and
/// the files that wrote them, never an entry, since a path is for `doctor` and a glob's matches are
/// the machine's.
fn filesystem_line(settled: Option<&bravebot_config::Filesystem>) -> Option<Line> {
    let lists = &settled.filter(|settled| !settled.lists.is_empty())?.lists;
    let sources = sources(
        [
            &lists.allow_read,
            &lists.deny_read,
            &lists.allow_write,
            &lists.deny_write,
        ]
        .into_iter()
        .flatten()
        .map(|entry| &entry.by),
        t!(status_sandbox_filesystem_flags),
    );
    Some(
        Line::new(
            t!(status_sandbox_filesystem),
            t!(
                status_sandbox_filesystem_counts,
                allow_read = lists.allow_read.len(),
                deny_read = lists.deny_read.len(),
                allow_write = lists.allow_write.len(),
                deny_write = lists.deny_write.len()
            ),
        )
        .with_note(t!(
            status_sandbox_filesystem_files,
            files = sources.join(", ")
        )),
    )
}

/// The line for the hosts a program `run` starts may reach, where a list is set: how many entries
/// each list holds and the files that wrote them, never an entry, for the reason the filesystem
/// line gives. Nothing for a session with no `allowedHosts`, which filters nothing, and nothing
/// under `off`, where nothing confines a program to the list.
fn hosts_line(
    settled: Option<&bravebot_config::sandbox_network::Resolved>,
    mode: bravebot_sandbox::SandboxMode,
) -> Option<Line> {
    let hosts = &settled?.hosts;
    let allowed = hosts.allowed.as_ref()?;
    if mode == bravebot_sandbox::SandboxMode::Off {
        return None;
    }
    let sources = sources(
        allowed.iter().chain(&hosts.denied).map(|entry| &entry.by),
        t!(status_hosts_managed),
    );
    let counts = t!(
        status_hosts_counts,
        allowed = allowed.len(),
        denied = hosts.denied.len()
    );
    let asks = hosts.on_unlisted == Some(bravebot_config::sandbox_network::OnUnlisted::Ask);
    Some(
        Line::new(t!(status_hosts), counts).with_note(match (sources.is_empty(), asks) {
            (true, false) => t!(status_hosts_refused).to_string(),
            (true, true) => t!(status_hosts_asked).to_string(),
            (false, false) => t!(status_hosts_files, files = sources.join(", ")),
            (false, true) => t!(status_hosts_files_ask, files = sources.join(", ")),
        }),
    )
}

/// Why an entry of one of the filesystem lists is not in force, in the words `doctor` uses.
pub fn filesystem_reason(reason: bravebot_sandbox::rules::Reason) -> &'static str {
    bravebot_agent::permissions::filesystem_reason(reason)
}

/// Who closed the network for the programs `run` starts, in the words `/status` and `doctor` share.
pub fn run_network_source(decided: &bravebot_config::Decided) -> String {
    use bravebot_config::Decided;
    match decided {
        Decided::Default => t!(status_network_by_default).to_string(),
        Decided::Flag => t!(status_network_by_flag).to_string(),
        Decided::Settings(Some(path)) => t!(
            status_network_by_settings,
            path = path.display().to_string()
        ),
        Decided::Settings(None) => t!(status_network_by_a_setting).to_string(),
        Decided::Managed(Some(path)) => {
            t!(status_network_pinned, path = path.display().to_string())
        }
        Decided::Managed(None) => t!(status_network_pinned_by_policy).to_string(),
    }
}

/// Compose the report.
pub fn report(facts: &Facts<'_>) -> Report {
    let mut lines = Vec::new();

    lines.push(Line::new(
        t!(status_session),
        if facts.session_name.is_empty() {
            t!(status_session_untitled).to_string()
        } else {
            facts.session_name.to_string()
        },
    ));
    lines.push(Line::new(t!(status_session_id), facts.session_id));

    let directory = Line::new(t!(status_directory), abbreviate(facts.directory));
    lines.push(if facts.turn_holds_rules {
        directory
    } else if facts.trust.is_trusted(".") {
        directory.with_note(t!(status_directory_trusted))
    } else {
        directory.with_note(t!(status_directory_untrusted))
    });

    // Under the directory it is about, since it is the answer the next session here starts from
    // and nothing else on the screen will ever say so: the question it stops is the only place a
    // person would have seen it.
    if let Some(kept) = facts.kept_trust {
        lines.push(
            Line::new("", t!(status_directory_kept, when = kept.when)).with_note(match kept.root {
                Some(root) => {
                    t!(status_directory_kept_root_note, root = root.display())
                }
                None => t!(status_directory_kept_note).to_string(),
            }),
        );
        lines.push(Line::new(
            "",
            t!(status_directory_kept_where, path = kept.path.display()),
        ));
    }

    for added in facts.added_directories {
        lines.push(
            Line::new(t!(status_also_open), abbreviate(added))
                .with_note(t!(status_added_directory)),
        );
    }

    // Named for what it is, beside the directories a person opened themselves. The trust lines
    // below cannot report it: it has no rule, which is the whole of what makes it different from
    // an added directory, so without a line here a session holds a directory it may write in that
    // nobody asked for and nothing says so.
    if let Some(scratch) = facts.scratch {
        lines.push(
            Line::new(t!(status_scratch), abbreviate(scratch)).with_note(t!(status_scratch_note)),
        );
    }

    // The transcript says a checkout was kept but not where: its path goes to the planner alone,
    // beside the delegate's report.
    for checkout in facts.checkouts {
        lines.push(
            Line::new(t!(status_checkout), abbreviate(&checkout.path)).with_note(t!(
                status_checkout_note,
                id = &checkout.id,
                commit = checkout.commit.get(..10).unwrap_or(&checkout.commit),
                delegate = checkout.delegate.to_string()
            )),
        );
    }

    lines.extend(model_and_agent_lines(facts));

    lines.extend(choice_lines(facts));

    lines.extend(sandbox_lines(facts));

    lines.extend(standing_lines(facts));

    lines.extend(running_lines(facts));

    lines.extend(usage_lines(facts));

    // Last because it is the part that grows. What a write recorded is the thing nothing else
    // reports: a file an earlier turn marked untrusted is invisible until it refuses to be read.
    let remembered = facts.remembered.filter(|record| !record.lines.is_empty());
    // The turn in flight owns the trust map and the vouched programs and answers into them as it
    // goes, so a copy taken when it began would be a claim about an earlier moment, and the line
    // above about every run being asked is the one this report must not get wrong.
    if facts.turn_holds_rules {
        lines.push(Line::new(t!(status_trust), t!(status_held_by_the_turn)));
    } else {
        let rules: Vec<(&str, Option<Integrity>)> = facts.trust.rules().collect();
        if rules.is_empty() {
            lines.push(Line::new(t!(status_trust), t!(status_nothing_vouched_for)));
        } else {
            lines.push(Line::new(
                t!(status_trust),
                t!(count_rules, count = rules.len()),
            ));
            for (path, integrity) in rules.iter() {
                // The empty name is the project root and is drawn as `.`. Any other name goes
                // through `shown_name`, so a rule about a name made of whitespace, or ending in
                // one, is not a row that reads as another name or as nothing.
                let shown = match path.is_empty() {
                    true => ".".to_string(),
                    false => bravebot_agent::confirm::shown_name(path),
                };
                lines.push(match integrity {
                    Some(Integrity::Trusted) => Line::new("", shown).with_note(t!(status_trusted)),
                    Some(Integrity::Untrusted) => {
                        Line::new("", shown).with_note(t!(status_untrusted))
                    }
                    None => Line::new("", shown).with_note(t!(status_undecided)),
                });
            }
        }

        // A standing permission the user gave earlier and cannot otherwise see. Every other prompt in
        // this session announces itself by appearing; this is the one that stops appearing, so without
        // a line here there is nothing to tell them a command now runs unasked and that what it prints
        // is being read as trusted.
        let vouched: Vec<&bravebot_core::programs::Command> = facts.programs.iter().collect();

        if vouched.is_empty() {
            // Two different true things, and the wider one is only true where the record is empty as
            // well. A session that has vouched for nothing but carries a remembered line does not put
            // every run to the person, and this is the screen responsible for saying what they are
            // carrying: a flat "every run is put to you" above a list of lines that run unasked is the
            // one claim this report must not make.
            lines.push(Line::new(
                t!(status_programs),
                match remembered.is_some() {
                    true => t!(status_nothing_vouched_this_session),
                    false => t!(status_every_run_is_asked),
                },
            ));
        } else {
            lines.push(
                Line::new(
                    t!(status_trusted_commands),
                    t!(count_commands, count = vouched.len()),
                )
                .with_note(t!(status_trusted_commands_note)),
            );
            // Every one of them, however many there are: a vouched command that the report will not
            // show is a permission with nothing anywhere to say it is held, since the prompt it
            // answers is the thing that has stopped appearing.
            //
            // With the tree it was given in, because that is part of what the entry covers (RUN-8) and
            // the entry is what this report exists to read back: two entries for one command in two
            // directories would otherwise draw as one line twice, and a reader could not tell which
            // grant they hold. Relative to the workspace where it is inside it, the way every other
            // path on this screen is written.
            for command in vouched.iter() {
                lines.push(Line::new("", command.display()).with_note(t!(
                    status_command_in,
                    directory = within(facts.directory, &command.directory)
                )));
            }
        }
    }

    if let Some(record) = remembered {
        lines.extend(remembered_lines(facts, record));
    }

    lines.extend(grant_lines(facts));

    if let Some(session) = &facts.servers.session {
        lines.extend(server_policy_lines(&session.policies()));
    }

    Report { lines }
}

/// The model, the agent and the model that answered, where it is not the one asked for.
fn model_and_agent_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();

    // The definition's model where it names one, since `/model` was not what chose it.
    let definitions = facts
        .agent
        .and_then(|agent| agent.model.as_deref().map(|model| (agent, model)));
    lines.extend(match (definitions, facts.model) {
        (Some((agent, model)), _) => model_lines(
            facts.config,
            model,
            t!(status_model_definitions, definition = &agent.name),
        ),
        (None, Some(model)) => {
            model_lines(facts.config, model, t!(status_model_chosen).to_string())
        }
        (None, None) => model_lines(
            facts.config,
            &facts.config.default_model,
            t!(status_model_default).to_string(),
        ),
    });

    // Nothing else on the screen shows that the session is not the planner's, since the input box
    // is unchanged.
    if let Some(agent) = facts.agent {
        let note = match facts.agent_by_setting {
            true => t!(status_agent_by_setting),
            false => t!(status_agent_every_turn),
        };
        lines.push(Line::new(t!(status_agent), &agent.name).with_note(note));
    }

    // What actually answered, where that is not what was asked for. The endpoint substitutes a
    // model it will not serve rather than refusing, so the line above can name Opus for a whole
    // session that was answered by something else every turn. Reported beside it, since the two
    // together are the fact and either alone is misleading.
    //
    // Both halves are the one turn's: what it asked for and what came back. The model line above
    // is not the first half, because it is the choice the next turn will be sent and `/model`
    // moves it without a turn running. Comparing the two names a model nothing was asked of as the
    // one that was asked for, and puts the line on a session that got exactly what it wanted.
    if let (Some(asked), Some(served)) = (facts.substituted_model, facts.served_model) {
        lines.push(
            Line::new(t!(status_served), served)
                .with_note(t!(status_served_instead, asked = asked)),
        );
    }

    lines
}

/// The effort level, the theme and the endpoint: what a person chose, and what it reaches.
fn choice_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();

    // Beside the model, because the two together are what a turn costs: the same question asked of
    // the same model bills differently at either end of this range.
    let word = |level: Option<bravebot_aichat::protocol::Effort>| match level {
        Some(level) => level.as_str().to_string(),
        None => t!(effort_unset).to_string(),
    };
    lines.push(match (facts.effort, facts.model_reads_effort) {
        // A model of the account is sent the level its session began with, which is the level a
        // request carries and so the one named, with the level chosen since beside it. Where the
        // two are the same the line is the ordinary one.
        (chosen, true) if facts.account_effort.is_some_and(|began| began != chosen) => {
            let began = facts.account_effort.flatten();
            Line::new(t!(status_effort), word(began))
                .with_note(t!(status_effort_kept_for_account, chosen = word(chosen)))
        }
        // A level the model will not read is still the level somebody chose, so it is named rather
        // than hidden. What changes is the note: reporting it as in force would be this panel
        // telling them a request carries something it does not.
        (Some(level), false) => {
            Line::new(t!(status_effort), level.as_str()).with_note(t!(status_effort_not_read))
        }
        (Some(level), true) => {
            Line::new(t!(status_effort), level.as_str()).with_note(t!(status_effort_chosen))
        }
        (None, _) => {
            Line::new(t!(status_effort), t!(effort_unset)).with_note(t!(status_effort_default))
        }
    });

    lines.push(Line::new(t!(status_theme), facts.theme).with_note(t!(status_theme_chosen)));

    // The environment rather than the host. See the note at the top of this file.
    //
    // The note says which tier the last turn actually ran on, not whether this build knows a premium
    // host. It used to say the latter, which is baked in at compile time and true of every build:
    // a session that never presented a key still reported "premium configured" while every
    // request went out without one and came back answered by a weaker model. What a person wants
    // from this line is what they are getting, and that is a fact about a request.
    lines.push(
        Line::new(t!(status_endpoint), environment(&facts.config.endpoint)).with_note(
            match (facts.config.premium_key.is_some(), facts.premium) {
                (true, Some(true)) => t!(status_premium_in_use),
                (true, Some(false)) => t!(status_premium_not_spent),
                // Nothing has run yet, so nothing has been observed. Saying which tier is in use
                // before a request has been made would be the same guess as before.
                (_, None) | (false, _) => configured_tier(facts.config),
            },
        ),
    );

    lines
}

/// The confinement level and the network, sockets, filesystem and host lists that qualify it.
fn sandbox_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();

    // The level is what the platform can enforce over a process running code we did not write, and
    // a session is inside no such boundary. Reported with the note rather than dropped, because
    // which of the three a machine offers is the question somebody asks this panel; reported without
    // it, the same line reads as a guarantee over the reads, writes and programs the session runs,
    // which have never been confined. A local MCP server is the one process it does confine.
    lines.push(
        Line::new(
            t!(status_confinement),
            with_the_sandbox_mode(facts.confinement, facts.sandbox_mode),
        )
        .with_note(match facts.servers.confined {
            true => t!(status_confinement_servers),
            false => t!(status_confinement_nothing_confined),
        }),
    );

    // Only where it is closed, for the reason the mode and the vetting lines are: an open network is
    // what every session had, and a line saying so on each would be skimmed past. The note says who
    // closed it, because the way to open it again depends on that.
    lines.extend(network_line(
        bravebot_config::settled_run_network(),
        facts.sandbox_mode,
    ));
    // Only where the kernel leaves them open, which is most Linux machines today: a person who
    // trusts the profile to keep a program from the SSH agent should be told where it does not.
    lines.extend(sockets_line(
        facts.sockets_with_a_path_reachable,
        facts.sandbox_mode,
    ));
    // Beside it and on the same terms: nothing for a session that wrote no list, which is the
    // session this screen has always described.
    lines.extend(filesystem_line(
        bravebot_config::settled_sandbox_filesystem(),
    ));

    // After the filesystem lists and on the same terms: nothing for a session that wrote no host
    // list, which is the session this screen has always described.
    lines.extend(hosts_line(
        bravebot_config::sandbox_network::settled(),
        facts.sandbox_mode,
    ));

    lines
}

/// The standing answers that hold the session back: MCP servers, permission mode, programs write and auto-vetting.
fn standing_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();

    // Named rather than counted, since the question is which of them this session can reach, and
    // said where there are none, since a checkout that asked for one is where somebody looks. The
    // note is how each one's tools stand, since a started server whose list nobody has read yet
    // offers the model nothing, and that is the thing somebody wondering why it was not used needs.
    lines.push(match facts.servers.started.as_slice() {
        [] => Line::new(t!(status_mcp_servers), t!(status_mcp_servers_none)),
        started => {
            let line = Line::new(t!(status_mcp_servers), started.join(", "));
            match &facts.servers.session {
                Some(session) => line.with_note(offering(&session.offering())),
                None => line,
            }
        }
    });

    // Only where the mode is not the ordinary one. A line saying "asking" on every session would
    // teach people to skim past exactly the one that matters. Beside confinement because it is the
    // other half of the same question: what is holding this session back.
    if let Some(named) = named_mode(facts.permission_mode, facts.began_in_bypass) {
        lines
            .push(Line::new(t!(status_permissions), named).with_note(t!(status_permissions_cycle)));
    }

    // Only where it is so, for the reason the network line is: the ordinary session writes the
    // directories it was opened on, and a line saying so on each would be skimmed past. A
    // directory the person has not trusted accepts no request, so its stages are given none of
    // the rows and the line is not drawn; while a turn holds the trust map the report cannot tell
    // which, and says that instead of a claim about an earlier moment (PATHREQ-5).
    if facts.programs_write_unasked {
        if facts.turn_holds_rules {
            lines.push(Line::new(
                t!(status_programs_write),
                t!(status_held_by_the_turn),
            ));
        } else if facts.trust.is_trusted(".") {
            lines.push(
                Line::new(
                    t!(status_programs_write),
                    t!(status_programs_write_anywhere),
                )
                .with_note(t!(status_programs_write_except)),
            );
        }
    }

    // Beside it for the same reason, and only where it is on: this is the other standing answer
    // that stops a prompt appearing, and the only record of it otherwise is a note at the top of
    // the session that has scrolled away by the time somebody wonders.
    if facts.auto_vetting {
        lines.push(
            Line::new(t!(status_vetting), t!(status_vetting_auto))
                .with_note(t!(status_vetting_where)),
        );
    }

    lines
}

/// What happens without anybody typing: the loop, the goal, the watches and the jobs.
fn running_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();

    // What is going to happen without anybody typing anything, which is the one thing about a
    // session that a person cannot read off the transcript.
    let several = facts.looping.len() > 1;
    for running in facts.looping {
        let pace = running.pace();
        let when = running.when(std::time::Instant::now());
        // The repeated line is the value, the way the goal line below carries its condition. A
        // panel saying only how often something happens leaves the reader to remember what they
        // set going, which is the half of it they cannot get from the pacing.
        //
        // Its first line only. A prompt is whatever somebody typed, a turn may arrange a loop over
        // one they pasted, and this panel spends one row per fact: the rest of a multi-line prompt
        // would land where the next fact goes.
        //
        // Told apart by number where there are several, the number `/loop stop` takes. A line
        // `/after` holds is sent later, not repeated, and the label says so.
        let label = match (running.once(), several) {
            (true, true) => t!(status_after_numbered, number = running.number()).to_string(),
            (true, false) => t!(status_after).to_string(),
            (false, true) => t!(status_loop_numbered, number = running.number()).to_string(),
            (false, false) => t!(status_loop).to_string(),
        };
        lines.push(
            Line::new(&label, crate::render::one_line(running.prompt()))
                .with_note(format!("{pace} · {when}")),
        );
    }

    // The other thing that happens without anybody typing. Beside the loop because it answers the
    // same question, and the count is the part a person cannot read off the transcript: a goal on
    // its ninth round is one turn from giving up.
    if let Some(goal) = facts.goal {
        let note = match goal.rounds() {
            _ if goal.is_paused() => t!(status_goal_paused).to_string(),
            0 => t!(goal_never_checked).to_string(),
            rounds => t!(status_goal_rounds, rounds = rounds, left = goal.left()),
        };
        let note = t!(
            status_goal_usage,
            note = note,
            elapsed = crate::indicator::format_elapsed(goal.elapsed(std::time::Instant::now())),
            tokens = tokens(goal.spent(facts.tokens + facts.tokens_in_flight))
        );
        lines.push(Line::new(t!(status_goal), goal.condition()).with_note(note));
    }

    // The third thing that happens without anybody typing, and the one with the least to go on
    // elsewhere: a fire arrives hours after the turn that armed it, so the line carries the
    // number a person ends the watch by, which turn armed it, and how long it has left.
    let now = std::time::Instant::now();
    for watch in facts.watches {
        lines.push(
            Line::new(
                &t!(status_watch, number = watch.number()),
                watch.path().to_string(),
            )
            .with_note(t!(
                status_watch_armed_by,
                turn = watch.armed_by(),
                left = crate::loops::spell(watch.left(now))
            )),
        );
    }

    // After the watches, being the other thing running that no line on the screen may still be
    // about: the block that started a job scrolls away while it runs (RUN-26). Every word is the
    // driver's or this end's clock, and none is anything the job printed.
    for (row, job) in &facts.jobs {
        let label = match job.delegate {
            Some(delegate) => t!(
                status_job_of_delegate,
                name = job.name.clone(),
                number = delegate.to_string()
            ),
            None => t!(status_job, name = job.name.clone()),
        };
        let origin = match job.moved_after {
            Some(after) => t!(status_job_moved, after = crate::loops::spell(after)),
            None => t!(status_job_started).to_string(),
        };
        lines.push(Line::new(&label, cut(&row.command, JOB_LINE)).with_note(t!(
            status_job_note,
            standing = job.standing(&row.outcome),
            origin = origin
        )));
    }

    lines
}

/// What the session has cost: turns and tokens, time, and the cache.
fn usage_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();

    lines.push(Line::new(
        t!(status_this_session),
        format!(
            "{} · {}",
            t!(count_turns, count = facts.turns),
            tokens(facts.tokens)
        ),
    ));

    // Under the turn and token counts, because it is the same question about the same session:
    // what did this cost. A session with no turn yet has as much to say here as any other, since an
    // aside asked before the first prompt waits on the model for however long it waits.
    //
    // The threshold is a whole second rather than any time at all, because the figures are rendered
    // by the same formatter the indicator uses and it floors to seconds: a part of 400ms would be
    // drawn as `0s`, which reads as "none" beside a note saying where the time went.
    if facts.timing.wall_ms >= 1_000 {
        lines.push(Line::new(
            t!(status_time),
            crate::indicator::format_elapsed(std::time::Duration::from_millis(
                facts.timing.wall_ms,
            )),
        ));
        // Only the parts that happened. A session that never ran a tool has nothing to say about
        // tool time, and a zero beside it invites the reader to work out whether it means "none" or
        // "not measured".
        for (millis, note) in [
            (facts.timing.inference_ms, t!(status_time_inference)),
            (facts.timing.tools_ms, t!(status_time_tools)),
            (facts.timing.stalled_ms, t!(status_time_stalled)),
            (facts.timing.overhead_ms(), t!(status_time_overhead)),
        ] {
            if millis >= 1_000 {
                lines.push(
                    Line::new(
                        "",
                        crate::indicator::format_elapsed(std::time::Duration::from_millis(millis)),
                    )
                    .with_note(note),
                );
            }
        }
    }

    // Beside the token count because it answers the question that count cannot. A prompt is the
    // same size whether the service read it or recognised it, and these two are the difference in
    // what that cost: a cached token is charged at a fraction of a fresh one, and a written one
    // above it.
    //
    // Drawn only where the service reported something. Both figures zero says a backend that
    // states nothing about a cache as readily as it says a turn whose cache missed, and a panel
    // cannot report the second without asserting it was not the first.
    //
    // The heading carries no figure of its own, which is where this departs from the time report
    // above it. Adding the two together would report the one number that says nothing: a read is
    // charged at a fraction of a fresh token and a write above it, so a turn that saved almost the
    // whole prompt and a turn that paid a premium on it come to the same sum.
    if let Some(cached) = facts.cached.filter(bravebot_aichat::protocol::Cached::any) {
        lines.push(Line::new(t!(status_cache), ""));
        for (count, note) in [
            (cached.read_tokens, t!(status_cache_read)),
            (cached.written_tokens, t!(status_cache_written)),
        ] {
            if count > 0 {
                lines.push(Line::new("", tokens(count)).with_note(note));
            }
        }
        // The setting, not a reading: no reply says how long a service kept a prefix. Beside the
        // figures because a read of zero after a pause reads differently once the lifetime is known.
        lines.push(
            Line::new(
                "",
                match facts.config.prompt_cache_ttl {
                    Some(bravebot_config::CacheTtl::FiveMinutes) => {
                        t!(status_cache_lifetime_five_minutes)
                    }
                    Some(bravebot_config::CacheTtl::OneHour) => {
                        t!(status_cache_lifetime_one_hour)
                    }
                    None => t!(status_cache_lifetime_service_default),
                },
            )
            .with_note(t!(status_cache_lifetime)),
        );
    }

    lines
}

/// The other standing answer that stops a prompt appearing, and the one a person is least able
/// to account for from memory: it was given in a session that has ended, possibly last week, and
/// nothing on the screen since has mentioned it. So each line says which of the two lifetimes it
/// is carrying, and the report says where the file is, since deleting a line from it is the way
/// back. Said only where there is something to say: a session that has remembered nothing learns
/// nothing from a line about it, and the programs line above already says every run is asked.
fn remembered_lines(facts: &Facts<'_>, record: Remembered<'_>) -> Vec<Line> {
    let mut lines = Vec::new();
    // Each scope as the one item it was answered as, before the lines: it is the wider grant, so it
    // is the one that must not be cut off by the length limit below.
    let entries: Vec<(String, &str)> = record
        .lines
        .scopes()
        .map(|entry| {
            (
                crate::confirm::scope_name(&entry.scope),
                entry.answered_in.as_str(),
            )
        })
        .chain(
            record
                .lines
                .iter()
                .map(|entry| (entry.line.display(), entry.answered_in.as_str())),
        )
        .collect();
    lines.push(
        Line::new(
            t!(status_remembered),
            t!(count_remembered, count = entries.len()),
        )
        .with_note(t!(status_remembered_note)),
    );
    for (shown, answered_in) in entries.iter().take(MAX_REMEMBERED) {
        lines.push(Line::new("", shown.as_str()).with_note(
            match *answered_in == facts.session_id {
                true => t!(status_remembered_this_session),
                false => t!(status_remembered_earlier),
            },
        ));
    }
    if entries.len() > MAX_REMEMBERED {
        // How many of each was left out, not just how many: the two lifetimes are the point,
        // and a person deciding what to delete cannot tell from a bare count which answers
        // they are still carrying from another session.
        let left_out = &entries[MAX_REMEMBERED..];
        let earlier = left_out
            .iter()
            .filter(|(_, answered_in)| *answered_in != facts.session_id)
            .count();
        lines.push(Line::new(
            "",
            t!(
                status_remembered_and_more,
                count = left_out.len(),
                earlier = earlier
            ),
        ));
    }
    lines.push(Line::new(
        "",
        t!(status_remembered_where, path = record.path.display()),
    ));

    lines
}

/// The third standing answer, and the one that changes a plan a person is shown: a command's
/// plan carries a row they did not see requested. The rows are `/reach`'s own, so the number on
/// each is the one `/reach remove` takes.
fn grant_lines(facts: &Facts<'_>) -> Vec<Line> {
    let mut lines = Vec::new();
    if !facts.reach.is_empty() {
        lines.extend(rows(
            Line::new(
                t!(status_reach),
                t!(count_reach_grants, count = facts.reach.len()),
            )
            .with_note(t!(status_reach_note)),
            facts.reach,
        ));
    }

    if !facts.reached_directories.is_empty() {
        lines.extend(rows(
            Line::new(
                t!(status_reached_directories),
                t!(
                    count_reached_directories,
                    count = facts.reached_directories.len()
                ),
            )
            .with_note(t!(status_reached_directories_note)),
            facts.reached_directories,
        ));
    }

    if !facts.requested_paths.is_empty() {
        lines.extend(rows(
            Line::new(
                t!(status_requested_paths),
                t!(count_requested_paths, count = facts.requested_paths.len()),
            )
            .with_note(t!(status_requested_paths_note)),
            facts.requested_paths,
        ));
    }
    lines
}

/// What each started local server's confinement grants: a heading naming it, a line each for what
/// its policy says of the network, of subprocesses and of where it starts, then one line for each
/// path its backend was handed and each path that policy was resolved without (SERVERS-20). What a
/// row allows is its label, so a path that fills the width of the screen is still one line.
///
/// Nothing for a session that started no local server, which is the session this screen has always
/// described. Every row rather than a count, since this is what a refusal from inside a server is
/// checked against, and a count cannot say whether the path it refused was granted. The rows are
/// the policy's own and no word a server sent is among them. A path is drawn in full, as a server
/// names it in a refusal and as the prompts that name one draw it, so one that holds a control
/// character, or a character that draws as nothing, is not a row that reads as another.
fn server_policy_lines(servers: &[(String, EffectivePolicy)]) -> Vec<Line> {
    let mut lines = Vec::new();
    for (alias, policy) in servers {
        lines.push(Line::new(
            t!(status_server_policy),
            bravebot_agent::mcp::shown(alias),
        ));
        lines.extend(
            policy
                .described()
                .into_iter()
                .map(|(label, value)| Line::new(&label, value)),
        );
    }
    lines
}

/// A heading line, then one line under it for each row.
fn rows(heading: Line, rows: &[String]) -> Vec<Line> {
    let mut lines = vec![heading];
    lines.extend(rows.iter().map(|row| Line::new("", row.as_str())));
    lines
}

/// What can be said about the tier before a request has settled it.
///
/// The configuration and nothing else, which is why the premium wording stops short of claiming the
/// key will be accepted. The premium host may still refuse it, and whether it does is settled by a
/// request rather than by looking.
///
/// Shared with the opening screen rather than written twice, so the line drawn at startup and the
/// line `/status` shows an hour later cannot drift apart.
pub fn configured_tier(config: &Config) -> &'static str {
    match config.premium_key {
        Some(_) => t!(status_premium_available),
        None => t!(status_no_subscription),
    }
}

/// What the info panel and `/status` call the model whose request name is `model`, where the
/// configuration names it.
///
/// The name the `/model` picker gives its row ([`bravebot_config::bedrock::Entry::display_name`]),
/// so a person who chose "Sonnet" there is not shown an inference-profile ARN for it. A tier word
/// gets the service after it, since no heading over it says whose account answers and the Brave
/// roster has a Sonnet of its own; a name a `provider` block chose is the person's own words and
/// stands alone. `None` for a model no configured account offers, or one the configuration gave no
/// name other than its request name, which is drawn by that.
pub fn model_label(config: &Config, model: &str) -> Option<String> {
    let entry = config.bedrock_entry(model)?;
    let label = match (&entry.name, entry.tier) {
        (None, Some(_)) => t!(model_label_bedrock, name = entry.display_name()),
        _ => entry.display_name().to_string(),
    };
    (label != model).then_some(label)
}

/// The request names and labels of every model a configured account offers, for a session to draw
/// its chosen model by without holding the configuration.
pub fn model_labels(config: &Config) -> Vec<(String, String)> {
    config
        .bedrock_entries()
        .filter_map(|entry| Some((entry.id.clone(), model_label(config, &entry.id)?)))
        .collect()
}

/// The `/status` lines for the model in force: its name, and under it the request name where the
/// two differ, because that is what an error from the service quotes.
fn model_lines(config: &Config, model: &str, note: String) -> Vec<Line> {
    match model_label(config, model) {
        Some(label) => vec![
            Line::new(t!(status_model), label).with_note(note),
            Line::new("", t!(status_model_id, id = model)),
        ],
        None => vec![Line::new(t!(status_model), model).with_note(note)],
    }
}

/// Which deployment an endpoint names, without naming the host.
///
/// Matched on the host rather than parsed, because the answer wanted is one of three words and a
/// URL parser would still leave the mapping to be written. An unrecognised host is reported as
/// custom rather than guessed at.
fn environment(endpoint: &str) -> &'static str {
    if endpoint.contains("127.0.0.1") || endpoint.contains("localhost") {
        t!(environment_local)
    } else if endpoint.contains(".brave.software") {
        t!(environment_dev)
    } else if endpoint.contains(".brave.com") {
        t!(environment_prod)
    } else {
        t!(environment_custom)
    }
}

/// `path` as this screen writes it: relative to `root` where it is inside it, abbreviated
/// otherwise.
///
/// The root itself reads as `.`, which is how the trust map's rules above are written, so a vouched
/// entry given at the root and a rule about the root are spelled the same way. A directory the
/// person opened by name is not under the root and has no relative spelling, so it keeps its
/// absolute one.
fn within(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rest) if rest.as_os_str().is_empty() => ".".to_string(),
        Ok(rest) => rest.display().to_string(),
        Err(_) => abbreviate(path),
    }
}

/// A path with the home directory written as `~`, which is shorter and less personal.
pub(crate) fn abbreviate(path: &Path) -> String {
    abbreviate_against(path, std::env::var_os("HOME").as_deref())
}

/// The logic of [`abbreviate`], with `HOME` taken as a value rather than read from the process
/// environment, so a test can give it one without mutating a variable every thread of the test
/// binary shares.
///
/// The prefix is stripped by path component, not by text: a directory whose name merely starts
/// with the home directory's is not under it. `home` empty or the root abbreviates nothing,
/// since every absolute path would otherwise read as `~`.
fn abbreviate_against(path: &Path, home: Option<&std::ffi::OsStr>) -> String {
    let shown = path.display().to_string();
    let Some(home) = home else {
        return shown;
    };
    let home = Path::new(home);
    if home.as_os_str().is_empty() || home == Path::new("/") {
        return shown;
    }
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => shown,
    }
}

/// Tokens, in the units a person reads them in.
pub(crate) fn tokens(count: u64) -> String {
    if count < 1_000 {
        return t!(count_tokens, count = count);
    }
    // The one fraction the interface shows, so the one place a language that does not write a
    // point between a whole number and its fraction has anything to say about it.
    let thousands =
        format!("{:.1}", count as f64 / 1_000.0).replace('.', t!(number_decimal_separator));
    t!(count_tokens_thousands, thousands = thousands)
}

/// How each server's tools stand, one clause for each.
pub(crate) fn offering(servers: &[(String, bravebot_agent::mcp::Offering)]) -> String {
    servers
        .iter()
        .map(|(alias, offering)| match offering {
            bravebot_agent::mcp::Offering::Unasked => t!(status_mcp_servers_unread, alias = alias),
            bravebot_agent::mcp::Offering::Tools(count) => {
                t!(status_mcp_servers_tools, alias = alias, count = *count)
            }
            bravebot_agent::mcp::Offering::Declined => {
                t!(status_mcp_servers_declined, alias = alias)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::servers::{Access, PolicyEgress};
    use bravebot_config::DEFAULT_MODEL;

    /// A configuration whose premium host is `premium` where one is named, with a key stored for it
    /// as `bravebot auth login brave` leaves one. Without `premium` there is no key, so the build
    /// has no premium tier to report.
    fn config_for(endpoint: &str, premium: Option<&str>) -> Config {
        let mut config = Config::from_lookup(|key| match key {
            "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.to_string()),
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT" => premium.map(str::to_string),
            _ => None,
        })
        .expect("config");
        config.premium_key = premium.map(|_| bravebot_config::Secret::new("a-premium-key"));
        config
    }

    /// Leaked once so a `Facts` built by the helper can borrow it for the test's lifetime.
    static NOTHING_VOUCHED: std::sync::LazyLock<TrustedPrograms> =
        std::sync::LazyLock::new(TrustedPrograms::new);

    /// A session that started no MCP server, which is what every test but the servers' own is.
    static NO_SERVERS: crate::state::Servers = crate::state::Servers {
        started: Vec::new(),
        confined: false,
        notes: Vec::new(),
        session: None,
        disabled: Vec::new(),
    };

    fn trusting() -> TrustStore {
        let mut trust = TrustStore::new("/work");
        trust.trust(".");
        trust
    }

    /// SANDBOX-22: the line names `strict`, `read-only` and `off` and says nothing for
    /// `workspace-write`, and the report draws the mode its facts carry. The regressions it rejects
    /// are `off` shown as the platform's level alone, which reads as a session whose programs are
    /// confined, `read-only` shown as `workspace-write`, which hides that a program cannot write the
    /// project, and a report that reads the mode start-up settled and so ignores `/sandbox`.
    #[test]
    fn the_confinement_line_names_the_mode_unless_it_is_workspace_write() {
        use bravebot_sandbox::SandboxMode;
        let level = "kernel-enforced";
        assert_eq!(
            with_the_sandbox_mode(level, SandboxMode::WorkspaceWrite),
            level
        );
        let strict = with_the_sandbox_mode(level, SandboxMode::Strict);
        assert!(
            strict.starts_with(level) && strict.contains("strict"),
            "{strict}"
        );
        let read_only = with_the_sandbox_mode(level, SandboxMode::ReadOnly);
        assert!(
            read_only.starts_with(level) && read_only.contains("read-only"),
            "{read_only}"
        );
        let off = with_the_sandbox_mode(level, SandboxMode::Off);
        assert!(off.starts_with(level) && off.contains("off"), "{off}");
        assert_ne!(off, strict);
        assert_ne!(read_only, strict);

        let config = config_for("https://ai-chat.bsg.brave.com", None);
        let trust = TrustStore::new("/work");
        for mode in [SandboxMode::Strict, SandboxMode::ReadOnly, SandboxMode::Off] {
            let mut facts = facts(&config, &trust);
            facts.sandbox_mode = mode;
            let shown = report(&facts)
                .lines
                .into_iter()
                .find(|line| line.label.trim() == t!(status_confinement))
                .expect("the confinement is on the report")
                .value;
            assert!(shown.contains(mode.name()), "{mode}: {shown}");
        }
    }

    /// SANDBOX-3: `/status` says a session's programs reach every unix socket that has a path where
    /// the kernel leaves them open, under `workspace-write`, with why beside it. The regressions it
    /// rejects: no line on such a kernel, a line where the kernel governs the sockets, and a line
    /// under `strict` or `read-only`, which refuse the stage, or under `off`, which confines nothing.
    #[test]
    fn reachable_sockets_with_a_path_are_reported_under_workspace_write_alone() {
        use bravebot_sandbox::SandboxMode;
        let config = config_for("https://ai-chat.bsg.brave.com", None);
        let trust = TrustStore::new("/work");
        let shown = |mode, reachable| {
            let mut facts = facts(&config, &trust);
            facts.sandbox_mode = mode;
            facts.sockets_with_a_path_reachable = reachable;
            report(&facts)
                .lines
                .into_iter()
                .find(|line| line.label.trim() == t!(status_path_sockets))
        };

        let mode = SandboxMode::WorkspaceWrite;
        {
            let line = shown(mode, true)
                .unwrap_or_else(|| panic!("{mode}: the sockets are on the report"));
            assert_eq!(line.value, t!(status_path_sockets_reachable), "{mode}");
            assert_eq!(line.note, t!(status_path_sockets_why), "{mode}");
        }
        for (mode, reachable) in [
            (SandboxMode::WorkspaceWrite, false),
            (SandboxMode::ReadOnly, true),
            (SandboxMode::ReadOnly, false),
            (SandboxMode::Strict, true),
            (SandboxMode::Off, true),
        ] {
            assert_eq!(shown(mode, reachable), None, "{mode} {reachable}");
        }
    }

    /// Facts with nothing vouched for, which is what a session that has not been asked looks like.
    /// Tests about the programs line build their own list and set it.
    fn facts<'a>(config: &'a Config, trust: &'a TrustStore) -> Facts<'a> {
        Facts {
            session_name: "the parser bug",
            session_id: "1787860306-65099",
            directory: Path::new("/tmp/project"),
            added_directories: &[],
            // No scratch directory, which is what a session on a machine that could not give it
            // one looks like. The test about the line sets it itself.
            scratch: None,
            // No checkouts, on the same footing.
            checkouts: &[],
            model: None,
            agent: None,
            agent_by_setting: false,
            effort: None,
            model_reads_effort: true,
            account_effort: None,
            // Nothing observed, which is what a session looks like before its first turn. Tests
            // about the tier and the served model set these themselves.
            served_model: None,
            substituted_model: None,
            premium: None,
            theme: "brave",
            config,
            confinement: "kernel-enforced",
            // Governed, which is what every platform but a Linux kernel below Landlock ABI 9
            // reports. The test about the line sets it itself.
            sockets_with_a_path_reachable: false,
            sandbox_mode: bravebot_sandbox::SandboxMode::WorkspaceWrite,
            servers: &NO_SERVERS,
            // Asking, which is what every session does unless somebody changed it. The tests about
            // the line set this themselves.
            permission_mode: bravebot_agent::PermissionMode::Ask,
            began_in_bypass: false,
            programs_write_unasked: false,
            auto_vetting: false,
            turns: 4,
            tokens: 12_400,
            tokens_in_flight: 0,
            // Nothing measured, which is what a session looks like before its first turn. Tests
            // about the time report set this themselves.
            timing: bravebot_agent::timing::Timing::default(),
            // Nothing observed about a cache, on the same footing. Tests about the cache lines set
            // this themselves.
            cached: None,
            trust,
            programs: &NOTHING_VOUCHED,
            turn_holds_rules: false,
            // Nothing repeating, which is every session that has not been asked to. Tests about
            // the loop line set this themselves.
            looping: &[],
            // Nothing to work towards, on the same footing.
            goal: None,
            // Nothing watched, on the same footing. The test about the watch lines builds its
            // own registry and sets it.
            watches: &[],
            // No job running, on the same footing.
            jobs: Vec::new(),
            // Nothing remembered past a session, which is what a fresh directory looks like. Tests
            // about that line build their own record and set it.
            remembered: None,
            // No reach remembered for any command, on the same footing.
            reach: &[],
            requested_paths: &[],
            reached_directories: &[],
            // No answer about the directory kept past a session, on the same footing.
            kept_trust: None,
        }
    }

    /// A session that has run one turn, holding the pair a finished turn records: the name the
    /// service was actually sent, and whether that name and a reply's are drawn from one roster.
    /// Both come from the backend, asked as `app` asks them, so a test here drives the derivation a
    /// real turn drives rather than a bool it picked for itself.
    fn after_a_turn(config: &Config, chosen: Option<&str>, served: &str) -> crate::state::Session {
        let mut session = crate::state::Session::new("kernel-enforced");
        if let Some(model) = chosen {
            session.choose_model(model.to_string(), config);
        }
        let in_force = chosen.unwrap_or(&config.default_model);
        session.served(
            bravebot_agent::backend::Backend::name_as_asked(config, in_force),
            served,
            false,
            bravebot_agent::backend::Backend::reports_the_model_it_was_asked_for(config, in_force),
        );
        session
    }

    /// Point the panel's three model fields at one session, as `app` does.
    fn as_of<'a>(facts: &mut Facts<'a>, session: &'a crate::state::Session) {
        facts.model = session.model();
        facts.served_model = session.served_model();
        facts.substituted_model = session.substituted_model();
    }

    /// What is going to happen without anybody typing is the one thing about a session that
    /// cannot be read back off the transcript, and a watch is the sharpest case: a fire arrives
    /// hours after the turn that armed it, so the line has to carry both the number a person ends
    /// it by and the turn they asked for it in.
    #[test]
    fn the_report_lists_every_live_watch_with_the_turn_that_armed_it() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut watches = watch::Watches::new();
        watches
            .arm(
                "notes/plan.md".to_string(),
                std::path::PathBuf::from("/work"),
                3,
                watch::Looked::Saw("first".to_string()),
                std::time::Instant::now(),
            )
            .expect("a watch");

        let mut facts = facts(&config, &trust);
        facts.watches = watches.live();
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_watch, number = 1))
            .expect("the watch is on the report");
        assert_eq!(line.value, "notes/plan.md");
        assert!(line.note.contains('3'), "{:?}", line.note);
    }

    /// The time on the line is what the watch has left of its week, which is when it stops
    /// itself. A watch armed ninety seconds ago has just under seven days left, where a line that
    /// reported how long the watch had been on would show a minute and a half.
    #[test]
    fn the_report_says_how_long_a_watch_has_left() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let armed_ninety_seconds_ago = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(90))
            .expect("the clock has run for ninety seconds");
        let mut watches = watch::Watches::new();
        watches
            .arm(
                "notes/plan.md".to_string(),
                std::path::PathBuf::from("/work"),
                3,
                watch::Looked::Saw("first".to_string()),
                armed_ninety_seconds_ago,
            )
            .expect("a watch");

        let mut facts = facts(&config, &trust);
        facts.watches = watches.live();
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_watch, number = 1))
            .expect("the watch is on the report");
        assert!(line.note.contains("6d 23h left"), "{:?}", line.note);
    }

    /// A line saying a thing is not happening is a line on every report for the sake of the few
    /// where it is.
    #[test]
    fn a_session_watching_nothing_says_nothing_about_watches() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let report = report(&facts(&config, &trust));

        assert!(
            !report
                .lines
                .iter()
                .any(|line| line.label.trim() == t!(status_watch, number = 1)),
            "a session with no watch reported one"
        );
    }

    /// A job runs while nobody watches it and its block scrolls away, so the report names each
    /// one, how it stands, and how it came to be in the background.
    #[test]
    fn the_report_lists_every_job_and_how_it_came_to_run_in_the_background() {
        use bravebot_agent::report::JobEvent;
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut session = crate::state::Session::new("none");
        session.job(JobEvent::Started {
            name: "job:1".to_string(),
            line: format!("cargo test {}", "--workspace ".repeat(10)),
            moved_after: Some(std::time::Duration::from_secs(72)),
            stop: bravebot_core::cancel::JobStop::new(),
        });
        session.job(JobEvent::Started {
            name: "job:2".to_string(),
            line: "sleep 600".to_string(),
            moved_after: None,
            stop: bravebot_core::cancel::JobStop::new(),
        });
        session.job(JobEvent::Ended {
            name: "job:2".to_string(),
            outcome: bravebot_agent::report::Outcome::Failed {
                detail: "exit 3".to_string(),
                confinement: None,
            },
        });

        let mut facts = facts(&config, &trust);
        facts.jobs = session.jobs().collect();
        let report = report(&facts);
        let line = |name: &str| {
            report
                .lines
                .iter()
                .find(|line| line.label.trim() == t!(status_job, name = name))
                .unwrap_or_else(|| panic!("{name} is not on the report: {report:?}"))
                .clone()
        };

        let moved = line("job:1");
        assert!(moved.value.starts_with("cargo test --workspace"));
        assert!(moved.value.ends_with('…'), "{:?}", moved.value);
        assert_eq!(moved.value.chars().count(), JOB_LINE);
        // The line started 72 seconds before it was moved, and its clock counts those too.
        assert!(moved.note.contains("running 1m 12s"), "{:?}", moved.note);
        assert!(
            moved
                .note
                .contains("moved from the foreground after 1m 12s"),
            "{:?}",
            moved.note
        );

        let ended = line("job:2");
        assert_eq!(ended.value, "sleep 600");
        assert!(ended.note.contains("exit 3"), "{:?}", ended.note);
        assert!(
            ended.note.contains("started in the background"),
            "{:?}",
            ended.note
        );
    }

    /// The report gives the line as it was approved: only what cannot sit on one row is folded,
    /// and a wide character counts as the two columns it takes.
    #[test]
    fn a_job_line_keeps_its_spacing_and_is_cut_by_the_columns_it_takes() {
        assert_eq!(cut("printf \"a    b\"", JOB_LINE), "printf \"a    b\"");
        assert_eq!(
            cut("cd src &&\n\tmake\tall", JOB_LINE),
            "cd src && make all"
        );
        let wide = cut(&"漢".repeat(JOB_LINE), JOB_LINE);
        assert!(wide.ends_with('…'), "{wide:?}");
        assert!(crate::wrap::display_width(&wide) <= JOB_LINE, "{wide:?}");
    }

    /// A delegate numbers its jobs from one as the turn does, so a line that did not say whose
    /// job it was would show two `job:1` lines as the same job.
    #[test]
    fn the_report_says_which_delegate_a_job_belongs_to() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut session = crate::state::Session::new("none");
        let id = bravebot_agent::report::DelegateId::nth(1);
        session.delegate_started(bravebot_agent::report::Delegation {
            stop: Default::default(),
            id,
            kind: "reader".to_string(),
            task: "find the parser".to_string(),
        });
        session.reporting_for(Some(id));
        session.job(bravebot_agent::report::JobEvent::Started {
            name: "job:1".to_string(),
            line: "sleep 600".to_string(),
            moved_after: None,
            stop: bravebot_core::cancel::JobStop::new(),
        });

        let mut facts = facts(&config, &trust);
        facts.jobs = session.jobs().collect();
        let report = report(&facts);
        let label = t!(
            status_job_of_delegate,
            name = "job:1",
            number = id.to_string()
        );
        assert!(
            report.lines.iter().any(|line| line.label.trim() == label),
            "{report:?}"
        );
    }

    /// For the reason a session watching nothing says nothing about watches.
    #[test]
    fn a_session_with_no_job_says_nothing_about_jobs() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let report = report(&facts(&config, &trust));
        assert!(
            !report
                .lines
                .iter()
                .any(|line| line.label.trim().starts_with("Background")),
            "a session with no job reported one: {report:?}"
        );
    }

    /// The other thing that happens without anybody typing. The count is the part a person
    /// cannot read off the transcript: a goal on its ninth round is one turn from giving up.
    #[test]
    fn the_report_says_what_the_session_is_working_towards_and_how_many_rounds_are_left() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut goal = crate::goals::Running::begin("cargo test exits 0".to_string(), 0, false);
        goal.not_met("nothing above runs the tests".to_string(), 1);

        let mut facts = facts(&config, &trust);
        facts.goal = Some(&goal);
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_goal))
            .expect("the goal is on the report");
        assert!(
            line.value.contains("cargo test exits 0"),
            "{:?}",
            line.value
        );
        assert!(
            line.note.contains('1'),
            "the report did not say how many rounds had gone: {}",
            line.note
        );
    }

    /// A person deciding whether a goal is converging wants the time and the spend as much as the
    /// round count, and the spend is counted from when the goal was armed.
    #[test]
    fn the_report_says_how_long_a_goal_has_run_and_what_it_has_spent() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut facts = facts(&config, &trust);
        // The session had already spent part of its total before the goal was set.
        let goal = crate::goals::Running::begin("cargo test exits 0".to_string(), 10_000, false);
        facts.goal = Some(&goal);
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_goal))
            .expect("the goal is on the report");
        assert!(line.note.ends_with(" · 2.4k tokens"), "{:?}", line.note);
    }

    /// `/status` is answered at once during a turn, and a turn is charged to the session only when
    /// it ends, so the goal's spend adds what the running turn has spent.
    #[test]
    fn the_goal_spend_includes_the_turn_in_flight() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut facts = facts(&config, &trust);
        let goal = crate::goals::Running::begin("cargo test exits 0".to_string(), 10_000, false);
        facts.goal = Some(&goal);
        facts.tokens_in_flight = 500;
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_goal))
            .expect("the goal is on the report");
        assert!(line.note.ends_with(" · 2.9k tokens"), "{:?}", line.note);
    }

    const SONNET_ARN: &str =
        "arn:aws:bedrock:us-west-2:1:application-inference-profile/pzko74tz7aq5";
    const HAIKU_ARN: &str = "arn:aws:bedrock:us-west-2:1:application-inference-profile/h4ik0";

    fn bedrock_tiers() -> Config {
        Config::from_lookup(|key| match key {
            "BRAVEBOT_USE_BEDROCK" => Some("1".into()),
            "AWS_REGION" => Some("us-west-2".into()),
            "ANTHROPIC_DEFAULT_SONNET_MODEL" => Some(SONNET_ARN.into()),
            "ANTHROPIC_DEFAULT_HAIKU_MODEL" => Some(HAIKU_ARN.into()),
            _ => None,
        })
        .expect("config")
    }

    /// PANEL-9. A tier set to an ARN is shown by the name the `/model` picker gave its row, with
    /// the ARN kept under it because the service's errors quote that.
    ///
    /// Rejects: the ARN on the model line, the label of another tier, a label on a model no account
    /// offers, and a second line under a model that is already shown by its own name.
    #[test]
    fn the_model_line_names_a_tier_and_keeps_its_arn_beneath() {
        let config = bedrock_tiers();
        let trust = trusting();
        let model_rows = |model: Option<&str>| {
            let mut facts = facts(&config, &trust);
            facts.model = model;
            let report = report(&facts);
            let at = report
                .lines
                .iter()
                .position(|line| line.label.trim() == t!(status_model))
                .expect("a model line");
            (
                report.lines[at].value.clone(),
                report.lines[at].note.clone(),
                report.lines[at + 1].clone(),
            )
        };

        let (value, note, below) = model_rows(Some(SONNET_ARN));
        assert_eq!(value, "Sonnet (Bedrock)");
        assert_eq!(note, t!(status_model_chosen));
        assert_eq!(below.label, "");
        assert_eq!(below.value, t!(status_model_id, id = SONNET_ARN));

        let (value, _, below) = model_rows(Some(HAIKU_ARN));
        assert_eq!(value, "Haiku (Bedrock)");
        assert_eq!(below.value, t!(status_model_id, id = HAIKU_ARN));

        // No model chosen: the configured default is the strongest tier set, here Sonnet.
        let (value, note, below) = model_rows(None);
        assert_eq!(value, "Sonnet (Bedrock)");
        assert_eq!(note, t!(status_model_default));
        assert_eq!(below.value, t!(status_model_id, id = SONNET_ARN));

        let (value, _, below) = model_rows(Some("claude-3-sonnet"));
        assert_eq!(value, "claude-3-sonnet");
        assert_ne!(
            below.value,
            t!(status_model_id, id = "claude-3-sonnet"),
            "a model no account offers has no second name to quote"
        );
    }

    /// A model a `provider` block named is shown by the name the block gave it, as written: that
    /// name is the person's own words and already says which service answers. One it gave no name
    /// is drawn by its id, with no second line quoting the same id.
    #[test]
    fn a_name_a_provider_block_chose_is_shown_without_a_service_added() {
        let mut config = config_for("http://127.0.0.1:1", None);
        config.bedrock = Some(
            bravebot_config::bedrock::Bedrock::from_provider(
                "us-west-2".to_string(),
                None,
                vec![bravebot_config::bedrock::Entry {
                    tier: None,
                    id: "openai.gpt-5.6-sol".to_string(),
                    name: Some("GPT-5.6 Sol (Bedrock)".to_string()),
                    context_window: None,
                    output_limit: None,
                }],
            )
            .expect("a valid region"),
        );
        assert_eq!(
            model_label(&config, "openai.gpt-5.6-sol").as_deref(),
            Some("GPT-5.6 Sol (Bedrock)")
        );

        config.bedrock = Some(
            bravebot_config::bedrock::Bedrock::from_provider(
                "us-west-2".to_string(),
                None,
                vec![bravebot_config::bedrock::Entry {
                    tier: None,
                    id: "openai.gpt-5.6-sol".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: None,
                }],
            )
            .expect("a valid region"),
        );
        assert_eq!(
            model_label(&config, "openai.gpt-5.6-sol"),
            None,
            "a model with no name but its id has no label to put before the id"
        );
    }

    /// CLI-17. The note saying a session works under a definition scrolls away, and the input box
    /// looks the same either way, so the report is where a person sees which definition every turn
    /// is addressed to. A session started under none shows no such line. A model the definition
    /// names is reported as the definition's, since `/model` did not choose it.
    #[test]
    fn the_report_names_the_definition_every_turn_is_addressed_to() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let labelled = |report: &Report, label: &str| {
            report
                .lines
                .iter()
                .find(|line| line.label.trim() == label)
                .map(|line| (line.value.clone(), line.note.clone()))
        };
        let agent_line = |report: &Report| labelled(report, t!(status_agent));
        let model_line = |report: &Report| labelled(report, t!(status_model));

        assert_eq!(agent_line(&report(&facts(&config, &trust))), None);

        let naming_no_model = crate::state::Addressed {
            name: "rule-reviewer".to_string(),
            model: None,
        };
        let naming_a_model = crate::state::Addressed {
            name: "rule-reviewer".to_string(),
            model: Some("the-definitions-model".to_string()),
        };
        let mut facts = facts(&config, &trust);
        facts.model = Some("the-sessions-model");
        facts.agent = Some(&naming_no_model);
        assert_eq!(
            agent_line(&report(&facts)),
            Some((
                "rule-reviewer".to_string(),
                t!(status_agent_every_turn).to_string()
            ))
        );
        assert_eq!(
            model_line(&report(&facts)),
            Some((
                "the-sessions-model".to_string(),
                t!(status_model_chosen).to_string()
            ))
        );

        facts.agent_by_setting = true;
        assert_eq!(
            agent_line(&report(&facts)),
            Some((
                "rule-reviewer".to_string(),
                t!(status_agent_by_setting).to_string()
            )),
            "a definition the setting chose was said to be named with --agent"
        );
        facts.agent_by_setting = false;

        facts.model = Some("the-definitions-model");
        facts.agent = Some(&naming_a_model);
        assert_eq!(
            model_line(&report(&facts)),
            Some((
                "the-definitions-model".to_string(),
                t!(status_model_definitions, definition = "rule-reviewer").to_string()
            )),
            "the definition's model was reported as one /model chose"
        );
    }

    /// A held goal is still the session's, so it is on the report, and the report says it is held
    /// rather than counting rounds as though the next turn would be judged.
    #[test]
    fn the_report_says_a_goal_is_paused() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut goal = crate::goals::Running::begin("cargo test exits 0".to_string(), 0, false);
        goal.not_met("nothing above runs the tests".to_string(), 1);
        goal.pause();

        let mut facts = facts(&config, &trust);
        facts.goal = Some(&goal);
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_goal))
            .expect("a paused goal is on the report");
        assert!(
            line.value.contains("cargo test exits 0"),
            "{:?}",
            line.value
        );
        assert!(
            line.note.starts_with(t!(status_goal_paused)),
            "{:?}",
            line.note
        );
    }

    #[test]
    fn a_session_with_no_goal_does_not_mention_one() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let report = report(&facts(&config, &trust));
        assert!(
            !report
                .lines
                .iter()
                .any(|line| line.label.trim() == t!(status_goal))
        );
    }

    /// The one thing about a session that cannot be read off the transcript: what is going to
    /// happen next without anybody typing anything.
    #[test]
    fn the_report_says_what_is_repeating_and_when_it_is_next_due() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut running =
            crate::loops::Running::begin(crate::loops::request("5m check the deploy"));
        running.dispatching();
        running.ended(None, std::time::Instant::now());

        let mut facts = facts(&config, &trust);
        facts.looping = std::slice::from_ref(&running);
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_loop))
            .expect("the loop is reported");
        assert_eq!(
            line.value, "check the deploy",
            "the report did not say what repeats"
        );
        assert!(
            line.note.contains(&t!(status_loop_every, every = "5m")),
            "{}",
            line.note
        );
        // The wording that introduces the countdown rather than the number, which moves while the
        // test runs.
        assert!(
            line.note.contains(t!(status_loop_next, next = "").trim()),
            "{}",
            line.note
        );
    }

    /// LOOP-13: with several loops each has its own row under its number, the one `/loop stop`
    /// takes, and a single loop's row keeps the plain label.
    #[test]
    fn the_report_has_a_row_per_loop_under_its_number() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut loops = crate::loops::Loops::default();
        loops.start(crate::loops::Running::begin(crate::loops::request(
            "5m check the deploy",
        )));
        loops.start(crate::loops::Running::begin(crate::loops::request(
            "1h check the pull request",
        )));

        let mut facts = facts(&config, &trust);
        facts.looping = loops.as_slice();
        let report = report(&facts);
        let rows: Vec<(String, &str)> = report
            .lines
            .iter()
            .filter(|line| line.label.trim().starts_with(t!(status_loop).trim()))
            .map(|line| (line.label.trim().to_string(), line.value.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    t!(status_loop_numbered, number = 1).to_string(),
                    "check the deploy"
                ),
                (
                    t!(status_loop_numbered, number = 2).to_string(),
                    "check the pull request"
                ),
            ]
        );
    }

    /// An `/after` line is sent later and not repeated (LOOP-18), so the panel labels it that way.
    /// A wrong implementation lists it under the loop's label.
    #[test]
    fn the_report_lists_an_after_line_as_sent_later_and_not_as_a_loop() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let running = crate::loops::Running::after(
            crate::loops::request_after("5m tail the log"),
            std::time::Instant::now(),
        );

        let mut facts = facts(&config, &trust);
        facts.looping = std::slice::from_ref(&running);
        let report = report(&facts);

        assert!(
            !report
                .lines
                .iter()
                .any(|line| line.label.trim() == t!(status_loop)),
            "an after line was listed as a loop"
        );
        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_after))
            .expect("the after line is reported");
        assert_eq!(line.value, "tail the log");
    }

    /// A loop a turn arranged is always self-paced, so its pacing says nothing about the work.
    /// The line is the only thing on the row that can, and it is the one reached by pasting.
    #[test]
    fn the_report_says_what_a_self_paced_loop_is_repeating() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let running = crate::loops::Running::armed(
            "tell me when a.txt changes\nand say what changed".to_string(),
            crate::loops::Wakeup::asked(900, false),
            std::time::Instant::now(),
        );

        let mut facts = facts(&config, &trust);
        facts.looping = std::slice::from_ref(&running);
        let report = report(&facts);

        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_loop))
            .expect("the loop is reported");
        // The first line and no more: the panel spends one row per fact, so the rest of a
        // multi-line prompt would land where the next fact goes.
        assert_eq!(line.value, "tell me when a.txt changes");
        assert!(
            line.note.contains(t!(status_loop_self_paced)),
            "{}",
            line.note
        );
        assert!(
            line.note.contains(t!(status_loop_next, next = "").trim()),
            "{}",
            line.note
        );
    }

    /// A session nobody asked to repeat anything says nothing about loops, rather than carrying a
    /// line about a thing that is not happening.
    #[test]
    fn a_session_with_no_loop_does_not_mention_one() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let report = report(&facts(&config, &trust));
        assert!(
            !report
                .lines
                .iter()
                .any(|line| line.label.trim() == t!(status_loop))
        );
    }

    /// The one standing permission that stops announcing itself. Every other prompt in a session
    /// is visible by appearing; this is the one that makes prompts stop, so without a line here a
    /// user has no way to find out that a program now runs unasked.
    #[test]
    fn the_report_names_the_programs_that_run_without_asking() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let vouched = TrustedPrograms::from_iter([
            bravebot_core::programs::Command::new(
                "/usr/bin/git",
                vec!["log".to_string()],
                "/tmp/project",
            ),
            bravebot_core::programs::Command::new(
                "/usr/bin/make",
                vec!["check".to_string()],
                "/tmp/project/crates",
            ),
        ]);
        let mut facts = facts(&config, &trust);
        facts.programs = &vouched;

        let shown = rendered(&report(&facts));
        // The arguments are part of what was vouched for, so they are part of what is reported:
        // "git" alone would not tell a reader which command they trusted.
        assert!(shown.contains("/usr/bin/git log"), "{shown}");
        assert!(shown.contains("/usr/bin/make check"), "{shown}");
        // And the tree, which is the other part (RUN-8): an entry given in `crates/` runs unasked
        // there and nowhere else, so a report naming only the command would have a reader believing
        // they hold a grant at the root as well. The root itself reads as ".", the way the trust
        // map's rules above it are written.
        assert!(shown.contains("in ."), "{shown}");
        assert!(shown.contains("in crates"), "{shown}");
        // Both halves of the grant, said where the user can see them.
        assert!(shown.contains("output is trusted"), "{shown}");
    }

    /// RUN-9: every command vouched for is readable back, however many there are. A vouched
    /// command is the one permission whose whole effect is that a prompt stops appearing, so one
    /// the report leaves out is a standing grant with nothing anywhere to say it is held.
    #[test]
    fn every_vouched_command_is_listed_however_many_there_are() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let vouched = TrustedPrograms::from_iter((0..12).map(|nth| {
            bravebot_core::programs::Command::new(
                "/usr/bin/make",
                vec![format!("check-{nth:02}")],
                "/tmp/project",
            )
        }));
        let mut facts = facts(&config, &trust);
        facts.programs = &vouched;

        let shown = rendered(&report(&facts));
        for nth in 0..12 {
            assert!(
                shown.contains(&format!("/usr/bin/make check-{nth:02}")),
                "{shown}"
            );
        }
    }

    /// RUN-8, RUN-9: the tree is part of the entry, so it is part of what the report reads back.
    /// Two entries for one command in two directories draw as two lines saying where each holds;
    /// a report naming the command alone would draw them as one line twice and leave a reader
    /// unable to tell which grant they have.
    #[test]
    fn the_report_names_the_tree_each_vouched_command_runs_unasked_in() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let vouched = TrustedPrograms::from_iter([
            bravebot_core::programs::Command::new(
                "/usr/bin/make",
                vec!["check".to_string()],
                "/tmp/project",
            ),
            bravebot_core::programs::Command::new(
                "/usr/bin/make",
                vec!["check".to_string()],
                "/tmp/project/crates/tui",
            ),
        ]);
        let mut facts = facts(&config, &trust);
        facts.programs = &vouched;

        let shown = rendered(&report(&facts));
        assert_eq!(
            shown.matches("/usr/bin/make check").count(),
            2,
            "two entries for one command in two trees did not read back as two: {shown}"
        );
        // The root reads as ".", the way the trust map's own rules on this screen are written, and
        // a tree inside the workspace reads relative to it rather than as an absolute path a reader
        // has to compare by eye.
        assert!(shown.contains("in ."), "{shown}");
        assert!(shown.contains("in crates/tui"), "{shown}");
    }

    /// The ordinary case has to say so rather than say nothing, or a user reading the report
    /// cannot tell the difference between "no program is vouched for" and "this report does not
    /// cover programs".
    #[test]
    fn a_session_that_vouched_for_nothing_says_every_run_is_asked_about() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(shown.contains("every run is put to you"), "{shown}");
    }

    /// A record holding `count` lines, the first of them answered in this session and the rest in
    /// an earlier one, which is the mixture the reading back exists to tell apart.
    fn record_of(count: usize) -> bravebot_core::remembered::Remembered {
        (0..count)
            .map(|nth| bravebot_core::remembered::Entry {
                line: bravebot_core::remembered::RememberedLine {
                    steps: bravebot_core::remembered::Shape::Pipeline(vec![
                        bravebot_core::remembered::RememberedStep {
                            program: "make".to_string(),
                            resolved: std::path::PathBuf::from("/usr/bin/make"),
                            started_as: std::path::PathBuf::from("/usr/bin/make"),
                            args: vec![bravebot_core::remembered::RememberedArg::Literal(format!(
                                "check{nth}"
                            ))],
                            environment: Vec::new(),
                            routes: Vec::new(),
                        },
                    ]),
                },
                answered_in: match nth {
                    0 => "1787860306-65099".to_string(),
                    _ => "a-session-last-week".to_string(),
                },
            })
            .collect()
    }

    /// TRUST-12: a yes kept past a session stops the only screen that would have shown it, so the
    /// report says it is held, when it was given, where it is written and how to take it back.
    /// Nothing is said where none is kept, which is every directory nobody answered yes about.
    #[test]
    fn the_report_says_a_directory_is_trusted_by_a_remembered_answer() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let quiet = rendered(&report(&facts(&config, &trust)));
        assert!(!quiet.contains("/forget-trust"), "{quiet}");

        let mut facts = facts(&config, &trust);
        facts.kept_trust = Some(KeptTrust {
            when: "3 days ago",
            path: Path::new("/home/someone/.bravebot/trusted/-tmp-project.jsonl"),
            root: None,
        });
        let shown = rendered(&report(&facts));
        assert!(shown.contains("without asking"), "{shown}");
        assert!(shown.contains("3 days ago"), "{shown}");
        assert!(
            shown.contains("/home/someone/.bravebot/trusted/-tmp-project.jsonl"),
            "{shown}"
        );
        assert!(shown.contains("/forget-trust"), "{shown}");
    }

    /// TRUST-24: an answer kept about the worktree root above the directory names that root, since
    /// the directory itself is not the one a person remembers having answered about.
    #[test]
    fn the_report_names_the_root_whose_remembered_answer_settles_the_directory() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut facts = facts(&config, &trust);
        facts.kept_trust = Some(KeptTrust {
            when: "3 days ago",
            path: Path::new("/home/someone/.bravebot/trusted/-tmp-project.jsonl"),
            root: Some(Path::new("/tmp/project")),
        });
        let shown = rendered(&report(&facts));
        assert!(shown.contains("/tmp/project"), "{shown}");
    }

    /// RUN-19: the answer that outlives the session is the one a person can least account for from
    /// memory, since it was given in a session that has ended and nothing since has mentioned it.
    /// So the report names the lines, says which of the two lifetimes each one is, and says where
    /// the file is, because deleting a line from it is the way back.
    #[test]
    fn the_report_names_the_lines_remembered_past_a_session_and_who_answered_them() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let lines = record_of(2);
        let mut facts = facts(&config, &trust);
        facts.remembered = Some(Remembered {
            lines: &lines,
            path: Path::new("/home/someone/.bravebot/remembered/-tmp-project.jsonl"),
        });

        let shown = rendered(&report(&facts));
        // The arguments are what the entry keys on, so they are what is reported.
        assert!(shown.contains("/usr/bin/make check0"), "{shown}");
        assert!(shown.contains("/usr/bin/make check1"), "{shown}");
        assert!(
            shown.contains(t!(status_remembered_this_session)),
            "{shown}"
        );
        assert!(shown.contains(t!(status_remembered_earlier)), "{shown}");
        // The half nothing else says: it stops the asking and leaves the output where it was.
        assert!(shown.contains("stays quarantined"), "{shown}");
        assert!(
            shown.contains("/home/someone/.bravebot/remembered"),
            "{shown}"
        );
    }

    /// A scope for `gh pr view 12 --repo brave/bravebot`, as the prompt would have offered it.
    fn a_remembered_scope() -> bravebot_core::remembered::Scope {
        use bravebot_core::command::{Plan, Step, Steps};
        let plan = Plan {
            line: String::new(),
            directory: std::path::PathBuf::from("/work"),
            steps: Steps::Pipeline(vec![Step {
                program: "gh".to_string(),
                resolved: std::path::PathBuf::from("/usr/bin/gh"),
                started_as: std::path::PathBuf::from("/usr/bin/gh"),
                args: ["pr", "view", "12", "--repo", "brave/bravebot"]
                    .map(String::from)
                    .to_vec(),
                environment: Vec::new(),
                routes: Vec::new(),
            }]),
            writes: Vec::new(),
            reads: Vec::new(),
            stdin: None,
        };
        bravebot_core::remembered::scopes::offered_for(&plan).expect("a listed line")
    }

    /// RUN-20: a remembered scope is listed as the one item it was answered as, named in words, with
    /// who answered it. The lines it expands to are not listed beside it, and a long list of lines
    /// does not push it off the end, because it is the widest answer in the record.
    #[test]
    fn the_report_lists_a_remembered_scope_as_one_item_in_words() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut record = record_of(MAX_REMEMBERED + 3);
        record.record_scope(a_remembered_scope(), "a-session-last-week");
        let mut facts = facts(&config, &trust);
        facts.remembered = Some(Remembered {
            lines: &record,
            path: Path::new("/home/someone/.bravebot/remembered/-tmp-project.jsonl"),
        });

        let shown = rendered(&report(&facts));
        assert!(
            shown.contains("Read pull requests and issues in brave/bravebot"),
            "{shown}"
        );
        for expanded in ["<number>", "pr diff", "pr checks", "issue view"] {
            assert!(
                !shown.contains(expanded),
                "the scope was listed as its lines ({expanded}): {shown}"
            );
        }
        assert!(
            shown.contains(&t!(count_remembered, count = MAX_REMEMBERED + 4)),
            "{shown}"
        );
    }

    /// SANDBOX-23: a reach remembered for a command changes the plan a person is shown, so the
    /// report lists each row as `/reach` does, with the note saying how to forget one. The
    /// regressions it rejects: a report that lists none, rows without their numbers, and a heading
    /// drawn for a session that has none.
    #[test]
    fn the_report_lists_the_reach_remembered_for_commands() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let quiet = rendered(&report(&facts(&config, &trust)));
        assert!(!quiet.contains(t!(status_reach)), "{quiet}");

        let rows = vec![
            "1. ls also reads and writes /tmp/project, allowed 2026-10-07, for this session in /tmp/project"
                .to_string(),
            "2. git push also reads the remote scope, allowed 2026-10-06, always".to_string(),
        ];
        let mut facts = facts(&config, &trust);
        facts.reach = &rows;
        let shown = rendered(&report(&facts));

        assert!(shown.contains(t!(status_reach)), "{shown}");
        assert!(
            shown.contains(&t!(count_reach_grants, count = 2)),
            "{shown}"
        );
        assert!(shown.contains(t!(status_reach_note)), "{shown}");
        for row in &rows {
            assert!(shown.contains(row.as_str()), "{shown}");
        }
    }

    /// SANDBOX-28: a path the person let programs reach is listed as `/reach paths` lists it, with
    /// the note that the directory is not trusted and how to end one. A session that was asked for
    /// none draws no heading.
    #[test]
    fn the_report_lists_the_paths_programs_were_let_reach() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let quiet = rendered(&report(&facts(&config, &trust)));
        assert!(!quiet.contains(t!(status_requested_paths)), "{quiet}");

        let rows = vec![
            "1. every command also reads /opt/toolchain, for this session".to_string(),
            "2. every command also reads and writes /srv/out, for this session".to_string(),
        ];
        let mut facts = facts(&config, &trust);
        facts.requested_paths = &rows;
        let shown = rendered(&report(&facts));

        assert!(shown.contains(t!(status_requested_paths)), "{shown}");
        assert!(
            shown.contains(&t!(count_requested_paths, count = 2)),
            "{shown}"
        );
        assert!(shown.contains(t!(status_requested_paths_note)), "{shown}");
        for row in &rows {
            assert!(shown.contains(row.as_str()), "{shown}");
        }
    }

    /// PATHREQ-7: a directory bypass mode took the file tools into is listed with the call that first
    /// reached it and a note that it is not opened or trusted. A session that reached none draws no
    /// heading, so a session outside bypass is not shown a section about a mode it is not in.
    #[test]
    fn the_report_lists_the_directories_bypass_took_the_file_tools_into() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let quiet = rendered(&report(&facts(&config, &trust)));
        assert!(!quiet.contains(t!(status_reached_directories)), "{quiet}");

        let rows = vec!["/srv/out, first reached by write_file (path)".to_string()];
        let mut facts = facts(&config, &trust);
        facts.reached_directories = &rows;
        let shown = rendered(&report(&facts));

        assert!(shown.contains(t!(status_reached_directories)), "{shown}");
        assert!(
            shown.contains(&t!(count_reached_directories, count = 1)),
            "{shown}"
        );
        assert!(
            shown.contains(t!(status_reached_directories_note)),
            "{shown}"
        );
        assert!(shown.contains(rows[0].as_str()), "{shown}");
    }

    /// RUN-19: where the list is shortened it says how many of each it left out. A bare count would
    /// leave a person unable to tell how much of what they are carrying came from another session,
    /// which is the whole question the two lifetimes raise.
    #[test]
    fn a_shortened_list_of_remembered_lines_says_how_many_came_from_an_earlier_session() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let lines = record_of(MAX_REMEMBERED + 3);
        let mut facts = facts(&config, &trust);
        facts.remembered = Some(Remembered {
            lines: &lines,
            path: Path::new("/home/someone/.bravebot/remembered/-tmp-project.jsonl"),
        });

        let shown = rendered(&report(&facts));
        assert!(
            shown.contains(&t!(status_remembered_and_more, count = 3, earlier = 3)),
            "{shown}"
        );
    }

    /// The report must not say every run is put to the person while listing lines that run unasked.
    /// This is the screen a person goes to in order to find out what they are carrying, so the one
    /// claim it cannot make is the one that contradicts the list below it.
    #[test]
    fn a_session_carrying_a_remembered_line_is_not_told_every_run_is_asked_about() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let lines = record_of(1);
        let mut facts = facts(&config, &trust);
        facts.remembered = Some(Remembered {
            lines: &lines,
            path: Path::new("/home/someone/.bravebot/remembered/-tmp-project.jsonl"),
        });

        let shown = rendered(&report(&facts));
        assert!(
            !shown.contains(t!(status_every_run_is_asked)),
            "the report claimed every run is asked about beside a line that is not: {shown}"
        );
        assert!(
            shown.contains(t!(status_nothing_vouched_this_session)),
            "{shown}"
        );
    }

    /// A directory nobody has remembered anything in says nothing about it, rather than carrying a
    /// line about a thing that is not happening: the programs line above already says every run is
    /// put to the person.
    #[test]
    fn a_directory_with_nothing_remembered_does_not_mention_the_record() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let empty = bravebot_core::remembered::Remembered::new();
        for record in [
            None,
            Some(Remembered {
                lines: &empty,
                path: Path::new("/home/someone/.bravebot/remembered/-tmp-project.jsonl"),
            }),
        ] {
            let mut facts = facts(&config, &trust);
            facts.remembered = record;
            let shown = rendered(&report(&facts));
            assert!(!shown.contains(t!(status_remembered)), "{shown}");
        }
    }

    /// The other standing permission that stops announcing itself, and the broader one: it changes
    /// what happens to every prompt rather than one program's. The line under the box says so while
    /// it holds, and this is where a person goes back to when they want to know why.
    #[test]
    fn the_report_names_a_mode_that_is_not_asking() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        for mode in [
            bravebot_agent::PermissionMode::AcceptEdits,
            bravebot_agent::PermissionMode::Plan,
            bravebot_agent::PermissionMode::Bypass,
        ] {
            let mut facts = facts(&config, &trust);
            facts.permission_mode = mode;
            let shown = rendered(&report(&facts));
            let named = named_mode(mode, false).expect("every mode but asking has a name");
            assert!(shown.contains(named), "{mode:?} was not reported: {shown}");
            // And how to change it, since a mode nobody can find the key for is one they restart to
            // get out of.
            assert!(shown.contains("shift-tab"), "{mode:?}: {shown}");
        }
    }

    /// A standing answer that stops a prompt appearing has to be readable for the rest of the
    /// session. The note at the top of the transcript has scrolled away by the time somebody
    /// wonders why they are not being asked, and the file it is kept in is where they undo it.
    #[test]
    fn a_session_that_stopped_asking_about_a_check_says_so() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut facts = facts(&config, &trust);
        facts.auto_vetting = true;
        let shown = rendered(&report(&facts));
        assert!(shown.contains("vetting"), "{shown}");
        assert!(
            shown.contains("~/.bravebot/vetting"),
            "the report did not say where the answer is kept: {shown}"
        );
    }

    /// Nothing is said where the session asks, which is every session nobody turned the mode on
    /// for. A line reporting the ordinary state would be one more line to skim past.
    #[test]
    fn an_ordinary_session_says_nothing_about_a_check() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(!shown.contains("~/.bravebot/vetting"), "{shown}");
    }

    /// Nothing is said where the session is asking. A line reporting the ordinary state on every
    /// session is a line people learn to skim, and this report has to keep the one above worth
    /// reading.
    #[test]
    fn an_ordinary_session_says_nothing_about_its_permission_mode() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(named_mode(bravebot_agent::PermissionMode::Ask, false).is_none());
        assert!(!shown.contains("shift-tab"), "{shown}");
    }

    /// While a turn holds the trust map and the vouched programs the report states neither: a copy
    /// from before the turn began would say every run is asked about after the turn had been given
    /// a standing yes, and would call a directory untrusted that the turn had since vouched for.
    #[test]
    fn a_report_whose_rules_are_the_turns_states_neither_the_trust_nor_the_programs() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut facts = facts(&config, &trust);
        facts.turn_holds_rules = true;

        let shown = rendered(&report(&facts));

        assert!(shown.contains(t!(status_held_by_the_turn)), "{shown}");
        for stated in [
            t!(status_every_run_is_asked),
            t!(status_nothing_vouched_for),
            t!(status_directory_trusted),
            t!(status_directory_untrusted),
        ] {
            assert!(!shown.contains(stated), "stated {stated:?}: {shown}");
        }
    }

    /// A session started with the flag that skips permissions begins in bypass. When the key moves
    /// it to asking, a report that goes quiet is the same as one from a session that never skipped
    /// anything, so asking is named there, and only there.
    #[test]
    fn named_mode_names_asking_where_the_session_began_in_bypass() {
        use bravebot_agent::PermissionMode::Ask;
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let asking = named_mode(Ask, true).expect("asking after the flag has a name");
        assert_ne!(
            Some(asking),
            named_mode(bravebot_agent::PermissionMode::Bypass, true)
        );
        assert_eq!(named_mode(Ask, false), None);

        let mut facts = facts(&config, &trust);
        facts.permission_mode = Ask;
        facts.began_in_bypass = true;
        let shown = rendered(&report(&facts));
        assert!(shown.contains(asking), "{shown}");
        assert!(shown.contains("shift-tab"), "{shown}");

        facts.began_in_bypass = false;
        let shown = rendered(&report(&facts));
        assert!(!shown.contains(asking), "{shown}");
    }

    /// SANDBOX-22, SANDBOX-28: a session whose programs write what a request would be granted says
    /// so, and a session that does not is silent. The regressions it rejects are a line shown in
    /// `strict`, in `off`, outside bypass, on Windows, or with no home directory, where the stage is
    /// given no extra row, and a line shown for a directory the person has not trusted, which
    /// accepts no request and so gives no row. While a turn holds the trust map the line says so
    /// and states no reach, since either answer would be about an earlier moment.
    #[test]
    fn the_report_says_programs_write_what_a_request_would_be_granted_only_where_they_do() {
        use bravebot_agent::PermissionMode::{AcceptEdits, Ask, Bypass};
        use bravebot_sandbox::SandboxMode::{Off, Strict, WorkspaceWrite};
        let home = std::env::temp_dir();
        let home = Some(home.as_path());

        assert_eq!(
            programs_write_unasked(WorkspaceWrite, Bypass, home),
            cfg!(any(target_os = "linux", target_os = "macos"))
        );
        for (mode, permission, home) in [
            (WorkspaceWrite, Bypass, None),
            (WorkspaceWrite, Ask, home),
            (WorkspaceWrite, AcceptEdits, home),
            (Strict, Bypass, home),
            (Off, Bypass, home),
        ] {
            assert!(
                !programs_write_unasked(mode, permission, home),
                "{mode:?} {permission:?} {home:?}"
            );
        }

        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let declined = TrustStore::new("/work");
        let said = |facts: &Facts<'_>| {
            report(facts)
                .lines
                .into_iter()
                .find(|line| line.label == t!(status_programs_write))
                .map(|line| line.value)
        };

        let mut untrusted = facts(&config, &declined);
        untrusted.programs_write_unasked = true;
        assert_eq!(said(&untrusted), None, "a directory not trusted");

        let mut facts = facts(&config, &trust);
        assert_eq!(said(&facts), None, "the mode gives nothing");
        facts.programs_write_unasked = true;
        assert_eq!(
            said(&facts).as_deref(),
            Some(t!(status_programs_write_anywhere))
        );
        facts.turn_holds_rules = true;
        assert_eq!(
            said(&facts).as_deref(),
            Some(t!(status_held_by_the_turn)),
            "a turn holds the trust map"
        );
    }

    fn rendered(report: &Report) -> String {
        report
            .lines
            .iter()
            .map(|line| format!("{} {} {}\n", line.label, line.value, line.note))
            .collect()
    }

    /// The endpoint host and the key id are what a screenshot should not spread, and `doctor` is
    /// where they belong. This is the one property worth pinning hardest.
    #[test]
    fn the_host_and_the_key_id_are_never_reported() {
        let config = config_for(
            "https://ai-chat.bsg.brave.software",
            Some("https://ai-chat-premium.bsg.brave.software"),
        );
        let trust = trusting();
        let shown = rendered(&report(&facts(&config, &trust)));

        assert!(!shown.contains("ai-chat"), "the host was reported: {shown}");
        assert!(!shown.contains("a-key-id"), "the key id was reported");
        assert!(!shown.contains("a-signing-key"), "the key was reported");
        assert!(shown.contains("dev"), "the environment was not reported");
    }

    #[test]
    fn each_environment_is_named_without_its_host() {
        assert_eq!(environment("https://ai-chat.bsg.brave.software"), "dev");
        assert_eq!(environment("https://ai-chat.bsg.brave.com"), "prod");
        assert_eq!(environment("http://127.0.0.1:8080"), "local");
        assert_eq!(environment("https://example.invalid"), "custom");
    }

    /// Whether this directory is trusted is the first thing a person wants from a status panel, and
    /// a declined directory has to say what that means rather than only that it happened.
    #[test]
    fn the_directory_says_whether_it_is_trusted() {
        let config = config_for("http://127.0.0.1:1", None);

        let trusted = trusting();
        let shown = rendered(&report(&facts(&config, &trusted)));
        assert!(shown.contains("trusted"), "{shown}");

        let declined = TrustStore::new("/work");
        let shown = rendered(&report(&facts(&config, &declined)));
        assert!(shown.contains("not trusted"), "{shown}");
        assert!(shown.contains("every write is shown"), "{shown}");
    }

    /// The bug this replaced. Every build knows a premium host, so reporting premium from the
    /// configuration said "premium" for a session that never presented a key, while every request
    /// went out without one and came back answered by a weaker model. A panel that cannot be
    /// trusted on this point is worse than one that omits it.
    #[test]
    fn the_tier_reported_is_the_one_the_last_turn_actually_ran_on() {
        let config = config_for(
            "https://ai-chat.bsg.brave.com",
            Some("https://ai-chat-premium.bsg.brave.com"),
        );
        let trust = trusting();

        // A premium host is configured and a turn ran without presenting the key. The old line said
        // "premium configured" here, which is the sentence that hid a whole broken session.
        let mut free = facts(&config, &trust);
        free.premium = Some(false);
        let shown = rendered(&report(&free));
        assert!(shown.contains("the premium key was not sent"), "{shown}");
        assert!(
            !shown.contains("premium, the premium key was sent"),
            "a turn that sent no key was reported as premium: {shown}"
        );

        let mut premium = facts(&config, &trust);
        premium.premium = Some(true);
        let shown = rendered(&report(&premium));
        assert!(shown.contains("the premium key was sent"), "{shown}");
    }

    /// What the opening screen draws before anything has run is what `/status` says at that moment,
    /// because both take it from here. Two copies of this wording would be two things to keep true,
    /// and the one that drifted would be the one nobody re-reads.
    ///
    /// Neither sends anything. A key that is held may still be refused, so holding one is not the
    /// tier: only a request settles that.
    #[test]
    fn the_opening_line_and_the_panel_say_the_same_thing_before_a_turn_runs() {
        let premium = config_for(
            "https://ai-chat.bsg.brave.com",
            Some("https://ai-chat-premium.bsg.brave.com"),
        );
        let trust = trusting();
        let shown = rendered(&report(&facts(&premium, &trust)));
        assert!(shown.contains(configured_tier(&premium)), "{shown}");

        // And a build that cannot reach premium at all says so in both places.
        let free = config_for("https://ai-chat.bsg.brave.com", None);
        assert_eq!(configured_tier(&free), t!(status_no_subscription));
        let shown = rendered(&report(&facts(&free, &trust)));
        assert!(shown.contains(configured_tier(&free)), "{shown}");
    }

    /// The words the opening screen draws are the configuration's, and nothing between the two may
    /// compose its own.
    ///
    /// The screen used to be handed the tier as a string, so the line the panel above is held to
    /// agree with was itself held to nothing: a caller could pass any wording at all and every
    /// test named by this clause would still pass. It takes the configuration now, and this is
    /// what pins the two ends of that to one another.
    #[test]
    fn the_opening_screen_draws_the_tier_the_configuration_settles() {
        let premium = config_for(
            "https://ai-chat.bsg.brave.com",
            Some("https://ai-chat-premium.bsg.brave.com"),
        );
        let free = config_for("https://ai-chat.bsg.brave.com", None);
        assert_ne!(
            configured_tier(&premium),
            configured_tier(&free),
            "both configurations say the same thing, so nothing here distinguishes them"
        );

        for config in [&premium, &free] {
            let session = crate::state::Session::new("kernel-enforced").on_tier(config);
            let opening = crate::logo::lines(
                &session.confinement,
                session.sandbox_mode(),
                session.sockets_with_a_path_reachable,
                &session.tier,
                90,
                24,
            )
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
            assert!(
                opening.contains(configured_tier(config)),
                "the opening screen says something the configuration does not: {opening}"
            );
        }
    }

    /// A closed network is on the report with who closed it, an open one and a session that never
    /// settled one are not, and a pin says it cannot be changed from a flag.
    #[test]
    fn a_closed_network_is_reported_with_who_closed_it_and_an_open_one_is_not() {
        use bravebot_config::{Decided, RunNetwork};
        use bravebot_sandbox::SandboxMode::{self, WorkspaceWrite};
        use bravebot_sandbox::network::Network;
        let settled = |network, decided| RunNetwork { network, decided };

        assert!(network_line(None, WorkspaceWrite).is_none());
        assert!(
            network_line(Some(&settled(Network::Open, Decided::Flag)), WorkspaceWrite).is_none()
        );
        let flag = network_line(
            Some(&settled(Network::Closed, Decided::Flag)),
            WorkspaceWrite,
        )
        .expect("a line");
        assert!(
            network_line(
                Some(&settled(Network::Closed, Decided::Flag)),
                SandboxMode::Off
            )
            .is_none(),
            "a closed network was claimed for programs nothing confines"
        );
        // `read-only` closes it whatever the session settled, or did not settle (SANDBOX-22).
        for open in [None, Some(settled(Network::Open, Decided::Flag))] {
            let closed = network_line(open.as_ref(), SandboxMode::ReadOnly)
                .expect("read-only closes the network");
            assert_eq!(closed.value, t!(status_network_closed));
        }
        assert_eq!(flag.label.trim(), t!(status_network));
        assert!(flag.note.contains("--run-network"));
        let pinned = network_line(
            Some(&settled(
                Network::Closed,
                Decided::Managed(Some("/etc/bravebot/managed.json".into())),
            )),
            WorkspaceWrite,
        )
        .expect("a line");
        let note = pinned.note.as_str();
        assert!(
            note.contains("/etc/bravebot/managed.json") && note.contains("pinned"),
            "{note}"
        );
    }

    /// The lists are on the report with how many entries each holds and the files that wrote them,
    /// never an entry, and a session with none says nothing.
    #[test]
    fn filesystem_rules_are_reported_by_count_and_file_and_never_by_path() {
        use bravebot_sandbox::rules::{Entry, Lists};
        let entry = |path: &str, by: Option<&str>| Entry {
            path: path.into(),
            by: by.map(Into::into),
            pinned: false,
        };
        let settled = |lists| bravebot_config::Filesystem {
            lists,
            ..Default::default()
        };

        assert!(filesystem_line(None).is_none());
        assert!(filesystem_line(Some(&settled(Lists::default()))).is_none());
        let line = filesystem_line(Some(&settled(Lists {
            deny_read: vec![
                entry("~/very-secret", Some("/home/a/.bravebot/settings.json")),
                entry("~/other", None),
            ],
            deny_write: vec![entry(".env", Some("/home/a/.bravebot/settings.json"))],
            ..Lists::default()
        })))
        .expect("a line");

        assert_eq!(line.label.trim(), t!(status_sandbox_filesystem));
        assert!(
            line.value.contains("2 denyRead") && line.value.contains("1 denyWrite"),
            "{}",
            line.value
        );
        assert!(!line.value.contains("very-secret") && !line.note.contains("very-secret"));
        assert!(line.note.contains("/home/a/.bravebot/settings.json"));
        assert!(line.note.contains(t!(status_sandbox_filesystem_flags)));
    }

    /// The files that wrote a list are named once each, in the order they are first met, and an
    /// entry no file wrote is named by the caller's own words. The faults it rejects are a file
    /// named once per entry, a sort, and a nameless entry dropped or named for the wrong line.
    #[test]
    fn sources_are_listed_once_each_in_first_seen_order() {
        let by = |path: Option<&str>| path.map(std::path::PathBuf::from);
        let entries = [
            by(Some("/b.json")),
            by(None),
            by(Some("/a.json")),
            by(Some("/b.json")),
            by(None),
        ];

        assert_eq!(
            sources(entries.iter(), "managed"),
            ["/b.json", "managed", "/a.json"]
        );
        assert!(sources([].iter(), "managed").is_empty());
    }

    /// Two paths that differ only in a byte no UTF-8 name holds print the same, so the note names
    /// them once: a list of what the person sees has no use for two identical words. The fault it
    /// rejects is a dedup keyed on the path, which would print the same file twice.
    #[cfg(unix)]
    #[test]
    fn sources_that_print_alike_are_named_once() {
        use std::os::unix::ffi::OsStrExt;
        let raw = Some(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
            b"/tool-\xff",
        )));
        let lossy = Some(std::path::PathBuf::from("/tool-\u{FFFD}"));
        assert_ne!(raw, lossy);

        assert_eq!(sources([raw, lossy].iter(), "managed"), ["/tool-\u{FFFD}"]);
    }

    /// The four filesystem lists are read in the order `allow_read`, `deny_read`, `allow_write`,
    /// `deny_write`, so the files in the note come in that order, not sorted and not by list size.
    #[test]
    fn filesystem_files_are_named_in_list_order() {
        use bravebot_sandbox::rules::{Entry, Lists};
        let entry = |by: &str| Entry {
            path: "~/p".into(),
            by: Some(by.into()),
            pinned: false,
        };
        let line = filesystem_line(Some(&bravebot_config::Filesystem {
            lists: Lists {
                allow_read: vec![entry("/4.json")],
                deny_read: vec![entry("/3.json")],
                allow_write: vec![entry("/2.json"), entry("/4.json")],
                deny_write: vec![entry("/1.json")],
            },
            ..Default::default()
        }))
        .expect("a line");

        let at = |file: &str| line.note.find(file).expect(file);
        assert!(
            at("/4.json") < at("/3.json")
                && at("/3.json") < at("/2.json")
                && at("/2.json") < at("/1.json"),
            "{}",
            line.note
        );
        assert_eq!(line.note.matches("/4.json").count(), 1, "{}", line.note);
    }

    /// The host list is on the report with how many entries each list holds and the files that
    /// wrote them, never a host, and a session with no `allowedHosts` or none under `off` says
    /// nothing. The regression it rejects is a list in force that the screen never mentions.
    #[test]
    fn host_lists_are_reported_by_count_and_file_and_never_by_host() {
        use bravebot_config::sandbox_network::{HostEntry, Hosts, Resolved};
        use bravebot_sandbox::SandboxMode::{Off, WorkspaceWrite};
        let entry = |host: &str, by: Option<&str>| HostEntry {
            entry: host.to_string(),
            by: by.map(Into::into),
        };
        let settled = |allowed, denied| Resolved {
            hosts: Hosts {
                allowed,
                denied,
                on_unlisted: None,
            },
            ..Default::default()
        };

        assert!(hosts_line(None, WorkspaceWrite).is_none());
        assert!(
            hosts_line(
                Some(&settled(None, vec![entry("a.example", None)])),
                WorkspaceWrite
            )
            .is_none()
        );
        let listed = settled(
            Some(vec![
                entry(
                    "very-secret.example",
                    Some("/home/a/.bravebot/settings.json"),
                ),
                entry("*.other.example", Some("/home/a/.bravebot/settings.json")),
            ]),
            vec![entry("bad.example", None)],
        );
        assert!(hosts_line(Some(&listed), Off).is_none());
        let line = hosts_line(Some(&listed), WorkspaceWrite).expect("a line");

        assert_eq!(line.label.trim(), t!(status_hosts));
        assert!(
            line.value.contains("2 allowed") && line.value.contains("1 denied"),
            "{}",
            line.value
        );
        let shown = format!("{} {}", line.value, line.note);
        assert!(
            !shown.contains("very-secret") && !shown.contains("bad.example"),
            "{shown}"
        );
        assert!(line.note.contains("/home/a/.bravebot/settings.json"));
        assert!(line.note.contains(t!(status_hosts_managed)));
        let empty =
            hosts_line(Some(&settled(Some(vec![]), vec![])), WorkspaceWrite).expect("a line");
        assert!(empty.value.contains("0 allowed"), "{}", empty.value);
        assert_eq!(empty.note, t!(status_hosts_refused));

        let asking = Resolved {
            hosts: Hosts {
                on_unlisted: Some(bravebot_config::sandbox_network::OnUnlisted::Ask),
                ..settled(Some(vec![]), vec![]).hosts
            },
            ..Default::default()
        };
        let asked = hosts_line(Some(&asking), WorkspaceWrite).expect("a line");
        assert_eq!(asked.note, t!(status_hosts_asked));

        let asking_with_files = Resolved {
            hosts: Hosts {
                on_unlisted: Some(bravebot_config::sandbox_network::OnUnlisted::Ask),
                ..listed.hosts.clone()
            },
            ..Default::default()
        };
        let asked_files = hosts_line(Some(&asking_with_files), WorkspaceWrite).expect("a line");
        assert!(
            asked_files.note.contains("/home/a/.bravebot/settings.json")
                && asked_files.note != line.note,
            "{}",
            asked_files.note
        );
    }

    /// Before the first turn nothing has been observed, so the panel says premium is available
    /// rather than claiming it is or is not in use. Claiming either would be the same guess the
    /// configuration line used to make.
    #[test]
    fn a_session_with_no_turn_yet_does_not_claim_a_tier() {
        let config = config_for(
            "https://ai-chat.bsg.brave.com",
            Some("https://ai-chat-premium.bsg.brave.com"),
        );
        let trust = trusting();
        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(shown.contains("nothing sent yet"), "{shown}");
    }

    /// The endpoint answers a model name it will not serve by substituting a weaker one, with a 200
    /// and an ordinary reply. So a panel that reports only what was asked for names a model that
    /// never answered anything.
    #[test]
    fn a_substituted_model_is_reported_beside_the_one_asked_for() {
        let config = config_for("https://ai-chat.bsg.brave.com", None);
        let trust = trusting();

        let session = after_a_turn(&config, Some("claude-opus"), "qwen-14b-instruct");
        let mut substituted = facts(&config, &trust);
        as_of(&mut substituted, &session);
        let shown = rendered(&report(&substituted));
        // Both halves: what was asked for, and what actually answered.
        assert!(shown.contains("claude-opus"), "{shown}");
        assert!(shown.contains("qwen-14b-instruct"), "{shown}");
        assert!(shown.contains("served instead"), "{shown}");

        // Served what was asked for: nothing to report, or the line would be on every session.
        let session = after_a_turn(&config, Some("claude-opus"), "claude-opus");
        let mut honoured = facts(&config, &trust);
        as_of(&mut honoured, &session);
        let shown = rendered(&report(&honoured));
        assert!(!shown.contains("served instead"), "{shown}");
    }

    /// `/model` moves the choice the next turn will be sent and leaves the last turn's record
    /// alone, so the choice and the served name are from different turns. Read as a pair, a
    /// session answered by exactly the model it asked for reports a substitution, and the model it
    /// names as the one asked for is one nothing was ever asked of.
    #[test]
    fn picking_a_model_after_a_turn_reports_the_turn_that_ran() {
        let config = config_for("https://ai-chat.bsg.brave.com", None);
        let trust = trusting();

        // Asked for claude-opus and got it, then picked something else for the next turn.
        let mut honoured = after_a_turn(&config, Some("claude-opus"), "claude-opus");
        honoured.choose_model("claude-sonnet".to_string(), &config);
        let mut shown = facts(&config, &trust);
        as_of(&mut shown, &honoured);
        let shown = rendered(&report(&shown));
        assert!(!shown.contains("served instead"), "{shown}");

        // The other direction: the turn really was substituted, and picking the model that
        // answered it does not make the substitution stop having happened.
        let mut substituted = after_a_turn(&config, Some("claude-opus"), "qwen-14b-instruct");
        substituted.choose_model("qwen-14b-instruct".to_string(), &config);
        let mut shown = facts(&config, &trust);
        as_of(&mut shown, &substituted);
        let shown = rendered(&report(&shown));
        assert!(shown.contains("served instead"), "{shown}");
        assert!(
            shown.contains("claude-opus"),
            "the model the turn asked for is not named: {shown}"
        );
    }

    /// A gateway answers under the name it knows the model by, while a session holds that name
    /// qualified by the provider's id. Compared as held, the two never match and every gateway
    /// session reports a substitution that did not happen.
    #[test]
    fn a_gateway_answering_under_its_own_name_is_not_a_substitution() {
        let mut config = config_for("https://ai-chat.bsg.brave.com", None);
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"openrouter": {
                "options": {"baseURL": "https://openrouter.example.invalid/api/v1"}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        config.providers = bravebot_config::provider::Provider::all(&root);
        let trust = trusting();

        let session = after_a_turn(&config, Some("openrouter/z-ai/glm-4.6"), "z-ai/glm-4.6");
        let mut gateway = facts(&config, &trust);
        as_of(&mut gateway, &session);
        let shown = rendered(&report(&gateway));
        assert!(!shown.contains("served instead"), "{shown}");

        // A gateway that really did answer with a different model still says so.
        let session = after_a_turn(&config, Some("openrouter/z-ai/glm-4.6"), "moonshot/kimi-k2");
        let mut elsewhere = facts(&config, &trust);
        as_of(&mut elsewhere, &session);
        let shown = rendered(&report(&elsewhere));
        assert!(shown.contains("served instead"), "{shown}");
    }

    /// `automatic` is the server choosing per request, so a concrete name coming back is the feature
    /// working rather than a substitution. Flagging it would put a warning on the default config.
    #[test]
    fn automatic_being_resolved_to_a_real_model_is_not_a_substitution() {
        let config = config_for("https://ai-chat.bsg.brave.com", None);
        let trust = trusting();
        let session = after_a_turn(&config, None, "claude-3-haiku");
        let mut automatic = facts(&config, &trust);
        as_of(&mut automatic, &session);

        let shown = rendered(&report(&automatic));
        assert!(!shown.contains("served instead"), "{shown}");
    }

    /// A chosen model and the configured default are different facts, and reporting one as the other
    /// would explain the wrong thing.
    #[test]
    fn the_model_says_whether_it_was_chosen() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(shown.contains(DEFAULT_MODEL), "{shown}");
        assert!(shown.contains("the configured default"), "{shown}");

        let mut chosen = facts(&config, &trust);
        chosen.model = Some("claude-3-sonnet");
        let shown = rendered(&report(&chosen));
        assert!(shown.contains("claude-3-sonnet"), "{shown}");
        assert!(shown.contains("chosen with /model"), "{shown}");
    }

    /// The level is half of what a turn costs, and a session cannot say what it is spending if the
    /// panel reports the model without it.
    #[test]
    fn the_effort_says_whether_it_was_chosen() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(shown.contains("Effort"), "{shown}");
        assert!(
            shown.contains("whatever the service does on its own"),
            "{shown}"
        );

        let mut chosen = facts(&config, &trust);
        chosen.effort = Some(bravebot_aichat::protocol::Effort::Xhigh);
        let shown = rendered(&report(&chosen));
        assert!(shown.contains("xhigh"), "{shown}");
        assert!(shown.contains("chosen with /effort"), "{shown}");
    }

    /// CHATGPT-10: on a model of the account the line names the level a request carries, which is the
    /// one the session began with, and says which level was chosen since, where the two differ. The
    /// regressions it rejects are the chosen level shown as the one in force, a note for a session
    /// whose levels agree, and the same line for a model that is not the account's.
    #[test]
    fn on_the_account_the_effort_names_the_level_sent_and_the_one_chosen_where_they_differ() {
        use bravebot_aichat::protocol::Effort;
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let mut differs = facts(&config, &trust);
        differs.effort = Some(Effort::High);
        differs.account_effort = Some(Some(Effort::Low));
        let shown = rendered(&report(&differs));
        assert!(
            shown.contains("low") && shown.contains("the level this ChatGPT session began with"),
            "{shown}"
        );
        assert!(
            shown.contains("high was chosen with /effort and applies from the next session"),
            "{shown}"
        );

        let mut began_with_none = facts(&config, &trust);
        began_with_none.effort = Some(Effort::High);
        began_with_none.account_effort = Some(None);
        let shown = rendered(&report(&began_with_none));
        assert!(shown.contains("default"), "{shown}");
        assert!(shown.contains("high was chosen with /effort"), "{shown}");

        let mut agrees = facts(&config, &trust);
        agrees.effort = Some(Effort::Low);
        agrees.account_effort = Some(Some(Effort::Low));
        let shown = rendered(&report(&agrees));
        assert!(shown.contains("chosen with /effort"), "{shown}");
        assert!(!shown.contains("began with"), "{shown}");

        let mut elsewhere = facts(&config, &trust);
        elsewhere.effort = Some(Effort::High);
        let shown = rendered(&report(&elsewhere));
        assert!(
            shown.contains("high") && !shown.contains("began with"),
            "{shown}"
        );

        let mut unread = facts(&config, &trust);
        unread.effort = Some(Effort::High);
        unread.account_effort = Some(Some(Effort::Low));
        unread.model_reads_effort = false;
        let shown = rendered(&report(&unread));
        assert!(shown.contains("this model reads none"), "{shown}");
    }

    /// A level the model will not read is still named, since it is what somebody chose, but the
    /// note must not call it in force: a request does not carry it.
    #[test]
    fn a_level_the_model_does_not_read_is_reported_as_unread() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let mut unread = facts(&config, &trust);
        unread.effort = Some(bravebot_aichat::protocol::Effort::Max);
        unread.model_reads_effort = false;

        let shown = rendered(&report(&unread));
        assert!(shown.contains("max"), "the choice was hidden: {shown}");
        assert!(shown.contains("this model reads none"), "{shown}");
        assert!(
            !shown.contains("chosen with /effort, but this model reads none")
                || !shown.contains("whatever the service"),
            "{shown}"
        );
    }

    /// The level is a fact about the platform, and nothing this session runs is confined with it,
    /// so the level standing alone tells somebody their reads, writes and programs sit inside a
    /// kernel boundary that nothing has put them in.
    #[test]
    fn the_confinement_is_reported_as_available_rather_than_in_force() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let report = report(&facts(&config, &trust));
        let line = report
            .lines
            .iter()
            .find(|line| line.label.trim() == t!(status_confinement))
            .expect("the confinement is on the report");

        assert_eq!(
            line.value, "kernel-enforced",
            "the level itself was dropped"
        );
        assert_eq!(
            line.note,
            t!(status_confinement_nothing_confined),
            "the level stands alone, so the panel reads as a boundary the session is inside"
        );
    }

    /// A session that started a local server confines that server, and the note says so rather
    /// than that nothing is confined; the servers line names what started, and says none where
    /// nothing did.
    #[test]
    fn the_servers_a_session_started_are_named_and_what_is_confined_follows_them() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let line = |report: &Report, label: &str| {
            report
                .lines
                .iter()
                .find(|line| line.label.trim() == label)
                .map(|line| (line.value.clone(), line.note.clone()))
                .expect("the line is on the report")
        };

        let none = report(&facts(&config, &trust));
        assert_eq!(
            line(&none, t!(status_mcp_servers)),
            (t!(status_mcp_servers_none).to_string(), String::new())
        );

        let started = crate::state::Servers {
            started: vec!["docs".to_string(), "weather".to_string()],
            confined: true,
            notes: Vec::new(),
            session: None,
            disabled: Vec::new(),
        };
        let some = report(&Facts {
            servers: &started,
            ..facts(&config, &trust)
        });
        assert_eq!(line(&some, t!(status_mcp_servers)).0, "docs, weather");
        assert_eq!(
            line(&some, t!(status_confinement)),
            (
                "kernel-enforced".to_string(),
                t!(status_confinement_servers).to_string()
            )
        );

        let remote = crate::state::Servers {
            started: vec!["docs".to_string()],
            confined: false,
            notes: Vec::new(),
            session: None,
            disabled: Vec::new(),
        };
        let unconfined = report(&Facts {
            servers: &remote,
            ..facts(&config, &trust)
        });
        assert_eq!(
            line(&unconfined, t!(status_confinement)).1,
            t!(status_confinement_nothing_confined),
            "a remote server is no process this session confines"
        );
    }

    /// Each server's clause says how its tools stand, since a started server whose list nobody has
    /// read offers the model nothing and a declined one offers nothing for the rest of the session.
    #[test]
    fn each_servers_note_says_how_its_tools_stand() {
        use bravebot_agent::mcp::Offering;
        let note = offering(&[
            ("docs".to_string(), Offering::Unasked),
            ("weather".to_string(), Offering::Tools(3)),
            ("coins".to_string(), Offering::Tools(1)),
            ("maps".to_string(), Offering::Declined),
        ]);
        assert_eq!(
            note,
            [
                t!(status_mcp_servers_unread, alias = "docs"),
                t!(status_mcp_servers_tools, alias = "weather", count = 3),
                t!(status_mcp_servers_tools, alias = "coins", count = 1),
                t!(status_mcp_servers_declined, alias = "maps"),
            ]
            .join("; ")
        );
        assert!(note.contains("weather: 3 tools"), "{note}");
        assert!(note.contains("coins: one tool"), "{note}");
    }

    /// The policy a started server's backend was handed, drawn for the tests of `/status`.
    fn a_policy_for_the_report() -> EffectivePolicy {
        use bravebot_agent::servers::PolicyRow;
        let row = |path: &str, access| PolicyRow {
            path: path.into(),
            access,
        };
        EffectivePolicy {
            rows: vec![
                row("/srv/weather", Access::Read),
                row("/srv/cache", Access::ReadWrite),
                row("/var/out", Access::Write),
            ],
            egress: PolicyEgress::Hosts,
            children: false,
            starts_in: Some("/srv/cache".into()),
            left_out: vec!["/opt/absent".into(), "/opt/also absent".into()],
            git_held_back: true,
        }
    }

    /// SERVERS-20: `/status` lists what a started server's backend was handed, one fact to a line:
    /// the network, whether it may start subprocesses, where it starts, whether a `.git` is held
    /// back, every row with what it allows, and the paths left out with the reason. A row is drawn
    /// as the access it grants and no other, so a write-only row is not read as a read.
    #[test]
    fn a_started_servers_policy_is_listed_row_by_row_with_what_each_row_allows() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let started =
            crate::mcp_command::tests::holding_started_under("weather", a_policy_for_the_report());

        let drawn = report(&Facts {
            servers: &started.servers,
            ..facts(&config, &trust)
        });

        let at = drawn
            .lines
            .iter()
            .position(|line| line.label == t!(status_server_policy))
            .expect("the server's policy is on the report");
        let listed: Vec<(&str, &str, &str)> = drawn.lines[at..]
            .iter()
            .map(|line| (line.label.as_str(), line.value.as_str(), line.note.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                (t!(status_server_policy), "weather", ""),
                (
                    t!(status_server_policy_egress),
                    t!(status_server_policy_egress_hosts),
                    ""
                ),
                (
                    t!(status_server_policy_subprocesses),
                    t!(status_server_policy_subprocesses_refused),
                    ""
                ),
                (t!(status_server_policy_starts_in), "/srv/cache", ""),
                (
                    t!(status_server_policy_git),
                    t!(status_server_policy_git_held_back),
                    ""
                ),
                (t!(status_server_policy_read), "/srv/weather", ""),
                (t!(status_server_policy_read_write), "/srv/cache", ""),
                (t!(status_server_policy_write), "/var/out", ""),
                (
                    t!(status_server_policy_left_out),
                    t!(status_server_policy_left_out_reason),
                    ""
                ),
                ("", "/opt/absent", ""),
                ("", "/opt/also absent", ""),
            ]
        );
    }

    /// SERVERS-20: a session with no confined server has no policy to show, whether it started
    /// nothing or only a server reached over HTTP, which is no process of the session's.
    #[test]
    fn a_session_with_no_confined_server_shows_no_server_policy() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let remote = crate::mcp_command::tests::holding(&["docs"]);

        for servers in [&NO_SERVERS, &remote.servers] {
            let drawn = report(&Facts {
                servers,
                ..facts(&config, &trust)
            });

            assert!(
                drawn
                    .lines
                    .iter()
                    .all(|line| line.label != t!(status_server_policy)),
                "a policy was shown for a server that has none"
            );
        }
    }

    /// SERVERS-20: a path is drawn in full as the prompts that name one draw it, so a control
    /// character in a directory name is its picture and not a sequence the terminal obeys, and a
    /// character that draws as nothing is spelled out so two rows cannot look alike.
    #[test]
    fn a_path_in_a_servers_policy_is_drawn_without_sending_its_control_characters() {
        let mut policy = a_policy_for_the_report();
        policy.rows[0].path = "/srv/\u{1b}[2Jweather".into();
        policy.rows[1].path = "/srv/data\u{3164}".into();
        policy.rows[2].path = "/srv/my cache".into();
        policy.left_out = vec!["/opt/\u{1b}[2Jabsent".into()];

        let lines = server_policy_lines(&[("weather".to_string(), policy)]);

        let values: Vec<&str> = lines.iter().map(|line| line.value.as_str()).collect();
        assert_eq!(
            values
                .iter()
                .filter(|value| value.contains("\u{241b}[2J"))
                .count(),
            2,
            "a row and a path left out hold the escape as its picture: {values:?}"
        );
        assert!(values.contains(&r#""/srv/data\u{3164}""#), "{values:?}");
        assert!(values.contains(&"/srv/my cache"), "{values:?}");
        assert!(
            values
                .iter()
                .all(|value| !value.contains('\u{1b}') && !value.contains('\u{3164}')),
            "a character that is not drawn as itself reached the screen: {values:?}"
        );
    }

    /// The markings a write recorded are the part nothing else reports: a poisoned file is otherwise
    /// invisible until something refuses to read it.
    #[test]
    fn an_untrusted_path_a_write_recorded_is_reported() {
        let config = config_for("http://127.0.0.1:1", None);
        let mut trust = trusting();
        trust.distrust("vendor/lib.js");

        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(shown.contains("vendor/lib.js"), "{shown}");
        assert!(shown.contains("untrusted"), "{shown}");
    }

    /// Every rule is readable back however many there are. A session that wrote a dozen files
    /// holds a rule each, and a rule the panel will not show is one whose subject has to be
    /// remembered instead, which is the thing this report exists to save anyone doing.
    #[test]
    fn every_trust_rule_is_listed_however_many_there_are() {
        let config = config_for("http://127.0.0.1:1", None);
        let mut trust = trusting();
        for index in 0..12 {
            trust.distrust(&format!("file{index}.txt"));
        }

        let shown = rendered(&report(&facts(&config, &trust)));
        for index in 0..12 {
            assert!(shown.contains(&format!("file{index}.txt")), "{shown}");
        }
    }

    /// PROMPT-1: a rule about a name made of whitespace, or ending in it, is a row that says so. The
    /// name as held draws as an empty row for the first and as another name for the second.
    #[test]
    fn a_rule_about_a_name_that_would_draw_as_another_is_listed_as_what_it_is() {
        let config = config_for("http://127.0.0.1:1", None);
        let mut trust = trusting();
        trust.distrust(" ");
        trust.distrust("note.md ");
        trust.distrust("a\u{202e}b");
        trust.distrust("note.md");

        let shown = rendered(&report(&facts(&config, &trust)));
        for spelled in ["\"\u{2423}\"", "\"note.md\u{2423}\"", "\"a\\u{202e}b\""] {
            assert!(shown.contains(spelled), "{spelled}\n{shown}");
        }
        // The plain name and the root keep the rows they had, and the whitespace-ended one is not
        // one of them.
        assert!(shown.contains("note.md untrusted"), "{shown}");
        assert!(!shown.contains("note.md  untrusted"), "{shown}");
    }

    /// A directory opened with /add-dir is reachable and vouched for, so a panel that omitted it
    /// would understate what the session can touch.
    #[test]
    fn an_added_directory_is_reported() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let added = vec![std::path::PathBuf::from("/tmp/notes")];
        let mut with_added = facts(&config, &trust);
        with_added.added_directories = &added;

        let shown = rendered(&report(&with_added));
        assert!(shown.contains("/tmp/notes"), "{shown}");
        assert!(shown.contains("added with /add-dir"), "{shown}");
    }

    /// The session's own directory outside the project, which nothing else on the panel can
    /// report: it carries no trust rule, so the trust lines say nothing about it.
    #[test]
    fn the_sessions_scratch_directory_is_reported_for_what_it_is() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let scratch = std::path::PathBuf::from("/tmp/bravebot-scratch-1-2-3");
        let mut with_scratch = facts(&config, &trust);
        with_scratch.scratch = Some(&scratch);

        let shown = rendered(&report(&with_scratch));
        assert!(shown.contains("/tmp/bravebot-scratch-1-2-3"), "{shown}");
        assert!(shown.contains(&*t!(status_scratch_note)), "{shown}");
    }

    /// A session that could not be given one says nothing about a directory it does not have.
    #[test]
    fn a_session_with_no_scratch_directory_reports_none() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(!shown.contains(&*t!(status_scratch)), "{shown}");
    }

    /// Each checkout is a line of its own, under the directories and the session's own, with its
    /// number, its commit and the delegate it was made for.
    #[test]
    fn each_checkout_the_session_has_is_a_line_of_its_own() {
        use bravebot_agent::workspace::SessionCheckout;
        use bravebot_core::delegate::DelegateId;
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let checkouts = [
            SessionCheckout {
                applied: false,
                id: "c1".into(),
                path: "/state/checkouts/work/c1".into(),
                repository: "/work/.git".into(),
                commit: "0123456789abcdef0123456789abcdef01234567".into(),
                delegate: DelegateId::nth(1),
                worked_in: true,
                candidates: Default::default(),
                size: None,
            },
            SessionCheckout {
                applied: false,
                id: "c3".into(),
                path: "/state/checkouts/work/c3".into(),
                repository: "/work/.git".into(),
                commit: "fedcba9876543210fedcba9876543210fedcba98".into(),
                delegate: DelegateId::nth(2).child(1).expect("a child"),
                worked_in: true,
                candidates: Default::default(),
                size: None,
            },
        ];
        let added = [std::path::PathBuf::from("/tmp/beside")];
        let mut with_checkouts = facts(&config, &trust);
        with_checkouts.checkouts = &checkouts;
        with_checkouts.added_directories = &added;
        with_checkouts.scratch = Some(Path::new("/tmp/scratch"));

        let lines = report(&with_checkouts).lines;
        let at = |path: &str| {
            lines
                .iter()
                .position(|line| line.value == path)
                .unwrap_or_else(|| panic!("no line for {path}: {lines:?}"))
        };
        let (first, second) = (
            at("/state/checkouts/work/c1"),
            at("/state/checkouts/work/c3"),
        );
        assert_eq!(second, first + 1);
        for directory in ["/tmp/project", "/tmp/beside", "/tmp/scratch"] {
            assert!(
                first > at(directory),
                "a checkout is listed above {directory}"
            );
        }
        assert_eq!(lines[first].label, t!(status_checkout));
        assert_eq!(
            lines[first].note,
            t!(
                status_checkout_note,
                id = "c1",
                commit = "0123456789",
                delegate = "d1"
            )
        );
        assert_eq!(
            lines[second].note,
            t!(
                status_checkout_note,
                id = "c3",
                commit = "fedcba9876",
                delegate = "d2.1"
            )
        );
    }

    /// The groups `report` is built from keep the order the rows have always had when every one of
    /// them has something to say: what was chosen (model, effort, theme, endpoint), then what
    /// holds the session back, what runs unattended, what the session cost, and the trust rules
    /// last. A group drawn out of turn, or a call dropped from `report`, moves one of the
    /// boundaries below.
    #[test]
    fn the_groups_of_rows_keep_their_order_when_every_group_has_rows() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut goal = crate::goals::Running::begin("cargo test exits 0".to_string(), 0, false);
        goal.not_met("nothing above runs the tests".to_string(), 1);
        let mut mixed = facts(&config, &trust);
        mixed.goal = Some(&goal);
        mixed.auto_vetting = true;
        mixed.timing = bravebot_agent::timing::Timing {
            wall_ms: 30_000,
            inference_ms: 20_000,
            tools_ms: 5_000,
            stalled_ms: 0,
        };

        let lines = report(&mixed).lines;
        let at = |label: String| {
            lines
                .iter()
                .position(|line| line.label == label)
                .unwrap_or_else(|| panic!("no row labelled {label:?}: {lines:?}"))
        };
        let order = [
            at(t!(status_model).to_string()),
            at(t!(status_effort).to_string()),
            at(t!(status_theme).to_string()),
            at(t!(status_endpoint).to_string()),
            at(t!(status_confinement).to_string()),
            at(t!(status_mcp_servers).to_string()),
            at(t!(status_vetting).to_string()),
            at(t!(status_goal).to_string()),
            at(t!(status_this_session).to_string()),
            at(t!(status_time).to_string()),
            at(t!(status_trust).to_string()),
        ];
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "the groups are out of order: {order:?}"
        );
    }

    /// A session with no checkout says nothing about checkouts.
    #[test]
    fn a_session_with_no_checkout_reports_none() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();

        let lines = report(&facts(&config, &trust)).lines;
        assert!(
            lines.iter().all(|line| line.label != t!(status_checkout)),
            "{lines:?}"
        );
    }

    /// A total is unactionable. The panel has to say which of the three things took the time, since
    /// the answer decides whether a person wants a faster model, a faster test suite, or fewer
    /// prompts.
    #[test]
    fn the_panel_says_where_the_session_spent_its_time() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut spent = facts(&config, &trust);
        spent.timing = bravebot_agent::timing::Timing {
            wall_ms: 600_000,
            inference_ms: 120_000,
            tools_ms: 60_000,
            stalled_ms: 400_000,
        };

        let shown = rendered(&report(&spent));
        assert!(
            shown.contains("10m 00s"),
            "the total was not reported: {shown}"
        );
        assert!(shown.contains("waiting on you"), "{shown}");
        // The remainder is the figure with nobody to blame for it, and it is the one nothing else
        // can show.
        assert!(shown.contains("unaccounted for"), "{shown}");
        assert!(
            shown.contains("6m 40s"),
            "the stall was not reported: {shown}"
        );
    }

    /// A session that has measured nothing has nothing to say about where its time went, and a
    /// panel of zeroes reads as a broken feature rather than as a session that has not started.
    #[test]
    fn a_session_with_no_turn_yet_reports_no_time() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let shown = rendered(&report(&facts(&config, &trust)));
        assert!(!shown.contains("waiting on you"), "{shown}");
        assert!(!shown.contains("unaccounted for"), "{shown}");
    }

    /// A part that did not happen says nothing rather than `0s`, which beside a note about where the
    /// time went reads as a measurement rather than as an absence.
    #[test]
    fn a_part_that_never_happened_is_not_reported_as_zero() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut quiet = facts(&config, &trust);
        // A session that was never asked anything and ran no tool: all of it on the model.
        quiet.timing = bravebot_agent::timing::Timing {
            wall_ms: 30_000,
            inference_ms: 30_000,
            tools_ms: 0,
            stalled_ms: 0,
        };

        let shown = rendered(&report(&quiet));
        assert!(shown.contains("on the model"), "{shown}");
        assert!(
            !shown.contains("waiting on you"),
            "a stall that never happened was reported: {shown}"
        );
        assert!(!shown.contains("running tools"), "{shown}");
    }

    /// The token count says what the turn sent, which is the same figure whether the service read
    /// the prompt or recognised it. Only this pair says which, and a person asking whether caching
    /// is working has nowhere else to look.
    #[test]
    fn the_panel_says_how_much_of_the_prompt_came_out_of_the_cache() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut hit = facts(&config, &trust);
        hit.cached = Some(bravebot_aichat::protocol::Cached {
            read_tokens: 41_200,
            written_tokens: 1_800,
        });

        let shown = rendered(&report(&hit));
        assert!(shown.contains("served from the cache"), "{shown}");
        assert!(
            shown.contains("41.2k"),
            "the read was not reported: {shown}"
        );
        assert!(shown.contains("written to it for the next turn"), "{shown}");
        assert!(
            shown.contains("1.8k"),
            "the write was not reported: {shown}"
        );
        // The two are priced differently, so their sum is the one figure that says nothing about
        // what the turn cost. Reporting it would undo the split the rest of this reports.
        assert!(
            !shown.contains("43k"),
            "the read and the write were added together: {shown}"
        );
        // The counts above this line are the session's, so the heading has to say this one is not.
        assert!(shown.contains("last turn"), "{shown}");
    }

    /// A read of zero after a pause means one thing under a five-minute lifetime and another under an
    /// hour's, so the report has to say which the settings asked for. Each setting is checked against
    /// the other two so a swapped or constant answer fails.
    #[test]
    fn the_panel_says_which_cache_lifetime_the_settings_asked_for() {
        use bravebot_config::CacheTtl;

        let words = ["5 minutes", "1 hour", "the service's own"];
        let cases = [
            (Some(CacheTtl::FiveMinutes), words[0]),
            (Some(CacheTtl::OneHour), words[1]),
            (None, words[2]),
        ];
        for (chosen, expected) in cases {
            let mut config = config_for("http://127.0.0.1:1", None);
            config.prompt_cache_ttl = chosen;
            let trust = trusting();
            let mut turn = facts(&config, &trust);
            turn.cached = Some(bravebot_aichat::protocol::Cached {
                read_tokens: 0,
                written_tokens: 2_400,
            });

            let shown = rendered(&report(&turn));
            assert!(shown.contains("lifetime asked of the service"), "{shown}");
            for word in words {
                assert_eq!(
                    shown.contains(word),
                    word == expected,
                    "{chosen:?} should show {expected:?} and no other: {shown}"
                );
            }
        }
    }

    /// Every backend but Bedrock reports nothing about a cache. Presenting that as a session whose
    /// cache missed would be reporting a measurement nobody took, and it is the reading a person
    /// would take from two zeroes.
    #[test]
    fn a_backend_that_reports_nothing_about_a_cache_gets_no_cache_lines() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut silent = facts(&config, &trust);
        silent.cached = Some(bravebot_aichat::protocol::Cached::default());

        let shown = rendered(&report(&silent));
        assert!(!shown.contains("served from the cache"), "{shown}");
        assert!(!shown.contains("Prompt cache"), "{shown}");
        assert!(!shown.contains("lifetime"), "{shown}");
    }

    /// A turn that established a prefix and read nothing back reports the write alone, on the
    /// footing a part of the time report that never happened says nothing rather than `0`.
    #[test]
    fn a_turn_that_only_wrote_to_the_cache_does_not_report_a_read_of_zero() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut first = facts(&config, &trust);
        first.cached = Some(bravebot_aichat::protocol::Cached {
            read_tokens: 0,
            written_tokens: 2_400,
        });

        let shown = rendered(&report(&first));
        assert!(shown.contains("written to it for the next turn"), "{shown}");
        assert!(
            !shown.contains("served from the cache"),
            "a read that never happened was reported: {shown}"
        );
    }

    /// A session with nothing sent has no name yet, and saying so is better than an empty line.
    #[test]
    fn a_session_with_no_name_says_so() {
        let config = config_for("http://127.0.0.1:1", None);
        let trust = trusting();
        let mut fresh = facts(&config, &trust);
        fresh.session_name = "";

        let shown = rendered(&report(&fresh));
        assert!(shown.contains("nothing sent yet"), "{shown}");
    }

    #[test]
    fn counts_read_the_way_a_person_says_them() {
        assert_eq!(t!(count_turns, count = 1), "1 turn");
        assert_eq!(t!(count_turns, count = 0), "0 turns");
        assert_eq!(t!(count_rules, count = 4), "4 rules");
        assert_eq!(tokens(940), "940 tokens");
        assert_eq!(tokens(1), "1 token");
        assert_eq!(tokens(12_400), "12.4k tokens");
    }

    /// The home directory is both longer and more personal than `~`, and a status panel is pasted
    /// into issues.
    #[test]
    fn a_path_under_home_is_abbreviated() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let home = Path::new(&home);
        assert_eq!(
            abbreviate(&home.join("projects/bravebot")),
            "~/projects/bravebot"
        );
        assert_eq!(abbreviate(Path::new("/tmp/elsewhere")), "/tmp/elsewhere");
    }

    /// `/home/alice` is not inside `/home/al`; it only starts with the same characters. Stripping
    /// the prefix as text would draw it as `~ice/data`.
    #[test]
    fn a_sibling_whose_name_starts_with_homes_is_not_abbreviated() {
        assert_eq!(
            abbreviate_against(
                Path::new("/home/alice/data"),
                Some(std::ffi::OsStr::new("/home/al"))
            ),
            "/home/alice/data"
        );
    }

    /// A trailing slash on `HOME` is not a component of it.
    #[test]
    fn a_trailing_slash_on_home_does_not_change_the_abbreviation() {
        assert_eq!(
            abbreviate_against(
                Path::new("/home/al/out"),
                Some(std::ffi::OsStr::new("/home/al/"))
            ),
            "~/out"
        );
    }

    /// `HOME` set to the root, as some containers and service accounts set it, abbreviates
    /// nothing: every absolute path starts with `/`, so stripping it as a prefix would draw every
    /// path as though it were inside home.
    #[test]
    fn home_set_to_the_root_abbreviates_nothing() {
        let root = Some(std::ffi::OsStr::new("/"));
        assert_eq!(abbreviate_against(Path::new("/usr/lib"), root), "/usr/lib");
        assert_eq!(abbreviate_against(Path::new("/tmp"), root), "/tmp");
    }
}
