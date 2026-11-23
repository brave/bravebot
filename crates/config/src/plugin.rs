//! The manifest a plugin carries, `plugin.toml` (PLUGIN-2).
//!
//! A plugin's manifest is untrusted bytes. This reader parses them as data and decides one thing
//! from them: whether to refuse. It never opens a directory the manifest names, never follows a
//! path, and gives back the names and paths as text to be carried to the person's prompt and to
//! the copy. What a part path reaches on disk is checked by the installer, not here.
//!
//! [`read_hooks`] and [`read_servers`] are the two places a part file is opened. Each opens the one
//! file the manifest named, without following a link, and parses it as data with the reader the
//! person's own `hooks.json` and `mcp.json` use. They decide nothing from what they read: what
//! they could not use is reported so a listing can say so, and `${PLUGIN_ROOT}` is replaced in
//! the words of an argv and nowhere else ([`expand_root`]).
//!
//! The format is the small subset of TOML a manifest needs: `key = "value"` lines, blank lines and
//! `#` comments. Anything else is refused with the line it is on, so a manifest means the same here
//! as in any TOML reader or is not read.
//!
//! Adding a refusal means adding a variant to [`Refusal`] and an arm to [`Refusal::key`]. The keys
//! a manifest may carry are [`Field`]; a key this build does not read is classified by
//! [`classify`], which is where a key that must refuse the manifest whole is told from one that is
//! only reported.

use crate::hooks::{Hooks, Unkept};
use crate::mcp::{Declarations, Entry as ServerEntry};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The manifest's file name, the only manifest read.
pub const FILE: &str = "plugin.toml";

/// The most bytes a manifest may hold. A longer one is refused unread.
pub const LARGEST: u64 = 64 * 1024;

/// The most characters a plugin's name may hold: it is a directory name under the state directory.
pub const LONGEST_NAME: usize = 64;

/// One of the four kinds of part a plugin may name, in the order the manifest spec lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A directory of skills.
    Skills,
    /// A directory of delegate definitions.
    Agents,
    /// One file of hooks.
    Hooks,
    /// One file of server declarations.
    Servers,
}

impl Part {
    /// Every kind of part.
    pub const ALL: [Self; 4] = [Self::Skills, Self::Agents, Self::Hooks, Self::Servers];

    /// The manifest key that names this part.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Skills => "skills",
            Self::Agents => "agents",
            Self::Hooks => "hooks",
            Self::Servers => "servers",
        }
    }

    /// Whether the path names a directory (`skills`, `agents`) or one file (`hooks`, `servers`).
    pub const fn is_directory(self) -> bool {
        matches!(self, Self::Skills | Self::Agents)
    }
}

/// A key this build reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Version,
    Description,
    Part(Part),
}

impl Field {
    /// The key as the manifest writes it.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Version => "version",
            Self::Description => "description",
            Self::Part(part) => part.key(),
        }
    }
}

/// What a key is to the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A key this build reads.
    Read(Field),
    /// A key this build does not read: reported, with its line, and ignored.
    Unread,
    /// A key that names a decision (PLUGIN-6): the manifest is refused whole.
    Decision,
}

/// The keys that name a decision a plugin may not carry: a permission rule, an allow, ask or deny
/// entry, a grant, a trusted path, a mode, a model, an endpoint or a setting (PLUGIN-6).
///
/// A closed list written in the program. Each entry is compared with a key after [`fold_key`], so
/// every entry here is lowercase with no `-` or `_`, and `trustedPaths`, `trusted_paths` and
/// `trusted-paths` are one entry. A key not in the list is [`Class::Unread`], not a decision.
const DECISION_KEYS: [&str; 30] = [
    "permission",
    "permissions",
    "rule",
    "rules",
    "allow",
    "allowed",
    "allowlist",
    "ask",
    "deny",
    "denied",
    "denylist",
    "grant",
    "grants",
    "trust",
    "trusted",
    "trustedpath",
    "trustedpaths",
    "trustedroots",
    "mode",
    "modes",
    "defaultmode",
    "permissionmode",
    "skippermissions",
    "model",
    "models",
    "endpoint",
    "endpoints",
    "baseurl",
    "setting",
    "settings",
];

/// A key with its case and any `-` or `_` removed, which is how [`DECISION_KEYS`] is compared.
fn fold_key(key: &str) -> String {
    key.chars()
        .filter(|c| *c != '-' && *c != '_')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Whether `key` names a decision a plugin may not carry. Exact match against the closed list.
pub fn is_decision_key(key: &str) -> bool {
    let folded = fold_key(key);
    DECISION_KEYS.contains(&folded.as_str())
}

/// A path the manifest gave for a part: relative, with no `..` and no backslash, as written.
///
/// Checked as text only. Whether what it names exists, or is a link that leaves the plugin, is for
/// whoever opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartPath(String);

impl PartPath {
    /// The path as the manifest wrote it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path under a plugin's root.
    pub fn under(&self, root: &Path) -> PathBuf {
        self.0
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .fold(root.to_path_buf(), |path, segment| path.join(segment))
    }
}

/// What is wrong with the text of a manifest, on one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// A line that is not blank, a comment or `key = value`.
    NotKeyValue,
    /// A key that is not letters, digits, `_` and `-`, as written.
    BadKey(String),
    /// A table header, `[name]`, which this reader does not read. The header as written.
    Table(String),
    /// A value that is not a single-line string in double or single quotes.
    NotAString,
    /// A string with no closing quote on its line.
    UnterminatedString,
    /// Text after the closing quote that is not a comment.
    TextAfterValue,
    /// A backslash escape that is not one TOML defines.
    BadEscape,
    /// A control character inside a string.
    ControlCharacter,
    /// A key the manifest already gave. The key as written.
    DuplicateKey(String),
}

/// Why a manifest was refused. Nothing is installed from a refused manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The directory holds no `plugin.toml`.
    NoManifest,
    /// The directory holds no `plugin.toml` and holds another tool's manifest (PLUGIN-2). The
    /// missing file is [`FILE`]. Which other manifest was found.
    NoPluginToml(OtherManifest),
    /// The file could not be read.
    Unreadable,
    /// The file is longer than [`LARGEST`].
    TooLarge,
    /// The file is not UTF-8 text.
    NotText,
    /// A line is not in the subset this reader reads.
    Syntax { line: usize, fault: Fault },
    /// A key that names a decision (PLUGIN-6), written as a key or as a table header. The
    /// manifest is refused whole. The key as written.
    DecisionKey { line: usize, key: String },
    /// A key every manifest needs is absent.
    Missing(Field),
    /// A key every manifest needs is an empty string.
    Empty(Field),
    /// `name` is not letters, digits and hyphens, or is longer than [`LONGEST_NAME`].
    BadName,
    /// A part path is absolute.
    AbsolutePath(Part),
    /// A part path holds a `..` segment.
    ParentPath(Part),
    /// A part path is empty, holds a backslash or holds a control character.
    UnusablePath(Part),
    /// A link at a part path, on the way to it or inside a part directory has a target that is
    /// outside the plugin's root or cannot be resolved (PLUGIN-2). The part's key is named.
    LinkLeavesRoot(Part),
}

