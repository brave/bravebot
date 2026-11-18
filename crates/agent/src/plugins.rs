//! Installing a plugin from a local directory (PLUGIN-3 local-path part, PLUGIN-5).
//!
//! `stage` reads the plugin in place with the manifest reader, runs the link check, copies the
//! tree to a staging directory and digests the copy. `commit_install` moves the copy to
//! `<state>/plugins/<name>/` and records its source, enabled state and digest beside it.
//! Nothing in the plugin is executed. The manifest and every file are carried as bytes: the only
//! decisions made here are about names, paths and file types.
//!
//! Every function takes the state directory (`~/.bravebot`, the `home` that skill and definition
//! discovery take) as a parameter, so a test points it at a temporary directory. The staging
//! directory is made directly under it, on the same filesystem as `plugins/`, and never in the
//! system temporary directory, which other accounts can write.

use bravebot_config::plugin::{self, Manifest, Part};
use sha2::{Digest as _, Sha256};
use std::io;
use std::path::{Path, PathBuf};

/// The directory under the state directory that holds installed plugins.
const PLUGINS: &str = "plugins";

/// The suffix of the file recorded beside an installed plugin's directory. A plugin name holds no
/// dot, so `<name>.record` is never another plugin's directory.
const RECORD_SUFFIX: &str = ".record";

/// Why an install did not happen. Nothing is installed when one is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The manifest, or a link in the tree, was refused by the reader (PLUGIN-2, PLUGIN-6).
    Manifest(plugin::Refusal),
    /// The manifest names a part path that does not exist in the source.
    PartMissing(Part),
    /// The source is not a directory or could not be read.
    Source(io::ErrorKind),
    /// The source path is not text, so it cannot be recorded.
    SourceNotText,
    /// The staging copy or the install failed.
    Io(io::ErrorKind),
    /// The name is installed from another source. `installed` is `None` when only the directory
    /// is there and its record cannot be read.
    AlreadyInstalled {
        name: String,
        installed: Option<PathBuf>,
        new: PathBuf,
    },
    /// Nobody can be asked (a one-shot run), so nothing is installed (PLUGIN-4).
    NobodyToAsk,
    /// The person answered no. Nothing was copied or recorded.
    Declined,
    /// The name is installed from this same source. Updating is not built (decision 1).
    UpdateNotBuilt { name: String, source: PathBuf },
    /// Another installed plugin, enabled or not, already offers a skill or definition of this
    /// name (PLUGIN-12). Both plugins are named and no prefix is added.
    NameOffered {
        offered: String,
        new_plugin: String,
        existing_plugin: String,
    },
}

/// A plugin copied to a staging directory and digested, not yet installed. The staging directory
/// is removed when this is dropped.
#[derive(Debug)]
pub struct Staged {
    /// Owns the staging directory and removes it when the value is dropped.
    _holder: Holder,
    tree: PathBuf,
    /// The source directory as a canonical absolute path.
    pub source: PathBuf,
    pub manifest: Manifest,
    /// Lowercase hex SHA-256 of the staged tree.
    pub digest: String,
    /// Paths, relative to the source, of links and other non-regular files that were not copied.
    pub skipped: Vec<PathBuf>,
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "passed by name to `map_err`, which hands the error over by value"
)]
fn io_refusal(error: io::Error) -> Refusal {
    Refusal::Io(error.kind())
}

/// Copies the regular files and directories under `from` into `to`, which is created. A link or
/// any other kind of entry is never followed and never copied: its path, relative to the tree's
/// root, is returned. File types come from the directory entry, not from `metadata`, so a link is
/// never looked through.
fn copy_tree(from: &Path, to: &Path) -> Result<Vec<PathBuf>, Refusal> {
    let mut skipped = Vec::new();
    std::fs::create_dir(to).map_err(io_refusal)?;
    copy_dir(from, to, Path::new(""), &mut skipped)?;
    skipped.sort();
    Ok(skipped)
}

fn copy_dir(
    from: &Path,
    to: &Path,
    relative: &Path,
    skipped: &mut Vec<PathBuf>,
) -> Result<(), Refusal> {
    for entry in std::fs::read_dir(from).map_err(io_refusal)? {
        let entry = entry.map_err(io_refusal)?;
        let kind = entry.file_type().map_err(io_refusal)?;
        let name = entry.file_name();
        if kind.is_dir() {
            let target = to.join(&name);
            std::fs::create_dir(&target).map_err(io_refusal)?;
            copy_dir(&entry.path(), &target, &relative.join(&name), skipped)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(&name)).map_err(io_refusal)?;
        } else {
            skipped.push(relative.join(&name));
        }
    }
    Ok(())
}

/// The SHA-256 of a tree as lowercase hex: every directory and file in sorted order of relative
/// path, each as a tag byte, the length and bytes of its relative path (`/` between names) and,
/// for a file, the length and bytes of its content. A tree that holds a link is an error, since a
/// staged copy never does.
fn digest_tree(root: &Path) -> Result<String, Refusal> {
    let mut found: Vec<(Vec<u8>, PathBuf, bool)> = Vec::new();
    collect(root, &[], &mut found)?;
    found.sort();
    let mut hasher = Sha256::new();
    for (relative, path, is_dir) in &found {
        hasher.update([if *is_dir { b'D' } else { b'F' }]);
        hasher.update((relative.len() as u64).to_be_bytes());
        hasher.update(relative);
        if !*is_dir {
            let length = std::fs::symlink_metadata(path).map_err(io_refusal)?.len();
            hasher.update(length.to_be_bytes());
            feed(&mut hasher, path)?;
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn collect(
    dir: &Path,
    prefix: &[u8],
    found: &mut Vec<(Vec<u8>, PathBuf, bool)>,
) -> Result<(), Refusal> {
    for entry in std::fs::read_dir(dir).map_err(io_refusal)? {
        let entry = entry.map_err(io_refusal)?;
        let kind = entry.file_type().map_err(io_refusal)?;
        let mut relative = prefix.to_vec();
        if !relative.is_empty() {
            relative.push(b'/');
        }
        relative.extend_from_slice(entry.file_name().as_encoded_bytes());
        if kind.is_dir() {
            found.push((relative.clone(), entry.path(), true));
            collect(&entry.path(), &relative, found)?;
        } else if kind.is_file() {
            found.push((relative, entry.path(), false));
        } else {
            return Err(Refusal::Io(io::ErrorKind::InvalidData));
        }
    }
    Ok(())
}

fn feed(hasher: &mut Sha256, path: &Path) -> Result<(), Refusal> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(io_refusal)?;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io_refusal)?;
        if read == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..read]);
    }
}

