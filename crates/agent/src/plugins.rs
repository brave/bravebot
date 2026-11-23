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

use crate::confirm::{Classified, FetchRequest};
use crate::plugin_source::{Commit, GitSource, NotASource, Requested, Source, classify};
use bravebot_config::plugin::{self, FileFault, HookList, Manifest, Part, ServerList};
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
    /// The typed text is not a source: an archive, an npm package, a plain URL or a git URL or
    /// ref that cannot be used. Nothing was read or fetched and no host was asked about.
    NotASource(NotASource),
    /// The source is a git URL and no `git` program is installed where bravebot may start one:
    /// the only one found is in a directory a confined stage may write. Nothing was fetched.
    GitNotInstalled,
    /// `git` did not fetch the source or did not resolve a commit. Its own words are not read.
    CloneFailed,
    /// The source path is not text, so it cannot be recorded.
    SourceNotText,
    /// The staging copy or the install failed.
    Io(io::ErrorKind),
    /// The name is installed from another source. `installed` is `None` when only the directory
    /// is there and its record cannot be read.
    AlreadyInstalled {
        name: String,
        installed: Option<Box<Source>>,
        new: Box<Source>,
    },
    /// Nobody can be asked (a one-shot run), so nothing is installed (PLUGIN-4).
    NobodyToAsk,
    /// The person answered no. Nothing was copied or recorded.
    Declined,
    /// The name is installed from this same source, and `/plugin add` does not replace an installed
    /// plugin. The word for that is `/plugin update <name>` ([`update_typed`], PLUGIN-5).
    SameSource { name: String, source: Source },
    /// `/plugin update` was given a name that cannot be an installed plugin's, or none is
    /// installed under it (PLUGIN-9).
    Manage(ManageRefusal),
    /// The plugin has a directory and no record that can be read, so there is no recorded source
    /// to fetch again and no digest to compare with. Remove it and add it again.
    RecordUnreadable { name: String },
    /// The recorded source names a full commit, which no fetch of it can move (PLUGIN-14). Nothing
    /// was fetched and nothing was asked.
    NothingToUpdateTo { name: String },
    /// The source now holds a plugin of another name. Replacing `name` with it is not an update
    /// of `name`, so nothing is changed.
    UpdateRenamed { name: String, found: String },
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
    /// Where the plugin was read from. A local directory is its canonical absolute path.
    pub source: Source,
    pub manifest: Manifest,
    /// Lowercase hex SHA-256 of the staged tree.
    pub digest: String,
    /// Paths, relative to the source, of links and other non-regular files that were not copied.
    pub skipped: Vec<PathBuf>,
    /// The plugin's hooks file read from the staged tree as data, with `${PLUGIN_ROOT}` already
    /// replaced by the directory the plugin will have once installed (PLUGIN-13). `None` where the
    /// manifest names none; an `Err` says the file could not be read, and the listing says so.
    pub hooks: Option<Result<HookList, FileFault>>,
    /// The plugin's servers file, read and expanded the same way.
    pub servers: Option<Result<ServerList, FileFault>>,
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
    stage_directory(&source, Source::Local(source.clone()), home)
}

/// [`stage`] for a plugin tree that is already a directory on this machine, recorded as coming
/// from `source`. A local source passes its own canonical path. A git source passes the directory
/// a clone was made into and the source naming the URL, the requested ref and the resolved commit.
pub(crate) fn stage_directory(
    directory: &Path,
    source: Source,
    home: &Path,
) -> Result<Staged, Refusal> {
    let manifest = plugin::read(directory).map_err(Refusal::Manifest)?;
    plugin::check_links(directory, &manifest.named_parts()).map_err(Refusal::Manifest)?;
    for (part, path) in manifest.named_parts() {
        if std::fs::symlink_metadata(path.under(directory)).is_err() {
            return Err(Refusal::PartMissing(part));
        }
    }
    let holder = staging_dir(home)?;
    let tree = holder.dir.join("tree");
    let skipped = copy_tree(directory, &tree)?;
    let digest = digest_tree(&tree)?;
    let will_be = installed_dir(home, &manifest.name);
    let hooks = manifest.hooks_list(&tree, &will_be);
    let servers = manifest.servers_list(&tree, &will_be);
    Ok(Staged {
        _holder: holder,
        tree,
        source,
        manifest,
        digest,
        skipped,
        hooks,
        servers,
    })
}

/// The plugin's hooks, read from its installed directory with `${PLUGIN_ROOT}` replaced by that
/// directory (PLUGIN-13): the list a later step digests and fires from.
///
/// `None` where the plugin is not installed, its manifest cannot be read or names no hooks file.
/// `Some(Err)` where the file cannot be read. The manifest is read again from the installed
/// directory, so an edit there shows here and is for the digest check to refuse. Nothing is run.
pub fn installed_hooks(home: &Path, name: &str) -> Option<Result<HookList, FileFault>> {
    let dir = installed_dir(home, name);
    plugin::read(&dir).ok()?.hooks_list(&dir, &dir)
}

/// The plugin's servers, read from its installed directory and expanded as [`installed_hooks`] is.
pub fn installed_servers(home: &Path, name: &str) -> Option<Result<ServerList, FileFault>> {
    let dir = installed_dir(home, name);
    plugin::read(&dir).ok()?.servers_list(&dir, &dir)
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

/// Why an installed plugin's directory is not the one its record describes (PLUGIN-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mismatch {
    /// The plugin has no readable record, so there is no digest to hold the directory to.
    NoRecord,
    /// The directory could not be hashed: it is missing, holds a link or another entry that is not
    /// a file or a directory, or a file in it could not be read.
    Unreadable,
    /// The directory hashes to something other than the digest recorded at install.
    Differs,
}

/// Hashes the installed directory of `name` again and compares it with the digest the install
/// recorded, the check made before a plugin's hook fires or a plugin's server starts (PLUGIN-7).
///
/// The driver uses the answer only to refuse. It reads no byte of the tree to decide anything else,
/// and an `Err` of any kind means the hook or the server does not run. The digest is the one
/// [`stage`] computed over the copy, so a tree nobody touched matches.
pub fn tree_matches_record(home: &Path, name: &str) -> Result<(), Mismatch> {
    let recorded = read_record(home, name).ok_or(Mismatch::NoRecord)?;
    let now = digest_tree(&installed_dir(home, name)).map_err(|_| Mismatch::Unreadable)?;
    if now == recorded.digest {
        Ok(())
    } else {
        Err(Mismatch::Differs)
    }
}

fn record_path(home: &Path, name: &str) -> PathBuf {
    plugins_dir(home).join(format!("{name}{RECORD_SUFFIX}"))
}

/// The record's text: `key = value` lines. A local source writes the four lines `source`,
/// `version`, `enabled` and `digest`. A git source adds `commit`, and `requested-ref` where the
/// person named a ref, so a record with no `commit` line is a local one and every record written
/// before git sources still reads. The source, the ref and the version hold no control character or
/// whitespace (the classifier and the manifest reader refuse one), so a value is one line and is
/// written as it is.
fn record_text(source: &Source, version: &str, enabled: bool, digest: &str) -> String {
    let git = match source {
        Source::Local(_) => String::new(),
        Source::Git {
            requested_ref,
            commit,
            ..
        } => {
            let asked = requested_ref
                .as_ref()
                .map_or_else(String::new, |name| format!("requested-ref = {name}\n"));
            format!("{asked}commit = {commit}\n")
        }
    };
    format!("source = {source}\n{git}version = {version}\nenabled = {enabled}\ndigest = {digest}\n")
}

/// Reads the record beside an installed plugin. `None` when there is no record or it does not say
/// all three things, so a damaged record is never read as a plugin that is enabled.
pub fn read_record(home: &Path, name: &str) -> Option<Installed> {
    read_record_in(&plugins_dir(home), name)
}