impl Refusal {
    /// The manifest key the refusal is about, where it is one this build reads.
    pub fn key(&self) -> Option<Field> {
        match self {
            Self::Missing(field) | Self::Empty(field) => Some(*field),
            Self::BadName => Some(Field::Name),
            Self::AbsolutePath(part)
            | Self::ParentPath(part)
            | Self::UnusablePath(part)
            | Self::LinkLeavesRoot(part) => Some(Field::Part(*part)),
            Self::NoManifest
            | Self::NoPluginToml(_)
            | Self::Unreadable
            | Self::TooLarge
            | Self::NotText
            | Self::DecisionKey { .. }
            | Self::Syntax { .. } => None,
        }
    }

    /// The key as written that a message names: a decision key as the manifest wrote it, or the
    /// key of a field this build reads.
    pub fn named_key(&self) -> Option<String> {
        match self {
            Self::DecisionKey { key, .. } => Some(key.clone()),
            other => other.key().map(|field| field.key().to_string()),
        }
    }
}

/// Another tool's plugin manifest, as found beside a missing `plugin.toml` (PLUGIN-2). Only the
/// presence of the file is looked at, never its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherManifest {
    /// `.claude-plugin/plugin.json`.
    ClaudePlugin,
    /// `.cursor-plugin/plugin.json`.
    CursorPlugin,
    /// `.codex-plugin/plugin.json`.
    CodexPlugin,
    /// `gemini-extension.json`.
    GeminiExtension,
    /// `plugin.json` at the root.
    PluginJson,
}

impl OtherManifest {
    /// Every manifest of another tool this build names, a closed list written in the program.
    pub const ALL: [Self; 5] = [
        Self::ClaudePlugin,
        Self::CursorPlugin,
        Self::CodexPlugin,
        Self::GeminiExtension,
        Self::PluginJson,
    ];

    /// Where the manifest sits below a plugin's root, `/`-separated.
    pub const fn path(self) -> &'static str {
        match self {
            Self::ClaudePlugin => ".claude-plugin/plugin.json",
            Self::CursorPlugin => ".cursor-plugin/plugin.json",
            Self::CodexPlugin => ".codex-plugin/plugin.json",
            Self::GeminiExtension => "gemini-extension.json",
            Self::PluginJson => "plugin.json",
        }
    }

    /// The first of [`Self::ALL`] present below `root`, by existence alone: nothing is opened,
    /// read or followed.
    pub fn found_in(root: &Path) -> Option<Self> {
        Self::ALL.into_iter().find(|other| {
            let path = other
                .path()
                .split('/')
                .fold(root.to_path_buf(), |p, s| p.join(s));
            std::fs::symlink_metadata(path).is_ok()
        })
    }
}

/// A key the manifest carried that this build does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    /// The line it is on, from 1.
    pub line: usize,
    /// The key as written.
    pub key: String,
}

/// A parsed manifest. Every text field is untrusted: carry it, draw it as untrusted content, and
/// compare it only to refuse or to mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Letters, digits and hyphens: the directory name under `~/.bravebot/plugins`.
    pub name: String,
    pub version: String,
    /// What the plugin says about itself. Never read into a turn (PLUGIN-10).
    pub description: String,
    pub skills: Option<PartPath>,
    pub agents: Option<PartPath>,
    pub hooks: Option<PartPath>,
    pub servers: Option<PartPath>,
    /// Keys this build does not read, in the order written.
    pub unread: Vec<Unread>,
}

impl Manifest {
    /// The path the manifest gave for a part, if it named one.
    pub fn part(&self, part: Part) -> Option<&PartPath> {
        match part {
            Part::Skills => self.skills.as_ref(),
            Part::Agents => self.agents.as_ref(),
            Part::Hooks => self.hooks.as_ref(),
            Part::Servers => self.servers.as_ref(),
        }
    }

    /// The parts the manifest named, in [`Part::ALL`] order.
    pub fn named_parts(&self) -> Vec<(Part, &PartPath)> {
        Part::ALL
            .into_iter()
            .filter_map(|part| self.part(part).map(|path| (part, path)))
            .collect()
    }

    /// The hooks the manifest names, read from `tree` and with `${PLUGIN_ROOT}` replaced by
    /// `install_dir` ([`read_hooks`]). `None` where the manifest names no hooks file.
    pub fn hooks_list(
        &self,
        tree: &Path,
        install_dir: &Path,
    ) -> Option<Result<HookList, FileFault>> {
        self.hooks
            .as_ref()
            .map(|path| read_hooks(tree, path, install_dir))
    }

    /// The servers the manifest names, read from `tree` and with `${PLUGIN_ROOT}` replaced by
    /// `install_dir` ([`read_servers`]). `None` where the manifest names no servers file.
    pub fn servers_list(
        &self,
        tree: &Path,
        install_dir: &Path,
    ) -> Option<Result<ServerList, FileFault>> {
        self.servers
            .as_ref()
            .map(|path| read_servers(tree, path, install_dir))
    }
}

/// One `key = "value"` line, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The line it is on, from 1.
    pub line: usize,
    pub key: String,
    pub value: String,
}

/// Split a manifest into its entries.
///
/// Blank lines and `#` comments are skipped. A line that is anything else is refused with its
/// number, and so is a key written twice.
pub fn entries(text: &str) -> Result<Vec<Entry>, Refusal> {
    let mut found: Vec<Entry> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let at = |fault| Refusal::Syntax { line, fault };
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            let name = trimmed.trim_matches(['[', ']', ' ', '\t']);
            if is_decision_key(table_head(name)) {
                return Err(Refusal::DecisionKey {
                    line,
                    key: name.to_string(),
                });
            }
            return Err(at(Fault::Table(trimmed.to_string())));
        }
        let Some((key, rest)) = trimmed.split_once('=') else {
            return Err(at(Fault::NotKeyValue));
        };
        let key = key.trim();
        // A decision key refuses the manifest whole before anything else on its line is read, so
        // `permissions = ["x"]` is refused naming the key and not as a value this reader cannot read.
        if is_decision_key(table_head(key)) {
            return Err(Refusal::DecisionKey {
                line,
                key: key.to_string(),
            });
        }
        if key.is_empty() || !key.bytes().all(is_key_byte) {
            return Err(at(Fault::BadKey(key.to_string())));
        }
        if !seen.insert(key) {
            return Err(at(Fault::DuplicateKey(key.to_string())));
        }
        let value = string_value(rest.trim_start()).map_err(at)?;
        found.push(Entry {
            line,
            key: key.to_string(),
            value,
        });
    }
    Ok(found)
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// The first segment of a dotted key or table name, so `permissions.allow` is tested as
/// `permissions`. Quotes around the segment are dropped.
fn table_head(name: &str) -> &str {
    name.split('.')
        .next()
        .unwrap_or_default()
        .trim_matches(['"', '\'', ' ', '\t'])
}