/// A staging directory, removed when dropped. After a commit the tree has moved out of it and
/// what is removed is the empty holder. Directories that were made to hold it and are empty again
/// are removed too, so a refused or declined install leaves the state directory as it was. Errors
/// are ignored: a leftover staging directory holds a copy of a plugin and nothing else.
#[derive(Debug)]
struct Holder {
    dir: PathBuf,
    /// The state directory and each parent of it that this staging made, innermost first.
    made: Vec<PathBuf>,
}

impl Drop for Holder {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
        remove_made(&self.made);
    }
}

/// Removes each directory of `made` that is empty, innermost first. A directory holding anything
/// is kept, since the remove fails.
fn remove_made(made: &[PathBuf]) {
    for directory in made {
        std::fs::remove_dir(directory).ok();
    }
}

/// A fresh, empty staging directory directly under the state directory `home`, outside every
/// workspace. It is beside `plugins/`, not in it, so a refused install leaves `plugins/` as it was.
fn staging_dir(home: &Path) -> Result<Holder, Refusal> {
    let made: Vec<PathBuf> = home
        .ancestors()
        .take_while(|directory| !is_present(directory))
        .map(Path::to_path_buf)
        .collect();
    std::fs::create_dir_all(home).map_err(io_refusal)?;
    for attempt in 0_u32..64 {
        let dir = home.join(format!(
            ".plugin-staging-{}-{attempt}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.subsec_nanos()),
        ));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(Holder { dir, made }),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                remove_made(&made);
                return Err(io_refusal(error));
            }
        }
    }
    remove_made(&made);
    Err(Refusal::Io(io::ErrorKind::AlreadyExists))
}

/// Reads the plugin at `source_dir` in place, checks it and copies it to a staging directory.
///
/// The order is the reader, the link check, then the copy, so a refused manifest copies nothing
/// and creates nothing. Nothing in the plugin is executed. The only thing made under the state
/// directory `home` is the staging directory, which is removed when the result is dropped. Links
/// are never followed or copied; they are listed in [`Staged::skipped`].
pub fn stage(source_dir: &Path, home: &Path) -> Result<Staged, Refusal> {
    let source =
        std::fs::canonicalize(source_dir).map_err(|error| Refusal::Source(error.kind()))?;
    if source
        .to_str()
        .is_none_or(|text| text.chars().any(char::is_control))
    {
        return Err(Refusal::SourceNotText);
    }
    let manifest = plugin::read(&source).map_err(Refusal::Manifest)?;
    plugin::check_links(&source, &manifest.named_parts()).map_err(Refusal::Manifest)?;
    for (part, path) in manifest.named_parts() {
        if part.is_installed() && std::fs::symlink_metadata(path.under(&source)).is_err() {
            return Err(Refusal::PartMissing(part));
        }
    }
    let holder = staging_dir(home)?;
    let tree = holder.dir.join("tree");
    let skipped = copy_tree(&source, &tree)?;
    let digest = digest_tree(&tree)?;
    Ok(Staged {
        _holder: holder,
        tree,
        source,
        manifest,
        digest,
        skipped,
    })
}

/// `<state>/plugins`, where `state` is the state directory itself (`~/.bravebot`, or what
/// `BRAVEBOT_HOME` names), the same directory skill and definition discovery is given as `home`.
pub fn plugins_dir(home: &Path) -> PathBuf {
    home.join(PLUGINS)
}

/// The directory an installed plugin is copied to.
pub fn installed_dir(home: &Path, name: &str) -> PathBuf {
    plugins_dir(home).join(name)
}

fn record_path(home: &Path, name: &str) -> PathBuf {
    plugins_dir(home).join(format!("{name}{RECORD_SUFFIX}"))
}

/// The record's text: four `key = value` lines. The source and the version hold no control
/// character (stage and the manifest reader refuse one), so a value is one line and is written as
/// it is.
fn record_text(source: &Path, version: &str, enabled: bool, digest: &str) -> String {
    format!(
        "source = {}\nversion = {version}\nenabled = {enabled}\ndigest = {digest}\n",
        source.display()
    )
}

/// Reads the record beside an installed plugin. `None` when there is no record or it does not say
/// all three things, so a damaged record is never read as a plugin that is enabled.
pub fn read_record(home: &Path, name: &str) -> Option<Installed> {
    read_record_in(&plugins_dir(home), name)
}

fn read_record_in(plugins: &Path, name: &str) -> Option<Installed> {
    let text = std::fs::read_to_string(plugins.join(format!("{name}{RECORD_SUFFIX}"))).ok()?;
    let (mut source, mut version, mut enabled, mut digest) = (None, None, None, None);
    for line in text.lines() {
        let (key, value) = line.split_once(" = ")?;
        match key {
            "source" => source = Some(PathBuf::from(value)),
            "version" => version = Some(value.to_string()),
            "enabled" => {
                enabled = match value {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => return None,
                }
            }
            "digest" => digest = Some(value.to_string()),
            _ => return None,
        }
    }
    Some(Installed {
        name: name.to_string(),
        source: source?,
        version: version?,
        enabled: enabled?,
        digest: digest?,
    })
}

