//! `bravebot doctor`: the report of how this build and machine are configured, and the functions
//! that build each of its sections.

use crate::exit::{Ending, fail};
use crate::model::{how_to_configure_a_model, managed_refusal, model_for_this_run};
use crate::workspace::{confinement_name, current_workspace, unattended_trust_with};
use crate::{auth, import, mcp, progress, sandbox_refusal};
use bravebot_agent::Workspace;
use bravebot_config::{Config, Managed};
use bravebot_i18n::t;
use bravebot_net::Transport;
use bravebot_net::transport::{
    CERTIFICATE_DIRECTORY, CERTIFICATE_FILE, PROXY_VARIABLES, TrustRoots,
};
use bravebot_sandbox::SandboxError;
use bravebot_sandbox::policy::Capabilities;
use std::path::Path;
use std::process::ExitCode;

/// The line `doctor` gives the sandbox mode: which it is and where it was chosen, or what the
/// managed file refuses.
///
/// Resolved from the files alone. A flag on `doctor` is not part of what the machine is set to, and
/// the report is of that.
pub(crate) fn sandbox_mode_line(
    settings: &bravebot_config::Settings,
    managed: &Managed,
) -> Result<String, String> {
    use bravebot_config::sandbox::{Source, resolve};
    let label = t!(doctor_sandbox_mode);
    match resolve(None, settings, managed) {
        Ok(choice) => {
            let detail = match choice.source {
                Source::Default | Source::Flag => {
                    t!(doctor_sandbox_default, mode = choice.mode.name()).to_string()
                }
                Source::File(path) | Source::Managed(path) => t!(
                    doctor_sandbox_from,
                    mode = choice.mode.name(),
                    path = path.display().to_string()
                )
                .to_string(),
            };
            Ok(aligned(label, detail, DETAIL))
        }
        Err(refused) => Err(sandbox_refusal(&refused)),
    }
}

/// Which of two endings a report that found both ends on.
///
/// `doctor` does not stop at the first problem, the whole picture being what somebody runs it
/// for, so the ending is accumulated across the report rather than returned by the line that
/// found one. A later problem is not a more important one and a count of problems is not a
/// status, so the rule is which of the two a caller can do something with: CLI-6's row 3 names
/// what to go and fix, and its row 1 is the catch-all that names nothing. A machine whose
/// configuration cannot be used and whose platform confines nothing is one where the
/// configuration is the thing to change, so the report ends on row 3.
pub(crate) fn ends_on(found: Ending, also: Ending) -> Ending {
    match (found, also) {
        (Ending::Configuration, _) | (_, Ending::Configuration) => Ending::Configuration,
        (Ending::Done, other) | (other, Ending::Done) => other,
        (kept, _) => kept,
    }
}

/// Say something `doctor` found that the report itself has no line for, and record its ending.
///
/// For the two problems the report cannot state: one that stopped it before the configuration
/// section printed anything, and one there is no section for because a platform offering no
/// confinement has no capabilities to describe. Everything else wrong with a machine is a line
/// the report already prints, and saying it a second time here would put the same sentence on
/// both streams.
///
/// It leaves through [`fail`], which is what makes CLI-6's identifier a property of one function
/// rather than of every place a report can go wrong.
fn doctor_found(ending: &mut Ending, found: Ending, problem: impl std::fmt::Display) {
    let _ = fail(found, problem);
    *ending = ends_on(*ending, found);
}

/// Every settings layer that tried to declare an MCP server (SERVERS-1).
///
/// A failure rather than a note: whoever wrote the entry believes a server is configured, and
/// nothing reads it, since the one file that declares a server is `~/.bravebot/mcp.json`. Named by
/// key and file only, because the entry may hold an argv and the values of variables.
fn report_mcp_declared(settings: &bravebot_config::Settings, ending: &mut Ending) {
    for (path, key) in settings.mcp_declared() {
        *ending = ends_on(*ending, Ending::Failed);
        fact(
            t!(doctor_settings_ignored),
            t!(
                doctor_settings_mcp_declared,
                key = mcp::shown(key),
                path = path.display().to_string()
            ),
        );
    }
}

/// One line per skill whose file declares a key nothing here reads.
///
/// Such a key stops nothing: the skill loads with it ignored, which is what lets one directory serve
/// more than one agent. What it needs is to be said somewhere, because its author believes the line
/// is in force and no session tells them otherwise (SKILL-16). Here rather than in a session notice,
/// because almost every skill written for another agent carries one and a line repeated every turn
/// about something that is working is how a notice stops being read.
///
/// The set is read the way a turn starting in this directory would read it, through the same gate,
/// the same trust map and the same rules, so no skill a turn would drop is named and one it would
/// offer is. A project whose directory nobody has vouched for offers none, which is not silence
/// about a mistake: it is the answer a session there would give too.
///
/// Nothing is said about a skill whose every key is read, which is the ordinary case, so a report on
/// a machine with nothing to fix carries no skills section at all.
fn skill_keys_unread(
    workspace: &Workspace,
    home: Option<&Path>,
    trust: bravebot_core::TrustStore,
    permissions: bravebot_core::permissions::Permissions,
) -> Vec<String> {
    let sink = &mut bravebot_core::event::NullSink;
    bravebot_agent::skills::resolved(workspace, home, trust, permissions, sink)
        .iter()
        .filter(|skill| !skill.unread.is_empty())
        .map(|skill| {
            aligned(
                t!(doctor_skill_key_unread),
                t!(
                    doctor_skill_keys_unread,
                    skill = &skill.origin,
                    count = skill.unread.len() as i64,
                    keys = skill.unread.join(", ")
                ),
                FACT,
            )
        })
        .collect()
}

/// The trust map a session starting in this directory would open with, without asking anybody.
///
/// A kept yes answers here as it answers there (TRUST-23); anything else is a directory
/// nothing has vouched for, which is what an unanswered question leaves. Nothing is written and
/// nobody is asked: `doctor` changes nothing and puts no question.
fn trust_already_answered(root: &Path) -> bravebot_core::TrustStore {
    unattended_trust_with(
        bravebot_agent::home::directory().as_deref(),
        bravebot_agent::home::profile().as_deref(),
        root,
        false,
    )
    .0
}

/// What `doctor` says about each entry of the four filesystem lists: the entry with the file or
/// flag that wrote it, and for one that is not in force, why.
///
/// `capabilities` is what the backend a stage would run under reports, so an entry that backend
/// cannot hold back is reported as the refusal a stage meets and not as one in force.
pub(crate) fn sandbox_filesystem_lines(
    rules: &bravebot_sandbox::rules::Rules,
    directory: &Path,
    capabilities: Option<&Capabilities>,
) -> Vec<String> {
    let source = |entry: &bravebot_sandbox::rules::Entry| match &entry.by {
        Some(path) => path.display().to_string(),
        None => t!(doctor_sandbox_filesystem_source_flag).to_string(),
    };
    bravebot_agent::permissions::filesystem_standing(rules, directory, capabilities)
        .into_iter()
        .map(|(item, why)| match why {
            None => t!(
                doctor_sandbox_filesystem_entry,
                key = item.list.key(),
                path = item.entry.path.clone(),
                source = source(&item.entry)
            )
            .to_string(),
            Some(reason) => t!(
                doctor_sandbox_filesystem_refused,
                key = item.list.key(),
                path = item.entry.path.clone(),
                reason = reason,
                source = source(&item.entry)
            )
            .to_string(),
        })
        .collect()
}

/// One fact line under `label` for each item, said by `say`. The caller names the message in
/// `say`, so each is still named where it is used.
fn each<T>(label: impl AsRef<str>, items: impl IntoIterator<Item = T>, say: impl Fn(T) -> String) {
    for item in items {
        fact(label.as_ref(), say(item));
    }
}

/// `each` under `doctor_settings_ignored`, for the settings that name only the file that held them.
fn ignored<'a>(paths: impl IntoIterator<Item = &'a Path>, say: impl Fn(String) -> String) {
    each(t!(doctor_settings_ignored), paths, |path| {
        say(path.display().to_string())
    });
}

