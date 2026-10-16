//! The workspace, trust map, rules and definition a run starts with.

use bravebot_agent::Workspace;
use bravebot_config::{Config, Managed};
use bravebot_core::event::RecordingSink;
use bravebot_i18n::t;
use std::path::Path;

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
}

/// The directory this run writes what is not part of the project into, made and reachable.
///
/// Nothing on a machine that cannot give it one, which is not a reason to refuse to run: a full or
/// read-only temporary directory leaves a turn with nowhere to put an intermediate file and nothing
/// else. On stderr, beside every other line this run has to say about itself, because a turn told
/// there is nowhere to write leaves nothing else to read it off.
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
    workspace.open_scratch(scratch.as_ref().map(|held| held.path().to_path_buf()));
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
        eprintln!("{}", t!(cli_notice, notice = notice));
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
            bravebot_tui::trust_prompt::trusting_the_workspace(root),
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

/// Open every directory the command line named, or say which one could not be opened.
///
/// Reachability and nothing else. `/add-dir` grants a second thing, recording that the person
/// vouched for the directory, and this deliberately does not: a run nobody is watching holds an
/// empty trust map, the directory it was started in included, so a rule trusting a sibling
/// checkout would leave the tree the run was pointed at more trusted than the one it works in.
/// Reads there are on the same footing as reads of the project's own files.
///
/// `~` is left to the shell, which expands it before this ever sees the path. A path that is not
/// absolute, does not exist, is not a directory, or lies inside the working one is refused by the
/// workspace, and the refusal names which.
///
/// Returns what the person is to be told about the ones that opened: a directory holding the
/// working directory leaves no delegate a checkout (CHECKOUT-7), and a run is where nobody is
/// watching to find that out.
pub(crate) fn open_directories(
    workspace: &mut Workspace,
    directories: &[String],
) -> Result<Vec<String>, String> {
    let mut notices = Vec::new();
    for directory in directories {
        let added = workspace.add_directory(directory).map_err(|problem| {
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
    }
    Ok(notices)
}

/// The rules a one-shot run works under.
///
/// Nobody is watching one, so the allow list is left out: an allow rule answers a prompt in
/// advance, and where there is nobody to ask it would instead be a line in a settings file letting
/// the run write, execute or fetch unwatched, beside the flag that is meant to be the only way
/// that happens. The deny and ask lists carry over, and both still decide something here.
///
/// With the flag, the person answered every one of those prompts themselves when they typed it, so
/// their file is read whole.
pub(crate) fn rules_for_a_one_shot_run(
    settings: &bravebot_config::Settings,
    profile: Option<&Path>,
    workspace: &Path,
    skip_permissions: bool,
) -> (
    bravebot_core::permissions::Permissions,
    Vec<bravebot_core::permissions::Rejected>,
) {
    match skip_permissions {
        true => bravebot_agent::permissions::from_settings(settings, profile, workspace),
        false => bravebot_agent::permissions::for_an_unattended_run(settings, profile, workspace),
    }
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
            }
            .to_string(),
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