fn is_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Installs a staged plugin at `<state>/plugins/<name>/` with a record beside it.
///
/// Refused, with nothing created or changed, when the name is already installed: a different
/// source is named with the installed one, and the same source is refused as an update, which is
/// not built (decision 1). The name is checked before the plugins directory is created.
#[expect(
    clippy::needless_pass_by_value,
    reason = "consumes the staged copy, so its staging directory is gone once the install ends"
)]
pub fn commit_install(staged: Staged, home: &Path) -> Result<(), Refusal> {
    let name = staged.manifest.name.clone();
    let target = installed_dir(home, &name);
    let record = record_path(home, &name);
    if is_present(&target) || is_present(&record) {
        let installed = read_record(home, &name).map(|read| read.source);
        return Err(match installed {
            Some(source) if source == staged.source => Refusal::UpdateNotBuilt { name, source },
            installed => Refusal::AlreadyInstalled {
                name,
                installed,
                new: staged.source.clone(),
            },
        });
    }
    if let Some((offered, existing_plugin)) = conflicts(&staged_offers(&staged), home) {
        return Err(Refusal::NameOffered {
            offered,
            new_plugin: name,
            existing_plugin,
        });
    }
    std::fs::create_dir_all(plugins_dir(home)).map_err(io_refusal)?;
    // A name holds no dot, so this is never another plugin's directory.
    let partial = plugins_dir(home).join(format!(".{name}.installing"));
    std::fs::remove_dir_all(&partial).ok();
    let moved = std::fs::rename(&staged.tree, &partial).is_ok()
        || copy_tree(&staged.tree, &partial).is_ok();
    if !moved {
        std::fs::remove_dir_all(&partial).ok();
        return Err(Refusal::Io(io::ErrorKind::Other));
    }
    std::fs::write(
        &record,
        record_text(
            &staged.source,
            &staged.manifest.version,
            true,
            &staged.digest,
        ),
    )
    .map_err(|error| {
        std::fs::remove_dir_all(&partial).ok();
        io_refusal(error)
    })?;
    std::fs::rename(&partial, &target).map_err(|error| {
        std::fs::remove_file(&record).ok();
        std::fs::remove_dir_all(&partial).ok();
        io_refusal(error)
    })
}

/// Stages the plugin at `source_dir` and installs it under `home` in one call. A refusal at any
/// step leaves `<state>/plugins` as it was. The caller that asks a person (PLUGIN-4) uses
/// `stage` and `commit_install` separately and puts the listing between them.
pub fn install(source_dir: &Path, home: &Path) -> Result<(), Refusal> {
    commit_install(stage(source_dir, home)?, home)
}

/// Whoever answers the one question an install puts (PLUGIN-4).
///
/// No mode, rule or setting answers it: an implementation puts it to a person or answers no.
pub trait Asker {
    /// Whether anybody is there to answer. Where nobody is, nothing is staged and nothing is asked.
    fn anybody_there(&self) -> bool;

    /// Put the listing of `staged` to the person. Only a true answer installs.
    fn confirm_install(&mut self, staged: &Staged) -> bool;
}

/// The asker of a run with nobody behind it, such as a one-shot run.
pub struct Nobody;

impl Asker for Nobody {
    fn anybody_there(&self) -> bool {
        false
    }

    fn confirm_install(&mut self, _staged: &Staged) -> bool {
        false
    }
}

/// Install the plugin at `source_dir` under `home` after `asker` says yes.
///
/// Where nobody is there, returns [`Refusal::NobodyToAsk`] before the source is read. A no is
/// [`Refusal::Declined`], and nothing is copied or recorded. The question is put once, after the
/// plugin is staged and before anything is installed.
pub fn install_asked<A: Asker + ?Sized>(
    source_dir: &Path,
    home: &Path,
    asker: &mut A,
) -> Result<String, Refusal> {
    if !asker.anybody_there() {
        return Err(Refusal::NobodyToAsk);
    }
    let staged = stage(source_dir, home)?;
    if !asker.confirm_install(&staged) {
        return Err(Refusal::Declined);
    }
    let name = staged.manifest.name.clone();
    commit_install(staged, home)?;
    Ok(name)
}

/// What the record beside an installed plugin says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub source: PathBuf,
    pub version: String,
    pub enabled: bool,
    pub digest: String,
}

/// Why list, remove or enable and disable did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManageRefusal {
    /// The name is not letters, digits and hyphens, so it is never joined to a path.
    BadName,
    /// No plugin of that name is installed.
    NotInstalled(String),
    Io(io::ErrorKind),
}

fn checked_name(name: &str) -> Result<(), ManageRefusal> {
    if plugin::is_valid_name(name) {
        Ok(())
    } else {
        Err(ManageRefusal::BadName)
    }
}

/// Writes the record beside an installed plugin through a temporary file and a rename, so a
/// reader sees the old record or the new one and never half of one.
fn write_record(home: &Path, installed: &Installed) -> Result<(), ManageRefusal> {
    let temporary = plugins_dir(home).join(format!(".{}.record.tmp", installed.name));
    let text = record_text(
        &installed.source,
        &installed.version,
        installed.enabled,
        &installed.digest,
    );
    let write = std::fs::write(&temporary, text)
        .and_then(|()| std::fs::rename(&temporary, record_path(home, &installed.name)));
    write.map_err(|error| {
        std::fs::remove_file(&temporary).ok();
        ManageRefusal::Io(error.kind())
    })
}

/// The installed plugins in name order, from their records alone (PLUGIN-9). No manifest, skill
/// or definition is opened and nothing is run. A record that cannot be read is not listed.
pub fn list(home: &Path) -> Vec<Installed> {
    list_in(&plugins_dir(home))
}