/// The string a value is, and nothing after it but a comment.
///
/// A basic string (`"..."`) with the escapes TOML defines, or a literal string (`'...'`) with
/// none. A multi-line string, a number, an array and a table are not read.
fn string_value(rest: &str) -> Result<String, Fault> {
    let mut chars = rest.char_indices();
    let Some((_, quote @ ('"' | '\''))) = chars.next() else {
        return Err(Fault::NotAString);
    };
    if rest.starts_with("\"\"\"") || rest.starts_with("'''") {
        return Err(Fault::NotAString);
    }
    let mut value = String::new();
    let mut end = None;
    while let Some((at, c)) = chars.next() {
        if c == quote {
            end = Some(at + c.len_utf8());
            break;
        }
        if quote == '"' && c == '\\' {
            value.push(escape(&mut chars)?);
        } else {
            value.push(c);
        }
    }
    let Some(end) = end else {
        return Err(Fault::UnterminatedString);
    };
    if value.chars().any(char::is_control) {
        return Err(Fault::ControlCharacter);
    }
    let after = rest.get(end..).unwrap_or_default().trim();
    if !after.is_empty() && !after.starts_with('#') {
        return Err(Fault::TextAfterValue);
    }
    Ok(value)
}

/// The character after a backslash in a basic string.
fn escape(chars: &mut std::str::CharIndices<'_>) -> Result<char, Fault> {
    let digits = |chars: &mut std::str::CharIndices<'_>, count: usize| {
        let mut code = 0u32;
        for _ in 0..count {
            let (_, c) = chars.next().ok_or(Fault::BadEscape)?;
            code = code * 16 + c.to_digit(16).ok_or(Fault::BadEscape)?;
        }
        char::from_u32(code).ok_or(Fault::BadEscape)
    };
    match chars.next().map(|(_, c)| c) {
        Some('"') => Ok('"'),
        Some('\\') => Ok('\\'),
        Some('b') => Ok('\u{8}'),
        Some('t') => Ok('\t'),
        Some('n') => Ok('\n'),
        Some('f') => Ok('\u{c}'),
        Some('r') => Ok('\r'),
        Some('u') => digits(chars, 4),
        Some('U') => digits(chars, 8),
        _ => Err(Fault::BadEscape),
    }
}

/// How the reader treats a key. The one place a key is sorted: a key that must refuse the manifest
/// whole is a new [`Class`] variant here, and the match in [`parse`] then has to handle it.
pub fn classify(key: &str) -> Class {
    match key {
        "name" => Class::Read(Field::Name),
        "version" => Class::Read(Field::Version),
        "description" => Class::Read(Field::Description),
        "skills" => Class::Read(Field::Part(Part::Skills)),
        "agents" => Class::Read(Field::Part(Part::Agents)),
        "hooks" => Class::Read(Field::Part(Part::Hooks)),
        "servers" => Class::Read(Field::Part(Part::Servers)),
        _ if is_decision_key(key) => Class::Decision,
        _ => Class::Unread,
    }
}

/// Whether `name` may be a plugin's name: ASCII letters, digits and hyphens, at least one and at
/// most [`LONGEST_NAME`]. It becomes a directory name, so nothing else is allowed through.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= LONGEST_NAME
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Check a part path as text: relative, inside the plugin, naming something below the root.
///
/// An absolute path is a leading `/` or `\`, or a drive prefix such as `C:`. Both `/` and `\` split
/// segments, so `a\..\b` is a `..` path on a host where `\` separates. A path of nothing but `.`
/// and empty segments names the root itself and is unusable: a part names what is under it.
pub fn check_part_path(part: Part, written: &str) -> Result<PartPath, Refusal> {
    let bytes = written.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if written.starts_with('/') || written.starts_with('\\') || drive {
        return Err(Refusal::AbsolutePath(part));
    }
    let segments = || written.split(['/', '\\']);
    if segments().any(|segment| segment == "..") {
        return Err(Refusal::ParentPath(part));
    }
    let names_something = segments().any(|segment| !segment.is_empty() && segment != ".");
    if !names_something || written.contains('\\') || written.chars().any(char::is_control) {
        return Err(Refusal::UnusablePath(part));
    }
    Ok(PartPath(written.to_string()))
}

/// Refuse a plugin whose named parts reach outside its root through a link (PLUGIN-2).
///
/// For each part, in the order given: every component of the part path below `root`, then the part
/// path itself, and for a directory part every entry inside it at any depth, is looked at without
/// following it. A link whose target resolves outside the canonical `root`, or does not resolve,
/// is [`Refusal::LinkLeavesRoot`] naming the part's key. Nothing is read from a file and no link
/// is followed to find more entries, so a link that stays inside the root is not descended into
/// here and the copy must not follow it either (it copies regular files and directories only).
/// A part path that names nothing on disk is not a link problem and is left to the installer.
pub fn check_links(root: &Path, parts: &[(Part, &PartPath)]) -> Result<(), Refusal> {
    let inside = std::fs::canonicalize(root).map_err(|_| Refusal::Unreadable)?;
    let stays = |link: &Path| match std::fs::canonicalize(link) {
        Ok(target) => target.starts_with(&inside),
        Err(_) => false,
    };
    'parts: for (part, part_path) in parts {
        let leaves = Refusal::LinkLeavesRoot(*part);
        let mut at = root.to_path_buf();
        for segment in part_path
            .as_str()
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != ".")
        {
            at.push(segment);
            let Ok(metadata) = std::fs::symlink_metadata(&at) else {
                // Nothing here, or nothing that can be looked at: not a link.
                continue 'parts;
            };
            if metadata.file_type().is_symlink() && !stays(&at) {
                return Err(leaves);
            }
        }
        let is_real_directory = std::fs::symlink_metadata(&at)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        if !part.is_directory() || !is_real_directory {
            continue;
        }
        let mut pending = vec![at];
        while let Some(directory) = pending.pop() {
            let listing = std::fs::read_dir(&directory).map_err(|_| Refusal::Unreadable)?;
            for entry in listing {
                let entry = entry.map_err(|_| Refusal::Unreadable)?;
                let kind = entry.file_type().map_err(|_| Refusal::Unreadable)?;
                if kind.is_symlink() {
                    if !stays(&entry.path()) {
                        return Err(leaves);
                    }
                } else if kind.is_dir() {
                    pending.push(entry.path());
                }
            }
        }
    }
    Ok(())
}

/// Read a manifest from its text.
///
/// Refuses the first fault in line order for a syntax fault and in key order after that. A key this
/// build does not read is listed in [`Manifest::unread`] and ignored.
pub fn parse(text: &str) -> Result<Manifest, Refusal> {
    let mut values: HashMap<Field, String> = HashMap::new();
    let mut unread = Vec::new();
    for entry in entries(text)? {
        match classify(&entry.key) {
            Class::Read(field) => {
                values.insert(field, entry.value);
            }
            Class::Decision => {
                return Err(Refusal::DecisionKey {
                    line: entry.line,
                    key: entry.key,
                });
            }
            Class::Unread => unread.push(Unread {
                line: entry.line,
                key: entry.key,
            }),
        }
    }
    let mut required = |field: Field| values.remove(&field).ok_or(Refusal::Missing(field));
    let name = required(Field::Name)?;
    let version = required(Field::Version)?;
    let description = required(Field::Description)?;
    if !is_valid_name(&name) {
        return Err(Refusal::BadName);
    }
    if version.trim().is_empty() {
        return Err(Refusal::Empty(Field::Version));
    }
    let mut path = |part: Part| {
        values
            .remove(&Field::Part(part))
            .map(|written| check_part_path(part, &written))
            .transpose()
    };
    let skills = path(Part::Skills)?;
    let agents = path(Part::Agents)?;
    let hooks = path(Part::Hooks)?;
    let servers = path(Part::Servers)?;
    Ok(Manifest {
        name,
        version,
        description,
        skills,
        agents,
        hooks,
        servers,
        unread,
    })
}

