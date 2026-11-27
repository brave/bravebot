//! What the plugin install question and the update question list, as data (PLUGIN-4, PLUGIN-9,
//! PLUGIN-13), shared by the terminal interface and the desktop bridge so the two list the same
//! parts.
//!
//! The rows are built from what [`crate::plugins::stage`] read out of the staged tree and what
//! [`Before`] read of the installed copy: each skill and definition with what shadows it, each
//! hook's moment and argv and each server's transport, argv or url, variables and files, with
//! `${PLUGIN_ROOT}` already replaced by the directory the plugin will have. Nothing here decides
//! what runs or what is replaced, and nothing here draws: a front end draws these rows, and every
//! string in them but a kind, a transport and a digest is the plugin's own text.
//!
//! A hooks or servers file that could not be read is a fault with no rows, never an empty list, and
//! whatever the hook reader passed over is carried along, so a listing never says less than the
//! plugin carries.

use crate::plugin_servers;
use crate::plugin_source::Source;
use crate::plugins::{self, Before, FileChange, Installed, Offer, Offered, Staged};
use crate::skills::Catalogue;
use bravebot_config::hooks::{Hook, Unkept};
use bravebot_config::mcp::Declaration;
use bravebot_config::plugin::{FileFault, HookList, ServerList};
use bravebot_core::delegate::Definitions;

/// A skill the plugin offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    pub name: String,
    pub description: String,
    /// What takes the name instead, where a source above a plugin offers it.
    pub shadowed_by: Option<String>,
}

/// A definition the plugin offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionRow {
    pub name: String,
    pub kind: String,
    pub shadowed_by: Option<String>,
}

/// The rows only a git source has (PLUGIN-4, PLUGIN-14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRows {
    /// The ref the person typed after `#`, or `None` where they named none.
    pub requested_ref: Option<String>,
    /// The full 40-digit commit the clone resolved to.
    pub commit: String,
}

/// One hook the plugin's hooks file declares, argv as it will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRow {
    /// The moment, in the words the hooks file spells it with.
    pub moment: String,
    /// The tool the hook is about, where the entry named one.
    pub tool: Option<String>,
    /// The file a `file-changed` entry watches.
    pub watches: Option<String>,
    /// The program and its arguments, `${PLUGIN_ROOT}` replaced.
    pub argv: Vec<String>,
    /// The bound the entry states, in seconds.
    pub timeout_secs: Option<u64>,
}

/// Something a hook or a server would reach, which is on its own row (PLUGIN-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A program that would be started.
    Command(String),
    /// A host that would be connected to.
    Host(String),
    /// A file a local server may read.
    File(String),
    /// The directory a local server runs in.
    Directory(String),
}

/// One server the plugin's servers file declares, command as it will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRow {
    /// The alias as the file wrote it.
    pub alias: String,
    /// Whether the entry states a declaration that can be used. An entry that does not is listed
    /// as one and is never started.
    pub usable: bool,
    /// Whether a session can reach the server under `<plugin>_<alias>`. A usable entry whose joined
    /// alias is longer than an alias may be is listed in full and is never started.
    pub reachable: bool,
    /// `stdio` or `http`, empty for an entry that cannot be used.
    pub transport: String,
    /// A local server's program and arguments, `${PLUGIN_ROOT}` replaced.
    pub argv: Vec<String>,
    /// A remote server's url as written.
    pub url: Option<String>,
    /// The names of the variables it receives from the environment, or names for a header or a
    /// bearer token.
    pub variables: Vec<String>,
    /// The variables it is given with a value, as `NAME=value`, left as written.
    pub stored: Vec<String>,
    /// What it would reach, one row each.
    pub reaches: Vec<Reach>,
}

/// What one part file held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed<T> {
    pub rows: Vec<T>,
    /// Why the file could not be read at all, where it could not.
    pub fault: Option<FileFault>,
    /// What the hook reader passed over. Always none for servers, whose unusable entries are rows.
    pub unkept: Unkept,
}

impl<T> Listed<T> {
    /// A file that could not be read.
    fn unread(fault: FileFault) -> Self {
        Self {
            rows: Vec::new(),
            fault: Some(fault),
            unkept: Unkept::default(),
        }
    }
}