fn list_in(plugins: &Path) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(plugins) else {
        return Vec::new();
    };
    let mut found: Vec<Installed> = entries
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().into_string().ok()?;
            let name = file.strip_suffix(RECORD_SUFFIX)?;
            if !plugin::is_valid_name(name) {
                return None;
            }
            read_record_in(plugins, name)
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Deletes the plugin's directory and its record (PLUGIN-9). The directory goes first, so a
/// failure leaves the record and the removal can be run again.
pub fn remove(home: &Path, name: &str) -> Result<(), ManageRefusal> {
    checked_name(name)?;
    let directory = installed_dir(home, name);
    let record = record_path(home, name);
    if !is_present(&directory) && !is_present(&record) {
        return Err(ManageRefusal::NotInstalled(name.to_string()));
    }
    if is_present(&directory) {
        std::fs::remove_dir_all(&directory).map_err(|error| ManageRefusal::Io(error.kind()))?;
    }
    if is_present(&record) {
        std::fs::remove_file(&record).map_err(|error| ManageRefusal::Io(error.kind()))?;
    }
    Ok(())
}

/// Sets the recorded enabled state (PLUGIN-11). The directory and the rest of the record stay.
/// Nothing is asked for either direction: enabling gives a plugin nothing a person did not see
/// when it was installed, since what it offers is what was listed then.
pub fn set_enabled(home: &Path, name: &str, enabled: bool) -> Result<(), ManageRefusal> {
    checked_name(name)?;
    let Some(mut installed) = read_record(home, name) else {
        return Err(ManageRefusal::NotInstalled(name.to_string()));
    };
    installed.enabled = enabled;
    write_record(home, &installed)
}

/// What one plugin offers by name. Skill text and definition text are read only to find these
/// fields; the driver carries them to a listing and compares names, it decides nothing else from
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offered {
    Skill { description: String },
    Definition { kind: bravebot_core::delegate::Kind },
}

impl Offered {
    fn is_skill(&self) -> bool {
        matches!(self, Self::Skill { .. })
    }
}

/// A skill or a definition a plugin offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The plugin that offers it.
    pub plugin: String,
    pub name: String,
    pub what: Offered,
}

/// The skills and definitions in the parts of the plugin whose tree is at `root`, found by the
/// readers a session uses. A file those readers do not take as a skill or definition offers
/// nothing, as it offers nothing in a session. Only the two part directories the manifest names
/// are opened.
fn offers_in(root: &Path, manifest: &Manifest) -> Vec<Offer> {
    let mut found = Vec::new();
    if let Some(path) = manifest.part(Part::Skills) {
        let skills = path.under(root);
        for directory in crate::skills::skill_directories(&skills) {
            let file = skills.join(&directory).join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if let Some(front) = crate::skills::parse_frontmatter(&text) {
                found.push(Offer {
                    plugin: manifest.name.clone(),
                    name: front.name,
                    what: Offered::Skill {
                        description: front.description,
                    },
                });
            }
        }
    }
    if let Some(path) = manifest.part(Part::Agents) {
        let (files, _) = crate::agents::markdown_files(&path.under(root));
        for file in files {
            let Ok(text) = std::fs::read_to_string(path.under(root).join(&file)) else {
                continue;
            };
            if let Some((name, kind)) = crate::agents::declared_name_and_kind(&text, &file) {
                found.push(Offer {
                    plugin: manifest.name.clone(),
                    name,
                    what: Offered::Definition { kind },
                });
            }
        }
    }
    found
}

/// The part directories of one enabled installed plugin, for the session's skill and definition
/// discovery. Only paths: nothing is read here beyond the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    pub plugin: String,
    pub skills: Option<PathBuf>,
    pub agents: Option<PathBuf>,
}

/// The part directories of every enabled plugin under `state`, the state directory itself
/// (`~/.bravebot`, what `home::directory` returns), in plugin name order. A disabled plugin has
/// none (PLUGIN-11). The only directory read for a plugin is `<state>/plugins/<name>`, so a plugin
/// directory in a workspace offers nothing (PLUGIN-5): the caller passes the state directory,
/// never a workspace.
pub fn enabled_roots(state: &Path) -> Vec<Roots> {
    let plugins = state.join(PLUGINS);
    list_in(&plugins)
        .into_iter()
        .filter(|installed| installed.enabled)
        .filter_map(|installed| {
            let directory = plugins.join(&installed.name);
            let manifest = plugin::read(&directory).ok()?;
            Some(Roots {
                plugin: installed.name,
                skills: manifest
                    .part(Part::Skills)
                    .map(|path| path.under(&directory)),
                agents: manifest
                    .part(Part::Agents)
                    .map(|path| path.under(&directory)),
            })
        })
        .collect()
}

/// What the plugin staged for install would offer, for the install prompt and for `conflicts`.
pub fn staged_offers(staged: &Staged) -> Vec<Offer> {
    offers_in(&staged.tree, &staged.manifest)
}

/// What every ENABLED installed plugin offers, in plugin name order (PLUGIN-11). A disabled
/// plugin offers nothing. The manifest is read from the installed copy only to find the two part
/// directories.
pub fn offered(home: &Path) -> Vec<Offer> {
    let mut found = Vec::new();
    for installed in list(home).into_iter().filter(|plugin| plugin.enabled) {
        if let Ok(manifest) = plugin::read(&installed_dir(home, &installed.name)) {
            found.extend(offers_in(&installed_dir(home, &installed.name), &manifest));
        }
    }
    found
}

/// A staged offer and the source that already offers the same name above it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadowed {
    pub offer: Offer,
    /// The origin of the skill or definition that takes the name instead, as the lists show it
    /// (`built-in`, `~/.bravebot/skills/...`, a project path). `None` where nothing above the
    /// plugin offers the name, so the plugin's is the one a session gets.
    pub by: Option<String>,
}