/// Read `plugin.toml` from a plugin's root.
///
/// Opens that one file and nothing else: no other file in the directory is read, listed or
/// followed. A `plugin.toml` that is a link or is not a regular file is [`Refusal::Unreadable`].
pub fn read(root: &Path) -> Result<Manifest, Refusal> {
    let file = root.join(FILE);
    let metadata = match std::fs::symlink_metadata(&file) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(match OtherManifest::found_in(root) {
                Some(other) => Refusal::NoPluginToml(other),
                None => Refusal::NoManifest,
            });
        }
        Err(_) => return Err(Refusal::Unreadable),
    };
    if !metadata.is_file() {
        return Err(Refusal::Unreadable);
    }
    if metadata.len() > LARGEST {
        return Err(Refusal::TooLarge);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&file)
        .and_then(|handle| handle.take(LARGEST + 1).read_to_end(&mut bytes))
        .map_err(|_| Refusal::Unreadable)?;
    if bytes.len() as u64 > LARGEST {
        return Err(Refusal::TooLarge);
    }
    let text = String::from_utf8(bytes).map_err(|_| Refusal::NotText)?;
    parse(&text)
}

/// The text a hook's argv word or a server's command word may hold that names the plugin's own
/// directory (PLUGIN-13).
pub const ROOT_VARIABLE: &str = "${PLUGIN_ROOT}";

/// The most bytes a hooks file or a servers file may hold, the size each is held to in the state
/// directory. A longer one is a fault, not read.
pub const PART_FILE_LARGEST: u64 = 64 * 1024;

/// `word` with every [`ROOT_VARIABLE`] in it replaced by `install_dir`, the absolute path of
/// `<state>/plugins/<name>` as text (PLUGIN-13).
///
/// One pass over the word: the replacement is not searched again, nothing else is replaced, and no
/// environment is read. A word is expanded only by a caller that holds an argv word of a hook or a
/// local server; every other string of a part is left as written.
pub fn expand_root(word: &str, install_dir: &str) -> String {
    word.replace(ROOT_VARIABLE, install_dir)
}

/// Why a hooks file or a servers file could not be read for a listing. The file's own words are
/// never carried here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFault {
    /// The path names something that is not there, or could not be looked at.
    Unreadable,
    /// The path names a link, a directory or another kind of entry, not a regular file.
    NotARegularFile,
    /// The file is longer than [`PART_FILE_LARGEST`].
    TooLarge,
    /// The file is not UTF-8 text.
    NotText,
    /// The text is not an object with a `hooks` array.
    NotHooks,
    /// The text is not an object whose only key is `servers`, itself an object.
    NotServers,
    /// The installed directory's path is not text, so `${PLUGIN_ROOT}` has nothing to become.
    PathNotText,
}

/// The one file of a part, opened without following a link and read whole as text.
///
/// The manifest reader and the staging link check already refuse a link on the way to the file, and
/// an installed tree holds none, so this looks only at the file itself.
fn read_part_file(tree: &Path, path: &PartPath) -> Result<String, FileFault> {
    let file = path.under(tree);
    let metadata = std::fs::symlink_metadata(&file).map_err(|_| FileFault::Unreadable)?;
    if !metadata.is_file() {
        return Err(FileFault::NotARegularFile);
    }
    if metadata.len() > PART_FILE_LARGEST {
        return Err(FileFault::TooLarge);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&file)
        .and_then(|handle| handle.take(PART_FILE_LARGEST + 1).read_to_end(&mut bytes))
        .map_err(|_| FileFault::Unreadable)?;
    if bytes.len() as u64 > PART_FILE_LARGEST {
        return Err(FileFault::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| FileFault::NotText)
}

/// A plugin's hooks as a turn or a listing sees them: every word of each argv with
/// [`ROOT_VARIABLE`] already replaced, and a count of what the file said that was not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookList {
    /// The hooks the reader kept, argv expanded.
    pub hooks: Hooks,
    /// What the file held that the reader passed over (HOOK-7), so a listing says so.
    pub unkept: Unkept,
}

/// A plugin's servers: each entry by alias, with a local server's command and arguments already
/// expanded, or what is wrong with the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerList {
    pub entries: Vec<ServerEntry>,
}

/// Read the hooks file `path` names below `tree`, as `hooks.json` is read, and expand
/// [`ROOT_VARIABLE`] in each argv word to `install_dir`.
///
/// `tree` is where the file is now (a staged copy or the installed directory) and `install_dir` is
/// where the plugin is or will be, `<state>/plugins/<name>`, so a listing of a staged plugin shows
/// the path the hook will have. The file is parsed as data and nothing is run or decided from it.
/// The digest of a hook's argv is to be taken from the result, never from the file's own words
/// (PLUGIN-13).
pub fn read_hooks(tree: &Path, path: &PartPath, install_dir: &Path) -> Result<HookList, FileFault> {
    let root = install_dir.to_str().ok_or(FileFault::PathNotText)?;
    let text = read_part_file(tree, path)?;
    let (hooks, unkept) = Hooks::parse_accounting(&text).ok_or(FileFault::NotHooks)?;
    Ok(HookList {
        hooks: hooks.mapping_run(|word| expand_root(word, root)),
        unkept,
    })
}