/// What the install question lists. Built from the parts of the staged tree, not from a field the
/// manifest wrote about itself (PLUGIN-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    pub name: String,
    pub version: String,
    /// The source directory. A local path has no commit, so none is shown.
    pub source: String,
    /// What a git source adds to the listing. `None` for a local path, which has no commit.
    pub git: Option<GitRows>,
    pub skills: Vec<SkillRow>,
    pub definitions: Vec<DefinitionRow>,
    /// The hooks the plugin's hooks file declares, `${PLUGIN_ROOT}` already replaced. `None` where
    /// the manifest names no hooks file.
    pub hooks: Option<Listed<HookRow>>,
    /// The servers the plugin's servers file declares, `${PLUGIN_ROOT}` already replaced. `None`
    /// where the manifest names no servers file.
    pub servers: Option<Listed<ServerRow>>,
}

impl InstallRequest {
    /// The listing of `staged`, with each name a higher-ranked source offers marked as shadowed.
    /// `catalogue` and `definitions` are what discovery found for this session.
    pub fn from_staged(staged: &Staged, catalogue: &Catalogue, definitions: &Definitions) -> Self {
        let (skills, kinds) = offer_rows(&plugins::staged_offers(staged), catalogue, definitions);
        Self {
            name: staged.manifest.name.clone(),
            version: staged.manifest.version.clone(),
            source: staged.source.to_string(),
            git: git_rows(&staged.source),
            skills,
            definitions: kinds,
            hooks: hooks_from(staged.hooks.as_ref()),
            servers: servers_from(&staged.manifest.name, staged.servers.as_ref()),
        }
    }
}

/// One part of the plugin that differs between the installed copy and the staged one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta<T> {
    /// The staged tree has it and the installed copy does not.
    Added(T),
    /// The installed copy has it and the staged tree does not.
    Removed(T),
    /// Both have it and it differs.
    Changed { was: T, now: T },
}

/// The difference in a hooks or servers file, and what could not be read of either side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartDeltas<T> {
    pub deltas: Vec<Delta<T>>,
    /// Why the installed copy's file could not be read, where it could not. What it held is then
    /// not known, so every row of the new file shows as added and the listing says so.
    pub installed_fault: Option<FileFault>,
    /// Why the staged file could not be read, and what its reader passed over.
    pub new_fault: Option<FileFault>,
    pub new_unkept: Unkept,
}

/// The rows only a git source has: the commit installed and the one the recorded ref resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateGit {
    pub requested_ref: Option<String>,
    pub installed_commit: String,
    pub new_commit: String,
}

/// What the update question lists, built from the parts of the two trees and not from a field the
/// manifest wrote about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRequest {
    pub name: String,
    pub installed_version: String,
    pub new_version: String,
    pub source: String,
    /// `None` for a local path, which has no commit.
    pub git: Option<UpdateGit>,
    /// Whether the installed copy's manifest could be read. Where it could not, every part shows
    /// as added and the listing says that.
    pub installed_readable: bool,
    pub skills: Vec<Delta<SkillRow>>,
    pub definitions: Vec<Delta<DefinitionRow>>,
    pub hooks: Option<PartDeltas<HookRow>>,
    pub servers: Option<PartDeltas<ServerRow>>,
    /// Every file that differs, so a change to a script the parts do not name is listed too.
    /// `None` where the two trees could not be compared, and the listing says so.
    pub files: Option<Vec<FileChange>>,
}