/// For each skill and definition the staged plugin offers, whether a source above a plugin already
/// offers the name (PLUGIN-12), for the install prompt. `catalogue` and `definitions` are what
/// `skills::discover` and `agents::discover` found for the session, so the answer is the one the
/// session will give: the built-in, the shared directories, the user's own and the project's all
/// shadow a plugin, and a skill and a definition of one name do not shadow each other. An entry
/// that came from an installed plugin is not a shadow here; `conflicts` refuses that case. Only
/// names are compared.
pub fn shadow_marks(
    offers: &[Offer],
    catalogue: &crate::skills::Catalogue,
    definitions: &bravebot_core::delegate::Definitions,
) -> Vec<Shadowed> {
    offers
        .iter()
        .map(|offer| {
            let by = match offer.what {
                Offered::Skill { .. } => catalogue
                    .get(&offer.name)
                    .filter(|skill| skill.source != crate::skills::Source::Plugin)
                    .map(|skill| skill.origin.clone()),
                Offered::Definition { .. } => definitions
                    .get(&offer.name)
                    .map(|definition| definition.origin().to_string())
                    .filter(|origin| !origin.starts_with("~/.bravebot/plugins/")),
            };
            Shadowed {
                offer: offer.clone(),
                by,
            }
        })
        .collect()
}