fn read_record_in(plugins: &Path, name: &str) -> Option<Installed> {
    let text = std::fs::read_to_string(plugins.join(format!("{name}{RECORD_SUFFIX}"))).ok()?;
    let (mut source, mut version, mut enabled, mut digest) = (None, None, None, None);
    let (mut requested_ref, mut commit) = (None, None);
    for line in text.lines() {
        let (key, value) = line.split_once(" = ")?;
        match key {
            "source" => source = Some(value),
            "requested-ref" if !value.is_empty() => requested_ref = Some(value.to_string()),
            "commit" => commit = Some(Commit::parse(value)?),
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
    let source = match (commit, requested_ref) {
        (None, None) => Source::Local(PathBuf::from(source?)),
        (Some(commit), requested_ref) => Source::Git {
            url: source?.to_string(),
            requested_ref,
            commit,
        },
        // A ref with no commit it resolved to is not a record this build writes.
        (None, Some(_)) => return None,
    };
    Some(Installed {
        name: name.to_string(),
        source,
        version: version?,
        enabled: enabled?,
        digest: digest?,
    })
}

fn is_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Puts the staged tree at `partial` by a rename, or by a copy where the rename cannot be made (a
/// staging directory on another file system, or one that cannot be moved). Where neither works
/// nothing is left at `partial`.
fn move_staged_tree(staged: &Path, partial: &Path) -> Result<(), Refusal> {
    if std::fs::rename(staged, partial).is_ok() || copy_tree(staged, partial).is_ok() {
        return Ok(());
    }
    std::fs::remove_dir_all(partial).ok();
    Err(Refusal::Io(io::ErrorKind::Other))
}

/// Installs a staged plugin at `<state>/plugins/<name>/` with a record beside it.
///
/// Refused, with nothing created or changed, when the name is already installed: a different
/// source is named with the installed one, and the same source is refused with a pointer to
/// `/plugin update` ([`Refusal::SameSource`]). The name is checked before the plugins directory is
/// created.
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
            Some(source) if source.same_origin(&staged.source) => {
                Refusal::SameSource { name, source }
            }
            installed => Refusal::AlreadyInstalled {
                name,
                installed: installed.map(Box::new),
                new: Box::new(staged.source.clone()),
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
    move_staged_tree(&staged.tree, &partial)?;
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

    /// Put the host a git source is fetched from to the person, in the prompt a fetch uses, before
    /// anything is fetched. Only a true answer lets a request leave.
    ///
    /// Like [`Asker::confirm_install`] this has no default and takes no mode, rule or setting: an
    /// implementation puts it to a person or answers no.
    fn confirm_fetch_host(&mut self, request: &FetchRequest) -> bool;

    /// Put the listing of `staged` to the person. Only a true answer installs.
    fn confirm_install(&mut self, staged: &Staged) -> bool;

    /// Put the difference between the installed plugin and `staged`, a fetch of the recorded source
    /// whose digest differs, to the person (PLUGIN-9). `installed` is the record and `before` what
    /// the installed copy offers and declares, read as data for the listing. Only a true answer
    /// replaces the installed plugin.
    ///
    /// Like the other two this has no default and takes no mode, rule or setting: an implementation
    /// puts it to a person or answers no.
    fn confirm_update(&mut self, staged: &Staged, installed: &Installed, before: &Before) -> bool;
}

/// The asker of a run with nobody behind it, such as a one-shot run.
pub struct Nobody;

impl Asker for Nobody {
    fn anybody_there(&self) -> bool {
        false
    }

    fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
        false
    }

    fn confirm_install(&mut self, _staged: &Staged) -> bool {
        false
    }

    fn confirm_update(
        &mut self,
        _staged: &Staged,
        _installed: &Installed,
        _before: &Before,
    ) -> bool {
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
    confirm_and_commit(staged, home, asker)
}

/// [`install_asked`] for what a person typed after `/plugin add`.
///
/// The text is classified first, from the text alone ([`classify`]): an archive, an npm package, a
/// plain URL, a `file://` URL, another scheme or a bad ref is [`Refusal::NotASource`], with nothing
/// read, fetched or asked. A local path is read from `root` when it is relative, as
/// [`install_asked`] reads it. A git URL goes through [`install_git`].
pub fn install_typed<A: Asker + ?Sized>(
    typed: &str,
    root: &Path,
    home: &Path,
    asker: &mut A,
) -> Result<String, Refusal> {
    install_typed_with(
        typed,
        root,
        home,
        asker,
        crate::plugin_git::locate(),
        &crate::plugin_git::PROTOCOLS,
        // The directory the fetch is made under is a fresh one this account alone can enter, made
        // with a name that is not reused (`plugin_git::Scratch`), so the shared parent is not trusted.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        &std::env::temp_dir(), // shared-temp-name: only the parent is shared, the directory under it is random
    )
}

/// [`install_typed`] with the `git` program, the protocols it may use and the directory the fetch
/// is made under given, so a test can show which typed texts start no program and ask no question.
pub(crate) fn install_typed_with<A: Asker + ?Sized>(
    typed: &str,
    root: &Path,
    home: &Path,
    asker: &mut A,
    program: Option<bravebot_sandbox::programs::Resolved>,
    allow: &[&str],
    parent: &Path,
) -> Result<String, Refusal> {
    match classify(typed).map_err(Refusal::NotASource)? {
        Requested::Local(path) => install_asked(&root.join(path), home, asker),
        Requested::Git(git) => install_git(&git, home, asker, program, allow, parent),
    }
}

/// Installs a git source that has been classified: finds out whether anybody can be asked, whether
/// `git` is installed, puts the host to the person, and only after a yes fetches the source into a
/// staging directory and puts the install question.
///
/// The order is the whole of PLUGIN-3's network rule. Where nobody can be asked nothing is read.
/// Where there is no `git`, [`Refusal::GitNotInstalled`] is answered before a question is put. The
/// host question is put before any program is started, so a no starts none and a request never
/// leaves before the yes. `program` is the `git` to start, `allow` the protocols it may use, and
/// `parent` the directory the fetch's own directory is made in, which is outside the state
/// directory and every workspace and session directory for a person's install.
pub(crate) fn install_git<A: Asker + ?Sized>(
    git: &GitSource,
    home: &Path,
    asker: &mut A,
    program: Option<bravebot_sandbox::programs::Resolved>,
    allow: &[&str],
    parent: &Path,
) -> Result<String, Refusal> {
    let staged = fetch_staged(git, home, asker, program, allow, parent)?;
    confirm_and_commit(staged, home, asker)
}

/// The fetch an install and an update share (PLUGIN-3): finds out whether anybody can be asked and
/// whether `git` is installed, puts the host to the person, and only after a yes fetches the source
/// into a staging directory. Nothing leaves before the yes, and a no starts no program.
fn fetch_staged<A: Asker + ?Sized>(
    git: &GitSource,
    home: &Path,
    asker: &mut A,
    program: Option<bravebot_sandbox::programs::Resolved>,
    allow: &[&str],
    parent: &Path,
) -> Result<Staged, Refusal> {
    if !asker.anybody_there() {
        return Err(Refusal::NobodyToAsk);
    }
    let program = program.ok_or(Refusal::GitNotInstalled)?;
    let request = FetchRequest {
        url: git.url.clone(),
        host: git.host.clone(),
        redirected_from: None,
        classified: Classified::NotAsked,
    };
    if !asker.confirm_fetch_host(&request) {
        return Err(Refusal::Declined);
    }
    crate::plugin_git::stage_from(&program, git, allow, parent, home)
}

/// What an update did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Updated {
    /// The source holds the tree that is installed. Nothing was changed and nothing was asked.
    /// `newer_commit` is whether the recorded ref now resolves to another commit with the same
    /// tree, which is not recorded.
    Unchanged { newer_commit: bool },
    /// The person said yes to the difference and the installed plugin was replaced.
    Replaced,
}

/// `/plugin update <name>` (PLUGIN-9): fetches the recorded source again at the recorded ref and
/// compares the commit and the tree digest with the record.
///
/// A git source goes through the host question and the staging an install uses. A local path is
/// staged again from the recorded path with no host question, since nothing leaves the machine. A
/// source recorded as a full commit has nothing to update to and is answered before anything is
/// fetched or asked. Where nobody can be asked nothing is read. An unchanged digest
/// changes nothing and asks nothing. A different one is put to the person with
/// [`Asker::confirm_update`], and only a yes replaces anything.
pub fn update_typed<A: Asker + ?Sized>(
    name: &str,
    home: &Path,
    asker: &mut A,
) -> Result<Updated, Refusal> {
    update_with(
        name,
        home,
        asker,
        crate::plugin_git::locate(),
        &crate::plugin_git::PROTOCOLS,
        // The directory the fetch is made under is a fresh one this account alone can enter
        // (`plugin_git::Scratch`), so the shared parent is not trusted.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        &std::env::temp_dir(), // shared-temp-name: only the parent is shared, the directory under it is random
    )
}

/// [`update_typed`] with the `git` program, the protocols it may use and the directory the fetch
/// is made under given.
pub(crate) fn update_with<A: Asker + ?Sized>(
    name: &str,
    home: &Path,
    asker: &mut A,
    program: Option<bravebot_sandbox::programs::Resolved>,
    allow: &[&str],
    parent: &Path,
) -> Result<Updated, Refusal> {
    checked_name(name).map_err(Refusal::Manage)?;
    if !is_present(&installed_dir(home, name)) && !is_present(&record_path(home, name)) {
        return Err(Refusal::Manage(ManageRefusal::NotInstalled(
            name.to_string(),
        )));
    }
    // The installed directory is not hashed here: the record is what the new tree is compared
    // with, so a directory that was edited is replaced by an update (PLUGIN-9, Known costs).
    let installed = read_record(home, name).ok_or_else(|| Refusal::RecordUnreadable {
        name: name.to_string(),
    })?;
    let fetch = match &installed.source {
        Source::Local(_) => None,
        Source::Git {
            url, requested_ref, ..
        } => {
            // A ref that is a full commit cannot move, so there is nothing to fetch it for.
            if requested_ref
                .as_deref()
                .is_some_and(|text| Commit::parse(text).is_some())
            {
                return Err(Refusal::NothingToUpdateTo {
                    name: name.to_string(),
                });
            }
            // A recorded URL that no longer reads as a git source is a record this build cannot
            // update from, whatever the classifier now says about it.
            let Ok(Requested::Git(mut git)) = classify(url) else {
                return Err(Refusal::RecordUnreadable {
                    name: name.to_string(),
                });
            };
            git.requested_ref.clone_from(requested_ref);
            Some(git)
        }
    };
    update_fetching(
        &installed,
        fetch.as_ref(),
        home,
        asker,
        program,
        allow,
        parent,
    )
}

/// The rest of an update once the record is read: `fetch` is the git source to fetch again, or
/// `None` for a local path, which is staged again from the recorded path.
pub(crate) fn update_fetching<A: Asker + ?Sized>(
    installed: &Installed,
    fetch: Option<&GitSource>,
    home: &Path,
    asker: &mut A,
    program: Option<bravebot_sandbox::programs::Resolved>,
    allow: &[&str],
    parent: &Path,
) -> Result<Updated, Refusal> {
    if !asker.anybody_there() {
        return Err(Refusal::NobodyToAsk);
    }
    let staged = match (fetch, &installed.source) {
        (Some(git), _) => fetch_staged(git, home, asker, program, allow, parent)?,
        (None, Source::Local(path)) => stage(path, home)?,
        // A git record is always handed its source by `update_with`.
        (None, Source::Git { .. }) => {
            return Err(Refusal::RecordUnreadable {
                name: installed.name.clone(),
            });
        }
    };
    if staged.manifest.name != installed.name {
        return Err(Refusal::UpdateRenamed {
            name: installed.name.clone(),
            found: staged.manifest.name.clone(),
        });
    }
    if staged.digest == installed.digest {
        return Ok(Updated::Unchanged {
            newer_commit: staged.source.commit() != installed.source.commit(),
        });
    }
    // The name check comes before the question, so nobody is asked about an update that cannot be
    // made (PLUGIN-12). The plugin's own offers are not counted against it.
    refuse_taken_names(&staged, home, &installed.name)?;
    let before = before_of(home, &installed.name);
    if !asker.confirm_update(&staged, installed, &before) {
        return Err(Refusal::Declined);
    }
    commit_update(&staged, installed, home)?;
    Ok(Updated::Replaced)
}

/// What the installed copy of a plugin offers and declares, read from its directory as data, so the
/// question an update puts can list what is added, removed and changed. Nothing here decides
/// anything: the driver hands it to the listing and reads none of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Before {
    /// Whether the installed manifest could be read. Where it could not, nothing below is known
    /// and the listing says so rather than showing the plugin as offering nothing.
    pub readable: bool,
    pub offers: Vec<Offer>,
    pub hooks: Option<Result<HookList, FileFault>>,
    pub servers: Option<Result<ServerList, FileFault>>,
    /// Every file and directory of the installed tree by the bytes of its path relative to the
    /// plugin, with the SHA-256 of a file's bytes (a directory has none). `None` where the tree
    /// could not be read.
    files: Option<FileMap>,
}

/// How a file of the plugin differs between the installed copy and the staged one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Removed,
    Changed,
}