/// Read the servers file `path` names below `tree`, as `mcp.json` is read, and expand
/// [`ROOT_VARIABLE`] in each local server's command and arguments to `install_dir`.
///
/// Only the program and arguments are expanded: an environment value, the directory, the files it
/// may read, a url and every other field stay as written. The digest of a declaration is to be
/// taken from the result (PLUGIN-13).
pub fn read_servers(
    tree: &Path,
    path: &PartPath,
    install_dir: &Path,
) -> Result<ServerList, FileFault> {
    let root = install_dir.to_str().ok_or(FileFault::PathNotText)?;
    let text = read_part_file(tree, path)?;
    let declarations = Declarations::parse(&text).map_err(|_| FileFault::NotServers)?;
    let entries = declarations
        .entries()
        .into_iter()
        .map(|entry| ServerEntry {
            alias: entry.alias,
            declaration: entry
                .declaration
                .map(|declaration| declaration.mapping_argv(|word| expand_root(word, root))),
        })
        .collect();
    Ok(ServerList { entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    /// An empty scratch directory of this name.
    fn fresh(name: &str) -> PathBuf {
        let root = scratch_dir(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch directory");
        root
    }

    const GOOD: &str = "name = \"demo\"\nversion = \"0.1.0\"\ndescription = \"demo plugin\"\n";

    fn with(extra: &str) -> String {
        format!("{GOOD}{extra}\n")
    }

    #[test]
    fn a_manifest_gives_its_name_version_description_and_parts() {
        let text = with(
            "skills = \"skills\"\nagents = 'agents/defs'\nhooks = \"h.json\"\nservers = \"s.json\"",
        );
        let manifest = parse(&text).unwrap();
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.description, "demo plugin");
        assert_eq!(manifest.skills.as_ref().unwrap().as_str(), "skills");
        assert_eq!(manifest.agents.as_ref().unwrap().as_str(), "agents/defs");
        assert_eq!(manifest.hooks.as_ref().unwrap().as_str(), "h.json");
        assert_eq!(manifest.servers.as_ref().unwrap().as_str(), "s.json");
        assert!(manifest.unread.is_empty());
    }

    #[test]
    fn a_part_the_manifest_does_not_name_is_none() {
        let manifest = parse(GOOD).unwrap();
        assert!(manifest.named_parts().is_empty());
        let manifest = parse(&with("skills = \"skills\"")).unwrap();
        assert_eq!(manifest.named_parts().len(), 1);
        assert!(manifest.agents.is_none());
    }

    #[test]
    fn hooks_and_servers_are_named_parts_like_the_others() {
        let manifest = parse(&with("skills = \"s\"\nhooks = \"h\"\nservers = \"v\"")).unwrap();
        assert_eq!(
            manifest
                .named_parts()
                .into_iter()
                .map(|(part, _)| part)
                .collect::<Vec<_>>(),
            vec![Part::Skills, Part::Hooks, Part::Servers]
        );
    }

    #[test]
    fn an_absolute_part_path_is_refused_naming_the_key() {
        for (part, written) in [
            (Part::Skills, "/etc"),
            (Part::Agents, "\\\\host\\share"),
            (Part::Hooks, "C:/x"),
            (Part::Servers, "/s.json"),
        ] {
            let text = with(&format!(
                "{} = \"{}\"",
                part.key(),
                written.replace('\\', "\\\\")
            ));
            let refusal = parse(&text).unwrap_err();
            assert_eq!(refusal, Refusal::AbsolutePath(part), "{written}");
            assert_eq!(refusal.key(), Some(Field::Part(part)));
        }
    }

    #[test]
    fn a_parent_part_path_is_refused_naming_the_key() {
        for written in ["../x", "a/../b", "a/..", "..", "a/./../b"] {
            let refusal = parse(&with(&format!("skills = \"{written}\""))).unwrap_err();
            assert_eq!(refusal, Refusal::ParentPath(Part::Skills), "{written}");
            assert_eq!(refusal.key().map(Field::key), Some("skills"));
        }
        // A name that only contains two dots is not a parent segment.
        assert!(parse(&with("skills = \"a..b\"")).is_ok());
        assert!(parse(&with("skills = \"..a\"")).is_ok());
    }

    #[test]
    fn a_part_path_naming_nothing_below_the_root_is_refused() {
        for written in ["", ".", "./", "a\\b"] {
            let refusal = parse(&with(&format!(
                "agents = \"{}\"",
                written.replace('\\', "\\\\")
            )));
            assert_eq!(
                refusal,
                Err(Refusal::UnusablePath(Part::Agents)),
                "{written:?}"
            );
        }
    }

    #[test]
    fn a_name_is_letters_digits_and_hyphens_only() {
        for name in ["demo", "a-b-1", "X", "9", &"a".repeat(LONGEST_NAME)] {
            assert!(is_valid_name(name), "{name}");
        }
        for name in [
            "",
            "a b",
            "a_b",
            "a.b",
            "a/b",
            "../x",
            ".hidden",
            "é",
            "a\u{200b}b",
            "a\\b",
            &"a".repeat(LONGEST_NAME + 1),
        ] {
            assert!(!is_valid_name(name), "{name:?}");
        }
        let text = "name = \"../x\"\nversion = \"1\"\ndescription = \"d\"\n";
        let refusal = parse(text).unwrap_err();
        assert_eq!(refusal, Refusal::BadName);
        assert_eq!(refusal.key(), Some(Field::Name));
    }

    #[test]
    fn a_key_this_build_does_not_read_is_reported_and_ignored() {
        let manifest = parse(&with("# note\nauthor = \"x\"\nlicense = \"MIT\"")).unwrap();
        assert_eq!(
            manifest.unread,
            vec![
                Unread {
                    line: 5,
                    key: "author".into()
                },
                Unread {
                    line: 6,
                    key: "license".into()
                },
            ]
        );
        assert!(manifest.named_parts().is_empty());
    }

    #[test]
    fn a_name_version_or_description_that_is_missing_is_refused_naming_it() {
        for (key, line) in [("name", 0), ("version", 1), ("description", 2)] {
            let text: String = GOOD
                .lines()
                .enumerate()
                .filter(|(index, _)| *index != line)
                .map(|(_, text)| format!("{text}\n"))
                .collect();
            let refusal = parse(&text).unwrap_err();
            assert_eq!(refusal.key().map(Field::key), Some(key));
        }
        let text = "name = \"d\"\nversion = \" \"\ndescription = \"x\"\n";
        assert_eq!(parse(text), Err(Refusal::Empty(Field::Version)));
    }

    #[test]
    fn comments_blank_lines_and_escapes_are_read_as_toml_reads_them() {
        let text = "# top\n\nname = \"demo\" # why\nversion = '1.0'\ndescription = \"a b \\u00e9 \\\"q\\\" \\\\\"\n";
        let manifest = parse(text).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.description, "a b é \"q\" \\");
        // A control character is refused however it is written, so a value draws as one line.
        for written in ["a\\tb", "a\\nb", "a\\u0000b", "a\tb"] {
            let text = format!("name = \"a\"\nversion = \"1\"\ndescription = \"{written}\"");
            assert_eq!(
                parse(&text),
                Err(Refusal::Syntax {
                    line: 3,
                    fault: Fault::ControlCharacter
                }),
                "{written:?}"
            );
        }
        // A hash inside a string is text.
        let manifest = parse("name = \"a\"\nversion = \"#1\"\ndescription = \"d\"").unwrap();
        assert_eq!(manifest.version, "#1");
    }

    #[test]
    fn anything_outside_the_subset_is_refused_with_its_line() {
        let cases: [(&str, usize, Fault); 11] = [
            ("[plugin]", 1, Fault::Table("[plugin]".into())),
            ("[[x]]", 1, Fault::Table("[[x]]".into())),
            ("just text", 1, Fault::NotKeyValue),
            ("a b = \"x\"", 1, Fault::BadKey("a b".into())),
            ("= \"x\"", 1, Fault::BadKey(String::new())),
            ("a = 5", 1, Fault::NotAString),
            ("a = [\"x\"]", 1, Fault::NotAString),
            ("a = \"\"\"x\"\"\"", 1, Fault::NotAString),
            ("a = \"x", 1, Fault::UnterminatedString),
            ("a = \"x\" y", 1, Fault::TextAfterValue),
            ("a = \"\\q\"", 1, Fault::BadEscape),
        ];
        for (text, line, fault) in cases {
            let at = format!("name = \"d\"\nversion = \"1\"\n{text}\n");
            let refusal = parse(&at).unwrap_err();
            assert_eq!(
                refusal,
                Refusal::Syntax {
                    line: line + 2,
                    fault
                },
                "{text}"
            );
        }
        assert!(matches!(
            parse("a = \"x\"\na = \"y\""),
            Err(Refusal::Syntax {
                line: 2,
                fault: Fault::DuplicateKey(_)
            })
        ));
        assert!(matches!(
            parse("a = \"x\u{7}\""),
            Err(Refusal::Syntax {
                line: 1,
                fault: Fault::ControlCharacter
            })
        ));
    }

    #[test]
    fn a_table_header_is_refused_even_after_the_keys_are_read() {
        let refusal = parse(&with("[plugin]\nallow = \"x\"")).unwrap_err();
        assert_eq!(
            refusal,
            Refusal::Syntax {
                line: 4,
                fault: Fault::Table("[plugin]".into())
            }
        );
    }

    #[test]
    fn read_opens_plugin_toml_alone_and_names_nothing_else() {
        let root = fresh("plugin-read");
        assert_eq!(read(&root), Err(Refusal::NoManifest));
        std::fs::create_dir_all(root.join("skills/x")).unwrap();
        std::fs::write(root.join("skills/x/SKILL.md"), "not read").unwrap();
        std::fs::write(root.join("plugin.toml"), with("skills = \"nothing\"")).unwrap();
        let manifest = read(&root).unwrap();
        let part = manifest.skills.unwrap();
        assert_eq!(part.under(&root), root.join("nothing"));
        assert!(!root.join("nothing").exists());
    }

    #[test]
    fn read_refuses_an_oversized_or_non_text_or_non_file_manifest() {
        let root = fresh("plugin-refused-files");
        let file = root.join("plugin.toml");
        std::fs::write(&file, vec![b'#'; LARGEST as usize + 1]).unwrap();
        assert_eq!(read(&root), Err(Refusal::TooLarge));
        std::fs::write(&file, [0xff, 0xfe]).unwrap();
        assert_eq!(read(&root), Err(Refusal::NotText));
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        assert_eq!(read(&root), Err(Refusal::Unreadable));
    }

    #[cfg(unix)]
    #[test]
    fn read_does_not_follow_a_manifest_that_is_a_link() {
        let root = fresh("plugin-link-root");
        let elsewhere = fresh("plugin-link-elsewhere");
        std::fs::write(elsewhere.join("m.toml"), GOOD).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("m.toml"), root.join("plugin.toml")).unwrap();
        assert_eq!(read(&root), Err(Refusal::Unreadable));
    }

    const DECISIONS: [&str; 14] = [
        "permissions",
        "allow",
        "ask",
        "deny",
        "grant",
        "trustedPaths",
        "trusted_paths",
        "trusted-paths",
        "mode",
        "model",
        "endpoint",
        "settings",
        "Permissions",
        "ALLOW",
    ];

    #[test]
    fn a_decision_key_refuses_the_manifest_whole_however_it_is_written() {
        for key in DECISIONS {
            let line = |text: String| with(&text);
            for text in [
                line(format!("{key} = \"x\"")),
                line(format!("{key} = [\"x\"]")),
                line(format!("{key} = true")),
                line(format!("{key}.deep = \"x\"")),
                line(format!("\"{key}\" = \"x\"")),
                line(format!("[{key}]\nx = \"y\"")),
                line(format!("[ {key}.sub ]")),
                line(format!("  {key}   =  'x'  # c")),
            ] {
                let refusal = parse(&text).unwrap_err();
                assert!(
                    matches!(&refusal, Refusal::DecisionKey { line: 4, .. }),
                    "{text:?} gave {refusal:?}"
                );
            }
        }
        // The refusal names the key as written, and a manifest that is otherwise wrong is still
        // refused (the whole manifest, not the first thing a reader finds to read).
        let refusal = parse("mode = \"auto\"\nname = \"../x\"").unwrap_err();
        assert_eq!(
            refusal,
            Refusal::DecisionKey {
                line: 1,
                key: "mode".into()
            }
        );
        assert_eq!(refusal.named_key().as_deref(), Some("mode"));
        assert_eq!(refusal.key(), None);
    }

    #[test]
    fn a_manifest_with_a_decision_key_is_refused_by_read_and_nothing_is_made() {
        let root = fresh("plugin-decision-read");
        std::fs::write(root.join("plugin.toml"), with("allow = \"Bash(*)\"")).unwrap();
        let count = || std::fs::read_dir(&root).unwrap().count();
        let before = count();
        assert_eq!(
            read(&root),
            Err(Refusal::DecisionKey {
                line: 4,
                key: "allow".into()
            })
        );
        assert_eq!(before, count());
    }

    #[test]
    fn a_key_that_only_resembles_a_decision_key_is_not_one() {
        let manifest = parse(&with(
            "allowance = \"x\"\nmodel-card = \"x\"\nmoderator = \"x\"\nasking = \"x\"\nrules-url = \"x\"",
        ))
        .unwrap();
        assert_eq!(manifest.unread.len(), 5);
    }

    #[test]
    fn the_decision_list_and_the_keys_read_do_not_overlap() {
        for key in DECISION_KEYS {
            assert_eq!(fold_key(key), key, "{key} is written folded");
            assert_eq!(classify(key), Class::Decision, "{key}");
        }
        for field in [
            Field::Name,
            Field::Version,
            Field::Description,
            Field::Part(Part::Skills),
            Field::Part(Part::Agents),
            Field::Part(Part::Hooks),
            Field::Part(Part::Servers),
        ] {
            assert_eq!(classify(field.key()), Class::Read(field));
        }
        for key in DECISIONS {
            assert!(is_decision_key(key), "{key}");
        }
        assert!(!is_decision_key("author"));
    }

    #[test]
    fn the_manifest_has_no_field_that_can_carry_a_decision() {
        // Naming every field means a new field stops this compiling, and whoever adds it has to
        // read PLUGIN-6 and say here what the field is.
        let Manifest {
            name: _,
            version: _,
            description: _,
            skills: _,
            agents: _,
            hooks: _,
            servers: _,
            unread,
        } = parse(GOOD).unwrap();
        assert!(unread.is_empty());
    }

    #[test]
    fn another_tools_manifest_alone_is_refused_naming_plugin_toml() {
        for other in OtherManifest::ALL {
            let root = fresh("plugin-other");
            let path = other.path().split('/').fold(root.clone(), |p, s| p.join(s));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{}").unwrap();
            assert_eq!(read(&root), Err(Refusal::NoPluginToml(other)), "{other:?}");
            // plugin.toml present: the other manifest is not looked at.
            std::fs::write(root.join("plugin.toml"), GOOD).unwrap();
            assert!(read(&root).is_ok(), "{other:?}");
        }
        let root = fresh("plugin-other-none");
        assert_eq!(read(&root), Err(Refusal::NoManifest));
        assert_eq!(FILE, "plugin.toml");
    }

    #[cfg(unix)]
    mod links {
        use super::*;
        use std::os::unix::fs::symlink;

        /// A root holding `skills/a/SKILL.md` and `agents/`, and a sibling directory outside it.
        fn tree(name: &str) -> (PathBuf, PathBuf) {
            let base = fresh(name);
            let root = base.join("plugin");
            std::fs::create_dir_all(root.join("skills/a")).unwrap();
            std::fs::create_dir_all(root.join("agents")).unwrap();
            std::fs::write(root.join("skills/a/SKILL.md"), "x").unwrap();
            let outside = base.join("outside");
            std::fs::create_dir_all(&outside).unwrap();
            std::fs::write(outside.join("secret"), "s").unwrap();
            (root, outside)
        }

        fn check(root: &Path, text: &str) -> Result<(), Refusal> {
            parse(&with(text)).and_then(|manifest| check_links(root, &manifest.named_parts()))
        }

        #[test]
        fn a_plain_tree_and_a_link_inside_the_root_pass() {
            let (root, _) = tree("plugin-links-ok");
            symlink(root.join("skills/a"), root.join("skills/b")).unwrap();
            assert_eq!(
                check(&root, "skills = \"skills\"\nagents = \"agents\""),
                Ok(())
            );
        }

        #[test]
        fn a_part_that_is_a_link_leaving_the_root_is_refused_naming_the_key() {
            let (root, outside) = tree("plugin-links-part");
            symlink(&outside, root.join("ext")).unwrap();
            let refusal = check(&root, "skills = \"skills\"\nagents = \"ext\"").unwrap_err();
            assert_eq!(refusal, Refusal::LinkLeavesRoot(Part::Agents));
            assert_eq!(refusal.named_key().as_deref(), Some("agents"));
        }

        #[test]
        fn a_link_on_the_way_to_the_part_is_refused() {
            let (root, outside) = tree("plugin-links-way");
            std::fs::create_dir_all(outside.join("inner")).unwrap();
            symlink(&outside, root.join("hop")).unwrap();
            assert_eq!(
                check(&root, "skills = \"hop/inner\""),
                Err(Refusal::LinkLeavesRoot(Part::Skills))
            );
        }

        #[test]
        fn a_link_deep_inside_a_part_directory_is_refused_for_a_file_or_a_directory() {
            let (root, outside) = tree("plugin-links-deep");
            symlink(outside.join("secret"), root.join("skills/a/SKILL2.md")).unwrap();
            assert_eq!(
                check(&root, "skills = \"skills\""),
                Err(Refusal::LinkLeavesRoot(Part::Skills))
            );
            std::fs::remove_file(root.join("skills/a/SKILL2.md")).unwrap();
            symlink(&outside, root.join("skills/a/dir")).unwrap();
            assert_eq!(
                check(&root, "skills = \"skills\""),
                Err(Refusal::LinkLeavesRoot(Part::Skills))
            );
        }

        #[test]
        fn a_link_that_resolves_nowhere_is_refused() {
            let (root, _) = tree("plugin-links-dangling");
            symlink(root.join("nothing"), root.join("agents/x.md")).unwrap();
            assert_eq!(
                check(&root, "agents = \"agents\""),
                Err(Refusal::LinkLeavesRoot(Part::Agents))
            );
        }

        #[test]
        fn a_link_outside_every_named_part_is_not_this_checks_business() {
            let (root, outside) = tree("plugin-links-unnamed");
            symlink(&outside, root.join("README-link")).unwrap();
            assert_eq!(check(&root, "skills = \"skills\""), Ok(()));
        }

        #[test]
        fn a_part_that_does_not_exist_does_not_hide_a_later_part() {
            let (root, outside) = tree("plugin-links-missing");
            symlink(&outside, root.join("agents/out")).unwrap();
            assert_eq!(
                check(&root, "skills = \"absent\"\nagents = \"agents\""),
                Err(Refusal::LinkLeavesRoot(Part::Agents))
            );
        }
    }

    /// PLUGIN-13, S6 to S9: `${PLUGIN_ROOT}` in the part files, read from a tree that is not
    /// where the plugin will live, as the staged tree is.
    mod parts {
        use super::*;

        const INSTALL: &str = "/state/plugins/demo";

        fn tree_with(name: &str, file: &str, text: &str) -> (PathBuf, PartPath) {
            let root = fresh(name);
            std::fs::write(root.join(file), text).unwrap();
            (root, check_part_path(Part::Hooks, file).unwrap())
        }

        #[test]
        fn every_occurrence_in_a_word_is_replaced_once_and_nothing_else_is() {
            assert_eq!(
                expand_root("${PLUGIN_ROOT}/a", INSTALL),
                "/state/plugins/demo/a"
            );
            assert_eq!(
                expand_root("--x=${PLUGIN_ROOT}:${PLUGIN_ROOT}", INSTALL),
                "--x=/state/plugins/demo:/state/plugins/demo"
            );
            for left in [
                "$PLUGIN_ROOT",
                "${PLUGIN_ROOT",
                "${plugin_root}",
                "${HOME}",
                "${PLUGIN_ROOT_X}",
            ] {
                assert_eq!(expand_root(left, INSTALL), left);
            }
            // The replacement is not searched again.
            assert_eq!(
                expand_root("${PLUGIN_ROOT}", "/a/${PLUGIN_ROOT}"),
                "/a/${PLUGIN_ROOT}"
            );
        }

        /// The listing for a staged tree shows the installed path, not the staging path.
        #[test]
        fn a_hook_argv_names_the_directory_the_plugin_will_have_not_the_one_it_was_read_from() {
            let (tree, path) = tree_with(
                "plugin-hook-root",
                "hooks.json",
                r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh","--root","${PLUGIN_ROOT}"],"timeout":5}]}"#,
            );
            let read = read_hooks(&tree, &path, Path::new(INSTALL)).unwrap();
            let hook = &read.hooks.declared()[0];
            assert_eq!(
                hook.run(),
                [
                    "/state/plugins/demo/hook.sh",
                    "--root",
                    "/state/plugins/demo"
                ]
            );
            assert!(
                !hook
                    .run()
                    .iter()
                    .any(|word| word.contains(tree.to_str().unwrap()))
            );
            assert_eq!(hook.timeout(), Some(std::time::Duration::from_secs(5)));
            assert!(read.unkept.is_none());
        }

        /// The digest input: expanded argv from the one reader, equal to an argv written out by
        /// hand, and different for a plugin installed under another name.
        #[test]
        fn the_argv_a_digest_is_taken_over_is_the_expanded_one_and_follows_the_install_directory() {
            let text = r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh"]}]}"#;
            let (tree, path) = tree_with("plugin-hook-digest", "hooks.json", text);
            let by_name = |dir: &str| {
                read_hooks(&tree, &path, Path::new(dir))
                    .unwrap()
                    .hooks
                    .declared()[0]
                    .run()
                    .to_vec()
            };
            assert_eq!(
                by_name("/state/plugins/demo"),
                ["/state/plugins/demo/hook.sh"]
            );
            assert_eq!(
                by_name("/state/plugins/other"),
                ["/state/plugins/other/hook.sh"]
            );
            let written = Hooks::parse(
                r#"{"hooks":[{"on":"turn-started","run":["/state/plugins/demo/hook.sh"]}]}"#,
            );
            assert_eq!(by_name("/state/plugins/demo"), written.declared()[0].run());

            let servers =
                r#"{"servers":{"demo":{"transport":"stdio","argv":["${PLUGIN_ROOT}/server.py"]}}}"#;
            let (tree, path) = tree_with("plugin-server-digest", "servers.json", servers);
            let digest = |dir: &str| {
                let list = read_servers(&tree, &path, Path::new(dir)).unwrap();
                list.entries[0].declaration.as_ref().unwrap().digest()
            };
            let by_hand = Declarations::parse(
                r#"{"servers":{"x":{"transport":"stdio","argv":["/state/plugins/demo/server.py"]}}}"#,
            )
            .unwrap()
            .entries()[0]
                .declaration
                .as_ref()
                .unwrap()
                .digest();
            assert_eq!(digest("/state/plugins/demo"), by_hand);
            assert_ne!(digest("/state/plugins/other"), by_hand);
        }

        /// S9: an environment value, a directory, a read, a url and a hook's watched path keep the
        /// text as written, and nothing is looked up in the environment.
        #[test]
        fn the_variable_is_left_as_written_anywhere_but_an_argv_word() {
            let servers = r#"{"servers":{"demo":{"transport":"stdio",
                "argv":["${PLUGIN_ROOT}/s","${HOME}"],
                "env":{"TOKEN":"${PLUGIN_ROOT}/t"},
                "reads":["/etc/${PLUGIN_ROOT}"],"directory":"/w/${PLUGIN_ROOT}"},
                "web":{"transport":"http","url":"https://h.example/${PLUGIN_ROOT}"}}}"#;
            let (tree, path) = tree_with("plugin-server-env", "servers.json", servers);
            let list = read_servers(&tree, &path, Path::new(INSTALL)).unwrap();
            let demo = list.entries[0].declaration.as_ref().unwrap();
            assert_eq!(demo.argv(), ["/state/plugins/demo/s", "${HOME}"]);
            let stored: Vec<_> = demo.stored_values().collect();
            assert_eq!(stored, [("TOKEN", "${PLUGIN_ROOT}/t")]);
            assert_eq!(demo.reads(), ["/etc/${PLUGIN_ROOT}"]);
            assert_eq!(demo.directory(), Some("/w/${PLUGIN_ROOT}"));
            let web = list.entries[1].declaration.as_ref().unwrap();
            assert_eq!(web.url(), Some("https://h.example/${PLUGIN_ROOT}"));

            let hooks = r#"{"hooks":[{"on":"file-changed","path":"${PLUGIN_ROOT}/f","tool":"${PLUGIN_ROOT}","run":["x"]}]}"#;
            let (tree, path) = tree_with("plugin-hook-path", "hooks.json", hooks);
            let read = read_hooks(&tree, &path, Path::new(INSTALL)).unwrap();
            assert_eq!(read.hooks.declared()[0].path(), Some("${PLUGIN_ROOT}/f"));
            assert_eq!(read.hooks.declared()[0].tool(), Some("${PLUGIN_ROOT}"));
        }

        /// S29: the hook reader drops what it cannot use, and the read says how much, so a listing
        /// is not shorter than the file without saying so.
        #[test]
        fn what_the_hook_reader_passes_over_is_counted() {
            let text = r#"{"extra":1,"hooks":[
                {"on":"turn-started","run":["ok"],"color":"red"},
                {"on":"no-such-moment","run":["x"]},
                {"on":"turn-started","run":[]},
                {"on":"turn-started","run":["a",3]},
                {"on":"turn-started","run":["t"],"timeout":0}]}"#;
            let (tree, path) = tree_with("plugin-hook-unkept", "hooks.json", text);
            let read = read_hooks(&tree, &path, Path::new(INSTALL)).unwrap();
            assert_eq!(read.hooks.declared().len(), 1);
            assert_eq!(
                read.unkept,
                Unkept {
                    dropped: 4,
                    root_keys: 1,
                    entry_keys: 1
                }
            );
            assert!(!read.unkept.is_none());
        }

        #[test]
        fn a_file_that_cannot_be_used_is_a_fault_naming_why() {
            let (tree, path) = tree_with("plugin-part-faults", "hooks.json", "not json");
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::NotHooks)
            );
            std::fs::write(tree.join("hooks.json"), r#"{"servers":{}}"#).unwrap();
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::NotHooks)
            );
            assert_eq!(
                read_servers(&tree, &path, Path::new(INSTALL)).map(|_| ()),
                Ok(())
            );
            std::fs::write(tree.join("hooks.json"), r#"{"hooks":[]}"#).unwrap();
            assert_eq!(
                read_servers(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::NotServers)
            );
            std::fs::write(
                tree.join("hooks.json"),
                vec![b' '; PART_FILE_LARGEST as usize + 1],
            )
            .unwrap();
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::TooLarge)
            );
            std::fs::write(tree.join("hooks.json"), [0xff, 0xfe]).unwrap();
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::NotText)
            );
            std::fs::remove_file(tree.join("hooks.json")).unwrap();
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::Unreadable)
            );
            std::fs::create_dir(tree.join("hooks.json")).unwrap();
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::NotARegularFile)
            );
        }

        #[cfg(unix)]
        #[test]
        fn a_part_file_that_is_a_link_is_not_followed() {
            let (tree, _) = tree_with("plugin-part-link", "real.json", r#"{"hooks":[]}"#);
            let path = check_part_path(Part::Hooks, "hooks.json").unwrap();
            std::os::unix::fs::symlink(tree.join("real.json"), tree.join("hooks.json")).unwrap();
            assert_eq!(
                read_hooks(&tree, &path, Path::new(INSTALL)),
                Err(FileFault::NotARegularFile)
            );
        }

        /// An entry of the servers file that cannot be used is carried as the problem, not
        /// dropped, so a listing can show the alias.
        #[test]
        fn a_server_entry_that_cannot_be_used_is_listed_with_its_problem() {
            let text = r#"{"servers":{"bad":{"transport":"stdio"},"ok":{"transport":"stdio","argv":["p"]}}}"#;
            let (tree, path) = tree_with("plugin-server-bad", "servers.json", text);
            let list = read_servers(&tree, &path, Path::new(INSTALL)).unwrap();
            assert_eq!(list.entries.len(), 2);
            assert!(
                list.entries
                    .iter()
                    .any(|e| e.alias == "bad" && e.declaration.is_err())
            );
            assert!(
                list.entries
                    .iter()
                    .any(|e| e.alias == "ok" && e.declaration.is_ok())
            );
        }
    }
}