/// The first skill or definition among `staged_offers` that another installed plugin, enabled or
/// not, already offers, with that plugin's name (PLUGIN-12). A skill and a definition of one name
/// do not conflict. No prefix is added to either name.
pub fn conflicts(staged_offers: &[Offer], home: &Path) -> Option<(String, String)> {
    for installed in list(home) {
        let directory = installed_dir(home, &installed.name);
        let Ok(manifest) = plugin::read(&directory) else {
            continue;
        };
        let existing = offers_in(&directory, &manifest);
        for offer in staged_offers {
            let same = existing.iter().any(|have| {
                have.name == offer.name && have.what.is_skill() == offer.what.is_skill()
            });
            if same {
                return Some((offer.name.clone(), installed.name.clone()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const MANIFEST: &str =
        "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nskills = \"skills\"\n";

    fn plugin_dir(parent: &Path, name: &str, manifest: &str) -> PathBuf {
        let root = parent.join(name);
        std::fs::create_dir_all(root.join("skills").join("hello")).unwrap();
        std::fs::write(root.join(plugin::FILE), manifest).unwrap();
        std::fs::write(
            root.join("skills").join("hello").join("SKILL.md"),
            "hello\n",
        )
        .unwrap();
        root
    }

    /// Every path under `dir` with its bytes, so a test can show a tree is unchanged.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        fn walk(dir: &Path, found: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries {
                let entry = entry.unwrap();
                let kind = entry.file_type().unwrap();
                if kind.is_dir() {
                    found.insert(entry.path(), None);
                    walk(&entry.path(), found);
                } else {
                    found.insert(entry.path(), std::fs::read(entry.path()).ok());
                }
            }
        }
        let mut found = BTreeMap::new();
        walk(dir, &mut found);
        found
    }

    #[test]
    fn a_staged_plugin_is_a_copy_with_the_manifest_held_and_a_stable_digest() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        let first = stage(&source, &home).unwrap();
        let second = stage(&source, &home).unwrap();
        assert_eq!(first.manifest.name, "demo");
        assert_eq!(first.source, std::fs::canonicalize(&source).unwrap());
        assert_eq!(first.digest.len(), 64);
        assert!(first.digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(first.digest, second.digest);
        assert_eq!(
            std::fs::read(first.tree.join("skills").join("hello").join("SKILL.md")).unwrap(),
            b"hello\n"
        );
        // The source is read in place and left alone.
        assert!(source.join(plugin::FILE).is_file());
        std::fs::write(
            source.join("skills").join("hello").join("SKILL.md"),
            "changed\n",
        )
        .unwrap();
        assert_ne!(stage(&source, &home).unwrap().digest, first.digest);
    }

    #[test]
    fn the_staging_copy_is_removed_when_the_staged_value_is_dropped() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        let staged = stage(&source, &home).unwrap();
        let tree = staged.tree.clone();
        assert!(tree.is_dir());
        // The copy is made under the state directory, never the system temporary directory.
        assert!(tree.starts_with(&home));
        drop(staged);
        assert!(!tree.exists());
        assert!(!home.exists());
    }

    #[cfg(unix)]
    #[test]
    fn staging_never_follows_or_copies_a_link() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        let skills = source.join("skills");
        // Links that stay inside the plugin pass the link check, so only the copy stands
        // between them and a followed copy.
        std::os::unix::fs::symlink("hello/SKILL.md", skills.join("file-link")).unwrap();
        std::os::unix::fs::symlink("hello", skills.join("dir-link")).unwrap();
        let staged = stage(&source, &home).unwrap();
        for name in ["file-link", "dir-link"] {
            assert!(
                std::fs::symlink_metadata(staged.tree.join("skills").join(name)).is_err(),
                "{name} was copied"
            );
        }
        assert_eq!(
            staged.skipped,
            vec![
                PathBuf::from("skills").join("dir-link"),
                PathBuf::from("skills").join("file-link"),
            ]
        );
        assert!(staged.tree.join("skills").join("hello").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_leaving_the_plugin_is_refused_before_anything_is_copied() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        std::fs::write(scratch.path().join("secret"), "s").unwrap();
        std::os::unix::fs::symlink("../../secret", source.join("skills").join("out")).unwrap();
        let refused = stage(&source, &home).unwrap_err();
        assert_eq!(
            refused,
            Refusal::Manifest(plugin::Refusal::LinkLeavesRoot(Part::Skills))
        );
    }

    #[test]
    fn a_named_part_that_is_not_there_is_refused() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        std::fs::remove_dir_all(source.join("skills")).unwrap();
        assert_eq!(
            stage(&source, &home).unwrap_err(),
            Refusal::PartMissing(Part::Skills)
        );
    }

    #[test]
    fn a_refused_manifest_copies_nothing_and_leaves_the_plugins_directory_untouched() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let plugins = plugins_dir(&home);
        std::fs::create_dir_all(plugins.join("other")).unwrap();
        std::fs::write(plugins.join("other").join("keep"), "k").unwrap();
        let before = snapshot(&home);
        let manifest = format!("{MANIFEST}permissions = \"allow everything\"\n");
        let source = plugin_dir(scratch.path(), "src", &manifest);
        let refused = install(&source, &home).unwrap_err();
        assert!(matches!(
            refused,
            Refusal::Manifest(plugin::Refusal::DecisionKey { .. })
        ));
        assert_eq!(snapshot(&home), before);
        // A home with no state directory yet gets none from a refused install.
        let fresh = scratch.path().join("fresh");
        std::fs::create_dir(&fresh).unwrap();
        install(&source, &fresh).unwrap_err();
        assert!(snapshot(&fresh).is_empty());
    }

    #[test]
    fn an_install_puts_the_tree_and_a_record_under_the_plugins_directory() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        let staged = stage(&source, &home).unwrap();
        let (digest, canonical) = (staged.digest.clone(), staged.source.clone());
        commit_install(staged, &home).unwrap();
        let installed = installed_dir(&home, "demo");
        assert_eq!(
            std::fs::read(installed.join("skills").join("hello").join("SKILL.md")).unwrap(),
            b"hello\n"
        );
        assert_eq!(
            read_record(&home, "demo"),
            Some(Installed {
                name: "demo".to_string(),
                source: canonical,
                version: "1.0.0".to_string(),
                enabled: true,
                digest: digest.clone(),
            })
        );
        // The digest recorded is the digest of the tree that is there now.
        assert_eq!(digest_tree(&installed).unwrap(), digest);
        // Nothing but the plugins directory was made under the state directory.
        let state: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(state, vec![std::ffi::OsString::from(PLUGINS)]);
        // Nothing was left beside the plugin or in staging.
        let mut names: Vec<_> = std::fs::read_dir(plugins_dir(&home))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, vec!["demo".to_string(), "demo.record".to_string()]);
    }

    /// PLUGIN-1: an install changes no permission rule, no trust answer and no mode. The state
    /// directory holds the settings file (the rules), the `trusted` records (the trust map) and the
    /// other files a person's choices live in, so a snapshot of all of it before and after shows
    /// that every file there is byte for byte as it was and the only new paths are the plugin's
    /// directory and its record. Rejects an install that also writes a rule, a trust record or
    /// any other file beside the plugins directory.
    #[test]
    fn an_install_leaves_the_rules_the_trust_map_and_the_state_beside_it_untouched() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        std::fs::create_dir_all(home.join("trusted")).unwrap();
        std::fs::write(
            home.join("settings.json"),
            r#"{"permissions": {"deny": ["run(rm)"]}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join("trusted").join("project.jsonl"),
            "{\"trusted\":1}\n",
        )
        .unwrap();
        std::fs::write(home.join("effort"), "high\n").unwrap();
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        let before = snapshot(&home);

        install(&source, &home).unwrap();

        let after = snapshot(&home);
        for (path, bytes) in &before {
            assert_eq!(after.get(path), Some(bytes), "{path:?} changed");
        }
        let plugins = plugins_dir(&home);
        let installed = installed_dir(&home, "demo");
        let record = record_path(&home, "demo");
        for path in after.keys().filter(|path| !before.contains_key(*path)) {
            assert!(
                *path == plugins || *path == record || path.starts_with(&installed),
                "an install wrote {path:?}"
            );
        }
        assert!(after.contains_key(&record) && after.contains_key(&installed));
    }

    #[test]
    fn a_name_installed_from_another_source_is_refused_naming_both_and_changes_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let first = plugin_dir(scratch.path(), "one", MANIFEST);
        let second = plugin_dir(scratch.path(), "two", MANIFEST);
        std::fs::write(
            second.join("skills").join("hello").join("SKILL.md"),
            "other\n",
        )
        .unwrap();
        commit_install(stage(&first, &home).unwrap(), &home).unwrap();
        let before = snapshot(&home);
        let staged = stage(&second, &home).unwrap();
        let new = staged.source.clone();
        let refused = commit_install(staged, &home).unwrap_err();
        assert_eq!(
            refused,
            Refusal::AlreadyInstalled {
                name: "demo".to_string(),
                installed: Some(std::fs::canonicalize(&first).unwrap()),
                new,
            }
        );
        assert_eq!(snapshot(&home), before);
    }

    #[test]
    fn adding_the_same_source_again_is_refused_because_update_is_not_built() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_dir(scratch.path(), "src", MANIFEST);
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        std::fs::write(
            source.join("skills").join("hello").join("SKILL.md"),
            "newer\n",
        )
        .unwrap();
        let before = snapshot(&home);
        let refused = commit_install(stage(&source, &home).unwrap(), &home).unwrap_err();
        assert_eq!(
            refused,
            Refusal::UpdateNotBuilt {
                name: "demo".to_string(),
                source: std::fs::canonicalize(&source).unwrap(),
            }
        );
        assert_eq!(snapshot(&home), before);
    }

    #[test]
    fn a_record_that_does_not_say_all_four_things_reads_as_no_record() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path();
        std::fs::create_dir_all(plugins_dir(home)).unwrap();
        for text in [
            "source = /a\nversion = 1\nenabled = true\n",
            "source = /a\nenabled = true\ndigest = d\n",
            "source = /a\nversion = 1\nenabled = yes\ndigest = d\n",
            "source = /a\nversion = 1\nenabled = true\ndigest = d\nextra = 1\n",
        ] {
            std::fs::write(record_path(home, "demo"), text).unwrap();
            assert_eq!(read_record(home, "demo"), None, "{text}");
        }
    }

    /// A plugin called `name` offering the skill `skill` and, when given, the definition `agent`.
    fn offering(parent: &Path, name: &str, skill: &str, agent: Option<&str>) -> PathBuf {
        let root = parent.join(format!("src-{name}"));
        std::fs::create_dir_all(root.join("skills").join(skill)).unwrap();
        std::fs::write(
            root.join(plugin::FILE),
            format!(
                "name = \"{name}\"\nversion = \"1.0.0\"\ndescription = \"d\"\nskills = \"skills\"\nagents = \"agents\"\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("skills").join(skill).join("SKILL.md"),
            format!("---\nname: {skill}\ndescription: Does {skill}.\n---\n\nBody.\n"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("agents")).unwrap();
        if let Some(agent) = agent {
            std::fs::write(
                root.join("agents").join(format!("{agent}.md")),
                format!("---\nname: {agent}\ndescription: Helps.\nkind: checker\n---\n\nBody.\n"),
            )
            .unwrap();
        }
        root
    }

    #[test]
    fn list_shows_what_was_installed_from_the_records_alone() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        assert!(list(&home).is_empty());
        let one = offering(scratch.path(), "beta", "review", None);
        let two = offering(scratch.path(), "alpha", "lint", None);
        let staged = stage(&one, &home).unwrap();
        let digest = staged.digest.clone();
        commit_install(staged, &home).unwrap();
        install(&two, &home).unwrap();
        // The listing reads records: with every plugin's tree gone it still lists both.
        std::fs::remove_dir_all(installed_dir(&home, "alpha")).unwrap();
        std::fs::remove_dir_all(installed_dir(&home, "beta")).unwrap();
        let listed = list(&home);
        assert_eq!(
            listed.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
        assert_eq!(listed[1].source, std::fs::canonicalize(&one).unwrap());
        assert_eq!(listed[1].digest, digest);
        assert!(listed.iter().all(|p| p.enabled));
    }

    #[test]
    fn remove_deletes_the_directory_and_the_record_and_nothing_else() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        install(&offering(scratch.path(), "one", "a", None), &home).unwrap();
        install(&offering(scratch.path(), "two", "b", None), &home).unwrap();
        remove(&home, "one").unwrap();
        assert!(!installed_dir(&home, "one").exists());
        assert!(!record_path(&home, "one").exists());
        assert!(installed_dir(&home, "two").is_dir());
        assert_eq!(list(&home).len(), 1);
        assert_eq!(
            remove(&home, "one"),
            Err(ManageRefusal::NotInstalled("one".to_string()))
        );
    }

    #[test]
    fn disable_then_enable_rewrites_only_the_recorded_state() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        install(&offering(scratch.path(), "demo", "a", None), &home).unwrap();
        let before = read_record(&home, "demo").unwrap();
        let tree = snapshot(&installed_dir(&home, "demo"));
        set_enabled(&home, "demo", false).unwrap();
        let disabled = read_record(&home, "demo").unwrap();
        assert!(!disabled.enabled);
        assert_eq!(
            (&disabled.source, &disabled.digest),
            (&before.source, &before.digest)
        );
        assert_eq!(list(&home), vec![disabled]);
        assert_eq!(snapshot(&installed_dir(&home, "demo")), tree);
        set_enabled(&home, "demo", true).unwrap();
        assert_eq!(read_record(&home, "demo"), Some(before));
        // No temporary file is left beside the record.
        let names: Vec<_> = std::fs::read_dir(plugins_dir(&home))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert_eq!(
            set_enabled(&home, "absent", true),
            Err(ManageRefusal::NotInstalled("absent".to_string()))
        );
    }

    #[test]
    fn a_name_that_could_leave_the_plugins_directory_is_refused_before_a_path_is_made() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        install(&offering(scratch.path(), "demo", "a", None), &home).unwrap();
        // Something a `..` name would reach: a record and a directory beside the plugins directory.
        let outside = home.join("x");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            home.join("x.record"),
            "source = /s\nversion = 1\nenabled = true\ndigest = d\n",
        )
        .unwrap();
        let before = snapshot(&home);
        for name in ["../x", "..", "a/b", "", "x.y", "demo/../demo"] {
            assert_eq!(remove(&home, name), Err(ManageRefusal::BadName), "{name}");
            assert_eq!(
                set_enabled(&home, name, false),
                Err(ManageRefusal::BadName),
                "{name}"
            );
        }
        assert_eq!(snapshot(&home), before);
    }

    #[test]
    fn a_disabled_plugin_is_listed_and_offers_nothing_until_enabled() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        install(
            &offering(scratch.path(), "demo", "review", Some("helper")),
            &home,
        )
        .unwrap();
        let names =
            |offers: &[Offer]| -> Vec<String> { offers.iter().map(|o| o.name.clone()).collect() };
        assert_eq!(names(&offered(&home)), vec!["review", "helper"]);
        set_enabled(&home, "demo", false).unwrap();
        assert_eq!(list(&home).len(), 1);
        assert!(offered(&home).is_empty());
        set_enabled(&home, "demo", true).unwrap();
        let offers = offered(&home);
        assert_eq!(names(&offers), vec!["review", "helper"]);
        assert_eq!(
            offers[0].what,
            Offered::Skill {
                description: "Does review.".to_string()
            }
        );
        assert_eq!(
            offers[1].what,
            Offered::Definition {
                kind: bravebot_core::delegate::Kind::Checker
            }
        );
        assert_eq!(offers[0].plugin, "demo");
    }

    #[test]
    fn a_second_plugin_offering_an_offered_name_is_refused_naming_both_and_changes_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        install(&offering(scratch.path(), "first", "review", None), &home).unwrap();
        // The refusal holds while the first plugin is disabled too.
        set_enabled(&home, "first", false).unwrap();
        let before = snapshot(&home);
        let second = offering(scratch.path(), "second", "review", None);
        let refused = install(&second, &home).unwrap_err();
        assert_eq!(
            refused,
            Refusal::NameOffered {
                offered: "review".to_string(),
                new_plugin: "second".to_string(),
                existing_plugin: "first".to_string(),
            }
        );
        assert_eq!(snapshot(&home), before);
        let held: Vec<_> = std::fs::read_dir(plugins_dir(&home))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            held.iter()
                .all(|name| name.starts_with("first") || name.starts_with(".first"))
        );
        // A name offered by neither is not refused, and a definition named like a skill is not
        // the same offer.
        install(
            &offering(scratch.path(), "third", "other", Some("review")),
            &home,
        )
        .unwrap();
    }

    #[test]
    fn two_plugins_offering_one_definition_name_conflict() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        install(
            &offering(scratch.path(), "first", "a", Some("helper")),
            &home,
        )
        .unwrap();
        let refused = install(
            &offering(scratch.path(), "second", "b", Some("helper")),
            &home,
        );
        assert!(matches!(
            refused,
            Err(Refusal::NameOffered { ref offered, .. }) if offered == "helper"
        ));
    }

    /// An asker that answers as told and counts how often it was asked.
    struct Answering {
        there: bool,
        yes: bool,
        asked: usize,
    }

    impl Asker for Answering {
        fn anybody_there(&self) -> bool {
            self.there
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            self.asked += 1;
            self.yes
        }
    }

    /// PLUGIN-4: where nobody can be asked nothing is installed, nobody is asked, and not even the
    /// state directory is made.
    ///
    /// Rejects: an install that goes ahead, or stages, when `anybody_there` is false.
    #[test]
    fn where_nobody_can_be_asked_nothing_is_installed() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = offering(scratch.path(), "demo", "a", None);
        let mut asker = Answering {
            there: false,
            yes: true,
            asked: 0,
        };
        assert_eq!(
            install_asked(&source, &home, &mut asker),
            Err(Refusal::NobodyToAsk)
        );
        assert_eq!(asker.asked, 0);
        assert!(!home.exists());
        assert_eq!(
            install_asked(&source, &home, &mut Nobody),
            Err(Refusal::NobodyToAsk)
        );
        assert!(!home.exists());
    }

    /// A no installs nothing, and a yes installs once after one question.
    ///
    /// Rejects: an install that ignores the answer, and a question put twice.
    #[test]
    fn only_a_yes_installs_and_the_question_is_put_once() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = offering(scratch.path(), "demo", "a", None);
        let mut no = Answering {
            there: true,
            yes: false,
            asked: 0,
        };
        assert_eq!(
            install_asked(&source, &home, &mut no),
            Err(Refusal::Declined)
        );
        assert_eq!(no.asked, 1);
        assert!(!home.exists());
        let mut yes = Answering {
            there: true,
            yes: true,
            asked: 0,
        };
        assert_eq!(
            install_asked(&source, &home, &mut yes),
            Ok("demo".to_string())
        );
        assert_eq!(yes.asked, 1);
        assert!(read_record(&home, "demo").is_some());
    }

    /// PATH-003: a source directory whose name is not UTF-8 is refused as `SourceNotText`, before
    /// the manifest is read and before anything is created. Its lossy lookalike, `src-\u{FFFD}`,
    /// is a different directory with the same plugin in it and stages under its own path. Both
    /// plugins are valid, so the refusal can only come from the name. Linux only: a macOS file
    /// system cannot hold the first name.
    ///
    /// Rejects: a `stage` that renders the source lossily and so stages the first directory as
    /// the second, and one that refuses the lookalike as well.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_source_whose_name_is_not_text_is_refused_and_its_lossy_lookalike_is_staged() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let invalid = scratch.path().join(OsStr::from_bytes(b"src-\xff"));
        let lookalike = scratch.path().join(OsStr::new("src-\u{FFFD}"));
        std::fs::rename(plugin_dir(scratch.path(), "plain", MANIFEST), &invalid).unwrap();
        std::fs::rename(plugin_dir(scratch.path(), "other", MANIFEST), &lookalike).unwrap();

        assert_eq!(stage(&invalid, &home).unwrap_err(), Refusal::SourceNotText);
        assert!(
            !home.exists(),
            "a refused source created the state directory"
        );

        let staged = stage(&lookalike, &home).unwrap();
        assert_eq!(staged.source, std::fs::canonicalize(&lookalike).unwrap());
        assert_eq!(staged.manifest.name, "demo");
    }

    /// PATH-003: two plugin trees that differ only in a file named `tool-\xff` and one named
    /// `tool-\u{FFFD}`, with the same content, stage to different digests, and each staged copy
    /// holds the name it was given and not the other. A third tree with the first name again
    /// stages to the first digest, so the name is all that tells them apart. Linux only: a macOS
    /// file system cannot hold the first name.
    ///
    /// Rejects: a copy or a digest that renders a file name lossily, which makes the two trees
    /// one tree, so that an install of one is taken for the other.
    #[cfg(target_os = "linux")]
    #[test]
    fn trees_differing_only_in_a_name_that_is_not_text_and_its_lossy_lookalike_have_two_digests() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let invalid = OsStr::from_bytes(b"tool-\xff");
        let lookalike = OsStr::new("tool-\u{FFFD}");
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let with_tool = |directory: &str, name: &OsStr| {
            let root = plugin_dir(scratch.path(), directory, MANIFEST);
            std::fs::write(root.join(name), "tool\n").unwrap();
            root
        };
        let first = stage(&with_tool("one", invalid), &home).unwrap();
        let second = stage(&with_tool("two", lookalike), &home).unwrap();
        let again = stage(&with_tool("three", invalid), &home).unwrap();

        assert_ne!(first.digest, second.digest);
        assert_eq!(first.digest, again.digest);
        assert!(first.tree.join(invalid).is_file());
        assert!(!first.tree.join(lookalike).exists());
        assert!(second.tree.join(lookalike).is_file());
        assert!(!second.tree.join(invalid).exists());
    }
}