/// The `doctor` facts for `sandbox.filesystem`: each entry of the four lists with the file or flag
/// that wrote it, and the ones that are not in force with why.
///
/// Resolved the way a session resolves them, against the profile directory and the working
/// directory, so the report is the answer a stage would get; a glob is said by its entry and not by
/// what it matched.
fn doctor_sandbox_filesystem(settings: &bravebot_config::Settings, managed: &Managed) {
    let settled = bravebot_config::settled_sandbox_filesystem()
        .cloned()
        .unwrap_or_else(|| {
            bravebot_config::resolve_sandbox_filesystem(
                &bravebot_sandbox::rules::Lists::default(),
                settings,
                managed,
            )
        });
    let profile = bravebot_agent::home::profile();
    let directory = std::env::current_dir().unwrap_or_default();
    let rules = bravebot_sandbox::rules::resolve(&settled.lists, profile.as_deref(), &directory);
    let backend = bravebot_sandbox::for_current_platform().ok();
    let capabilities = backend.as_ref().map(|backend| backend.capabilities());
    for line in sandbox_filesystem_lines(&rules, &directory, capabilities.as_ref()) {
        fact(t!(doctor_sandbox_filesystem), line);
    }
    let managed_file = bravebot_config::managed_file().display().to_string();
    each(
        t!(doctor_settings_ignored),
        &settled.unread,
        |(list, entry)| {
            t!(
                doctor_managed_sandbox_unread,
                key = list.key(),
                path = entry.path.clone(),
                managed = managed_file.clone()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        settings.sandbox_filesystem_ignored(),
        |(path, key)| {
            t!(
                doctor_settings_sandbox_filesystem_ignored,
                key = key,
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        settings.sandbox_filesystem_misshapen(),
        |(path, key)| {
            t!(
                doctor_settings_sandbox_misshapen,
                key = key,
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        settings.sandbox_hosts_ignored(),
        |(path, key)| {
            t!(
                doctor_settings_sandbox_hosts_ignored,
                key = key,
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        settings.sandbox_hosts_misshapen(),
        |(path, key)| {
            t!(
                doctor_settings_sandbox_hosts_misshapen,
                key = key,
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    ignored(settings.git_identities_ignored(), |path| {
        t!(doctor_settings_git_identities_ignored, path = path).to_string()
    });
    each(
        t!(doctor_settings_ignored),
        settings.git_identities_misshapen(),
        |(path, key)| {
            t!(
                doctor_settings_git_identities_misshapen,
                key = key,
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    // Judged as a stage judges it, so a key a push would not be given is named before the push.
    if let Some(home) = bravebot_agent::home::profile() {
        for entry in &settings.git_identities().entries {
            let lent = bravebot_sandbox::scope::identity_key(&entry.key, &home)
                .and_then(|key| bravebot_sandbox::scope::ssh_command(&key));
            if lent.is_none() {
                fact(
                    t!(doctor_settings_ignored),
                    t!(
                        doctor_settings_git_identities_key_refused,
                        key = format!(
                            "git.identities.{}/{}",
                            entry.address.host, entry.address.owner
                        ),
                        value = entry.key.clone(),
                        path = entry
                            .by
                            .as_deref()
                            .map(|by| by.display().to_string())
                            .unwrap_or_default()
                    ),
                );
            }
        }
    }
    each(
        t!(doctor_settings_ignored),
        managed.filesystem_unreadable(),
        |list| {
            t!(
                doctor_managed_sandbox_misshapen,
                key = list.key(),
                path = managed_file.clone()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        managed.sandbox_hosts_unreadable(),
        |key| {
            t!(
                doctor_settings_sandbox_hosts_misshapen,
                key = key,
                path = managed_file.clone()
            )
            .to_string()
        },
    );
}

/// Names only. On some machines a value here is a credential, and a diagnostic that prints one is a
/// diagnostic people paste into issues.
///
/// The variables the file names, not everything it configured: a gateway is a block rather than a
/// variable and gets a section of its own. A file that only configures one therefore names nothing
/// here, which is absence rather than an empty list.
fn report_settings_sources(settings: &bravebot_config::Settings) {
    let named: Vec<&str> = settings.names().collect();
    match (named.is_empty(), settings.is_empty()) {
        (false, _) => fact(
            t!(doctor_settings),
            t!(doctor_settings_names, names = named.join(", ")),
        ),
        (true, false) => fact(t!(doctor_settings), t!(doctor_settings_no_variables)),
        (true, true) => fact(t!(doctor_settings), t!(doctor_settings_absent)),
    }

    // What a pattern in `run.scrubEnv` reached on this machine. An exact entry withholds what it
    // says and needs no line; a pattern's effect depends on the variables this process holds, so
    // the file alone does not tell a person whether it caught what they meant or more than they
    // meant. Names only, never a value (RUN-28).
    each(
        t!(doctor_scrub_pattern),
        bravebot_config::scrub::matched_by_patterns(settings),
        |(pattern, name)| {
            t!(
                doctor_scrub_pattern_matched,
                pattern = pattern,
                variable = name
            )
            .to_string()
        },
    );

    // Which files are in force, weakest first, and then only the names where that order decided
    // something. A person reading a value they did not expect has three places it could have come
    // from, and the paths are the whole of what tells them which.
    each(t!(doctor_settings_layer), settings.layers(), |layer| {
        layer.display().to_string()
    });
    each(
        t!(doctor_settings_override),
        settings.overridden(),
        |(name, path)| {
            t!(
                doctor_settings_overridden,
                name = name,
                path = path.display().to_string()
            )
            .to_string()
        },
    );
}

/// Every setting that was read and not obeyed, or not read at all, from the vetting key through the
/// narrowing keys. The network and filesystem ones follow in their own sections.
fn report_ignored_settings(settings: &bravebot_config::Settings, managed: &Managed) {
    let managed_file = bravebot_config::managed_file().display().to_string();
    // A key that looks like configuration and does nothing, which is the one worth saying out
    // loud: it is read from the home layer alone, so a checkout cannot stop whoever opened it being
    // asked, and somebody who wrote it into one has to be told rather than left wondering why the
    // prompt still appears.
    ignored(settings.vetting_ignored(), |path| {
        t!(doctor_settings_vetting_ignored, path = path).to_string()
    });
    ignored(settings.providers_ignored(), |path| {
        t!(doctor_settings_provider_ignored, path = path).to_string()
    });
    ignored(settings.model_ignored(), |path| {
        t!(doctor_settings_model_ignored, path = path).to_string()
    });
    ignored(settings.advisor_ignored(), |path| {
        t!(doctor_settings_advisor_ignored, path = path).to_string()
    });
    ignored(settings.fallback_ignored(), |path| {
        t!(doctor_settings_fallback_ignored, path = path).to_string()
    });
    ignored(settings.summary_ignored(), |path| {
        t!(doctor_settings_summary_ignored, path = path).to_string()
    });
    ignored(settings.agent_ignored(), |path| {
        t!(doctor_settings_agent_ignored, path = path).to_string()
    });
    ignored(settings.style_ignored(), |path| {
        t!(doctor_settings_style_ignored, path = path).to_string()
    });
    ignored(settings.references_ignored(), |path| {
        t!(doctor_settings_references_ignored, path = path).to_string()
    });

    each(
        t!(doctor_settings_ignored),
        settings.sandbox_ignored(),
        |(path, mode)| {
            t!(
                doctor_settings_sandbox_ignored,
                mode = mode.name(),
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        settings.sandbox_strict_ignored(managed),
        |(path, mode, held)| {
            t!(
                doctor_settings_sandbox_strict_ignored,
                path = path.display().to_string(),
                mode = mode.name(),
                held = held.display().to_string()
            )
            .to_string()
        },
    );
    ignored(settings.sandbox_unreadable(), |path| {
        t!(doctor_settings_sandbox_unreadable, path = path).to_string()
    });
    each(
        t!(doctor_settings_ignored),
        managed.sandbox_unreadable().then_some(()),
        |()| {
            t!(
                doctor_settings_sandbox_unreadable,
                path = managed_file.clone()
            )
            .to_string()
        },
    );

    // A key beside the ones this build reads, which the lines above cannot cover: those name a key
    // that was recognised and not obeyed, and this one was never read at all. The file is
    // documented as largely another tool's shape, so a pasted block holds `sandbox` or `hooks`, and
    // nothing else would tell whoever wrote one that the restriction they believe is in force was
    // never read. A note rather than a failure, on BACKEND-36's reasoning: a key written for a
    // later release must not stop this binary.
    each(
        t!(doctor_settings_unread),
        settings.unread_keys(),
        |(path, key)| {
            t!(
                doctor_settings_unread_key,
                key = key,
                path = path.display().to_string()
            )
            .to_string()
        },
    );

    // The same, for a key that only refuses and was spelled as something other than a boolean: it
    // is read as absence (PERM-16, PERM-17), so somebody who quoted `"true"` has a session as
    // permissive as one that named nothing, and nothing else would say so. The managed file among
    // them, since an administrator who mistyped a pin is the person least likely to be told by the
    // machine it binds.
    each(
        t!(doctor_settings_ignored),
        settings.narrowing_unreadable(),
        |(path, key)| {
            t!(
                doctor_settings_narrowing_ignored,
                key = format!("permissions.{key}"),
                path = path.display().to_string()
            )
            .to_string()
        },
    );
    each(
        t!(doctor_settings_ignored),
        managed.narrowing_unreadable(),
        |key| {
            t!(
                doctor_settings_narrowing_ignored,
                key = format!("permissions.{key}"),
                path = managed_file.clone()
            )
            .to_string()
        },
    );
}

/// The network for the programs `run` starts, named only where it is not open, with who decided
/// it, and the layers that tried to open what a person closed. A word that is neither is read as
/// absent, so it is said for the reason a mistyped pin is.
fn report_run_network(
    network: &bravebot_config::RunNetwork,
    settings: &bravebot_config::Settings,
    managed: &Managed,
) {
    if network.network.is_closed() {
        fact(
            t!(doctor_run_network),
            t!(
                doctor_run_network_closed,
                source = bravebot_tui::status::run_network_source(&network.decided)
            ),
        );
    }
    ignored(settings.run_network_ignored(), |path| {
        t!(doctor_settings_network_ignored, path = path).to_string()
    });
    ignored(settings.run_network_unreadable(), |path| {
        t!(doctor_settings_network_unreadable, path = path).to_string()
    });
    each(
        t!(doctor_settings_ignored),
        managed.network_unreadable().then_some(()),
        |()| {
            t!(
                doctor_managed_network_unreadable,
                path = bravebot_config::managed_file().display().to_string()
            )
            .to_string()
        },
    );
}

/// The `allow` rules a checkout or the home layer proposes, each named as granted or dropped. The
/// rules this workspace's record says the person granted are returned, since the permission count
/// that follows includes them.
fn report_allow_rules(
    settings: &bravebot_config::Settings,
) -> Vec<bravebot_agent::granted::Proposed> {
    // The same, for the other name a checkout cannot answer on its own: an `allow` entry
    // stops a prompt, so one read out of a file that arrived with a clone would run a
    // program nobody was asked about. A rule this workspace's own record says the person
    // granted is in force and says so; every other one is dropped and says that. Named one
    // rule at a time rather than counted, for the reason the vetting line gives and because
    // the two answers are per rule rather than per file.
    // The rules among them, which is what this is about: an entry that is not a rule is
    // nobody's grant, so it is left out here and named for what is wrong with it below,
    // under PERM-11, exactly as the same line in the home layer is.
    let proposed =
        bravebot_agent::permissions::proposed(settings, bravebot_agent::home::profile().as_deref());
    // The record is keyed on the workspace, so this answers about the directory `doctor` ran
    // in. Canonicalized, because a session keys on the root the workspace resolved and a
    // report keyed on the spelling would answer about a path that session never wrote: the
    // two agree on Unix and do not on Windows, where resolving prefixes a name. No state
    // directory, or one that cannot be read, is a record that says nothing, and a rule
    // nobody granted is one that is dropped.
    let here = std::env::current_dir().and_then(|cwd| cwd.canonicalize());
    let granted: Vec<bravebot_agent::granted::Proposed> =
        match (bravebot_agent::home::directory(), here) {
            (Some(home), Ok(cwd)) => bravebot_agent::granted::Store::new(&home, &cwd)
                .granted(&proposed)
                .into_iter()
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
    for rule in &proposed {
        let path = rule.path.display().to_string();
        match granted.contains(rule) {
            true => fact(
                t!(doctor_settings_granted),
                t!(
                    doctor_settings_allow_granted,
                    rule = &rule.rule,
                    path = path
                ),
            ),
            false => fact(
                t!(doctor_settings_ignored),
                t!(
                    doctor_settings_allow_ignored,
                    rule = &rule.rule,
                    path = path
                ),
            ),
        }
    }
    granted
}

/// Counted rather than listed: a rule is the user's own text and printing it back says nothing they
/// cannot read in the file. What is worth saying is which of them this build could not act on,
/// because those are the ones that look like protection and are not.
fn report_permissions(
    settings: &bravebot_config::Settings,
    granted: &[bravebot_agent::granted::Proposed],
    ending: &mut Ending,
) {
    // The granted rules are in the count, because they are rules in force: a report that left
    // them out would say a workspace had one rule where a session there has two.
    let (permissions, rejected) = bravebot_agent::permissions::with_granted(
        settings,
        &granted
            .iter()
            .map(|rule| rule.rule.clone())
            .collect::<Vec<_>>(),
        bravebot_agent::home::profile().as_deref(),
        // The directory `doctor` ran in, whose volume answers about case. Empty where it
        // cannot be read, which the probe reads as a volume that compares bytes.
        &std::env::current_dir()
            .and_then(|cwd| cwd.canonicalize())
            .unwrap_or_default(),
    );
    fact(
        t!(doctor_permissions),
        match permissions.is_empty() {
            true => t!(doctor_permissions_absent).to_string(),
            false => t!(doctor_permissions_count, count = permissions.len() as i64),
        },
    );
    for problem in &rejected {
        // The catch-all rather than the configuration status, which CLI-6 gives to a
        // configuration nothing ran on: the rest of the file is honoured and a session
        // here opens and works, so what is wrong is one line of it rather than the whole.
        *ending = ends_on(*ending, Ending::Failed);
        fact(
            t!(doctor_permissions_unreadable),
            bravebot_agent::permissions::describe(problem),
        );
    }
}

/// The backends a person can choose from and what would end each credential this build holds for
/// itself. Returns the stored subscription it loaded, which the model section reports.
fn report_backends(
    config: &Config,
    ending: &mut Ending,
) -> Option<bravebot_skus::StoredCredentials> {
    // Both, where both are reachable, because both are offered to a person choosing and a
    // report naming one of them explains only the half of the picker they happened to use.
    if config.serves_aichat() {
        report_aichat(config);
    }
    if let Some(bedrock) = config.bedrock.as_ref() {
        report_bedrock(bedrock);
    }
    for provider in &config.providers {
        report_gateway(provider);
    }
    // A file of gateway keys that cannot be read is read as none, so every gateway a key in
    // it was for is reported above as having none, and this says why. The catch-all
    // status, as for a rule above: a session opens and works, without those keys.
    if let Some(directory) = bravebot_agent::home::directory()
        && bravebot_config::keys::Keys::read(&directory).is_err()
    {
        *ending = ends_on(*ending, Ending::Failed);
        fact(
            t!(doctor_gateway_keys),
            t!(
                doctor_gateway_keys_unreadable,
                path = bravebot_config::keys::file(&directory).display()
            ),
        );
    }

    // What would end each credential this build holds for itself, and which tier the gate
    // walk left it on. Reported beside the backends rather than kept for a leak, because it
    // can only be written down while the arrangement is still understood, and the person
    // reading it after a key appears somewhere public needs the surface that revokes it
    // rather than the expiry, which is all anything here kept before.
    //
    // The batch is asked for here rather than inside the record, because the record is
    // configuration and the file the batch sits in is a fact about the machine.
    let subscription = bravebot_skus::store::load().ok();
    for held in config.held(subscription.is_some()) {
        fact(t!(doctor_ends), what_would_end(held));
        fact(t!(doctor_tier), what_tier_it_stands_at(held.tier()));
        if let Some(noticed) = how_soon_a_leak_is_noticed(held) {
            fact(t!(doctor_noticed), noticed);
        }
        if let Some(binding) = how_it_is_bound(held) {
            fact(t!(doctor_binding), binding);
        }
        if let Some(survives) = what_outlives_revoking(held) {
            fact(t!(doctor_outlives), survives);
        }
        // How it got here, after what would end it: the lines above name the credential
        // and the address somebody acts on, and these say which gate each drop failed and
        // whether anybody could have done otherwise.
        for drop in held.walk() {
            if let Some(dropped) = why_it_dropped(held, *drop) {
                fact(t!(doctor_dropped), dropped);
            }
        }
    }
    subscription
}

/// What a run would actually request, and the subscription where one means something.
fn report_model(
    config: &Config,
    settings: &bravebot_config::Settings,
    subscription: Option<&bravebot_skus::StoredCredentials>,
) {
    // What a run would actually request, since a choice made with `/model` overrides the
    // configured default and reporting only the default would explain the wrong thing. Not
    // where a checkout's settings outrank the choice, since naming it then would explain
    // the wrong thing the other way round.
    match bravebot_agent::backend::pick(
        config,
        bravebot_session::store::model(bravebot_session::store::load_model(), settings),
    ) {
        bravebot_agent::backend::Pick::InForce(chosen) => {
            fact(t!(doctor_model), t!(doctor_model_chosen, model = chosen))
        }
        bravebot_agent::backend::Pick::SetAside(pick) => fact(
            t!(doctor_model),
            t!(
                doctor_model_set_aside,
                model = &config.default_model,
                pick = pick
            ),
        ),
        // The configured model answers as it does above, and the reason differs: a model this
        // machine does not request is one a configured service would have served, so the
        // line above would name the wrong fault. Then the file, which is the one thing
        // somebody in this case can act on, and which nothing at the end of this section
        // names: that is asked about the model a run would request, the configured one by
        // then (BACKEND-48).
        bravebot_agent::backend::Pick::Refused { recorded, reason } => {
            fact(
                t!(doctor_model),
                t!(
                    doctor_model_refused,
                    model = &config.default_model,
                    pick = &recorded
                ),
            );
            fact(
                t!(doctor_settings_ignored),
                t!(
                    session_model_pick_refused,
                    model = &recorded,
                    reason = reason
                ),
            );
        }
        bravebot_agent::backend::Pick::Absent => fact(
            t!(doctor_model),
            t!(doctor_model_default, model = &config.default_model),
        ),
    }

    // Only where a subscription means something. A Leo credential is what the premium half of
    // the Brave roster needs, and it means nothing to Bedrock.
    if config.serves_aichat() {
        report_subscription(subscription);
    }
}

/// Last of the configuration section, and a failure, because a report that said "configuration OK"
/// about a machine where a session refuses to start is the one thing a person in that position is
/// certain to read first.
///
/// A model the machine-level layer refuses is the same failure read by the same person, and the one
/// case where what they have to do is ask somebody else (BACKEND-48).
fn report_serving(config: &Config, ending: &mut Ending) {
    let model = model_for_this_run(None, config);
    match bravebot_agent::backend::serving(config, &bravebot_net::Egress::new(), &model) {
        bravebot_agent::backend::Serving::NothingConfigured {
            subscription,
            a_service_is_configured,
        } => {
            *ending = ends_on(*ending, Ending::Configuration);
            println!();
            println!(
                "{}",
                how_to_configure_a_model(
                    subscription.as_deref(),
                    a_service_is_configured,
                    &import::looked(a_service_is_configured),
                )
            );
        }
        bravebot_agent::backend::Serving::Refused { file, why } => {
            *ending = ends_on(*ending, Ending::Configuration);
            println!();
            println!("{}", managed_refusal(&model, &file, why));
        }
        bravebot_agent::backend::Serving::Configured => {}
    }
}

/// Report whether configuration is usable, without revealing the signing key.
pub(crate) fn doctor() -> ExitCode {
    // What the report ends on, rather than whether it passed: CLI-6 gives a configuration this
    // build cannot use a status of its own, and a report collapsing every way a machine can be
    // wrong into `ExitCode::FAILURE` tells a caller checking one nothing it can act on.
    let mut ending = Ending::Done;

    // Read before the configuration so the file can be reported even when it is what made the
    // configuration wrong.
    let settings = bravebot_config::Settings::load();
    let managed = Managed::load();

    // Which variable answered as well as what it answered, since more than one can and the one in
    // force is what somebody has to change to put the state directory elsewhere.
    let resolved = bravebot_agent::home::resolved();

    match Config::from_env_and_settings(&settings, &managed) {
        Ok(config) => {
            println!("{}", t!(doctor_configuration_ok));
            report_settings_sources(&settings);
            report_ignored_settings(&settings, &managed);

            let network = bravebot_config::settled_run_network()
                .cloned()
                .unwrap_or_else(|| bravebot_config::resolve_run_network(None, &settings, &managed));
            report_run_network(&network, &settings, &managed);

            // The lists of paths the programs `run` starts are held to, entry by entry with the
            // file that wrote each, so a person finds what to edit; and each way an entry was not
            // obeyed, since a refusal that is not in force holds back nothing and says nothing.
            doctor_sandbox_filesystem(&settings, &managed);

            let granted = report_allow_rules(&settings);
            report_mcp_declared(&settings, &mut ending);

            // After the layers a person owns, because it is what answers for a name none of them
            // explains: a value they set and cannot see taking effect is pinned above all of them.
            for line in managed_layer(&managed) {
                println!("{line}");
            }

            report_permissions(&settings, &granted, &mut ending);
            let subscription = report_backends(&config, &mut ending);
            report_model(&config, &settings, subscription.as_ref());
            report_serving(&config, &mut ending);
        }
        Err(err) => {
            doctor_found(
                &mut ending,
                Ending::Configuration,
                t!(cli_configuration_problem, problem = err),
            );
            // Said here too, since a layer that tried to declare a server is wrong whatever else
            // the configuration is, and its author is not told anywhere else.
            report_mcp_declared(&settings, &mut ending);
            // The one line from the section above that still has to be printed. A pin is the case
            // where the person reading this can do nothing about the error, so the file that holds
            // it is the only actionable thing in the report.
            for line in managed_layer(&managed) {
                println!("{line}");
            }
        }
    }

    println!();
    // Outside the block above, which a configuration error stops before it prints anything: where
    // the state is kept is a fact about the machine either way, and a machine with nowhere to keep
    // it is one of the reasons the configuration above it can be wrong.
    for line in state_directory(
        resolved
            .as_ref()
            .map(|(variable, path)| (*variable, path.as_path())),
        bravebot_agent::home::PROFILE_VARIABLES,
        RESTRICTED,
    ) {
        println!("{line}");
    }

    println!();
    // Outside the configuration block too: which servers a session here starts is read from the
    // state directory and the settings, and is what `bravebot mcp list` reports (SERVERS-14).
    let home = mcp::Home {
        directory: bravebot_agent::home::directory(),
        writable: false,
    };
    let (lines, failed) = mcp::examined(&home, &managed, mcp::Here::current(&settings));
    for line in lines {
        println!("{line}");
    }
    if failed {
        ending = ends_on(ending, Ending::Failed);
    }

    // Outside the configuration block as well: which skills a session here loads is read off two
    // directories, a trust map and the rules, and a key nothing reads is wrong whatever the
    // configuration says.
    if let Ok(workspace) = current_workspace(&settings, &managed) {
        let (permissions, _) = bravebot_agent::permissions::from_settings(
            &settings,
            bravebot_agent::home::profile().as_deref(),
            workspace.root(),
        );
        let lines = skill_keys_unread(
            &workspace,
            bravebot_agent::home::directory().as_deref(),
            trust_already_answered(workspace.root()),
            permissions,
        );
        if !lines.is_empty() {
            println!();
        }
        for line in lines {
            println!("{line}");
        }
    }

    println!();
    // Outside the configuration block for the same reason the state directory is: what a handshake
    // is validated against and what a request is routed through are facts about the machine, and
    // they are most often what is wanted when the configuration above them looks right and nothing
    // connects.
    let transport = Transport::shared();
    for line in network(transport) {
        println!("{line}");
    }
    // Any of the three is a statement about this machine that the program is not honouring,
    // which is what a report exits non-zero over: nothing is trusted, a named path holds nothing, or
    // a proxy was named that requests are not taking. CLI-7 calls each of them a configuration
    // error rather than a finding, which is the status CLI-6 gives them.
    if !transport.trust_problems().is_empty()
        || transport.unusable_proxy().is_some()
        || !transport.unparseable_proxies().is_empty()
    {
        ending = ends_on(ending, Ending::Configuration);
    }

    println!();
    // Not a warning: without confinement, untrusted work will be refused rather than run, so
    // this is a hard problem for the user to solve. Not a configuration error either: nothing the
    // reader can write down would give this platform a sandbox, so it is CLI-6's catch-all.
    if let Some(problem) = report_confinement(
        bravebot_sandbox::for_current_platform().map(|sandbox| sandbox.capabilities()),
    ) {
        doctor_found(&mut ending, Ending::Failed, problem);
    }
    #[cfg(target_os = "linux")]
    if let Some(note) =
        network_namespace_note(bravebot_sandbox::linux::network_namespace_unavailable().as_deref())
    {
        println!("{note}");
    }

    // Beside the confinement the platform offers, since the two answers are read together: what
    // can be enforced here, and how much of the machine a program is held out of. A mode the managed
    // file refuses is a configuration error, as a file the session could not use is.
    match sandbox_mode_line(&settings, &managed) {
        Ok(line) => println!("{line}"),
        Err(problem) => doctor_found(&mut ending, Ending::Configuration, problem),
    }

    // Development setup is advisory: released binaries need neither facility.
    if let Ok(cwd) = std::env::current_dir() {
        let lines = development(&cwd, std::env::var_os("PATH").as_deref(), cfg!(windows));
        if !lines.is_empty() {
            println!();
        }
        for line in lines {
            println!("{line}");
        }
    }

    // The report is on stdout and says what is wrong, so the identifier of the status it exits with
    // would otherwise appear nowhere a log of the failure holds it. Said once, here, rather than
    // beside each line that raised the ending.
    if !ending.ok() {
        return fail(ending, t!(doctor_ended));
    }
    ending.code()
}

/// Recognise the source tree by its checked-in layout, including from a subdirectory.
/// No Git command or setup script is run by this read-only check.
pub(crate) fn development(
    cwd: &Path,
    path: Option<&std::ffi::OsStr>,
    windows: bool,
) -> Vec<String> {
    let mut boundary = false;
    let mut ancestors = cwd.ancestors().take_while(|root| {
        let include = !boundary;
        boundary = root.join(".git").exists();
        include
    });
    let Some(root) = ancestors.find(|root| {
        [
            "Cargo.toml",
            "crates/cli/Cargo.toml",
            "agents/setup.py",
            "agents/AGENTS.md",
            "docs/development/agent-configuration.md",
        ]
        .iter()
        .all(|marker| root.join(marker).is_file())
    }) else {
        return Vec::new();
    };
    vec![
        t!(doctor_development, path = root.display().to_string()),
        aligned("AGENTS.md", agent_discovery(root, windows), FACT),
        aligned(
            "direnv",
            if direnv_available(path) {
                t!(doctor_direnv_ok)
            } else {
                t!(doctor_direnv_missing)
            },
            FACT,
        ),
    ]
}

pub(crate) fn agent_discovery(root: &Path, windows: bool) -> String {
    let source = root.join("agents/AGENTS.md");
    let destination = root.join("AGENTS.md");
    match std::fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            match (destination.canonicalize(), source.canonicalize()) {
                (Ok(actual), Ok(expected)) if actual == expected => {
                    t!(doctor_agents_ok).to_string()
                }
                (Ok(_), _) => t!(doctor_agents_wrong).to_string(),
                _ => t!(doctor_agents_broken).to_string(),
            }
        }
        Ok(metadata) if windows && metadata.is_file() => {
            match (std::fs::read(&destination), std::fs::read(&source)) {
                (Ok(actual), Ok(expected)) if actual == expected => {
                    t!(doctor_agents_copy_ok).to_string()
                }
                _ => t!(doctor_agents_copy_stale).to_string(),
            }
        }
        Ok(_) => t!(doctor_agents_conflict).to_string(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            t!(doctor_agents_missing).to_string()
        }
        Err(_) => t!(doctor_agents_unreadable).to_string(),
    }
}

/// Inspect PATH without executing a tool or changing the process environment.
pub(crate) fn direnv_available(path: Option<&std::ffi::OsStr>) -> bool {
    path.is_some_and(|path| {
        std::env::split_paths(path).any(|directory| {
            let executable = directory.join(if cfg!(windows) {
                "direnv.exe"
            } else {
                "direnv"
            });
            std::fs::metadata(executable).is_ok_and(|metadata| {
                if !metadata.is_file() {
                    return false;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
        })
    })
}

/// What `doctor` says about the Bedrock half of the roster.
///
/// No key and no endpoint: there is no key, and the host is derived from the region rather than
/// configured. The credentials are the AWS CLI's to hold, which is why the profile is the useful
/// thing to report and there is nothing here to redact.
fn report_bedrock(bedrock: &bravebot_config::bedrock::Bedrock) {
    fact(t!(doctor_backend), t!(doctor_backend_bedrock));
    fact(t!(doctor_region), &bedrock.region);
    match bedrock.profile.as_deref() {
        Some(profile) => fact(t!(doctor_profile), profile),
        None => fact(t!(doctor_profile), t!(doctor_profile_absent)),
    }
    report_aws_session(bedrock.profile.as_deref());

    // The tiers a person may choose, by name. An ARN is unreadable and looks identical between
    // tiers, so the names are what tells someone whether the block did what they meant.
    match bedrock.models().is_empty() {
        false => fact(
            t!(doctor_tiers),
            bedrock
                .models()
                .iter()
                .map(bravebot_config::bedrock::Entry::display_name)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        true => fact(t!(doctor_tiers), t!(doctor_tiers_absent)),
    }
}

/// What `doctor` says about whether an AWS account can sign a request now.
///
/// Asked of the AWS CLI the way a turn asks before it signs, so the answer is the one a session
/// would get. Only the answer is printed: what the CLI exports to give it is a live AWS credential.
/// The sign-in command is named only where a sign-in is what is missing.
fn report_aws_session(profile: Option<&str>) {
    use bravebot_agent::backend::{Backend, CredentialError};
    fact(
        t!(doctor_aws_session),
        match Backend::session_of(profile) {
            Ok(()) => t!(doctor_aws_signed_in).to_string(),
            Err(CredentialError::Refused { .. }) => t!(doctor_aws_signed_out).to_string(),
            Err(CredentialError::NoSuchProfile { available, .. }) if available.is_empty() => {
                t!(doctor_aws_no_profiles).to_string()
            }
            Err(CredentialError::NoSuchProfile { available, .. }) => {
                t!(doctor_aws_no_profile, available = available.join(", "))
            }
            Err(CredentialError::NotInstalled) => t!(doctor_aws_no_cli).to_string(),
            Err(CredentialError::Undecodable { .. }) => t!(doctor_aws_undecodable).to_string(),
        },
    );
}

/// What `doctor` says about one configured gateway.
///
/// Whether a credential can be found rather than what it is, because on this path the value is a
/// bearer token and a diagnostic that printed one is a diagnostic people paste into issues. Which
/// models, because a gateway's own roster is far larger than the block names and the listed slugs are
/// what tells someone whether the block did what they meant.
fn report_gateway(provider: &bravebot_config::provider::Provider) {
    fact(
        t!(doctor_backend),
        t!(doctor_backend_gateway, gateway = provider.display_name()),
    );
    fact(t!(doctor_endpoint), provider.chat_completions_url());
    fact(
        t!(doctor_key_name),
        gateway_credential(provider, |name| std::env::var(name).ok()),
    );
    if let Some(bedrock) = provider.bedrock.as_ref() {
        // A block naming no profile is on the one the `aws` it starts inherits, and asked about
        // by that name so a profile the CLI does not have is told from a lapsed session.
        let profile = bedrock.profile.clone().or_else(auth::inherited_profile);
        fact(
            t!(doctor_profile),
            profile.as_deref().unwrap_or(t!(doctor_profile_absent)),
        );
        report_aws_session(profile.as_deref());
    }
    fact(t!(doctor_tiers), gateway_models(provider));
}

/// What `doctor` says a gateway offers: the models its block names, else the list compiled in for a
/// service that cannot be asked, else that the service is asked.
///
/// A compiled id is shown with the service's id in front, as it has to be named: it is on no list
/// a bare name is looked up in, which a model the block names is.
pub(crate) fn gateway_models(provider: &bravebot_config::provider::Provider) -> String {
    if !provider.models.is_empty() {
        return provider
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
    }
    match provider.compiled_roster() {
        Some(compiled) => {
            let models = compiled
                .iter()
                .map(|id| format!("{}/{id}", provider.id))
                .collect::<Vec<_>>()
                .join(", ");
            t!(doctor_gateway_models_compiled, models = models).to_string()
        }
        None => t!(doctor_gateway_models_absent).to_string(),
    }
}

/// What `doctor` says about a gateway's credential: that one was found, never what it is.
///
/// Separate from the printing so the withholding is testable. The value here is a long-lived bearer
/// token, so a diagnostic that echoed one would put a live credential in every issue somebody pastes
/// this into.
///
/// Three answers, because a block that named nowhere for a credential to live needs none and there is
/// nothing for anybody to go and set. Reported as absent, it reads as the thing to fix on a gateway
/// that is working.
///
/// A key `bravebot auth login gateway` stored is named as that, because it is the one place a token
/// can come from that neither the environment nor the settings file shows (CRED-25).
pub(crate) fn gateway_credential(
    provider: &bravebot_config::provider::Provider,
    lookup: impl Fn(&str) -> Option<String>,
) -> String {
    use bravebot_config::provider::{Credential, Source};
    match provider.credential_and_source(lookup) {
        (Credential::Token(_), Some(Source::Stored)) => t!(doctor_gateway_token_stored).to_string(),
        (Credential::Token(_), _) => t!(doctor_gateway_token).to_string(),
        (Credential::Absent, _) => t!(
            doctor_gateway_token_absent,
            id = progress::printable(&provider.id)
        ),
        (Credential::NotNeeded, _) => t!(doctor_gateway_token_not_needed).to_string(),
    }
}

/// What `doctor` says would end one credential: its issuer, the surface that revokes it, and
/// anything minted from it that revoking it would not reach, which is what CRED-25 asks be
/// recorded.
///
/// Separate from the printing so each credential's account is testable, and a `match` over the
/// record rather than a field on it because `bravebot-config` holds no words a person reads: a
/// credential added there does not compile until it has one here.
///
/// A `String` rather than a `&'static str` because one of the accounts names a host, which is the
/// address somebody acts on and belongs in the sentence rather than in the line above it: the
/// report lists every gateway a settings file configured, and a sentence that named none of them
/// would leave the reader of a machine with two to guess which one it meant.
pub(crate) fn what_would_end(held: bravebot_config::Held<'_>) -> String {
    match held {
        bravebot_config::Held::SigningKey => t!(doctor_ends_signing_key).to_string(),
        bravebot_config::Held::AwsAccessKey => t!(doctor_ends_aws_access_key).to_string(),
        bravebot_config::Held::AwsSession => t!(doctor_ends_aws_session).to_string(),
        bravebot_config::Held::GatewayToken { host } => {
            t!(doctor_ends_gateway_token, gateway = host)
        }
        bravebot_config::Held::SubscriptionBatch => t!(doctor_ends_subscription_batch).to_string(),
    }
}

/// What `doctor` says about the tier a credential stands at, which CRED-2 asks every credential in
/// use to carry and which the surface reading the record is the only place a person sees.
///
/// One sentence per tier rather than per credential, and the tier rather than the credential as the
/// argument: the tier is what the gate walk answered, and two credentials that stopped at the same
/// gate stand in the same position however differently they are issued. What is particular to each
/// of them is the account beside this line. Taking the tier is also what lets every tier the record
/// can express be held to a sentence, including the one no credential stands at.
///
/// Separate from the printing, and a `match` over [`bravebot_config::Tier`] rather than a field on
/// it, for the reason the account above is: `bravebot-config` holds no words a person reads, so a
/// tier added there does not compile until it has one here. Delegated has a sentence although no
/// credential stands there, because the record can express it and a report that could not name it
/// would answer the first credential to reach it with the tier below.
pub(crate) fn what_tier_it_stands_at(tier: bravebot_config::Tier) -> &'static str {
    match tier {
        bravebot_config::Tier::Delegated => t!(doctor_tier_delegated),
        bravebot_config::Tier::Granted => t!(doctor_tier_granted),
        bravebot_config::Tier::HeldBriefly => t!(doctor_tier_held_briefly),
        bravebot_config::Tier::Held => t!(doctor_tier_held),
    }
}

/// What `doctor` says about how quickly a leak of a credential would be noticed and acted on,
/// which CRED-10 asks of a credential standing at Held briefly.
///
/// Beside the 'ends' line rather than instead of it: the two answer different questions, and a
/// person reading them after a key appears somewhere public needs both. What would end it is where
/// to go; this is how long the leak runs before anybody goes there, and it is the number that says
/// whether the window is short enough for what the credential reaches.
///
/// The figure itself is [`bravebot_config::Held::noticed_within`] and not a number here, so the
/// sentence a person reads cannot state one the record disagrees with. Which credentials get a
/// sentence is the record's answer too, and this match is kept separate from it so the two can
/// disagree and a test can see that they have.
pub(crate) fn how_soon_a_leak_is_noticed(held: bravebot_config::Held<'_>) -> Option<String> {
    let minutes = held.noticed_within()?.as_secs() as i64 / 60;
    match held {
        bravebot_config::Held::AwsSession => {
            Some(t!(doctor_noticed_aws_session, minutes = minutes))
        }
        bravebot_config::Held::SigningKey
        | bravebot_config::Held::AwsAccessKey
        | bravebot_config::Held::GatewayToken { .. }
        | bravebot_config::Held::SubscriptionBatch => None,
    }
}

/// What `doctor` says about whether a derived credential is bound to its presenter, which CRED-26
/// asks the record to say.
///
/// `None` for a credential that is not derived from another, as the record's
/// [`bravebot_config::Held::binding`] answers. The sentence follows the record's answer: a bound
/// credential, a bearer secret the issuer offers no bound form of, and a bearer secret nobody has
/// asked a bound form of are three sentences, so a reader can tell which of them is a decision
/// made here.
pub(crate) fn how_it_is_bound(held: bravebot_config::Held<'_>) -> Option<&'static str> {
    use bravebot_config::{Attempt, Binding};
    Some(match held.binding()? {
        Binding::SenderConstrained => t!(doctor_binding_sender_constrained),
        Binding::Bearer(Attempt::Refused) => t!(doctor_binding_bearer_refused),
        Binding::Bearer(Attempt::NotAttempted) => t!(doctor_binding_bearer_not_attempted),
    })
}

/// What `doctor` says about one drop of a credential's gate walk: the gate, whether the
/// counterparty refused or nobody attempted it, and the condition that was not met, which is what
/// CRED-3 asks be recorded.
///
/// `None` where the credential's walk holds no such drop, so a report cannot state a reason the
/// record does not: a gate it passed, or a condition of a gate it failed on something else. Which
/// those are is the record's answer, and the two can disagree here where a test can see it.
///
/// Keyed on the condition rather than on the gate, because the condition is what the sentence
/// says. Keyed on the gate, a walk revised to fail gate 2 on the bound having no end would still
/// print the account of a bound nobody fixed, which is a stated reason that is not the one the
/// record holds.
///
/// A `match` over the record rather than a field on it, for the reason [`what_would_end`] is one:
/// `bravebot-config` holds no words a person reads, so a credential or a condition added there
/// does not compile until every credential has said whether it drops on it.
///
/// The gate number and the answer come from the drop rather than from the sentence, so a line
/// cannot state a gate the walk did not fail or an answer the record does not hold. No sentence
/// names the gateway host: a walk accounts for an arrangement, and the address somebody acts on is
/// on the line above it.
pub(crate) fn why_it_dropped(
    held: bravebot_config::Held<'_>,
    drop: bravebot_config::GateDrop,
) -> Option<String> {
    use bravebot_config::{Condition, Held};

    let gate = drop.condition.gate().number();
    let answer = whether_anybody_asked(drop.attempt);
    Some(match (held, drop.condition) {
        (Held::SigningKey, Condition::NothingDecidesEachUse) => t!(
            doctor_dropped_signing_key_nothing_decides_each_use,
            gate = gate,
            answer = answer
        ),
        (Held::SigningKey, Condition::NoBoundFixedBeforeIssue) => t!(
            doctor_dropped_signing_key_no_bound_fixed_before_issue,
            gate = gate,
            answer = answer
        ),
        (Held::SigningKey, Condition::NotMintedForOneStep) => t!(
            doctor_dropped_signing_key_not_minted_for_one_step,
            gate = gate,
            answer = answer
        ),
        (Held::AwsAccessKey, Condition::NothingDecidesEachUse) => t!(
            doctor_dropped_aws_access_key_nothing_decides_each_use,
            gate = gate,
            answer = answer
        ),
        (Held::AwsAccessKey, Condition::NoBoundFixedBeforeIssue) => t!(
            doctor_dropped_aws_access_key_no_bound_fixed_before_issue,
            gate = gate,
            answer = answer
        ),
        (Held::AwsAccessKey, Condition::NotMintedForOneStep) => t!(
            doctor_dropped_aws_access_key_not_minted_for_one_step,
            gate = gate,
            answer = answer
        ),
        (Held::AwsSession, Condition::NothingDecidesEachUse) => t!(
            doctor_dropped_aws_session_nothing_decides_each_use,
            gate = gate,
            answer = answer
        ),
        (Held::AwsSession, Condition::NoBoundFixedBeforeIssue) => t!(
            doctor_dropped_aws_session_no_bound_fixed_before_issue,
            gate = gate,
            answer = answer
        ),
        (Held::AwsSession, Condition::RenewableWithoutAuthority) => t!(
            doctor_dropped_aws_session_renewable_without_authority,
            gate = gate,
            answer = answer
        ),
        (Held::GatewayToken { .. }, Condition::NothingDecidesEachUse) => t!(
            doctor_dropped_gateway_token_nothing_decides_each_use,
            gate = gate,
            answer = answer
        ),
        (Held::GatewayToken { .. }, Condition::NoBoundFixedBeforeIssue) => t!(
            doctor_dropped_gateway_token_no_bound_fixed_before_issue,
            gate = gate,
            answer = answer
        ),
        (Held::GatewayToken { .. }, Condition::NotMintedForOneStep) => t!(
            doctor_dropped_gateway_token_not_minted_for_one_step,
            gate = gate,
            answer = answer
        ),
        (Held::SubscriptionBatch, Condition::NothingDecidesEachUse) => t!(
            doctor_dropped_subscription_batch_nothing_decides_each_use,
            gate = gate,
            answer = answer
        ),
        // The conditions each credential's walk does not hold. Gate 2 is passed by the
        // subscription batch, which is why it has no account of the gates below the one it
        // stopped at; the rest are the other conditions of the gates each credential does drop
        // at, and a drop recorded on one of them is an account nobody has written.
        (
            Held::SigningKey | Held::AwsAccessKey | Held::GatewayToken { .. },
            Condition::NothingCanRefuseAUse
            | Condition::BoundEnforcedWithinReach
            | Condition::BoundWithNoEnd
            | Condition::IssuerEndsNothing
            | Condition::RenewableWithoutAuthority,
        )
        | (
            Held::AwsSession,
            Condition::NothingCanRefuseAUse
            | Condition::BoundEnforcedWithinReach
            | Condition::BoundWithNoEnd
            | Condition::NotMintedForOneStep
            | Condition::IssuerEndsNothing,
        )
        | (
            Held::SubscriptionBatch,
            Condition::NothingCanRefuseAUse
            | Condition::NoBoundFixedBeforeIssue
            | Condition::BoundEnforcedWithinReach
            | Condition::BoundWithNoEnd
            | Condition::NotMintedForOneStep
            | Condition::IssuerEndsNothing
            | Condition::RenewableWithoutAuthority,
        ) => return None,
    })
}

/// Whether the counterparty refused or nobody attempted it, in the words a person reads.
///
/// The two answers end at the same tier and mean opposite things, so the sentence carries the
/// record's answer rather than one written into each account: only the second is a decision
/// anybody here can revisit, and a line that says the wrong one sends somebody to argue with AWS
/// about a request nobody made.
fn whether_anybody_asked(attempt: bravebot_config::Attempt) -> &'static str {
    match attempt {
        bravebot_config::Attempt::Refused => t!(doctor_dropped_refused),
        bravebot_config::Attempt::NotAttempted => t!(doctor_dropped_not_attempted),
    }
}

/// What `doctor` says survives revoking a credential, where anything does.
///
/// `None` rather than a sentence saying "nothing", because a line that reads the same for two of
/// the three is the line people learn to skip, and this is the fact worth reading: the disposition
/// somebody reaches for after a leak is the one that leaves something behind.
///
/// Which credentials those are is [`bravebot_config::Held::outlives_revocation`], not this match.
/// The record is the fact and this is the words for it, so a credential whose record says
/// something survives and whose report says nothing is a disagreement a test can see.
pub(crate) fn what_outlives_revoking(held: bravebot_config::Held<'_>) -> Option<&'static str> {
    match held {
        bravebot_config::Held::AwsAccessKey => Some(t!(doctor_outlives_aws_access_key)),
        bravebot_config::Held::SigningKey
        | bravebot_config::Held::AwsSession
        | bravebot_config::Held::GatewayToken { .. }
        | bravebot_config::Held::SubscriptionBatch => None,
    }
}

/// What `doctor` says about a build pointed at the Brave backend.
fn report_aichat(config: &Config) {
    fact(t!(doctor_backend), t!(doctor_backend_aichat));
    fact(t!(doctor_endpoint), config.chat_completions_url());
    match config.premium_chat_completions_url() {
        Some(url) => fact(t!(doctor_premium), &url),
        None => fact(t!(doctor_premium), t!(doctor_premium_absent)),
    }
    fact(t!(doctor_key_id), &config.key_id);
    // Redacting `Display`, so what is reported is the placeholder rather than the key.
    fact(
        t!(doctor_key_name),
        t!(doctor_key, key = &config.signing_key),
    );
}

/// Where the values line up in what `doctor` reports, and in its confinement section.
pub(crate) const FACT: usize = 10;
pub(crate) const DETAIL: usize = 17;

/// One line of what `doctor` found: a name, then what it is, in two columns.
///
/// The gap is computed rather than typed into each line, since a translated name is not the
/// length the English one was and a column of hand-counted spaces stops being a column. Counted
/// in characters, which is not the width a terminal draws for every script, but is much closer
/// to it than a count of bytes and needs nothing to work it out.
pub(crate) fn aligned(name: impl AsRef<str>, value: impl AsRef<str>, column: usize) -> String {
    let name = name.as_ref();
    let gap = column.saturating_sub(name.chars().count()).max(1);
    format!("  {name}{}{}", " ".repeat(gap), value.as_ref())
}

pub(crate) fn fact(name: impl AsRef<str>, value: impl AsRef<str>) {
    println!("{}", aligned(name, value, FACT));
}

/// What `doctor` says about the machine-level layer, which is nothing where there is no such file.
///
/// The names it pinned rather than the values, on the same footing as the settings above: a value
/// here is a host. Naming them is the whole point of the line, since a name in this list is the
/// answer to why a variable somebody exported is changing nothing.
///
/// A file that was read is named even where nothing in it could be pinned, because the alternative
/// leaves whoever wrote it with no way to tell a file this program never found from one holding
/// names it may not honour.
pub(crate) fn managed_layer(managed: &Managed) -> Vec<String> {
    let Some(path) = managed.path() else {
        return Vec::new();
    };
    let path = path.display().to_string();
    let pinned: Vec<&str> = managed.pinned().collect();
    vec![aligned(
        t!(doctor_managed),
        match pinned.is_empty() {
            true => t!(doctor_managed_nothing, path = &path).to_string(),
            false => t!(
                doctor_managed_pinned,
                names = pinned.join(", "),
                path = &path
            )
            .to_string(),
        },
        FACT,
    )]
}

/// Report the imported subscription, and how much of it is left.
///
/// Counts only: a credential is a bearer secret, so none of it is printed. The environment rather
/// than the channel it came from, because that is what decides whether the batch can be spent
/// against the endpoint this build talks to, and it is what the file records.
///
/// The batch arrives rather than being loaded here, because the record of what would end each
/// credential needs the same answer and a second load would let the two halves of one report
/// disagree about whether a subscription is held.
fn report_subscription(stored: Option<&bravebot_skus::StoredCredentials>) {
    if let Some(stored) = stored {
        fact(
            t!(doctor_leo),
            t!(
                doctor_subscription,
                environment = stored.environment.as_str(),
                unspent = stored.remaining(),
                total = stored.credentials.len()
            ),
        );
    }
}

/// Whether a file created under the state directory is reachable only by the account that owns it.
///
/// [`bravebot_agent::home::create_directory`] and [`bravebot_agent::home::write_file`] ask for that
/// mode as they create, and on a platform with no mode to ask for they cannot: the files carry
/// whatever the profile directory grants them instead. Read here and passed in, so a test on either
/// platform holds what the section says in both cases.
pub(crate) const RESTRICTED: bool = cfg!(unix);

/// The state directory section of `doctor`: where it is, or that there is none and what that costs.
///
/// Built rather than printed, so what the section says is a value a test can hold.
///
/// An absent directory is reported rather than failed on: STATE-2 makes no `HOME` a state this
/// program supports, and a container that has none of it wanted none of it. Saying so is still
/// owed, because every subsystem treats the absence as absence and none of them says a word. The
/// `settings` line above says at most that no file was found, which reads as a file nobody has
/// written rather than a directory there is nowhere to put.
///
/// Both halves are named because the absence is partial. A checkout's `.bravebot/settings.json`,
/// its skills and its `AGENTS.md` are read with no home at all, so a report that said only
/// "settings are not kept" would have somebody looking for why the file in front of them is being
/// ignored when it is in force.
///
/// A directory that is there is reported with the variable that named it, since more than one can
/// and the one that answered is what somebody has to change to put the directory elsewhere. It is
/// also where `restricted` is spent: a person keeping a shared or synced profile is owed the
/// difference between files this program narrowed and files carrying whatever they inherited, and
/// the state directory is the only section that could tell them.
///
/// `variables` is passed in for the same reason `restricted` is: the list is the one thing here that
/// differs by platform, and a host that states a profile directory in one variable could otherwise
/// not hold what the report says on a host that states it in two.
pub(crate) fn state_directory(
    found: Option<(&str, &Path)>,
    variables: &[&str],
    restricted: bool,
) -> Vec<String> {
    let Some((variable, path)) = found else {
        let variables = variables.join(" or ");
        return vec![
            t!(doctor_state_directory_absent, variables = &variables).to_string(),
            aligned(
                t!(doctor_state_directory_not_kept),
                t!(doctor_state_directory_forgotten),
                DETAIL,
            ),
            aligned(
                t!(doctor_state_directory_not_read),
                t!(doctor_state_directory_your_own),
                DETAIL,
            ),
            aligned(
                t!(doctor_state_directory_remedy),
                t!(doctor_state_directory_set_profile, variables = &variables),
                DETAIL,
            ),
        ];
    };
    let mut lines = vec![
        t!(
            doctor_state_directory,
            path = path.display().to_string(),
            variable = variable
        )
        .to_string(),
    ];
    lines.push(
        t!(
            doctor_state_directory_logs,
            path = path.join(bravebot_diag::DIRECTORY).display().to_string()
        )
        .to_string(),
    );
    if !restricted {
        lines.push(aligned(
            t!(doctor_state_directory_unprotected),
            t!(doctor_state_directory_permissions),
            DETAIL,
        ));
    }
    lines
}

/// The network section of `doctor`: the certificate authorities a handshake is validated against,
/// and the proxy a request goes through.
///
/// Built rather than printed, so what the section says is a value a test can hold.
///
/// Both exist to answer the failure that has nothing to say for itself. A machine behind a
/// TLS-inspecting proxy refuses every connection with a certificate error naming an authority the
/// user has already installed, and a machine with a proxy variable set sends every request
/// somewhere the rest of the report does not mention. Neither is visible anywhere else, and the
/// second is the one nobody thinks to check.
///
/// The proxy is named by protocol, host and port. Its uri is not printed, because a proxy that
/// requires a credential carries it there and a diagnostic that echoed one would put a live
/// password in every issue somebody pastes this into. That a credential is in use is still said:
/// a proxy rejecting an unauthenticated request is one of the failures this is run to explain.
/// A list of variable names as a sentence reads one: commas, and `or` before the last.
///
/// Every list here is a set of variables somebody has to go and set one of, so joining them all
/// with `or` would have the reader parsing a report rather than reading a remedy.
pub(crate) fn listed(names: &[&str]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

pub(crate) fn network(transport: &Transport) -> Vec<String> {
    let roots = match (transport.roots(), transport.trusts_nothing()) {
        (TrustRoots::Bundled, _) => t!(
            doctor_trust_roots_bundled,
            variables = listed(&[CERTIFICATE_FILE, CERTIFICATE_DIRECTORY])
        )
        .to_string(),
        (_, true) => t!(doctor_trust_roots_none).to_string(),
        (named, false) => t!(
            doctor_trust_roots_named,
            paths = named
                .paths()
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .to_string(),
    };

    let mut lines = vec![
        t!(doctor_network).to_string(),
        aligned(t!(doctor_trust_roots), roots, DETAIL),
    ];

    // A path that yielded nothing is named even where another one did, because the set in force is
    // then not the set that was asked for, and nothing else would say so. One line each: the two
    // variables are set by two decisions, and a reader who fixed the only path named would run this
    // again to be told about the other.
    for problem in transport.trust_problems() {
        lines.push(aligned(
            t!(doctor_trust_roots_unusable),
            problem.to_string(),
            DETAIL,
        ));
    }

    let proxy = match (transport.proxy_summary(), transport.unusable_proxy()) {
        (_, Some(protocol)) => t!(doctor_proxy_unsupported, protocol = protocol).to_string(),
        (None, None) => t!(doctor_proxy_absent, variables = listed(PROXY_VARIABLES)).to_string(),
        (Some(proxy), None) if transport.proxy_is_authenticated() => {
            t!(doctor_proxy_authenticated, proxy = proxy).to_string()
        }
        (Some(proxy), None) => t!(doctor_proxy_in_force, proxy = proxy).to_string(),
    };
    lines.push(aligned(t!(doctor_proxy), proxy, DETAIL));

    // A variable that holds something that is not a uri is passed over by the reader, which then
    // tries the next one, so the line above describes a route that is not the one somebody set. One
    // line each, naming the variable and never its value: the value is as likely to carry a
    // credential as a value that parsed.
    for name in transport.unparseable_proxies() {
        lines.push(aligned(
            t!(doctor_proxy_unparseable),
            t!(doctor_proxy_unparseable_detail, variable = name.as_str()),
            DETAIL,
        ));
    }

    // Which hosts the proxy is not used for, since that is what decides whether a proxy in force
    // applies to the host that is failing, and `NO_PROXY=*` leaves one configured and used for
    // nothing.
    if let Some(excluded) = transport.no_proxy() {
        lines.push(aligned(t!(doctor_no_proxy), excluded, DETAIL));
    }

    lines
}

/// What `doctor` says about confinement: the lines, and whether there was any.
pub(crate) struct Confinement {
    pub(crate) lines: Vec<String>,
    established: bool,
}

/// Report the confinement actually achieved here, and say whether there was any.
///
/// Printed rather than assumed: the guarantee differs by platform and kernel, and a
/// user is entitled to know which one they have before trusting the sandbox.
///
/// Takes what the lookup found rather than calling it: a machine has only its own backend to
/// look up, so what is said about the other two levels is otherwise unreachable.
///
/// A refusal is handed back rather than printed here, because it goes where a problem goes: on
/// stderr, behind the identifier CLI-6 gives it, and into the status the run ends on. A section
/// that printed it itself would be the one failure in the report with nothing in front of it.
pub(crate) fn report_confinement(found: Result<Capabilities, SandboxError>) -> Option<String> {
    let report = confinement(found);
    if !report.established {
        return Some(report.lines.join("\n"));
    }
    for line in &report.lines {
        println!("{line}");
    }
    None
}

/// The confinement section of `doctor`: the level in force, and what enforces it.
///
/// Built rather than printed, so what the section says about a level is a value a test can hold.
pub(crate) fn confinement(found: Result<Capabilities, SandboxError>) -> Confinement {
    match found {
        Ok(caps) => Confinement {
            lines: [
                t!(doctor_confinement, level = confinement_name(caps.level)).to_string(),
                aligned(t!(doctor_mechanisms), caps.mechanisms.join(", "), DETAIL),
                aligned(
                    t!(doctor_network_denial),
                    if caps.network_denial_enforced {
                        t!(doctor_kernel_enforced)
                    } else {
                        t!(doctor_not_enforced)
                    },
                    DETAIL,
                ),
            ]
            .into_iter()
            .chain(
                (!caps.governs_sockets_with_a_path)
                    .then(|| t!(doctor_sockets_with_a_path_reachable).to_string()),
            )
            .collect(),
            established: true,
        },
        Err(err) => Confinement {
            lines: vec![
                t!(doctor_confinement_unavailable).to_string(),
                format!("  {err}"),
            ],
            established: false,
        },
    }
}

/// What `doctor` says where a stage cannot be given a network namespace of its own, which is what
/// a closed network and `sandbox.network.allowedHosts` are applied with on Linux, and nothing
/// where it can.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn network_namespace_note(unavailable: Option<&str>) -> Option<String> {
    unavailable.map(|reason| t!(doctor_no_network_namespace, reason = reason).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use bravebot_net::Transport;
    use bravebot_net::transport::{
        CERTIFICATE_DIRECTORY, CERTIFICATE_FILE, PROXY_VARIABLES, TrustRoots,
    };
    use bravebot_sandbox::SandboxError;
    use bravebot_sandbox::policy::Capabilities;
    use bravebot_sandbox::policy::ConfinementLevel;
    use std::path::PathBuf;

    /// The two facts about the network nothing else in the report carries. A machine behind a
    /// TLS-inspecting proxy refuses every connection over an authority its user already installed,
    /// and a machine with a proxy variable set sends every request through somebody else's
    /// machine; neither is visible in any other line.
    #[test]
    fn the_network_section_names_the_roots_in_force_and_the_proxy() {
        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/ca.pem")),
                directory: None,
            },
            Some("http://proxy.corp.example:8080"),
            None,
        );

        let report = network(&transport).join("\n");

        assert!(report.contains("/etc/corp/ca.pem"), "{report}");
        assert!(
            report.contains("http://proxy.corp.example:8080"),
            "{report}"
        );
    }

    /// A proxy that requires a credential carries it in the uri. A diagnostic that echoed one would
    /// put a live password in every issue somebody pastes this into, so the report says a credential
    /// is in use and never what it is.
    #[test]
    fn the_network_section_never_prints_a_proxy_credential() {
        let transport = Transport::stated(
            TrustRoots::Bundled,
            Some("http://alice:s3cret@proxy.corp.example:8080"),
            None,
        );

        let report = network(&transport).join("\n");

        assert!(!report.contains("s3cret"), "{report}");
        assert!(!report.contains("alice"), "{report}");
        assert!(
            report.contains("http://proxy.corp.example:8080"),
            "{report}"
        );
    }

    /// Nothing named means the built-in set, and the variables that would name another are the whole
    /// of the remedy: which variables a machine states an authority in is not something the reader
    /// is expected to know.
    #[test]
    fn the_network_section_points_at_the_variables_when_nothing_names_a_root_or_a_proxy() {
        let report = network(&Transport::stated(TrustRoots::Bundled, None, None)).join("\n");

        assert!(report.contains(CERTIFICATE_FILE), "{report}");
        assert!(report.contains(CERTIFICATE_DIRECTORY), "{report}");
        for variable in PROXY_VARIABLES {
            assert!(report.contains(variable), "{report}");
        }
    }

    /// A named path that yields no certificate leaves nothing trusted, so every connection this
    /// program makes is about to fail. Reported as a path in force it would read as working
    /// configuration, and the one command run to explain the failure would explain nothing.
    #[test]
    fn a_trust_root_that_cannot_be_read_is_reported_as_the_reason_connections_will_fail() {
        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/absent.pem")),
                directory: None,
            },
            None,
            None,
        );

        let report = network(&transport).join("\n");

        assert!(transport.trusts_nothing());
        assert!(report.contains("/etc/corp/absent.pem"), "{report}");
        assert!(report.contains("fail"), "{report}");
    }

    /// Both variables failing is one machine with two paths to fix. Naming only the first would have
    /// whoever fixed it run this again to be told about the second, and the roots line says nothing
    /// is trusted without saying which path was asked for.
    #[test]
    fn the_network_section_names_every_path_that_yielded_nothing() {
        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/absent.pem")),
                directory: Some(PathBuf::from("/etc/corp/absent-dir")),
            },
            None,
            None,
        );

        let lines = network(&transport);

        let named: Vec<&String> = lines
            .iter()
            .filter(|line| line.contains("/etc/corp/"))
            .collect();
        assert_eq!(named.len(), 2, "one line each: {lines:?}");
        assert!(named[0].contains("/etc/corp/absent.pem"), "{lines:?}");
        assert!(named[1].contains("/etc/corp/absent-dir"), "{lines:?}");
    }

    /// A protocol this build cannot connect through is not the route requests take, so naming it as
    /// the proxy in force would send whoever ran this looking at a proxy that is seeing nothing.
    #[test]
    fn a_proxy_this_build_cannot_connect_through_is_reported_as_not_the_route() {
        let transport =
            Transport::stated(TrustRoots::Bundled, Some("socks5://proxy.corp:1080"), None);

        let report = network(&transport).join("\n");

        assert!(report.contains("socks5"), "{report}");
        assert!(report.contains("direct"), "{report}");
    }

    /// A variable the reader passed over is a route somebody stated, and the proxy line describes a
    /// different one. Saying nothing leaves them reading a report that denies the proxy they set.
    #[test]
    fn the_network_section_names_a_proxy_variable_that_cannot_be_parsed() {
        let transport = Transport::stated_with_unparseable_proxies(
            TrustRoots::Bundled,
            None,
            None,
            &["HTTPS_PROXY"],
        );

        let report = network(&transport).join("\n");

        assert!(report.contains("HTTPS_PROXY"), "{report}");
        assert!(
            !report.contains("alice") && !report.contains("s3cret"),
            "the value is never printed: {report}"
        );
    }

    /// Every one of them, in the order the variables are read. A reader who fixed the only variable
    /// named would run this again to be told about the next.
    #[test]
    fn the_network_section_names_every_proxy_variable_that_cannot_be_parsed() {
        let transport = Transport::stated_with_unparseable_proxies(
            TrustRoots::Bundled,
            None,
            None,
            &["ALL_PROXY", "HTTP_PROXY"],
        );

        let lines = network(&transport);
        let named: Vec<&String> = lines
            .iter()
            .filter(|line| line.contains("_PROXY is set"))
            .collect();

        assert_eq!(named.len(), 2, "one line each: {named:?}");
        assert!(named[0].contains("ALL_PROXY"), "{named:?}");
        assert!(named[1].contains("HTTP_PROXY"), "{named:?}");
    }

    /// A remedy naming three variables is read, not parsed, so the list is punctuated the way a
    /// sentence is.
    #[test]
    fn a_list_of_variables_is_punctuated_as_a_sentence() {
        assert_eq!(listed(&[]), "");
        assert_eq!(listed(&["ONE"]), "ONE");
        assert_eq!(listed(&["ONE", "TWO"]), "ONE or TWO");
        assert_eq!(listed(&["ONE", "TWO", "THREE"]), "ONE, TWO or THREE");
    }

    /// Which hosts a proxy is not used for decides whether the one in force applies to the host
    /// that is failing, and `NO_PROXY=*` leaves a proxy configured and used for nothing.
    #[test]
    fn the_network_section_names_the_hosts_a_proxy_is_not_used_for() {
        let transport = Transport::stated(
            TrustRoots::Bundled,
            Some("http://proxy.corp.example:8080"),
            Some("localhost,.internal.example"),
        );

        let report = network(&transport).join("\n");

        assert!(report.contains("localhost,.internal.example"), "{report}");
    }

    /// The column the English names were hand-spaced into, so extracting them changed nothing
    /// about what `doctor` prints.
    #[test]
    fn a_name_shorter_than_its_column_is_padded_out_to_it() {
        assert_eq!(
            aligned("endpoint", "https://example", FACT),
            "  endpoint  https://example"
        );
        assert_eq!(aligned("key id", "abc", FACT), "  key id    abc");
        assert_eq!(
            aligned("network denial", "kernel-enforced", DETAIL),
            "  network denial   kernel-enforced"
        );
    }

    /// A translation is not the length the English was, and a name that fills the column has to
    /// stay a name rather than running into what it is naming.
    #[test]
    fn a_name_longer_than_its_column_still_leaves_a_gap() {
        let line = aligned("point de terminaison", "https://example", FACT);
        assert_eq!(line, "  point de terminaison https://example");
    }

    /// The lines `doctor` prints for a set of capabilities, for the two tests below.
    fn confinement_report(level: ConfinementLevel, network_denial_enforced: bool) -> Vec<String> {
        confinement(Ok(Capabilities {
            level,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced,
            egress_limited_to_a_port: false,
            // Nothing `doctor` prints depends on this, so the report reads the same either way.
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant: true,
            governs_sockets_with_a_path: true,
        }))
        .lines
    }

    /// The guarantee genuinely differs by platform and kernel, so which of the three is in force
    /// is what somebody runs `doctor` to learn before trusting the sandbox with untrusted work. A
    /// level it does not name is one they have to assume.
    #[test]
    fn doctor_names_the_confinement_level_in_force() {
        // The opening line rather than the report, because "kernel-enforced" is also what the
        // network denial line says, and a level reported into the wrong field is not reported.
        let opening = |level| {
            confinement_report(level, false)
                .first()
                .expect("the report opens with the level")
                .clone()
        };

        for level in [
            ConfinementLevel::Kernel,
            ConfinementLevel::Partial,
            ConfinementLevel::None,
        ] {
            let line = opening(level);
            assert!(
                line.contains(&confinement_name(level)),
                "the level in force is not in the line that reports it: {line}"
            );
        }

        // Three openings that read the same would satisfy the loop above while telling a reader
        // nothing about which of the three they have.
        assert_ne!(
            opening(ConfinementLevel::Kernel),
            opening(ConfinementLevel::Partial)
        );
        assert_ne!(
            opening(ConfinementLevel::Partial),
            opening(ConfinementLevel::None)
        );
        assert_ne!(
            opening(ConfinementLevel::Kernel),
            opening(ConfinementLevel::None)
        );
    }

    /// An overstated capability is the failure this report exists to prevent. A backend that
    /// leaves network denial to convention rather than to the kernel is the case somebody most
    /// needs told, since it is the one where the guarantee they assume is not the one they have.
    #[test]
    fn doctor_says_whether_the_kernel_enforces_network_denial() {
        // One level, so the difference can only be the answer to that question.
        assert_ne!(
            confinement_report(ConfinementLevel::Kernel, true).last(),
            confinement_report(ConfinementLevel::Kernel, false).last(),
            "the report reads the same whether or not the kernel enforces network denial"
        );
    }

    /// A person whose kernel refuses user namespaces is refused every stage with no network or
    /// under an allowed-hosts list, and is told why only here. The regressions it rejects: no word
    /// at all, and a word that omits the reason or the setting it costs.
    #[test]
    fn doctor_says_when_network_namespaces_are_unavailable_and_what_that_costs() {
        assert_eq!(network_namespace_note(None), None);
        let note = network_namespace_note(Some("Permission denied (os error 13)"))
            .expect("a note where the namespace cannot be made");
        assert!(note.contains("Permission denied (os error 13)"), "{note}");
        assert!(note.contains("sandbox.network.allowedHosts"), "{note}");
    }

    /// SANDBOX-3: below Landlock ABI 9 a program under read-only or workspace-write reaches every unix socket that has
    /// a path and a strict stage is refused, and `doctor` is where a person learns both. The
    /// regressions it rejects: no line on such a kernel, a line where the sockets are governed,
    /// and a line that names neither the sockets, the kernel it needs, nor what strict does.
    #[test]
    fn doctor_says_when_sockets_with_a_path_are_reachable_and_what_that_costs() {
        let report = |governs_sockets_with_a_path| {
            confinement(Ok(Capabilities {
                level: ConfinementLevel::Partial,
                mechanisms: vec!["landlock"],
                network_denial_enforced: true,
                egress_limited_to_a_port: true,
                grants_paths_that_do_not_exist: false,
                subtracts_from_a_grant: true,
                governs_sockets_with_a_path,
            }))
            .lines
        };
        let (governed, reachable) = (report(true), report(false));

        let expected = t!(doctor_sockets_with_a_path_reachable).to_string();
        assert!(!governed.contains(&expected), "{governed:?}");
        assert_eq!(reachable.len(), governed.len() + 1, "{reachable:?}");
        assert_eq!(reachable.last(), Some(&expected));
        for word in [
            "D-Bus",
            "SSH_AUTH_SOCK",
            "docker.sock",
            "ABI 9",
            "Linux 7.1",
            "strict",
        ] {
            assert!(expected.contains(word), "{word}: {expected}");
        }
    }

    /// Without a backend, untrusted work is refused rather than run unconfined, so this is a thing
    /// the user has to go and fix: a `doctor` that reported it and still exited successfully would
    /// present it as one finding among the others, and one that did not say what was missing would
    /// leave them nothing to act on.
    #[test]
    fn confinement_that_could_not_be_established_fails_the_run() {
        let missing = || SandboxError::Unavailable {
            platform: "test",
            detail: "no backend is implemented here".into(),
        };

        let refusal = report_confinement(Err(missing()))
            .expect("doctor reported no confinement and still passed");
        assert!(
            refusal.contains(&missing().to_string()),
            "the refusal handed back does not say what was missing: {refusal}"
        );

        let lines = confinement(Err(missing())).lines;
        assert!(
            lines
                .iter()
                .any(|line| line.contains(&missing().to_string())),
            "the refusal does not say what was missing: {lines:?}"
        );
    }

    /// `doctor` reports the whole machine rather than stopping at the first thing wrong with it,
    /// so a run can reach the end holding a configuration it cannot use and a platform that
    /// confines nothing at once, and one status has to be chosen for both.
    ///
    /// Either order, because the failure this rejects is the obvious implementation: keeping the
    /// first ending found, or keeping the last. Both report a machine whose endpoint is
    /// unparseable as CLI-6's catch-all for half the reports that find one, which is the status
    /// meaning "a reason none of the others name" applied to a reason row 3 names.
    #[test]
    fn a_report_that_found_a_configuration_error_ends_on_it() {
        assert_eq!(
            ends_on(Ending::Configuration, Ending::Failed),
            Ending::Configuration,
            "a later catch-all displaced the configuration error in front of it"
        );
        assert_eq!(
            ends_on(Ending::Failed, Ending::Configuration),
            Ending::Configuration,
            "a configuration error found after a catch-all was dropped"
        );

        // And nothing else moves: a report with one kind of problem ends on that kind, and one
        // with none is still a success.
        assert_eq!(ends_on(Ending::Done, Ending::Failed), Ending::Failed);
        assert_eq!(ends_on(Ending::Failed, Ending::Failed), Ending::Failed);
        assert_eq!(
            ends_on(Ending::Done, Ending::Configuration),
            Ending::Configuration
        );
        assert_eq!(ends_on(Ending::Done, Ending::Done), Ending::Done);
    }

    /// A managed layer somebody cannot change has to say so somewhere, or "why is the variable I
    /// exported doing nothing" has no answer anywhere on the machine. The names and the file, since
    /// the file is what whoever can lift the pin has to be pointed at.
    #[test]
    fn doctor_names_what_the_managed_layer_pinned_and_the_file_it_came_from() {
        let scratch = Scratch::new("doctor-managed-pinned");
        let path = scratch.path.join("managed.json");
        std::fs::write(
            &path,
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        )
        .expect("a managed file");

        let lines = managed_layer(&Managed::at(&path));

        assert_eq!(lines.len(), 1, "one line, or the report grew: {lines:?}");
        assert!(
            lines[0].contains("BRAVE_AI_CHAT_ENDPOINT"),
            "the pinned name is not reported: {lines:?}"
        );
        assert!(
            lines[0].contains(&path.display().to_string()),
            "the file that pinned it is not named: {lines:?}"
        );
        assert!(
            !lines[0].contains("https://approved.example"),
            "the value is printed, and on some machines that is a credential: {lines:?}"
        );
    }

    /// The case every machine without an administrator is in. A report listing every place a file
    /// could have been is a report where the lines that matter are the hard ones to find.
    #[test]
    fn doctor_says_nothing_about_a_managed_layer_that_is_not_there() {
        let scratch = Scratch::new("doctor-managed-absent");
        let absent = scratch.path.join("managed.json");
        assert!(managed_layer(&Managed::at(&absent)).is_empty());
    }

    /// A file holding only names this layer may not pin did nothing, and saying nothing about it
    /// would leave whoever wrote it unable to tell that from a file this program never found.
    #[test]
    fn doctor_names_a_managed_file_that_pinned_nothing() {
        let scratch = Scratch::new("doctor-managed-nothing");
        let path = scratch.path.join("managed.json");
        std::fs::write(&path, r#"{"env": {"BRAVEBOT_CONTEXT_BUDGET": "4096"}}"#)
            .expect("a managed file");

        let lines = managed_layer(&Managed::at(&path));

        assert_eq!(lines.len(), 1, "the file is not reported: {lines:?}");
        assert!(
            lines[0].contains(
                &t!(doctor_managed_nothing, path = path.display().to_string()).to_string()
            ),
            "the line does not say the file pinned nothing: {lines:?}"
        );
    }

    /// Which directory this is depends on the environment of whoever started the process, so a
    /// person under `sudo`, or running the same binary from a service manager, has a different one
    /// from the session they are looking for. Naming it is what settles which of them is in force.
    ///
    /// The variable it came from is named with it, because more than one can name a profile
    /// directory and only the one that answered is worth changing: told the path alone, somebody
    /// moving the directory on Windows sets `USERPROFILE` and watches a `HOME` they forgot they had
    /// go on winning.
    #[test]
    fn doctor_names_the_state_directory_it_resolved() {
        let lines = state_directory(
            Some(("HOME", Path::new("/home/someone/.bravebot"))),
            &["HOME"],
            true,
        );

        assert!(
            lines
                .iter()
                .any(|line| line.contains("/home/someone/.bravebot")),
            "the state directory in use is not in the section that reports it: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("HOME")),
            "the section does not say which variable named the directory: {lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains(&t!(doctor_state_directory_forgotten).to_string())),
            "a machine with a state directory was told what it is not keeping: {lines:?}"
        );
    }

    /// A person filing a bug is told where the logs are, and the place is the one the log is
    /// written to, since a line naming a different directory sends them to look in an empty one.
    #[test]
    fn doctor_names_the_diagnostic_log_directory() {
        let lines = state_directory(
            Some(("HOME", Path::new("/home/someone/.bravebot"))),
            &["HOME"],
            true,
        );
        let expected = Path::new("/home/someone/.bravebot")
            .join(bravebot_diag::DIRECTORY)
            .display()
            .to_string();
        assert!(
            lines.iter().any(|line| line.contains(&expected)),
            "the log directory is not in the report: {lines:?}"
        );
    }

    /// The mode `home` asks for as it creates is what keeps a prompt history out of another
    /// account's reach, and on a platform that is given no mode the same files are written carrying
    /// whatever the profile directory grants. Somebody typing a token into a prompt on a shared or
    /// synced profile is entitled to know which of the two they have, and no other line in the
    /// report distinguishes them: the section reads identically either way.
    #[test]
    fn doctor_says_when_the_files_are_left_unrestricted() {
        let path = Path::new("C:\\Users\\someone\\.bravebot");
        let named = ["HOME", "USERPROFILE"];
        let unrestricted = state_directory(Some(("USERPROFILE", path)), &named, false).join("\n");

        assert!(
            unrestricted.contains(&t!(doctor_state_directory_permissions).to_string()),
            "a platform that narrows nothing said nothing about it: {unrestricted}"
        );
        assert!(
            !state_directory(Some(("USERPROFILE", path)), &named, true)
                .join("\n")
                .contains(&t!(doctor_state_directory_permissions).to_string()),
            "a platform that narrows every file reported them as unrestricted"
        );
    }

    /// The loss this reports is silent: the session records behind `--resume`, the prompt history
    /// and the recorded model are neither read nor written, every subsystem treats that as absence
    /// by design, and somebody whose `/model` choice does not survive the session has nothing else
    /// in the report to explain it. A stripped environment, a service manager and a container all
    /// reach it.
    ///
    /// What is still read has to be said in the same breath, because the absence is partial: a
    /// checkout's own settings, skills and `AGENTS.md` load with no home at all, and a report that
    /// left that out would send somebody looking for why a file that is in force is ignored.
    #[test]
    fn a_missing_state_directory_is_reported_with_what_it_costs() {
        let variables = bravebot_agent::home::PROFILE_VARIABLES;
        let lines = state_directory(None, variables, RESTRICTED);
        let section = lines.join("\n");

        for lost in ["sessions", "--resume", "prompt history", "model"] {
            assert!(
                section.contains(lost),
                "the section does not say that {lost} is not kept: {section}"
            );
        }
        assert!(
            section.contains("checkout"),
            "the section does not say that a checkout's own files are still read: {section}"
        );
        assert_ne!(
            lines,
            state_directory(
                Some(("HOME", Path::new("/home/someone/.bravebot"))),
                variables,
                RESTRICTED
            ),
            "a machine with no state directory reads the same as one with a state directory"
        );
    }

    /// Which variables a platform states a profile directory in is not something the reader knows,
    /// so the report names every one that was looked at: told only that `HOME` names nothing,
    /// somebody on a platform that answers with another variable is being sent to set the one that
    /// was never going to be consulted. The remedy carries the same list as the line above it, since
    /// the remedy is the half somebody acts on.
    ///
    /// Held against two variables rather than this host's own, which states one. Against a list of
    /// one, an assertion that the report names `HOME` is satisfied by the remedy's own wording
    /// whatever the report does with the list, which is a test that passes having pinned nothing.
    #[test]
    fn a_missing_state_directory_names_every_variable_it_looked_at() {
        let lines = state_directory(None, &["HOME", "USERPROFILE"], RESTRICTED);
        let absent = lines.first().expect("the absence is reported");
        let remedy = lines
            .iter()
            .find(|line| line.contains(&t!(doctor_state_directory_remedy).to_string()))
            .expect("the report says how to keep them");

        for line in [absent, remedy] {
            assert!(
                line.contains("HOME or USERPROFILE"),
                "a line of the report names fewer than the two variables looked at: {line}"
            );
        }
    }

    /// A host to stand in for a configured gateway in the accounts below. One that cannot resolve,
    /// so a test that started reaching it would fail rather than reach somebody's service.
    const GATEWAY_HOST: &str = "gateway.invalid";

    /// CRED-25: a gateway token is ended at the gateway, so its account names which one. A
    /// sentence that left the host out would be a record of the same arrangement for every
    /// gateway a settings file configured, which is the fact the person going to revoke it needs
    /// and the one a machine with two gateways cannot supply from the rest of the report: the
    /// lines above it name each gateway by the id somebody chose for a section of their own file.
    #[test]
    fn a_gateway_token_is_accounted_for_at_the_gateway_that_would_end_it() {
        let one = what_would_end(bravebot_config::Held::GatewayToken {
            host: "gateway-one.invalid",
        });
        let two = what_would_end(bravebot_config::Held::GatewayToken {
            host: "gateway-two.invalid",
        });

        assert!(
            one.contains("gateway-one.invalid"),
            "the account does not name the gateway that would end the token: {one}"
        );
        assert!(
            two.contains("gateway-two.invalid"),
            "the account does not name the gateway that would end the token: {two}"
        );
        assert_ne!(
            one, two,
            "two gateways are given one account of what would end their tokens"
        );
    }

    /// CRED-25: each credential this build holds gets its own account of what would end it. One
    /// arm copied to the next is the failure this catches, and it is the failure the report
    /// already had before this existed: the sentences a person was shown named `aws sso login`
    /// and `aws configure sso`, which are how to obtain a credential and say nothing about ending
    /// one, so the three arrangements were answered identically and none of them was answered.
    #[test]
    fn every_held_credential_has_its_own_account_of_what_would_end_it() {
        let every = bravebot_config::Held::all(GATEWAY_HOST);
        let accounts: Vec<String> = every.iter().map(|held| what_would_end(*held)).collect();

        for (held, account) in every.iter().zip(&accounts) {
            assert!(!account.is_empty(), "{held:?} is reported with nothing");
        }
        for (at, account) in accounts.iter().enumerate() {
            assert!(
                !accounts[at + 1..].contains(account),
                "two credentials share one account of what would end them: {account}"
            );
        }
    }

    /// CRED-25: the third thing a held credential records is what revoking it would not reach, and
    /// which credentials have one is the record's answer rather than this report's. Kept apart so
    /// they can disagree, and pinned together so they cannot: a credential given a line here that
    /// the record says nothing survives for tells somebody to chase a session that does not exist,
    /// and one the record says something survives for and that is reported without it leaves the
    /// live credential behind after the leak has been dealt with.
    #[test]
    fn what_survives_revoking_is_reported_for_exactly_the_credentials_that_have_one() {
        for held in bravebot_config::Held::all(GATEWAY_HOST) {
            assert_eq!(
                what_outlives_revoking(held).is_some(),
                held.outlives_revocation(),
                "{held:?} is reported and recorded differently"
            );
        }
    }

    /// CRED-2: the tier a credential stands at reaches the person reading the record, and each tier
    /// reads as itself. One arm copied to the next is the failure this catches, and it is the
    /// failure that costs the most here: a permanent secret reported in the words of a bounded one
    /// tells somebody the issuer will refuse a use nobody can refuse.
    ///
    /// Every tier the record can express rather than only the ones a credential stands at today,
    /// because the sentence a tier is missing is the one nobody notices until a credential arrives
    /// at it, and by then the report has answered it with a neighbouring tier.
    #[test]
    fn each_tier_a_credential_can_stand_at_is_reported_in_its_own_words() {
        let sentences: Vec<&str> = bravebot_config::Tier::ALL
            .iter()
            .map(|tier| what_tier_it_stands_at(*tier))
            .collect();

        for (tier, sentence) in bravebot_config::Tier::ALL.iter().zip(&sentences) {
            assert!(!sentence.is_empty(), "{tier:?} is reported with nothing");
        }
        for (at, sentence) in sentences.iter().enumerate() {
            assert!(
                !sentences[at + 1..].contains(sentence),
                "two tiers share one sentence: {sentence}"
            );
        }

        // The record is what decides which sentence a credential gets, so two credentials the
        // record separates are separated in the report. Reported alike, the tier line would be
        // decoration beside an account that already names the credential.
        assert_ne!(
            what_tier_it_stands_at(bravebot_config::Held::SubscriptionBatch.tier()),
            what_tier_it_stands_at(bravebot_config::Held::SigningKey.tier()),
            "a bounded credential and a permanent one are reported alike"
        );
    }

    /// CRED-26: the binding reaches the person reading the record, for exactly the credentials the
    /// record gives one, and each answer reads as itself. A session STS offers no bound form of and
    /// a batch nobody asked a bound form of are different facts, and only the second is a decision
    /// made here, so a report that printed one sentence for both would hide which one to revisit.
    /// Both say bearer, since that is the word the clause asks the record to use.
    #[test]
    fn a_derived_credential_is_reported_as_a_bearer_secret_in_words_that_keep_the_answers_apart() {
        for held in bravebot_config::Held::all(GATEWAY_HOST) {
            assert_eq!(
                how_it_is_bound(held).is_some(),
                held.binding().is_some(),
                "{held:?} is reported and recorded differently"
            );
        }

        let session =
            how_it_is_bound(bravebot_config::Held::AwsSession).expect("a derived credential");
        let batch = how_it_is_bound(bravebot_config::Held::SubscriptionBatch)
            .expect("a derived credential");
        assert!(session.contains("bearer"), "{session}");
        assert!(batch.contains("bearer"), "{batch}");
        assert_ne!(session, batch);
        assert_ne!(
            how_it_is_bound(bravebot_config::Held::SigningKey),
            Some(session)
        );
    }

    /// CRED-10: the figure for how quickly a leak would be noticed and acted on is reported for
    /// the credentials the record sizes and no others, and the number a person reads is the one
    /// the record holds. A sentence stating its own figure drifts from the record the first time
    /// either is revised, and the reader has no way to tell which of the two they are holding.
    #[test]
    fn how_soon_a_leak_is_noticed_is_reported_for_exactly_the_credentials_the_record_sizes() {
        for held in bravebot_config::Held::all(GATEWAY_HOST) {
            let reported = how_soon_a_leak_is_noticed(held);
            assert_eq!(
                reported.is_some(),
                held.noticed_within().is_some(),
                "{held:?} is reported and recorded differently"
            );

            let (Some(sentence), Some(window)) = (reported, held.noticed_within()) else {
                continue;
            };
            let minutes = (window.as_secs() / 60).to_string();
            assert!(
                sentence.contains(&minutes),
                "{held:?} is reported with a figure the record does not hold: {sentence}"
            );
        }
    }

    /// CRED-3: a reason is reported for exactly the drops the record holds. A credential whose
    /// drops go unreported leaves the tier an assertion, which is the state the clause exists to
    /// end. A reason reported for a drop the record does not hold is the other half and the worse
    /// one: the subscription batch's bound is enforced by the backend that verifies it, and a line
    /// saying it failed gate 2 sends somebody looking for a bound nothing is missing, while a gate
    /// 3 line naming a condition the walk did not fail is a stated reason that is not the reason.
    ///
    /// Asked of every condition of every gate rather than of the walk alone, since the walk is
    /// what both sides read and a report keyed on something coarser agrees with it by accident.
    #[test]
    fn a_reason_is_reported_for_exactly_the_drops_the_record_holds() {
        for held in bravebot_config::Held::all(GATEWAY_HOST) {
            for condition in bravebot_config::Condition::all() {
                let recorded = held.walk().iter().any(|drop| drop.condition == condition);
                let reported = why_it_dropped(
                    held,
                    bravebot_config::GateDrop {
                        condition,
                        attempt: bravebot_config::Attempt::NotAttempted,
                    },
                );

                assert_eq!(
                    reported.is_some_and(|line| !line.is_empty()),
                    recorded,
                    "{held:?} is reported and recorded differently at gate {}, on {condition:?}",
                    condition.gate().number()
                );
            }
        }
    }

    /// CRED-3: a reported drop states the gate and the answer the record holds. A sentence
    /// carrying its own gate number drifts from the walk the first time either is revised, and one
    /// carrying its own answer is the failure that matters: the counterparty refusing and nobody
    /// attempting end at the same tier, and only the second is anybody's here to revisit, so a
    /// report that always says one of them tells a reader nothing they can act on.
    #[test]
    fn a_drop_is_reported_with_the_gate_and_the_answer_the_record_holds() {
        for held in bravebot_config::Held::all(GATEWAY_HOST) {
            for drop in held.walk() {
                let line = why_it_dropped(held, *drop).expect("a drop the record holds");
                let gate = drop.condition.gate().number().to_string();
                assert!(
                    line.contains(&gate),
                    "{held:?} is reported at a gate the record does not name: {line}"
                );

                // The same condition with the other answer, which is the only difference between
                // a fact about the world and a decision made here.
                let other = bravebot_config::GateDrop {
                    attempt: match drop.attempt {
                        bravebot_config::Attempt::Refused => bravebot_config::Attempt::NotAttempted,
                        bravebot_config::Attempt::NotAttempted => bravebot_config::Attempt::Refused,
                    },
                    ..*drop
                };
                assert_ne!(
                    Some(line),
                    why_it_dropped(held, other),
                    "{held:?} reads the same at gate {gate} whether the counterparty refused or \
                     nobody attempted it"
                );
            }
        }
    }

    /// CRED-3: each drop has its own account of the condition that failed. One arm copied to the
    /// next is what this catches, and a walk reported as three identical sentences says which gates
    /// failed without saying what about them failed, which is the half of the clause that is not
    /// the tier.
    ///
    /// Compared with the gate numbers taken out, because the number is passed in from the record:
    /// an account copied from another gate is told apart by the digit the caller supplied rather
    /// than by anything it says, and the sentence a person reads is then about the wrong
    /// condition.
    #[test]
    fn every_drop_has_its_own_account_of_the_condition_it_failed() {
        let reported: Vec<String> = bravebot_config::Held::all(GATEWAY_HOST)
            .iter()
            .flat_map(|held| {
                held.walk()
                    .iter()
                    .filter_map(|drop| why_it_dropped(*held, *drop))
            })
            .map(|line| line.replace(|c: char| c.is_ascii_digit(), ""))
            .collect();

        for (at, line) in reported.iter().enumerate() {
            assert!(
                !reported[at + 1..].contains(line),
                "two drops share one account of the condition that failed: {line}"
            );
        }
    }

    /// The gateway a settings file configured, for the `doctor` tests below.
    fn configured_gateway(text: &str) -> bravebot_config::provider::Provider {
        bravebot_config::Settings::parse(text)
            .providers()
            .first()
            .expect("one provider")
            .clone()
    }

    /// A gateway's credential is a long-lived bearer token, so a diagnostic that echoed one would put
    /// a live credential in every issue somebody pastes this into.
    #[test]
    fn a_gateway_credential_is_reported_as_found_and_never_printed() {
        let provider = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "in-the-file"}
            }}}"#,
        );

        let from_variable = gateway_credential(&provider, |name| match name {
            "A_TOKEN_VARIABLE" => Some("secret-from-the-environment".to_string()),
            _ => None,
        });
        assert!(!from_variable.contains("secret-from-the-environment"));

        // A token written into the file is found too, and is withheld on the same footing.
        let from_file = gateway_credential(&provider, |_| None);
        assert!(!from_file.contains("in-the-file"));
        assert_eq!(from_variable, from_file);
    }

    /// A gateway nothing holds a credential for says so. Reporting one as found would answer the
    /// question `doctor` exists to answer wrongly, and the request then fails somewhere further away.
    #[test]
    fn a_gateway_with_no_credential_is_reported_as_having_none() {
        let provider = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["ABSENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1"}
            }}}"#,
        );

        assert_ne!(
            gateway_credential(&provider, |_| None),
            gateway_credential(&provider, |_| Some("anything".to_string()))
        );
    }

    /// A gateway whose block names no credential needs none, and saying "none found" of it reads as
    /// something to go and set on a gateway that is working. Distinct from the case above, which names
    /// a variable and really does want a token in it.
    #[test]
    fn a_gateway_needing_no_credential_is_reported_as_needing_none() {
        let needs_none = configured_gateway(
            r#"{"provider": {"ollama": {
                "options": {"baseURL": "http://localhost:11434/v1"}
            }}}"#,
        );
        let names_one = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["ABSENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1"}
            }}}"#,
        );

        assert_eq!(
            gateway_credential(&needs_none, |_| None),
            t!(doctor_gateway_token_not_needed)
        );
        assert_eq!(
            gateway_credential(&names_one, |_| None),
            t!(doctor_gateway_token_absent, id = "gw")
        );
    }

    /// CRED-25: a key `bravebot auth login gateway` stored is named as stored, since neither the
    /// environment nor the settings file shows it, and is withheld like any other. A variable the
    /// block names is sent before it, and is what the report then names.
    #[test]
    fn a_stored_gateway_key_is_named_as_stored_and_never_printed() {
        let mut provider = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://example.invalid/v1"}
            }}}"#,
        );
        provider.stored_key = Some(bravebot_config::Secret::new("placeholder-stored-key"));

        let stored = gateway_credential(&provider, |_| None);
        assert_eq!(stored, t!(doctor_gateway_token_stored));
        assert!(!stored.contains("placeholder-stored-key"));
        assert_eq!(
            gateway_credential(&provider, |_| Some("from-the-environment".to_string())),
            t!(doctor_gateway_token)
        );
    }

    /// A Google Vertex block naming no models is offered the compiled list, so `doctor` names those
    /// rather than saying the service is asked, which it never is, and names them as they are typed:
    /// a bare compiled id routes nowhere. A block naming models is offered those alone, and is
    /// reported so.
    #[test]
    fn doctor_names_the_compiled_models_a_google_vertex_service_is_offered() {
        let names_none = configured_gateway(
            r#"{"provider": {"google-vertex": {"options": {"project": "example-project-1"}}}}"#,
        );
        let reported = gateway_models(&names_none);
        let compiled = names_none.compiled_roster().expect("a compiled list");
        assert!(!compiled.is_empty());
        for id in compiled {
            let qualified = format!("google-vertex/{id}");
            assert!(
                reported.contains(&qualified),
                "{qualified} is not in {reported:?}"
            );
        }

        let names_one = configured_gateway(
            r#"{"provider": {"google-vertex": {
                "options": {"project": "example-project-1"},
                "models": {"google/gemini-3-flash-preview": {}}
            }}}"#,
        );
        assert_eq!(gateway_models(&names_one), "google/gemini-3-flash-preview");
    }

    fn development_fixture(name: &str) -> Scratch {
        let scratch = Scratch::new(name);
        for marker in [
            "Cargo.toml",
            "crates/cli/Cargo.toml",
            "agents/setup.py",
            "agents/AGENTS.md",
            "docs/development/agent-configuration.md",
        ] {
            let file = scratch.path.join(marker);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "source instructions").unwrap();
        }
        scratch
    }

    /// A user's workspace needs neither the repository's links nor its build tools.
    #[test]
    fn doctor_development_checks_only_apply_to_the_source_tree() {
        let scratch = Scratch::new("doctor-ordinary-workspace");
        scratch.directory(".git");
        std::fs::write(scratch.path.join("AGENTS.md"), "user instructions").unwrap();
        assert!(development(&scratch.path, None, false).is_empty());
        std::fs::remove_dir(scratch.path.join(".git")).unwrap();
        std::fs::write(scratch.path.join(".git"), "gitdir: elsewhere").unwrap();
        assert!(development(&scratch.path, None, false).is_empty());
        let source = development_fixture("doctor-source-tree");
        std::fs::write(source.path.join(".git"), "gitdir: elsewhere").unwrap();
        for windows in [false, true] {
            assert!(agent_discovery(&source.path, windows).starts_with("missing;"));
        }
        let nested = source.directory("crates/cli/src");
        let report = development(&nested, None, false).join("\n");
        assert!(report.contains("development environment"));
        assert!(report.contains("AGENTS.md"));
        assert!(report.contains("python3 agents/setup.py link"));
        assert!(report.contains("direnv"));
        assert!(report.contains("https://direnv.net/"));
        assert!(report.contains("brew install direnv"));
        assert!(!source.path.join("AGENTS.md").exists());
    }

    /// A hand-written path must survive a diagnostic and must not look healthy.
    #[test]
    fn doctor_reports_agent_discovery_conflicts_without_changing_them() {
        let scratch = development_fixture("doctor-conflicts");
        let destination = scratch.path.join("AGENTS.md");
        std::fs::write(&destination, "personal instructions").unwrap();
        let report = agent_discovery(&scratch.path, false);
        assert!(report.contains("conflict"));
        assert!(report.contains("resolve the existing file or directory first"));
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "personal instructions"
        );
        std::fs::remove_file(&destination).unwrap();
        std::fs::create_dir(&destination).unwrap();
        for windows in [false, true] {
            assert!(agent_discovery(&scratch.path, windows).contains("conflict"));
            assert!(destination.is_dir());
        }
    }

    /// Windows setup copies the source, so a current copy is healthy and a stale one is repairable.
    #[test]
    fn doctor_accepts_current_windows_copies_and_reports_stale_ones() {
        let scratch = development_fixture("doctor-windows-copy");
        let destination = scratch.path.join("AGENTS.md");
        std::fs::copy(scratch.path.join("agents/AGENTS.md"), &destination).unwrap();
        assert_eq!(
            agent_discovery(&scratch.path, true),
            "OK (Windows copy of agents/AGENTS.md)"
        );
        std::fs::write(&destination, "old instructions").unwrap();
        let report = agent_discovery(&scratch.path, true);
        assert!(report.contains("stale"));
        assert!(report.contains("python3 agents/setup.py link"));
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "old instructions"
        );
    }

    /// Existence alone does not mean an agent will read this repository's instructions.
    #[cfg(unix)]
    #[test]
    fn doctor_checks_resolved_agent_link_targets() {
        use std::os::unix::fs::symlink;
        let scratch = development_fixture("doctor-links");
        let destination = scratch.path.join("AGENTS.md");
        for target in [
            std::path::PathBuf::from("agents/AGENTS.md"),
            std::path::PathBuf::from("agents/../agents/AGENTS.md"),
            scratch.path.join("agents/AGENTS.md"),
        ] {
            symlink(target, &destination).unwrap();
            assert_eq!(
                agent_discovery(&scratch.path, false),
                "OK (link to agents/AGENTS.md)"
            );
            std::fs::remove_file(&destination).unwrap();
        }
        std::fs::write(scratch.path.join("other.md"), "source instructions").unwrap();
        symlink("other.md", &destination).unwrap();
        let report = agent_discovery(&scratch.path, false);
        assert!(report.contains("wrong target"), "{report}");
        assert!(report.contains("python3 agents/setup.py link"));
        assert_eq!(
            std::fs::read_link(&destination).unwrap(),
            Path::new("other.md")
        );
        std::fs::remove_file(scratch.path.join("other.md")).unwrap();
        let report = agent_discovery(&scratch.path, false);
        assert!(report.contains("broken"));
        assert!(report.contains("python3 agents/setup.py link"));
        assert!(destination.is_symlink());
    }

    /// PATH is supplied by the fixture, so an installed host tool cannot hide a missing-tool bug.
    #[test]
    fn doctor_finds_direnv_only_when_path_contains_an_executable() {
        let scratch = development_fixture("doctor-direnv");
        let bin = scratch.directory("bin");
        let path = std::env::join_paths([&bin]).unwrap();
        assert!(!direnv_available(None));
        assert!(!direnv_available(Some(&path)));
        let executable = bin.join(if cfg!(windows) {
            "direnv.exe"
        } else {
            "direnv"
        });
        std::fs::create_dir(&executable).unwrap();
        assert!(!direnv_available(Some(&path)));
        std::fs::remove_dir(&executable).unwrap();
        std::fs::write(&executable, "not executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(!direnv_available(Some(&path)));
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(direnv_available(Some(&path)));
        let report = development(&scratch.path, Some(&path), false).join("\n");
        assert!(report.contains("available on PATH"));
        assert!(!report.contains("brew install"));
    }

    /// SANDBOX-25: where the backend cannot hold a path back from the session directory, `doctor`
    /// says a denial inside it is not in force and why, so the report does not call "in force" an
    /// entry whose only effect is that every stage is refused. The regression it rejects is the line
    /// a stage that would run with the file reachable gets today: the bare entry.
    #[test]
    fn doctor_reports_a_denial_the_backend_cannot_subtract_as_not_in_force() {
        use bravebot_sandbox::rules::{Entry, Lists, resolve};
        let directory = scratch("cannot-subtract").canonicalize().unwrap();
        let entry = |path: &str| Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        };
        let outside = directory.parent().unwrap().join("outside.env");
        let outside = outside.to_str().unwrap();
        let lists = Lists {
            deny_read: vec![entry("secret.env")],
            deny_write: vec![entry("secret.env"), entry(outside)],
            ..Lists::default()
        };
        let rules = resolve(&lists, None, &directory);
        let backend = |subtracts_from_a_grant| Capabilities {
            level: ConfinementLevel::Partial,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            egress_limited_to_a_port: false,
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant,
            governs_sockets_with_a_path: true,
        };
        let cannot = t!(sandbox_rule_cannot_subtract).to_string();
        let flag = t!(doctor_sandbox_filesystem_source_flag);

        let refused = sandbox_filesystem_lines(&rules, &directory, Some(&backend(false)));
        assert_eq!(
            refused,
            [
                format!("denyRead secret.env is not in force: {cannot} ({flag})"),
                format!("denyWrite secret.env is not in force: {cannot} ({flag})"),
                format!("denyWrite {outside} ({flag})"),
            ]
        );
        for held in [
            sandbox_filesystem_lines(&rules, &directory, Some(&backend(true))),
            sandbox_filesystem_lines(&rules, &directory, None),
        ] {
            assert_eq!(
                held,
                [
                    format!("denyRead secret.env ({flag})"),
                    format!("denyWrite secret.env ({flag})"),
                    format!("denyWrite {outside} ({flag})"),
                ]
            );
        }
    }

    /// SANDBOX-22 and SANDBOX-10: `doctor` names the mode and where it came from, and names the
    /// managed file when the settings went under its pin. The regression it rejects is a report
    /// that says `workspace-write` where a machine pins `strict`.
    #[test]
    fn doctor_names_the_sandbox_mode_and_where_it_came_from() {
        let unset = sandbox_mode_line(&bravebot_config::Settings::default(), &Managed::default())
            .expect("nothing to refuse");
        assert!(unset.contains("workspace-write"), "{unset}");
        assert!(unset.contains("default"), "{unset}");

        let pin = pin_file("pinned", r#"{"sandbox": {"mode": "strict"}}"#);
        let pinned = sandbox_mode_line(&bravebot_config::Settings::default(), &pin)
            .expect("the pin alone is not a refusal");
        assert!(pinned.contains("strict"), "{pinned}");
        assert!(
            pinned.contains(&pin.path().expect("a pin").display().to_string()),
            "{pinned}"
        );
    }

    /// SANDBOX-22: a setting under the managed floor is reported with both files rather than as
    /// the pin. The regression it rejects is `doctor` calling a machine fine whose next session
    /// would be refused.
    #[test]
    fn doctor_reports_a_mode_the_managed_file_refuses() {
        let home = scratch("refused");
        let file = home.join("settings.json");
        std::fs::write(&file, r#"{"sandbox": {"mode": "off"}}"#).expect("home layer");
        let settings = bravebot_config::Settings::layered(Some(home), None, None);
        let pin = pin_file("floor", r#"{"sandbox": {"mode": "strict"}}"#);

        let refused = sandbox_mode_line(&settings, &pin).expect_err("under the floor");
        assert!(refused.contains(&file.display().to_string()), "{refused}");
        assert!(
            refused.contains(&pin.path().expect("a pin").display().to_string()),
            "{refused}"
        );
        assert!(
            refused.contains("strict") && refused.contains("off"),
            "{refused}"
        );
    }
}