/// One file or directory that is added, removed or has other bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// The bytes of the path relative to the plugin, `/` between names, a directory with a
    /// trailing `/`. The bytes are kept so two names that differ only where a text rendering would
    /// merge them stay two paths (PATH-001); the interface converts them where it draws the name.
    pub path: Vec<u8>,
    pub change: Change,
}

/// The files of a tree by the bytes of their relative paths, each with the digest of its content
/// or `None` for a directory.
type FileMap = std::collections::BTreeMap<Vec<u8>, Option<String>>;

/// The path of every file and directory under `root` with a file's digest.
fn file_digests(root: &Path) -> Result<FileMap, Refusal> {
    let mut found = Vec::new();
    collect(root, &[], &mut found)?;
    digest_found(found)
}

/// The digest of each file of `found`, `(relative path bytes, path on disk, is a directory)`, keyed
/// on the relative path bytes, with a `/` added to a directory's.
fn digest_found(found: Vec<(Vec<u8>, PathBuf, bool)>) -> Result<FileMap, Refusal> {
    let mut map = FileMap::new();
    for (mut name, path, is_dir) in found {
        if is_dir {
            name.push(b'/');
            map.insert(name, None);
            continue;
        }
        let mut hasher = Sha256::new();
        feed(&mut hasher, &path)?;
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        map.insert(name, Some(hex));
    }
    Ok(map)
}

/// Every file and directory that differs between the installed copy and `staged`, in path order
/// (PLUGIN-9). This is what the listing shows besides the parts, so a script changed under a hooks
/// file that reads the same is still on the screen. `None` where either tree could not be read, and
/// the listing says so. Only paths and digests are compared; no file is read for anything else.
pub fn file_changes(staged: &Staged, before: &Before) -> Option<Vec<FileChange>> {
    let old = before.files.as_ref()?;
    let new = file_digests(&staged.tree).ok()?;
    Some(changes_between(old, &new))
}

/// The paths that are in one of two file maps and not the other, or in both with other bytes, in
/// the order of the path bytes.
fn changes_between(old: &FileMap, new: &FileMap) -> Vec<FileChange> {
    let mut paths: std::collections::BTreeSet<&Vec<u8>> = old.keys().collect();
    paths.extend(new.keys());
    paths
        .into_iter()
        .filter_map(|path| {
            let change = match (old.get(path), new.get(path)) {
                (None, Some(_)) => Change::Added,
                (Some(_), None) => Change::Removed,
                (Some(had), Some(has)) if had != has => Change::Changed,
                _ => return None,
            };
            Some(FileChange {
                path: path.clone(),
                change,
            })
        })
        .collect()
}

/// [`Before`] for the installed plugin `name`. The manifest is read again from the installed
/// directory, as [`installed_hooks`] reads it.
fn before_of(home: &Path, name: &str) -> Before {
    let directory = installed_dir(home, name);
    match plugin::read(&directory) {
        Ok(manifest) => Before {
            readable: true,
            offers: offers_in(&directory, &manifest),
            hooks: manifest.hooks_list(&directory, &directory),
            servers: manifest.servers_list(&directory, &directory),
            files: file_digests(&directory).ok(),
        },
        Err(_) => Before {
            readable: false,
            offers: Vec::new(),
            hooks: None,
            servers: None,
            files: file_digests(&directory).ok(),
        },
    }
}

/// Refuses the update when a skill or definition the staged plugin offers is already offered by an
/// installed plugin other than `name` itself (PLUGIN-12), naming both. Only names are compared.
fn refuse_taken_names(staged: &Staged, home: &Path, name: &str) -> Result<(), Refusal> {
    match conflicts_except(&staged_offers(staged), home, Some(name)) {
        Some((offered, existing_plugin)) => Err(Refusal::NameOffered {
            offered,
            new_plugin: name.to_string(),
            existing_plugin,
        }),
        None => Ok(()),
    }
}

/// Replaces the installed plugin with the staged one after a yes (PLUGIN-9).
///
/// The order, so that a failure leaves the old plugin as it was wherever it can:
///
/// 1. the names are checked again and the staged tree is moved beside the installed one, which
///    changes nothing that is installed;
/// 2. the answers recorded for the plugin's hooks and servers are withdrawn, as `remove` does it
///    and before anything is replaced, so a withdrawal that cannot be written stops the update
///    with the old plugin in place. Every line for the plugin goes, not only a changed part's: the
///    hook and server approvals are keyed on the tree digest, so none of them would match the new
///    tree and each asks again;
/// 3. the installed directory is moved aside, the new one moved in, and the record written through
///    a temporary file and a rename. A step that fails puts the old directory back and leaves the
///    old record, so the old plugin works. A failure after step 2 leaves the old plugin working and
///    asking again for its hooks and servers.
///
/// The enabled state is kept, and the version, source, commit and digest are the staged ones.
fn commit_update(staged: &Staged, installed: &Installed, home: &Path) -> Result<(), Refusal> {
    let name = installed.name.as_str();
    refuse_taken_names(staged, home, name)?;
    let plugins = plugins_dir(home);
    let target = installed_dir(home, name);
    // A name holds no dot, so neither is another plugin's directory.
    let partial = plugins.join(format!(".{name}.updating"));
    let previous = plugins.join(format!(".{name}.previous"));
    // A crash between the two moves of an earlier update left the old copy here and no directory.
    if !is_present(&target) && is_present(&previous) {
        std::fs::rename(&previous, &target).map_err(io_refusal)?;
    }
    std::fs::remove_dir_all(&partial).ok();
    std::fs::remove_dir_all(&previous).ok();
    move_staged_tree(&staged.tree, &partial)?;
    let withdrawn = bravebot_config::hooks_approved::withdraw(home, name)
        .and_then(|()| bravebot_config::mcp::withdraw_plugin(home, name));
    if let Err(error) = withdrawn {
        std::fs::remove_dir_all(&partial).ok();
        return Err(io_refusal(error));
    }
    if let Err(error) = std::fs::rename(&target, &previous) {
        std::fs::remove_dir_all(&partial).ok();
        return Err(io_refusal(error));
    }
    if let Err(error) = std::fs::rename(&partial, &target) {
        std::fs::rename(&previous, &target).ok();
        std::fs::remove_dir_all(&partial).ok();
        return Err(io_refusal(error));
    }
    let replacement = Installed {
        name: installed.name.clone(),
        source: staged.source.clone(),
        version: staged.manifest.version.clone(),
        enabled: installed.enabled,
        digest: staged.digest.clone(),
    };
    if let Err(ManageRefusal::Io(kind)) = write_record(home, &replacement) {
        std::fs::remove_dir_all(&target).ok();
        std::fs::rename(&previous, &target).ok();
        return Err(Refusal::Io(kind));
    }
    std::fs::remove_dir_all(&previous).ok();
    Ok(())
}