impl UpdateRequest {
    /// The difference between the installed plugin `installed`, as `before` read it, and `staged`.
    /// `catalogue` and `definitions` are what discovery found for this session, so a name a
    /// higher-ranked source takes is marked as shadowed.
    pub fn from_parts(
        staged: &Staged,
        installed: &Installed,
        before: &Before,
        catalogue: &Catalogue,
        definitions: &Definitions,
    ) -> Self {
        let (old_skills, old_definitions) = offer_rows(&before.offers, catalogue, definitions);
        let (new_skills, new_definitions) =
            offer_rows(&plugins::staged_offers(staged), catalogue, definitions);
        let git = match (installed.source.commit(), staged.source.commit()) {
            (Some(old), Some(new)) => Some(UpdateGit {
                requested_ref: match &staged.source {
                    Source::Git { requested_ref, .. } => requested_ref.clone(),
                    Source::Local(_) => None,
                },
                installed_commit: old.to_string(),
                new_commit: new.to_string(),
            }),
            _ => None,
        };
        Self {
            name: staged.manifest.name.clone(),
            installed_version: installed.version.clone(),
            new_version: staged.manifest.version.clone(),
            source: staged.source.to_string(),
            git,
            installed_readable: before.readable,
            skills: deltas(
                &old_skills,
                &new_skills,
                |a, b| a.name == b.name && a.description == b.description,
                |skill| skill.name.clone(),
            ),
            definitions: deltas(
                &old_definitions,
                &new_definitions,
                |a, b| a.name == b.name && a.kind == b.kind,
                |definition| definition.name.clone(),
            ),
            hooks: part_deltas(
                hooks_from(before.hooks.as_ref()).as_ref(),
                hooks_from(staged.hooks.as_ref()).as_ref(),
                |hook| format!("{}\0{:?}\0{:?}", hook.moment, hook.tool, hook.watches),
            ),
            servers: part_deltas(
                servers_from(&installed.name, before.servers.as_ref()).as_ref(),
                servers_from(&staged.manifest.name, staged.servers.as_ref()).as_ref(),
                |server| server.alias.clone(),
            ),
            files: plugins::file_changes(staged, before),
        }
    }
}

/// The skill and definition rows of `offers`, each name a higher-ranked source offers marked as
/// shadowed. Shared by the install listing and the update difference.
pub fn offer_rows(
    offers: &[Offer],
    catalogue: &Catalogue,
    definitions: &Definitions,
) -> (Vec<SkillRow>, Vec<DefinitionRow>) {
    let mut skills = Vec::new();
    let mut kinds = Vec::new();
    for mark in plugins::shadow_marks(offers, catalogue, definitions) {
        match mark.offer.what {
            Offered::Skill { description } => skills.push(SkillRow {
                name: mark.offer.name,
                description,
                shadowed_by: mark.by,
            }),
            Offered::Definition { kind } => kinds.push(DefinitionRow {
                name: mark.offer.name,
                kind: kind.to_string(),
                shadowed_by: mark.by,
            }),
        }
    }
    (skills, kinds)
}

/// The rows only a git source has, or `None` for a local path, which has no commit.
pub fn git_rows(source: &Source) -> Option<GitRows> {
    match source {
        Source::Local(_) => None,
        Source::Git {
            requested_ref,
            commit,
            ..
        } => Some(GitRows {
            requested_ref: requested_ref.clone(),
            commit: commit.to_string(),
        }),
    }
}

fn hook_row(hook: &Hook) -> HookRow {
    HookRow {
        moment: hook.moment().as_str().to_string(),
        tool: hook.tool().map(str::to_string),
        watches: hook.path().map(str::to_string),
        argv: hook.run().to_vec(),
        timeout_secs: hook.timeout().map(|bound| bound.as_secs()),
    }
}

/// The hook rows of a hooks file as the staging or the installed copy read it, `None` where the
/// manifest names no hooks file.
pub fn hooks_from(read: Option<&Result<HookList, FileFault>>) -> Option<Listed<HookRow>> {
    read.map(|read| match read {
        Ok(list) => Listed {
            rows: list.hooks.declared().iter().map(hook_row).collect(),
            fault: None,
            unkept: list.unkept,
        },
        Err(fault) => Listed::unread(*fault),
    })
}

/// The hosts, files and directory a declaration would reach, and the program it would start.
fn reaches(declaration: &Declaration) -> Vec<Reach> {
    let mut found = Vec::new();
    if let Some(program) = declaration.argv().first() {
        found.push(Reach::Command(program.clone()));
    }
    if let Some(host) = declaration.url().and_then(bravebot_core::url::host_of) {
        found.push(Reach::Host(host));
    }
    for host in declaration.allowed_hosts().unwrap_or_default() {
        found.push(Reach::Host(host.clone()));
    }
    found.extend(declaration.reads().iter().cloned().map(Reach::File));
    found.extend(
        declaration
            .directory()
            .map(|d| Reach::Directory(d.to_string())),
    );
    found
}

