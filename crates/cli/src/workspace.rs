//! The workspace, trust map, rules and definition a run starts with.

use crate::exit::{Ending, fail};
use bravebot_agent::Workspace;
use bravebot_config::{Config, Managed};
use bravebot_core::event::RecordingSink;
use bravebot_i18n::t;
use std::path::Path;
use std::process::ExitCode;

/// The workspace is the current directory: file arguments resolve relative to it, and
/// confinement keeps reads inside it.
///
/// The search caps come in from the settings here rather than being read inside the workspace,
/// because a workspace is built by every test in the tree and one that read the settings would
/// answer differently on a machine whose owner had configured them.
///
/// Whether the file tools are held to that directory comes in the same way and from the same place
/// (PERM-16). The managed file is read beside the settings layers, being another layer and one that
/// may ask for this: the strictest thing any of them said is what holds (PERM-18).
pub(crate) fn current_workspace(
    settings: &bravebot_config::Settings,
    managed: &Managed,
) -> Result<Workspace, String> {
    let caps = settings.search();
    let inside = settings
        .narrowing()
        .strictest(managed.narrowing())
        .keeps_reads_in_the_workspace();
    std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| Workspace::new(dir).map_err(|e| e.to_string()))
        .map(|workspace| workspace.with_search_caps(caps.files, caps.time))
        .map(|workspace| workspace.with_reads_kept_inside(inside))
        .map(|workspace| {
            workspace.with_references(settings.references(), settings.references_unread())
        })
        .map(|workspace| {
            workspace.with_shared_skill_directories(settings.shared_skill_directories())
        })
}

/// The exit for a workspace that could not be opened, with the problem drawn: it can name a
/// directory a checkout chose (LAYER-3).
pub(crate) fn workspace_problem(problem: &str) -> ExitCode {
    fail(
        Ending::Failed,
        crate::progress::printable_lines(&t!(cli_workspace_problem, problem = problem)),
    )
}

/// The directory this run writes what is not part of the project into, made and reachable.
///
/// Nothing on a machine that cannot give it one, which is not a reason to refuse to run: a full or
/// read-only temporary directory leaves a turn with nowhere to put an intermediate file and nothing
/// else. On stderr, beside every other line this run has to say about itself, because a turn told
/// there is nowhere to write leaves nothing else to read it off.
///
/// The problem is printed as it stands, without [`crate::progress::printable_lines`] (LAYER-3).
/// `SessionScratch::create` fails only with an `io::Error` that the operating system made, and
/// that error's text is the system's own words and an error number. It holds no path, no
/// environment value and nothing a checkout or a model chose, so there is no control character in
/// it to draw. A test in `bravebot-agent` pins that the error names neither the directory it was
/// asked to make nor anything else beyond the system's text.
pub(crate) fn scratch_for_this_run(
    workspace: &mut Workspace,
) -> Option<bravebot_agent::SessionScratch> {
    let scratch = match bravebot_agent::SessionScratch::create() {
        Ok(scratch) => Some(scratch),
        Err(problem) => {
            eprintln!(
                "{}",
                t!(
                    cli_notice,
                    notice = t!(session_scratch_unavailable, problem = problem.to_string())
                )
            );
            None
        }
    };
    workspace.open_scratch(
        scratch
            .as_ref()
            .and_then(bravebot_agent::SessionScratch::directory),
    );
    bravebot_session::sessions::sweep_unlisted_checkouts(workspace);
    scratch
}

/// The trust map a one-shot run opens with (TRUST-23, TRUST-26).
///
/// What a yes to the startup question writes where the person typed `--trust-workspace`, or where
/// they kept an answer that settles a session here (TRUST-23): one about this directory or the
/// root of the git worktree around it. Both are their own gesture, so neither is a guess made in
/// their name (CLI-1). Anything else is the empty map. Nothing is written, and a kept answer is
/// said on stderr, since a run nobody watches is where it would otherwise go unnoticed.
pub(crate) fn unattended_trust(root: &Path, flagged: bool) -> bravebot_core::TrustStore {
    let (trust, notice) = unattended_trust_with(
        bravebot_agent::home::directory().as_deref(),
        bravebot_agent::home::profile().as_deref(),
        root,
        flagged,
    );
    if let Some(notice) = notice {
        crate::finish::say_notices(&mut std::io::stderr().lock(), &[notice]);
    }
    trust
}