/// Puts the listing of `staged` to `asker` and installs it on a yes.
fn confirm_and_commit<A: Asker + ?Sized>(
    staged: Staged,
    home: &Path,
    asker: &mut A,
) -> Result<String, Refusal> {
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
    pub source: Source,
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

/// Deletes the plugin's directory and its record (PLUGIN-9), after withdrawing the answers
/// recorded for its hooks in `hooks-approved`, so a later install of the same name asks again. The
/// withdrawal comes first: a failure after it leaves a plugin that asks again, and one before it
/// would leave a yes with nothing it applies to. The directory goes before the record, so a failure
/// leaves the record and the removal can be run again.
pub fn remove(home: &Path, name: &str) -> Result<(), ManageRefusal> {
    checked_name(name)?;
    let directory = installed_dir(home, name);
    let record = record_path(home, name);
    if !is_present(&directory) && !is_present(&record) {
        return Err(ManageRefusal::NotInstalled(name.to_string()));
    }
    bravebot_config::hooks_approved::withdraw(home, name)
        .map_err(|error| ManageRefusal::Io(error.kind()))?;
    bravebot_config::mcp::withdraw_plugin(home, name)
        .map_err(|error| ManageRefusal::Io(error.kind()))?;
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
    conflicts_except(staged_offers, home, None)
}

/// [`conflicts`] leaving out the installed plugin called `except`, so an update is not refused for
/// a name its own installed copy offers.
fn conflicts_except(
    staged_offers: &[Offer],
    home: &Path,
    except: Option<&str>,
) -> Option<(String, String)> {
    for installed in list(home) {
        if except == Some(installed.name.as_str()) {
            continue;
        }
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
        assert_eq!(
            first.source,
            Source::Local(std::fs::canonicalize(&source).unwrap())
        );
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
                installed: Some(Box::new(Source::Local(
                    std::fs::canonicalize(&first).unwrap()
                ))),
                new: Box::new(new),
            }
        );
        assert_eq!(snapshot(&home), before);
    }

    #[test]
    fn adding_the_same_source_again_is_refused_and_points_at_update() {
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
            Refusal::SameSource {
                name: "demo".to_string(),
                source: Source::Local(std::fs::canonicalize(&source).unwrap()),
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
        assert_eq!(
            listed[1].source,
            Source::Local(std::fs::canonicalize(&one).unwrap())
        );
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

        fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
            false
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            self.asked += 1;
            self.yes
        }

        fn confirm_update(&mut self, _s: &Staged, _i: &Installed, _b: &Before) -> bool {
            false
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

    /// PLUGIN-5, PLUGIN-14: a git source's record holds the URL, the requested ref, the commit it
    /// resolved to and the digest, and reads back the same through list, enable and disable. A
    /// source with no ref records none, and the same URL and ref added again is the update that is
    /// refused as [`Refusal::SameSource`], which points at `/plugin update`, even where the commit
    /// differs, while another ref of the URL is another source.
    ///
    /// Rejects: a record that drops the ref or the commit, a ref written for a source that named
    /// none, enable or disable rewriting the record without them, and an update test that compares
    /// the commit or ignores the ref.
    #[test]
    fn a_git_record_holds_the_url_ref_and_commit_and_the_same_url_and_ref_is_an_update() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let tree = plugin_dir(scratch.path(), "cloned", MANIFEST);
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let git = |requested: Option<&str>, commit: &str| Source::Git {
            url: "https://example.com/demo.git".to_string(),
            requested_ref: requested.map(str::to_string),
            commit: Commit::parse(commit).unwrap(),
        };
        let staged = stage_directory(&tree, git(Some("release-2"), commit), &home).unwrap();
        let digest = staged.digest.clone();
        commit_install(staged, &home).unwrap();
        let text = std::fs::read_to_string(record_path(&home, "demo")).unwrap();
        assert_eq!(
            text,
            format!(
                "source = https://example.com/demo.git\nrequested-ref = release-2\ncommit = {commit}\n\
                 version = 1.0.0\nenabled = true\ndigest = {digest}\n"
            )
        );
        let read = read_record(&home, "demo").unwrap();
        assert_eq!(read.source, git(Some("release-2"), commit));
        assert_eq!(read.digest, digest);
        set_enabled(&home, "demo", false).unwrap();
        set_enabled(&home, "demo", true).unwrap();
        assert_eq!(read_record(&home, "demo"), Some(read));
        assert_eq!(list(&home).len(), 1);

        let before = snapshot(&home);
        let moved = "f".repeat(40);
        let again = stage_directory(&tree, git(Some("release-2"), &moved), &home).unwrap();
        assert!(matches!(
            commit_install(again, &home),
            Err(Refusal::SameSource { .. })
        ));
        let other_ref = stage_directory(&tree, git(Some("dev"), commit), &home).unwrap();
        assert!(matches!(
            commit_install(other_ref, &home),
            Err(Refusal::AlreadyInstalled { .. })
        ));
        let no_ref = stage_directory(&tree, git(None, commit), &home).unwrap();
        assert!(matches!(
            commit_install(no_ref, &home),
            Err(Refusal::AlreadyInstalled { .. })
        ));
        assert_eq!(snapshot(&home), before);

        remove(&home, "demo").unwrap();
        commit_install(
            stage_directory(&tree, git(None, commit), &home).unwrap(),
            &home,
        )
        .unwrap();
        let text = std::fs::read_to_string(record_path(&home, "demo")).unwrap();
        assert!(!text.contains("requested-ref"), "{text}");
        assert_eq!(
            read_record(&home, "demo").unwrap().source,
            git(None, commit)
        );
    }

    /// PLUGIN-14: a record written before git sources, with four lines and no commit, reads as a
    /// local source, and a record that names a commit that is not 40 lowercase hex digits, a ref
    /// with no commit or an empty ref reads as no record.
    ///
    /// Rejects: a reader that requires the new lines, one that reads a damaged commit as a
    /// plugin, and one that reads a ref with no commit as a local source.
    #[test]
    fn an_old_record_still_reads_and_a_damaged_git_record_does_not() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path();
        std::fs::create_dir_all(plugins_dir(home)).unwrap();
        std::fs::write(
            record_path(home, "demo"),
            "source = /a/b\nversion = 1\nenabled = false\ndigest = d\n",
        )
        .unwrap();
        let old = read_record(home, "demo").unwrap();
        assert_eq!(old.source, Source::Local(PathBuf::from("/a/b")));
        assert_eq!(old.source.commit(), None);
        assert!(!old.enabled);
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let tail = "version = 1\nenabled = true\ndigest = d\n";
        for text in [
            format!(
                "source = https://h/x.git\ncommit = {}\n{tail}",
                &commit[..39]
            ),
            format!(
                "source = https://h/x.git\ncommit = {}\n{tail}",
                commit.to_uppercase()
            ),
            format!("source = https://h/x.git\ncommit = main\n{tail}"),
            format!("source = https://h/x.git\nrequested-ref = main\n{tail}"),
            format!("source = https://h/x.git\nrequested-ref = \ncommit = {commit}\n{tail}"),
            format!("commit = {commit}\n{tail}"),
        ] {
            std::fs::write(record_path(home, "demo"), &text).unwrap();
            assert_eq!(read_record(home, "demo"), None, "{text}");
        }
    }

    /// An asker that answers yes and counts how often it was asked.
    struct Counting {
        asked: usize,
    }

    impl Asker for Counting {
        fn anybody_there(&self) -> bool {
            true
        }

        fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
            false
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            self.asked += 1;
            true
        }

        fn confirm_update(&mut self, _s: &Staged, _i: &Installed, _b: &Before) -> bool {
            false
        }
    }

    /// PLUGIN-3: typed text that is not a source is refused before anything is read, staged or
    /// asked. A typed local path still installs from the directory the root names.
    ///
    /// Rejects: an archive, an npm name, a plain URL or a bad ref that reaches the asker or the
    /// state directory, a git URL that is read as a local path, and a local path that is no longer
    /// read from the root.
    #[test]
    fn typed_text_that_is_not_a_source_asks_and_writes_nothing_and_a_path_still_installs() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        plugin_dir(scratch.path(), "src", MANIFEST);
        let mut asker = Counting { asked: 0 };
        for (typed, why) in [
            ("https://example.com/x.tar.gz", NotASource::Archive),
            ("@scope/pkg", NotASource::NpmPackage),
            ("https://example.com/docs", NotASource::PlainUrl),
            ("file:///tmp/p", NotASource::FileUrl),
            ("git+ftp://example.com/x", NotASource::UnsupportedScheme),
            ("https://example.com/x.git#-x", NotASource::BadRef),
        ] {
            assert_eq!(
                install_typed(typed, scratch.path(), &home, &mut asker),
                Err(Refusal::NotASource(why)),
                "{typed}"
            );
        }
        assert_eq!(
            install_typed(
                "https://example.com/x.git",
                scratch.path(),
                &home,
                &mut Nobody
            ),
            Err(Refusal::NobodyToAsk)
        );
        assert_eq!(asker.asked, 0);
        assert!(!home.exists());
        assert_eq!(
            install_typed("src", scratch.path(), &home, &mut asker),
            Ok("demo".to_string())
        );
        assert_eq!(asker.asked, 1);
        assert_eq!(
            read_record(&home, "demo").unwrap().source,
            Source::Local(std::fs::canonicalize(scratch.path().join("src")).unwrap())
        );
    }

    /// An asker that records what it was shown and answers the two questions as told. `seen` reads
    /// the number of connections a listener has taken when the host question is put.
    struct Host {
        host_yes: bool,
        install_yes: bool,
        hosts: Vec<FetchRequest>,
        installs: usize,
        connections_when_asked: Vec<usize>,
        seen: Box<dyn Fn() -> usize>,
    }

    impl Host {
        fn new(host_yes: bool, install_yes: bool, seen: Box<dyn Fn() -> usize>) -> Self {
            Self {
                host_yes,
                install_yes,
                hosts: Vec::new(),
                installs: 0,
                connections_when_asked: Vec::new(),
                seen,
            }
        }
    }

    impl Asker for Host {
        fn anybody_there(&self) -> bool {
            true
        }

        fn confirm_fetch_host(&mut self, request: &FetchRequest) -> bool {
            self.connections_when_asked.push((self.seen)());
            self.hosts.push(request.clone());
            self.host_yes
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            self.installs += 1;
            self.install_yes
        }

        fn confirm_update(&mut self, _s: &Staged, _i: &Installed, _b: &Before) -> bool {
            false
        }
    }

    /// A loopback listener that counts the connections made to it and closes each at once.
    struct Listener {
        port: u16,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Listener {
        fn start() -> Self {
            use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
            let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            socket.set_nonblocking(true).unwrap();
            let port = socket.local_addr().unwrap().port();
            let count = std::sync::Arc::new(AtomicUsize::new(0));
            let stop = std::sync::Arc::new(AtomicBool::new(false));
            let (counted, stopped) = (count.clone(), stop.clone());
            let thread = std::thread::spawn(move || {
                while !stopped.load(Ordering::SeqCst) {
                    match socket.accept() {
                        Ok(_) => {
                            counted.fetch_add(1, Ordering::SeqCst);
                        }
                        Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                    }
                }
            });
            Self {
                port,
                count,
                stop,
                thread: Some(thread),
            }
        }

        fn connections(&self) -> usize {
            self.count.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn counter(&self) -> Box<dyn Fn() -> usize> {
            let count = self.count.clone();
            Box::new(move || count.load(std::sync::atomic::Ordering::SeqCst))
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                thread.join().ok();
            }
        }
    }

    fn a_git_source_at(listener: &Listener) -> GitSource {
        let Ok(Requested::Git(git)) = classify(&format!(
            "https://127.0.0.1:{}/demo.git#main",
            listener.port
        )) else {
            panic!("a git source");
        };
        git
    }

    /// PLUGIN-3, issue test 1: no connection is made to the host before the answer is yes, a no
    /// makes none and starts no program, and a yes lets the request leave.
    ///
    /// The listener is the host named in the URL. The asker reads its count while the question is
    /// put, and again after the install returns.
    ///
    /// Rejects: a fetch started before the question is put (the count is above zero when the asker
    /// reads it), a no that is ignored (the count is above zero after it), a question put with
    /// nobody to answer it, and a yes that fetches nothing (the control: a listener that counts
    /// zero after every answer would pass the first two on a build that never fetches).
    #[test]
    fn no_connection_is_made_to_the_host_before_a_yes_and_a_no_makes_none() {
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let listener = Listener::start();
        let source = a_git_source_at(&listener);

        let mut no = Host::new(false, true, listener.counter());
        assert_eq!(
            install_git(
                &source,
                &home,
                &mut no,
                Some(git.clone()),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::Declined)
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(no.connections_when_asked, vec![0]);
        assert_eq!(listener.connections(), 0, "a no made a connection");
        assert_eq!(no.installs, 0);
        assert!(!home.exists());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);

        let mut yes = Host::new(true, true, listener.counter());
        let answered = install_git(
            &source,
            &home,
            &mut yes,
            Some(git),
            &crate::plugin_git::PROTOCOLS,
            parent.path(),
        );
        assert_eq!(answered, Err(Refusal::CloneFailed));
        assert_eq!(yes.connections_when_asked, vec![0]);
        assert!(listener.connections() > 0, "a yes did not reach the host");
        assert_eq!(yes.installs, 0);
        assert!(!home.exists());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
    }

    /// PLUGIN-3, FETCH-2: the question carries the host the URL parser read and the URL, as the
    /// prompt of a fetch does, with no redirect and no classifier answer, and it is put once.
    ///
    /// Rejects: a question whose host is the text before the first `/` (the userinfo of
    /// `ssh://example.com@evil.test/x` is not the host), a missing URL, and a redirect row or a
    /// classifier answer carried into a question that is about a typed URL.
    #[test]
    fn the_host_question_carries_the_parsed_host_and_the_url_and_is_put_once() {
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let Ok(Requested::Git(source)) = classify("ssh://example.com@evil.test/x.git#v2") else {
            panic!("a git source");
        };
        let mut no = Host::new(false, true, Box::new(|| 0));
        assert_eq!(
            install_git(
                &source,
                &scratch.path().join("home"),
                &mut no,
                Some(git),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::Declined)
        );
        assert_eq!(
            no.hosts,
            vec![FetchRequest {
                url: "ssh://example.com@evil.test/x.git".to_string(),
                host: "evil.test".to_string(),
                redirected_from: None,
                classified: Classified::NotAsked,
            }]
        );
    }

    /// PLUGIN-3: where there is no `git` the refusal is the not-installed one and no question is
    /// put, and where nobody can be asked nothing is started and nothing is asked.
    ///
    /// Rejects: a host question for a program that cannot be started, and a fetch with nobody to
    /// answer for it.
    #[test]
    fn no_git_or_nobody_to_ask_puts_no_question() {
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let Ok(Requested::Git(source)) = classify("https://example.com/x.git") else {
            panic!("a git source");
        };
        let mut asker = Host::new(true, true, Box::new(|| 0));
        assert_eq!(
            install_git(
                &source,
                &home,
                &mut asker,
                None,
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::GitNotInstalled)
        );
        assert!(asker.hosts.is_empty());
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        assert_eq!(
            install_git(
                &source,
                &home,
                &mut Nobody,
                Some(git),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::NobodyToAsk)
        );
        assert!(!home.exists());
    }

    /// PLUGIN-3, PLUGIN-14: after both answers are yes, a git source installs the tree of the
    /// commit its branch pointed to and records that commit, and a no at the install question
    /// leaves the state directory as it was although the fetch ran.
    ///
    /// Rejects: an install that records the ref text instead of the commit, one that asks the
    /// install question before the fetch (the listing could not hold the commit), and one that
    /// writes the state directory on a no.
    #[test]
    fn a_yes_to_the_host_and_to_the_listing_installs_the_commit_the_branch_pointed_to() {
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        let remote = crate::plugin_git::fixtures::remote();
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = GitSource {
            url: remote.path.to_string_lossy().into_owned(),
            host: "local.test".to_string(),
            requested_ref: Some("main".to_string()),
        };
        let mut declined = Host::new(true, false, Box::new(|| 0));
        assert_eq!(
            install_git(
                &source,
                &home,
                &mut declined,
                Some(git.clone()),
                &["file"],
                parent.path()
            ),
            Err(Refusal::Declined)
        );
        assert_eq!(declined.installs, 1);
        assert!(!home.exists());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);

        let mut yes = Host::new(true, true, Box::new(|| 0));
        assert_eq!(
            install_git(
                &source,
                &home,
                &mut yes,
                Some(git),
                &["file"],
                parent.path()
            ),
            Ok("demo".to_string())
        );
        let record = read_record(&home, "demo").unwrap();
        assert_eq!(
            record.source.commit().map(Commit::as_str),
            Some(remote.second.as_str())
        );
        assert!(installed_dir(&home, "demo").join("second.txt").exists());
        assert!(!installed_dir(&home, "demo").join(".git").exists());
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
        assert_eq!(
            staged.source,
            Source::Local(std::fs::canonicalize(&lookalike).unwrap())
        );
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

    const WITH_PARTS: &str = "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nskills = \"skills\"\nhooks = \"hooks.json\"\nservers = \"servers.json\"\n";
    const HOOKS: &str = r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh"]},{"on":"nope","run":["x"]}]}"#;
    const SERVERS: &str = r#"{"servers":{"s":{"transport":"stdio","argv":["${PLUGIN_ROOT}/server.py"],"env":{"K":"${PLUGIN_ROOT}/k"}}}}"#;

    fn plugin_with_parts(parent: &Path) -> PathBuf {
        let root = plugin_dir(parent, "src", WITH_PARTS);
        std::fs::write(root.join("hooks.json"), HOOKS).unwrap();
        std::fs::write(root.join("servers.json"), SERVERS).unwrap();
        root
    }

    /// S1, S2, S6, S7, S29: stage reads both files from the staged copy as data, and a staged
    /// plugin's argv already names the directory it will have once installed, not the staging
    /// directory it was read from. What the hook reader passed over is counted.
    ///
    /// Rejects: a staged read that expands to the staging path, a read of the source directory
    /// instead of the copy, a hooks file that is not read at all, and a drop of an entry with no
    /// trace in the result.
    #[test]
    fn a_staged_plugin_reads_its_hooks_and_servers_with_the_path_it_will_have() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let staged = stage(&plugin_with_parts(scratch.path()), &home).unwrap();
        let will_be = installed_dir(&home, "demo");
        let hooks = staged.hooks.as_ref().unwrap().as_ref().unwrap();
        let hook = &hooks.hooks.declared()[0];
        assert_eq!(hooks.hooks.declared().len(), 1);
        assert_eq!(hook.run(), [format!("{}/hook.sh", will_be.display())]);
        assert_eq!(hooks.unkept.dropped, 1);
        let servers = staged.servers.as_ref().unwrap().as_ref().unwrap();
        let server = servers.entries[0].declaration.as_ref().unwrap();
        assert_eq!(server.argv(), [format!("{}/server.py", will_be.display())]);
        assert!(!server.argv()[0].contains(".plugin-staging"));
        // A plugin naming neither part has nothing to read.
        let bare = stage(&plugin_dir(scratch.path(), "bare", MANIFEST), &home).unwrap();
        assert!(bare.hooks.is_none() && bare.servers.is_none());
    }

    /// S8: the list read from the installed directory is the list the staged copy gave before the
    /// install, so a digest taken at install and one taken at a later check are over the same argv.
    ///
    /// Rejects: an installed read that expands against a different directory than a staged read.
    #[test]
    fn the_installed_read_gives_the_list_the_staged_read_gave() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let staged = stage(&plugin_with_parts(scratch.path()), &home).unwrap();
        let (hooks, servers) = (staged.hooks.clone(), staged.servers.clone());
        commit_install(staged, &home).unwrap();
        assert_eq!(installed_hooks(&home, "demo"), hooks);
        assert_eq!(installed_servers(&home, "demo"), servers);
        assert_eq!(installed_hooks(&home, "missing"), None);
    }

    /// S18: an install of a plugin carrying hooks and servers writes the tree and its record under
    /// `plugins/` and nothing else: no `hooks-approved`, no `mcp-approved`, no `mcp.json`.
    ///
    /// Rejects: an install that records an answer for a hook or a server.
    #[test]
    fn an_install_records_no_answer_for_a_hook_or_a_server() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        commit_install(
            stage(&plugin_with_parts(scratch.path()), &home).unwrap(),
            &home,
        )
        .unwrap();
        let mut top: Vec<String> = std::fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        top.sort();
        assert_eq!(top, ["plugins"]);
        let mut inside: Vec<String> = std::fs::read_dir(plugins_dir(&home))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        inside.sort();
        assert_eq!(inside, ["demo", "demo.record"]);
    }

    /// S20: the re-hash the servers worker calls. An untouched install matches its record. An
    /// edited file, an added file, a removed file and a missing directory are each a mismatch, and
    /// a plugin with no record has nothing to match.
    ///
    /// Rejects: a comparison that is skipped (always Ok), one that ignores an added or removed
    /// file, and one that reads a missing record as a match.
    #[test]
    fn the_installed_tree_is_hashed_again_and_compared_with_the_record() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        commit_install(
            stage(&plugin_with_parts(scratch.path()), &home).unwrap(),
            &home,
        )
        .unwrap();
        assert_eq!(tree_matches_record(&home, "demo"), Ok(()));
        let dir = installed_dir(&home, "demo");

        std::fs::write(dir.join("hooks.json"), HOOKS.replace("x", "y")).unwrap();
        assert_eq!(tree_matches_record(&home, "demo"), Err(Mismatch::Differs));
        std::fs::write(dir.join("hooks.json"), HOOKS).unwrap();
        assert_eq!(tree_matches_record(&home, "demo"), Ok(()));

        std::fs::write(dir.join("added"), "").unwrap();
        assert_eq!(tree_matches_record(&home, "demo"), Err(Mismatch::Differs));
        std::fs::remove_file(dir.join("added")).unwrap();
        std::fs::remove_file(dir.join("servers.json")).unwrap();
        assert_eq!(tree_matches_record(&home, "demo"), Err(Mismatch::Differs));

        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            tree_matches_record(&home, "demo"),
            Err(Mismatch::Unreadable)
        );
        assert_eq!(
            tree_matches_record(&home, "missing"),
            Err(Mismatch::NoRecord)
        );
    }

    /// S1, S2: a named hooks or servers file that is not there is refused like a missing skills
    /// directory, and one that cannot be used is carried as a fault for the listing, not refused.
    ///
    /// Rejects: a plugin that stages with a part path naming nothing, and a staged plugin whose
    /// unusable hooks file is read as no hooks.
    #[test]
    fn a_named_part_file_that_is_missing_is_refused_and_one_that_is_unusable_is_a_fault() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_with_parts(scratch.path());
        std::fs::write(source.join("hooks.json"), "not json").unwrap();
        let staged = stage(&source, &home).unwrap();
        assert_eq!(staged.hooks, Some(Err(plugin::FileFault::NotHooks)));
        drop(staged);
        std::fs::remove_file(source.join("servers.json")).unwrap();
        assert_eq!(
            stage(&source, &home).unwrap_err(),
            Refusal::PartMissing(Part::Servers)
        );
    }

    /// An asker for an update: answers the host question and the update question as told, and
    /// keeps what it was shown.
    struct Updating {
        there: bool,
        host_yes: bool,
        update_yes: bool,
        hosts: Vec<FetchRequest>,
        updates: Vec<(Option<String>, String, Option<String>)>,
        befores: Vec<Before>,
        installs: usize,
    }

    impl Updating {
        fn new(host_yes: bool, update_yes: bool) -> Self {
            Self {
                there: true,
                host_yes,
                update_yes,
                hosts: Vec::new(),
                updates: Vec::new(),
                befores: Vec::new(),
                installs: 0,
            }
        }
    }

    impl Asker for Updating {
        fn anybody_there(&self) -> bool {
            self.there
        }

        fn confirm_fetch_host(&mut self, request: &FetchRequest) -> bool {
            self.hosts.push(request.clone());
            self.host_yes
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            self.installs += 1;
            true
        }

        fn confirm_update(
            &mut self,
            staged: &Staged,
            installed: &Installed,
            before: &Before,
        ) -> bool {
            self.befores.push(before.clone());
            self.updates.push((
                staged
                    .source
                    .commit()
                    .map(|commit| commit.as_str().to_string()),
                staged.digest.clone(),
                installed
                    .source
                    .commit()
                    .map(|commit| commit.as_str().to_string()),
            ));
            self.update_yes
        }
    }

    /// Writes a line a hook answer would hold, so a test can show the approval files stay as they
    /// were.
    fn plant_answers(home: &Path) {
        std::fs::write(home.join("hooks-approved"), "demo 0123456789abcdef\n").unwrap();
        std::fs::write(
            home.join("mcp-approved"),
            "plugin demo s 0123456789abcdef\n",
        )
        .unwrap();
    }

    /// PLUGIN-9: where the source still holds the installed tree an update changes
    /// nothing and asks nothing. The record, the directory and the answers recorded for the
    /// plugin's hooks and servers are byte for byte what they were, and the staging directory is
    /// gone.
    ///
    /// Rejects: an update that rewrites the record or the directory when the digest is the same,
    /// one that asks the person about a difference that is not there, and one that reports a
    /// change.
    #[test]
    fn an_update_of_an_unchanged_local_source_changes_nothing_and_asks_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_with_parts(scratch.path());
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        plant_answers(&home);
        let before = snapshot(&home);
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update_typed("demo", &home, &mut asker),
            Ok(Updated::Unchanged {
                newer_commit: false
            })
        );
        assert!(asker.hosts.is_empty(), "a local path asks no host question");
        assert!(asker.updates.is_empty() && asker.installs == 0);
        assert_eq!(snapshot(&home), before);
    }

    /// PLUGIN-9, PLUGIN-14: a local source is staged again from the recorded path, so an edit there
    /// is a different digest that is put to the person once, with the host question not put, and a
    /// no leaves the directory, the record and the answers as they were.
    ///
    /// Rejects: an update that does not read the source again, one that puts a host question for a
    /// path, one that asks before comparing digests, and one that changes anything on a no.
    #[test]
    fn an_update_of_a_changed_local_source_asks_once_and_a_no_changes_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_with_parts(scratch.path());
        let installed = stage(&source, &home).unwrap();
        let old_digest = installed.digest.clone();
        commit_install(installed, &home).unwrap();
        plant_answers(&home);
        std::fs::write(
            source.join("hooks.json"),
            HOOKS.replace("hook.sh", "other.sh"),
        )
        .unwrap();
        let before = snapshot(&home);
        let mut asker = Updating::new(true, false);
        assert_eq!(
            update_typed("demo", &home, &mut asker),
            Err(Refusal::Declined)
        );
        assert!(asker.hosts.is_empty());
        assert_eq!(asker.updates.len(), 1);
        assert_ne!(asker.updates[0].1, old_digest);
        assert_eq!(snapshot(&home), before);
    }

    /// PLUGIN-9: where nobody can be asked nothing is read or asked, a name that is not a plugin's
    /// or not installed is said, a record that cannot be read is said, and a source that now
    /// holds another plugin is not an update of this one.
    ///
    /// Rejects: an update that stages with nobody to answer, one that joins a bad name to a path,
    /// one that reads a missing record as an unchanged plugin, and one that replaces `demo` with a
    /// plugin of another name.
    #[test]
    fn an_update_with_nobody_to_ask_or_a_bad_name_or_another_plugin_changes_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_with_parts(scratch.path());
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        let before = snapshot(&home);
        assert_eq!(
            update_typed("demo", &home, &mut Nobody),
            Err(Refusal::NobodyToAsk)
        );
        for name in ["../x", "", "a b"] {
            assert_eq!(
                update_typed(name, &home, &mut Updating::new(true, true)),
                Err(Refusal::Manage(ManageRefusal::BadName)),
                "{name}"
            );
        }
        assert_eq!(
            update_typed("absent", &home, &mut Updating::new(true, true)),
            Err(Refusal::Manage(ManageRefusal::NotInstalled(
                "absent".to_string()
            )))
        );
        assert_eq!(snapshot(&home), before);

        std::fs::write(
            source.join(plugin::FILE),
            WITH_PARTS.replace("\"demo\"", "\"other\""),
        )
        .unwrap();
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update_typed("demo", &home, &mut asker),
            Err(Refusal::UpdateRenamed {
                name: "demo".to_string(),
                found: "other".to_string()
            })
        );
        assert!(asker.updates.is_empty());
        assert_eq!(snapshot(&home), before);

        std::fs::remove_file(record_path(&home, "demo")).unwrap();
        assert_eq!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Err(Refusal::RecordUnreadable {
                name: "demo".to_string()
            })
        );
    }

    fn git_record(parent: &Path, home: &Path, requested: Option<&str>, url: &str) -> String {
        let tree = plugin_dir(parent, "recorded", MANIFEST);
        let git = Source::Git {
            url: url.to_string(),
            requested_ref: requested.map(str::to_string),
            commit: Commit::parse("0123456789abcdef0123456789abcdef01234567").unwrap(),
        };
        commit_install(stage_directory(&tree, git, home).unwrap(), home).unwrap();
        "demo".to_string()
    }

    /// PLUGIN-9, PLUGIN-14: a source recorded as a full commit has nothing to update to. It is said
    /// before any host question, any program and any connection.
    ///
    /// Rejects: an update that fetches a commit again, one that puts the host question for a ref
    /// that cannot move, and one that treats a 39-digit or uppercase ref (a branch name that looks
    /// like a commit) as a commit.
    #[test]
    fn a_source_recorded_as_a_full_commit_has_nothing_to_update_to_and_asks_nothing() {
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let listener = Listener::start();
        let url = format!("https://127.0.0.1:{}/demo.git", listener.port);
        let commit = "fedcba9876543210fedcba9876543210fedcba98";
        git_record(scratch.path(), &home, Some(commit), &url);
        let before = snapshot(&home);
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update_with(
                "demo",
                &home,
                &mut asker,
                Some(git.clone()),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::NothingToUpdateTo {
                name: "demo".to_string()
            })
        );
        assert!(asker.hosts.is_empty() && asker.updates.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(listener.connections(), 0);
        assert_eq!(snapshot(&home), before);

        // A ref that only looks like a commit is a branch name and is fetched again. The yes lets
        // the request leave, which is the control for the zero above: the listener does count a
        // connection when one is made, so the zero is not a listener nobody could reach.
        std::fs::remove_dir_all(&home).unwrap();
        git_record(scratch.path(), &home, Some(&commit[..39]), &url);
        let before = snapshot(&home);
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update_with(
                "demo",
                &home,
                &mut asker,
                Some(git),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::CloneFailed)
        );
        assert_eq!(asker.hosts.len(), 1);
        assert!(listener.connections() > 0, "a yes did not reach the host");
        assert_eq!(snapshot(&home), before);
    }

    /// PLUGIN-3, PLUGIN-9: the fetch of an update puts the same host question an install does,
    /// with the host the recorded URL names, before any connection, and a no makes none, starts no
    /// program and leaves the state as it was. Nobody to ask and no `git` are refused before a
    /// question is put.
    ///
    /// Rejects: an update that connects before the question, one that ignores the no, one that
    /// puts a question for a different host than the record's, and one that stages with no `git`.
    #[test]
    fn an_update_puts_the_host_question_before_any_connection_and_a_no_makes_none() {
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let listener = Listener::start();
        let url = format!("https://127.0.0.1:{}/demo.git", listener.port);
        git_record(scratch.path(), &home, Some("main"), &url);
        let before = snapshot(&home);
        let mut no = Updating::new(false, true);
        assert_eq!(
            update_with(
                "demo",
                &home,
                &mut no,
                Some(git.clone()),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::Declined)
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(listener.connections(), 0, "a no made a connection");
        assert_eq!(
            no.hosts,
            vec![FetchRequest {
                url: url.clone(),
                host: "127.0.0.1".to_string(),
                redirected_from: None,
                classified: Classified::NotAsked,
            }]
        );
        assert!(no.updates.is_empty());
        assert_eq!(snapshot(&home), before);
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);

        let mut none = Updating::new(true, true);
        assert_eq!(
            update_with(
                "demo",
                &home,
                &mut none,
                None,
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::GitNotInstalled)
        );
        assert!(none.hosts.is_empty());
        let mut nobody = Updating::new(true, true);
        nobody.there = false;
        assert_eq!(
            update_with(
                "demo",
                &home,
                &mut nobody,
                Some(git.clone()),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::NobodyToAsk)
        );
        assert!(nobody.hosts.is_empty());

        // A yes lets the request leave (the control for the zero above).
        let mut yes = Updating::new(true, true);
        assert_eq!(
            update_with(
                "demo",
                &home,
                &mut yes,
                Some(git),
                &crate::plugin_git::PROTOCOLS,
                parent.path()
            ),
            Err(Refusal::CloneFailed)
        );
        assert!(listener.connections() > 0, "a yes did not reach the host");
        assert_eq!(snapshot(&home), before);
    }

    /// PLUGIN-9, PLUGIN-14: an update fetches the recorded ref again. A branch that moved to a
    /// commit with other files is put to the person with the new commit and a new digest, the old
    /// commit being the record's, and a no changes nothing. A branch that moved to a commit with
    /// the same files changes nothing and asks nothing. A tag that did not move is unchanged.
    ///
    /// Rejects: an update that fetches the default branch instead of the recorded ref, one that
    /// compares only the commit (it would ask about the same-files commit), one that compares
    /// only the digest and shows the old commit, and one that rewrites the record on either.
    #[test]
    fn an_update_fetches_the_recorded_ref_and_compares_the_commit_and_the_digest() {
        let git = crate::plugin_git::locate().expect("git must be on PATH for this test");
        let remote = crate::plugin_git::fixtures::remote();
        let parent = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source_at = |requested: &str| GitSource {
            url: remote.path.to_string_lossy().into_owned(),
            host: "local.test".to_string(),
            requested_ref: Some(requested.to_string()),
        };
        let update = |requested: &str, asker: &mut Updating| {
            let installed = read_record(&home, "demo").unwrap();
            update_fetching(
                &installed,
                Some(&source_at(requested)),
                &home,
                asker,
                Some(git.clone()),
                &["file"],
                parent.path(),
            )
        };
        let mut yes = Host::new(true, true, Box::new(|| 0));
        install_git(
            &source_at("main"),
            &home,
            &mut yes,
            Some(git.clone()),
            &["file"],
            parent.path(),
        )
        .unwrap();
        let installed = read_record(&home, "demo").unwrap();
        plant_answers(&home);
        let before = snapshot(&home);

        // Nothing moved.
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update("main", &mut asker),
            Ok(Updated::Unchanged {
                newer_commit: false
            })
        );
        assert_eq!(asker.hosts.len(), 1);
        assert!(asker.updates.is_empty());
        assert_eq!(snapshot(&home), before);

        // The branch moved to a commit holding the same files.
        crate::plugin_git::fixtures::git(
            &remote.path,
            &["commit", "-q", "--allow-empty", "-m", "same files"],
        );
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update("main", &mut asker),
            Ok(Updated::Unchanged { newer_commit: true })
        );
        assert!(asker.updates.is_empty());
        assert_eq!(snapshot(&home), before);

        // The branch moved to a commit holding other files.
        std::fs::write(remote.path.join("second.txt"), "changed\n").unwrap();
        crate::plugin_git::fixtures::git(&remote.path, &["add", "-A"]);
        crate::plugin_git::fixtures::git(&remote.path, &["commit", "-q", "-m", "changed"]);
        let newest = crate::plugin_git::fixtures::git(&remote.path, &["rev-parse", "HEAD"]);
        let mut asker = Updating::new(true, false);
        assert_eq!(update("main", &mut asker), Err(Refusal::Declined));
        assert_eq!(asker.updates.len(), 1);
        let (new_commit, new_digest, old_commit) = &asker.updates[0];
        assert_eq!(new_commit.as_deref(), Some(newest.as_str()));
        assert_ne!(*new_digest, installed.digest);
        assert_eq!(old_commit.as_deref(), Some(remote.second.as_str()));
        assert_eq!(snapshot(&home), before);

        // The ref of the record is the one fetched: `main` moved and `v1` did not, and the
        // tree of `v1` is another plugin tree than the one installed.
        let mut asker = Updating::new(true, false);
        assert_eq!(update("v1", &mut asker), Err(Refusal::Declined));
        assert_eq!(
            asker.updates[0].0.as_deref(),
            Some(remote.first.as_str()),
            "the asked ref is fetched, not the default branch"
        );
    }

    /// A plugin called `demo` at `version` offering the skill `skills` with front matter, one hook
    /// that runs `hook_word`, one server and an extra file `notes.txt` holding `notes`.
    fn updatable(parent: &Path, version: &str, hook_word: &str, notes: &str) -> PathBuf {
        let root = parent.join("updatable");
        std::fs::create_dir_all(root.join("skills").join("hello")).unwrap();
        std::fs::write(
            root.join(plugin::FILE),
            WITH_PARTS.replace("1.0.0", version),
        )
        .unwrap();
        std::fs::write(
            root.join("skills").join("hello").join("SKILL.md"),
            "---\nname: hello\ndescription: Says hello.\n---\n\nBody.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("hooks.json"),
            format!(
                r#"{{"hooks":[{{"on":"turn-started","run":["${{PLUGIN_ROOT}}/{hook_word}"]}}]}}"#
            ),
        )
        .unwrap();
        std::fs::write(root.join("servers.json"), SERVERS).unwrap();
        std::fs::write(root.join("notes.txt"), notes).unwrap();
        root
    }

    /// A line in each approval file for `plugin`, so a test can show which lines a withdrawal took.
    fn answer_lines(plugin: &str, tag: &str) -> (String, String) {
        let digest = bravebot_config::hooks_approved::Digest::of(plugin, tag, &[], "t");
        (
            format!("{digest} {plugin}\n"),
            format!("plugin {digest} {plugin} s\n"),
        )
    }

    /// Writes the lines of `demo` and of `other` the way a person's yes would have left them.
    fn plant_lines(home: &Path) -> (String, String) {
        let (demo_hook, demo_server) = answer_lines("demo", "turn-started");
        let (other_hook, other_server) = answer_lines("other", "turn-started");
        std::fs::create_dir_all(home).unwrap();
        let hooks = format!("{demo_hook}{other_hook}");
        let servers = format!("{demo_server}{other_server}");
        std::fs::write(home.join("hooks-approved"), &hooks).unwrap();
        std::fs::write(home.join("mcp-approved"), &servers).unwrap();
        (other_hook, other_server)
    }

    /// PLUGIN-9, PLUGIN-5, PLUGIN-7: a yes replaces the directory and the record with the new tree,
    /// keeps whether the plugin is enabled, takes the new version and digest, leaves the tree
    /// matching its record, withdraws every answer recorded for this plugin's hooks and servers and
    /// no other plugin's, and leaves no half-made directory. The question is given what the
    /// installed copy held, so what was removed can be listed.
    ///
    /// Rejects: an update that enables a disabled plugin, one that leaves the old version or digest
    /// in the record, one that leaves a tree the record no longer describes (every hook would be
    /// stopped as changed), one that withdraws nothing or another plugin's lines, one that leaves
    /// its working directories behind, and a question that is not given the installed copy.
    #[test]
    fn a_yes_replaces_the_plugin_keeps_it_disabled_and_withdraws_only_its_own_answers() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = updatable(scratch.path(), "1.0.0", "hook.sh", "one");
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        set_enabled(&home, "demo", false).unwrap();
        let old_digest = read_record(&home, "demo").unwrap().digest;
        let (other_hook, other_server) = plant_lines(&home);

        let source = updatable(scratch.path(), "1.1.0", "other.sh", "two");
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update_typed("demo", &home, &mut asker),
            Ok(Updated::Replaced)
        );
        assert_eq!(asker.updates.len(), 1);
        let before = &asker.befores[0];
        assert!(before.readable);
        assert_eq!(before.offers.len(), 1);
        assert!(matches!(before.hooks, Some(Ok(_))));

        let record = read_record(&home, "demo").unwrap();
        assert_eq!(record.version, "1.1.0");
        assert!(!record.enabled, "an update turned a disabled plugin on");
        assert_ne!(record.digest, old_digest);
        assert_eq!(
            record.source,
            Source::Local(std::fs::canonicalize(&source).unwrap())
        );
        assert_eq!(tree_matches_record(&home, "demo"), Ok(()));
        assert_eq!(
            std::fs::read_to_string(installed_dir(&home, "demo").join("notes.txt")).unwrap(),
            "two"
        );
        let hooks = installed_hooks(&home, "demo").unwrap().unwrap();
        assert!(hooks.hooks.declared()[0].run()[0].ends_with("/other.sh"));

        assert_eq!(
            std::fs::read_to_string(home.join("hooks-approved")).unwrap(),
            other_hook
        );
        assert_eq!(
            std::fs::read_to_string(home.join("mcp-approved")).unwrap(),
            other_server
        );
        let held: Vec<String> = std::fs::read_dir(plugins_dir(&home))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(held.len(), 2, "{held:?}");
        assert!(held.contains(&"demo".to_string()) && held.contains(&"demo.record".to_string()));

        // An enabled plugin stays enabled.
        set_enabled(&home, "demo", true).unwrap();
        updatable(scratch.path(), "1.2.0", "third.sh", "three");
        assert_eq!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Ok(Updated::Replaced)
        );
        assert!(read_record(&home, "demo").unwrap().enabled);
    }

    /// PLUGIN-9, PLUGIN-12: a changed plugin that would offer a skill another installed plugin
    /// already offers is refused naming both plugins, before the question and with nothing changed.
    /// The plugin's own installed offers are not counted against it, so the update that keeps
    /// `hello` goes through (the control).
    ///
    /// Rejects: an update that replaces anyway, one that asks first, one that names only one plugin,
    /// and one that counts the plugin against its own old copy.
    #[test]
    fn an_update_offering_a_name_another_plugin_offers_is_refused_naming_both() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = updatable(scratch.path(), "1.0.0", "hook.sh", "one");
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        install(&offering(scratch.path(), "other", "review", None), &home).unwrap();
        plant_lines(&home);
        let before = snapshot(&home);

        std::fs::create_dir_all(source.join("skills").join("review")).unwrap();
        std::fs::write(
            source.join("skills").join("review").join("SKILL.md"),
            "---\nname: review\ndescription: Reviews.\n---\n\nBody.\n",
        )
        .unwrap();
        let mut asker = Updating::new(true, true);
        assert_eq!(
            update_typed("demo", &home, &mut asker),
            Err(Refusal::NameOffered {
                offered: "review".to_string(),
                new_plugin: "demo".to_string(),
                existing_plugin: "other".to_string(),
            })
        );
        assert!(
            asker.updates.is_empty(),
            "the person was asked about a refused update"
        );
        assert_eq!(snapshot(&home), before);

        // The control: without the colliding skill the same source updates.
        std::fs::remove_dir_all(source.join("skills").join("review")).unwrap();
        std::fs::write(source.join("notes.txt"), "changed").unwrap();
        assert_eq!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Ok(Updated::Replaced)
        );
    }

    /// PLUGIN-9: nothing is replaced where the replacement cannot be completed. A withdrawal that
    /// cannot be written stops the update before anything is replaced, and a record that cannot be
    /// written puts the old directory back. Either way the old directory, the old record and the
    /// old version are in place and the old tree still matches its record, and no working
    /// directory is left.
    ///
    /// Rejects: an update that moves the new tree in before the withdrawal, one that leaves the new
    /// directory with the old record (every hook would be stopped as changed), one that loses the
    /// old directory when the record write fails, and one that leaves its working directories.
    #[test]
    fn an_update_that_cannot_be_completed_leaves_the_old_plugin_working() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = updatable(scratch.path(), "1.0.0", "hook.sh", "one");
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        let record_before = std::fs::read(record_path(&home, "demo")).unwrap();
        let tree_before = snapshot(&installed_dir(&home, "demo"));
        updatable(scratch.path(), "1.1.0", "other.sh", "two");
        let intact = |home: &Path| {
            assert_eq!(
                std::fs::read(record_path(home, "demo")).unwrap(),
                record_before
            );
            assert_eq!(snapshot(&installed_dir(home, "demo")), tree_before);
            assert_eq!(tree_matches_record(home, "demo"), Ok(()));
            let leftovers: Vec<String> = std::fs::read_dir(plugins_dir(home))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .filter(|name| name.starts_with('.') && !name.ends_with(".tmp"))
                .collect();
            assert!(leftovers.is_empty(), "{leftovers:?}");
        };

        // A hooks-approved that is a directory cannot be read or rewritten, so the withdrawal fails.
        std::fs::create_dir(home.join("hooks-approved")).unwrap();
        assert!(matches!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Err(Refusal::Io(_))
        ));
        intact(&home);
        std::fs::remove_dir(home.join("hooks-approved")).unwrap();

        // A record that cannot be written: the temporary name it is written through is a directory.
        std::fs::create_dir(plugins_dir(&home).join(".demo.record.tmp")).unwrap();
        assert!(matches!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Err(Refusal::Io(_))
        ));
        intact(&home);
        assert_eq!(read_record(&home, "demo").unwrap().version, "1.0.0");

        // A no changes nothing either, and the same source updates once the obstacle is gone.
        std::fs::remove_dir(plugins_dir(&home).join(".demo.record.tmp")).unwrap();
        assert_eq!(
            update_typed("demo", &home, &mut Updating::new(true, false)),
            Err(Refusal::Declined)
        );
        intact(&home);
        assert_eq!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Ok(Updated::Replaced)
        );
        assert_eq!(read_record(&home, "demo").unwrap().version, "1.1.0");
    }

    /// PLUGIN-9, PLUGIN-14: a git record whose URL no longer reads as a git source (here a plain web
    /// address and a bare word, which reads as a local path) is refused as an unreadable record,
    /// before any question and with nothing changed.
    ///
    /// Rejects: an update that says the URL is not a source in words about archives or web
    /// addresses, one that stages the bare word as a local directory, and one that asks first.
    #[test]
    fn a_git_record_whose_url_no_longer_reads_as_a_git_source_is_unreadable() {
        for url in ["https://example.com/demo", "demo"] {
            let scratch = tempfile::tempdir().unwrap();
            let home = scratch.path().join("home");
            git_record(scratch.path(), &home, Some("main"), url);
            let before = snapshot(&home);
            let mut asker = Updating::new(true, true);
            assert_eq!(
                update_typed("demo", &home, &mut asker),
                Err(Refusal::RecordUnreadable {
                    name: "demo".to_string()
                }),
                "{url}"
            );
            assert!(asker.hosts.is_empty() && asker.updates.is_empty(), "{url}");
            assert_eq!(snapshot(&home), before, "{url}");
        }
    }

    /// PLUGIN-9, Known costs: a crash between the two renames of an earlier update left the old copy
    /// under `.<name>.previous` and no directory. The next update puts it back before it begins, so
    /// an update that then fails leaves the old plugin in place and one that goes through replaces
    /// it.
    ///
    /// Rejects: an update that deletes `.previous` without putting it back, which leaves no plugin
    /// directory after a failure, and one that fails because the directory is missing.
    #[test]
    fn an_update_after_a_crash_between_its_renames_puts_the_old_copy_back_first() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = updatable(scratch.path(), "1.0.0", "hook.sh", "one");
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        let tree_before = snapshot(&installed_dir(&home, "demo"));
        let previous = plugins_dir(&home).join(".demo.previous");
        let crash = || std::fs::rename(installed_dir(&home, "demo"), &previous).unwrap();
        updatable(scratch.path(), "1.1.0", "other.sh", "two");

        // The withdrawal cannot be written, so the update stops after the old copy was put back.
        crash();
        std::fs::create_dir(home.join("hooks-approved")).unwrap();
        assert!(matches!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Err(Refusal::Io(_))
        ));
        assert!(!is_present(&previous), "the old copy was left aside");
        assert_eq!(snapshot(&installed_dir(&home, "demo")), tree_before);
        assert_eq!(tree_matches_record(&home, "demo"), Ok(()));
        std::fs::remove_dir(home.join("hooks-approved")).unwrap();

        crash();
        assert_eq!(
            update_typed("demo", &home, &mut Updating::new(true, true)),
            Ok(Updated::Replaced)
        );
        assert!(!is_present(&previous));
        assert_eq!(read_record(&home, "demo").unwrap().version, "1.1.0");
        assert_eq!(tree_matches_record(&home, "demo"), Ok(()));
    }

    /// PLUGIN-9: where the staged tree cannot be renamed to the working directory, it is copied
    /// there and the update goes through. A directory is renamed to another parent only by someone
    /// who can write to it, so a read-only staged tree stands for any rename that fails while the
    /// tree can still be read. A process that ignores file modes (root) renames it anyway and this
    /// then shows only that the update goes through.
    ///
    /// Rejects: an update that is refused because the rename failed, and one that leaves its
    /// working directory behind.
    #[cfg(unix)]
    #[test]
    fn a_staged_tree_that_cannot_be_renamed_is_copied_into_place() {
        use std::os::unix::fs::PermissionsExt as _;
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = updatable(scratch.path(), "1.0.0", "hook.sh", "one");
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();
        let installed = read_record(&home, "demo").unwrap();
        updatable(scratch.path(), "1.1.0", "other.sh", "two");
        let staged = stage(&source, &home).unwrap();
        std::fs::set_permissions(&staged.tree, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = commit_update(&staged, &installed, &home);
        std::fs::set_permissions(&staged.tree, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(result, Ok(()));
        let record = read_record(&home, "demo").unwrap();
        assert_eq!(record.version, "1.1.0");
        assert_eq!(record.digest, staged.digest);
        assert_eq!(tree_matches_record(&home, "demo"), Ok(()));
        assert_eq!(
            std::fs::read_to_string(installed_dir(&home, "demo").join("notes.txt")).unwrap(),
            "two"
        );
        assert!(!is_present(&plugins_dir(&home).join(".demo.updating")));
    }

    /// PLUGIN-9: the files that differ list a file only in the installed copy as removed, a file
    /// only in the staged tree as added, one with other bytes as changed, and a directory by its name
    /// with a trailing `/`, in the order of the path bytes. Files that are the same are not listed.
    ///
    /// Rejects: a listing that leaves out what the update deletes, one that lists a directory
    /// without the `/` or not at all, and one that lists a file whose bytes are the same.
    #[test]
    fn the_files_that_differ_list_removed_files_and_directories_as_well_as_added_ones() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = updatable(scratch.path(), "1.0.0", "hook.sh", "one");
        std::fs::create_dir(source.join("docs")).unwrap();
        std::fs::write(source.join("docs").join("a.txt"), "a").unwrap();
        std::fs::write(source.join("old.txt"), "old").unwrap();
        commit_install(stage(&source, &home).unwrap(), &home).unwrap();

        std::fs::remove_dir_all(source.join("docs")).unwrap();
        std::fs::remove_file(source.join("old.txt")).unwrap();
        std::fs::create_dir(source.join("more")).unwrap();
        std::fs::write(source.join("more").join("b.txt"), "b").unwrap();
        std::fs::write(source.join("new.txt"), "new").unwrap();
        std::fs::write(source.join("notes.txt"), "two").unwrap();
        let staged = stage(&source, &home).unwrap();
        let changes = file_changes(&staged, &before_of(&home, "demo")).unwrap();
        let shown: Vec<(String, Change)> = changes
            .iter()
            .map(|file| (String::from_utf8(file.path.clone()).unwrap(), file.change))
            .collect();
        assert_eq!(
            shown,
            vec![
                ("docs/".to_string(), Change::Removed),
                ("docs/a.txt".to_string(), Change::Removed),
                ("more/".to_string(), Change::Added),
                ("more/b.txt".to_string(), Change::Added),
                ("new.txt".to_string(), Change::Added),
                ("notes.txt".to_string(), Change::Changed),
                ("old.txt".to_string(), Change::Removed),
            ]
        );
    }

    /// PLUGIN-9, PATH-001, PATH-003: two files whose names differ only where a text rendering would
    /// make them equal (`f-\xff` and `f-\u{FFFD}`) are two entries of the listing's file map, so a
    /// change to one of them is listed with its own bytes. The names are built in memory because
    /// some file systems cannot hold the first.
    ///
    /// Rejects: a file map keyed on a lossy rendering of the name, which holds one entry for both
    /// and can drop a changed file from the listing.
    #[test]
    fn names_that_differ_only_in_invalid_bytes_stay_two_paths_in_the_file_map() {
        let scratch = tempfile::tempdir().unwrap();
        let file = |name: &str, text: &str| {
            let path = scratch.path().join(name);
            std::fs::write(&path, text).unwrap();
            path
        };
        let invalid = b"f-\xff".to_vec();
        let lookalike = "f-\u{FFFD}".as_bytes().to_vec();
        let (one, two) = (file("one", "1"), file("two", "2"));
        let old = digest_found(vec![
            (invalid.clone(), one, false),
            (lookalike.clone(), two.clone(), false),
        ])
        .unwrap();
        assert_eq!(old.len(), 2);
        let changed = file("changed", "11");
        let new = digest_found(vec![
            (invalid.clone(), changed, false),
            (lookalike, two, false),
        ])
        .unwrap();
        assert_eq!(
            changes_between(&old, &new),
            vec![FileChange {
                path: invalid,
                change: Change::Changed
            }]
        );
    }
}