fn server_row(plugin: &str, alias: &str, declaration: &Declaration) -> ServerRow {
    let mut variables: Vec<String> = declaration.variables().to_vec();
    variables.extend(declaration.bearer_env().map(str::to_string));
    variables.extend(
        declaration
            .env_http_headers()
            .map(|(header, variable)| format!("{variable} ({header})")),
    );
    ServerRow {
        alias: alias.to_string(),
        usable: true,
        reachable: plugin_servers::is_reachable(plugin, alias),
        transport: declaration.transport().to_string(),
        argv: declaration.argv().to_vec(),
        url: declaration.url().map(str::to_string),
        variables,
        stored: declaration
            .stored_values()
            .map(|(name, value)| format!("{name}={value}"))
            .collect(),
        reaches: reaches(declaration),
    }
}

/// The server rows of a servers file as the staging or the installed copy read it, for the plugin
/// called `plugin`, `None` where the manifest names no servers file.
pub fn servers_from(
    plugin: &str,
    read: Option<&Result<ServerList, FileFault>>,
) -> Option<Listed<ServerRow>> {
    read.map(|read| match read {
        Ok(list) => Listed {
            rows: list
                .entries
                .iter()
                .map(|entry| match &entry.declaration {
                    Ok(declaration) => server_row(plugin, &entry.alias, declaration),
                    Err(_) => ServerRow {
                        alias: entry.alias.clone(),
                        usable: false,
                        reachable: false,
                        transport: String::new(),
                        argv: Vec::new(),
                        url: None,
                        variables: Vec::new(),
                        stored: Vec::new(),
                        reaches: Vec::new(),
                    },
                })
                .collect(),
            fault: None,
            unkept: Unkept::default(),
        },
        Err(fault) => Listed::unread(*fault),
    })
}

/// The deltas turning `old` into `new`. An item that is equal (by `same`) on both sides is not a
/// delta. Of what is left, an item of `new` with the same `key` as one of `old` is changed, one with
/// no partner is added, and an item of `old` with none is removed. Added and changed come in the
/// order of `new`, then removed in the order of `old`.
pub fn deltas<T: Clone>(
    old: &[T],
    new: &[T],
    same: impl Fn(&T, &T) -> bool,
    key: impl Fn(&T) -> String,
) -> Vec<Delta<T>> {
    let mut left: Vec<Option<&T>> = old.iter().map(Some).collect();
    let mut unmatched: Vec<&T> = Vec::new();
    for item in new {
        let equal = left
            .iter()
            .position(|held| held.is_some_and(|held| same(held, item)));
        match equal {
            Some(at) => left[at] = None,
            None => unmatched.push(item),
        }
    }
    let mut found = Vec::new();
    for item in unmatched {
        let partner = left
            .iter()
            .position(|held| held.is_some_and(|held| key(held) == key(item)));
        found.push(match partner.and_then(|at| left[at].take()) {
            Some(was) => Delta::Changed {
                was: was.clone(),
                now: item.clone(),
            },
            None => Delta::Added(item.clone()),
        });
    }
    found.extend(left.into_iter().flatten().cloned().map(Delta::Removed));
    found
}

/// The difference between two readings of a hooks or servers file, `None` where neither names one.
fn part_deltas<T: Clone + PartialEq>(
    old: Option<&Listed<T>>,
    new: Option<&Listed<T>>,
    key: impl Fn(&T) -> String,
) -> Option<PartDeltas<T>> {
    if old.is_none() && new.is_none() {
        return None;
    }
    let rows = |listed: Option<&Listed<T>>| listed.map_or_else(Vec::new, |some| some.rows.clone());
    Some(PartDeltas {
        deltas: deltas(&rows(old), &rows(new), |a, b| a == b, key),
        installed_fault: old.and_then(|listed| listed.fault),
        new_fault: new.and_then(|listed| listed.fault),
        new_unkept: new.map_or_else(Unkept::default, |listed| listed.unkept),
    })
}