/// [`unattended_trust`] with the state directory and the home given, and the notice returned.
pub(crate) fn unattended_trust_with(
    home: Option<&Path>,
    profile: Option<&Path>,
    root: &Path,
    flagged: bool,
) -> (bravebot_core::TrustStore, Option<String>) {
    if flagged {
        return (
            bravebot_tui::trust_prompt::trusting_the_workspace(root),
            None,
        );
    }
    match bravebot_agent::trusted::honoured(home, profile, root) {
        Some(bravebot_agent::trusted::Honoured { store, kept }) => (
            bravebot_tui::trust_prompt::trusting_as_kept(root, &kept),
            Some(t!(
                cli_trusting_kept,
                directory = kept.root.display().to_string(),
                when = bravebot_session::sessions::how_long_ago(kept.at),
                path = store.path().display().to_string()
            )),
        ),
        None => (bravebot_agent::workspace::trust_store(root), None),
    }
}

/// What opening the directories the command line named leaves the run with.
#[derive(Debug)]
pub(crate) struct Opened {
    /// What the person is to be told about the ones that opened: a directory holding the working
    /// directory leaves no delegate a checkout (CHECKOUT-7), and a run is where nobody is watching
    /// to find that out.
    pub(crate) notices: Vec<String>,
    /// Each directory as the workspace opened it, in the order the command line named them: the
    /// canonical path, which is what a rule about it is recorded under (TRUST-9).
    pub(crate) directories: Vec<std::path::PathBuf>,
}

/// Open every directory the command line named, or say which one could not be opened.
///
/// Reachability only. Whether the run trusts what it opened is decided from the map the run
/// starts with, by [`trusting_the_added_directories`], once that map exists.
///
/// A relative name, such as `--add-dir ../notes`, is resolved against the working directory
/// first (CLI-11), through the one implementation `/cd` and `/add-dir` resolve a name against it
/// with (TRUST-9), so the two cannot come to resolve the same name apart.
///
/// `~` is left to the shell, which expands it before this ever sees the path: unlike a relative
/// name, an unexpanded `~` is read as an ordinary directory component rather than as the home
/// directory, since giving it a second, possibly different reading here is a second place for a
/// `~` the person types to resolve apart from the one the shell already gave it.
///
/// A path that is not absolute once resolved, does not exist, is not a directory, or lies inside
/// the working one is refused by the workspace, and the refusal names which.
pub(crate) fn open_directories(
    workspace: &mut Workspace,
    directories: &[String],
) -> Result<Opened, String> {
    let mut notices = Vec::new();
    let mut opened = Vec::new();
    for directory in directories {
        let named = bravebot_agent::workspace::named_directory(workspace.root(), directory)
            .map_err(|problem| {
                t!(
                    session_directory_not_added,
                    directory = directory,
                    problem = problem.to_string()
                )
            })?;
        let added = workspace.add_directory(&named).map_err(|problem| {
            t!(
                session_directory_not_added,
                directory = directory,
                problem = problem.to_string()
            )
        })?;
        if workspace.ends_checkouts(&added) {
            notices.push(t!(
                cli_directory_ends_checkouts,
                directory = added.display().to_string()
            ));
        }
        opened.push(added);
    }
    Ok(Opened {
        notices,
        directories: opened,
    })
}

/// `trust` with each directory the command line opened vouched for, where `trust` vouches for the
/// working directory (CLI-11, TRUST-26).
///
/// The rule `/add-dir` records in a session (TRUST-9), under the canonical path the directory was
/// opened at, so it covers that directory and what is beneath it and nothing beside or above it.
/// The person who trusted the directory the run works in, with `--trust-workspace`, a kept answer
/// or the session the run carries on, also named these directories on the same command line, which
/// is the gesture `/add-dir` is in a session whose map trusts its directory. A map that does not
/// trust the working directory is returned as it is: a rule trusting a directory beside it would
/// leave that directory more trusted than the tree the run works in.
///
/// Only `opened` is vouched for. A directory a settings file declares as a reference is opened the
/// same way and records no trust (REFER-3), so the directories the workspace holds are not read
/// here. Nothing is written to the store of kept answers.
pub(crate) fn trusting_the_added_directories(
    mut trust: bravebot_core::TrustStore,
    opened: &[std::path::PathBuf],
) -> bravebot_core::TrustStore {
    if trust.is_trusted(".") {
        for directory in opened {
            trust.trust(&bravebot_agent::workspace::key_of(directory));
        }
    }
    trust
}

/// The rules a one-shot run works under, which are the rules a session in `workspace` works under
/// (CLI-1).
///
/// All three lists are read from the same layers a session reads. An allow rule states what the
/// person wants done with an effect before it comes up, and that does not depend on whether
/// anybody is watching. An `allow` rule a checkout's file proposed is in force only where this
/// workspace's record already holds the person's grant of it (PERM-15): nobody is here to be asked
/// for one, so a rule nobody granted stays out of force, as it does in a session that declines.
///
/// `state` is the state directory the record of grants is kept in ([`bravebot_agent::home::directory`]),
/// taken as an argument so a test reads a record of its own; `None` reads no grant.
pub(crate) fn rules_for_a_one_shot_run(
    settings: &bravebot_config::Settings,
    state: Option<&Path>,
    profile: Option<&Path>,
    workspace: &Path,
) -> (
    bravebot_core::permissions::Permissions,
    Vec<bravebot_core::permissions::Rejected>,
) {
    let proposed = bravebot_agent::permissions::proposed(settings, profile);
    let granted: Vec<String> = state
        .map(|state| {
            bravebot_agent::granted::Store::new(state, workspace)
                .granted(&proposed)
                .into_iter()
                .map(|rule| rule.rule.clone())
                .collect()
        })
        .unwrap_or_default();
    bravebot_agent::permissions::with_granted(settings, &granted, profile, workspace)
}

/// The model the definition `--agent` named will ask for, where it names one the command line did
/// not outrank, matched as the run's first turn will match it.
///
/// Resolved with the trust a run hands its turn, which vouches for no directory, so a checkout's
/// definitions are counted but not read. The count goes into the refusal because a name missing
/// from an untrusted project and a name missing everywhere need different fixes.
pub(crate) fn definition_for_a_run(
    config: &Config,
    workspace: &Workspace,
    permissions: &bravebot_core::permissions::Permissions,
    name: &str,
    model_named: bool,
) -> Result<Option<String>, bravebot_tui::app::Unusable> {
    let trust = bravebot_agent::workspace::trust_store(workspace.root());
    let definitions = bravebot_agent::agents::resolved(
        workspace,
        bravebot_agent::home::directory().as_deref(),
        trust.clone(),
        permissions.clone(),
        &mut RecordingSink::new(),
    );
    if definitions.get(name).is_none() {
        let names = definitions.names().join(", ");
        return Err(bravebot_tui::app::Unusable::Missing(
            match bravebot_agent::agents::not_vouched_for(workspace, &trust) {
                0 => t!(cli_agent_no_such_definition, name = name, names = names),
                count => t!(
                    cli_agent_no_such_definition_unread,
                    name = name,
                    names = names,
                    count = count
                ),
            },
        ));
    }
    // Nothing asks for the definition's model when the command line named one, so its sign-in
    // does not matter.
    if model_named {
        return Ok(None);
    }
    bravebot_tui::app::definition_named(config, &definitions, name).map(|addressed| addressed.model)
}

/// What to call the confinement that was achieved.
///
/// Named here rather than by `bravebot_sandbox`, whose Display is a diagnostic and whose whole
/// point is to hold no words meant for a person: it depends on nothing, and a catalog is a
/// dependency. So it reports which of the three it got and this says it in the reader's language.
pub(crate) fn confinement_name(level: bravebot_sandbox::policy::ConfinementLevel) -> String {
    use bravebot_sandbox::policy::ConfinementLevel;
    match level {
        ConfinementLevel::Kernel => t!(confinement_kernel),
        ConfinementLevel::Partial => t!(confinement_partial),
        ConfinementLevel::None => t!(confinement_none),
    }
    .to_string()
}
