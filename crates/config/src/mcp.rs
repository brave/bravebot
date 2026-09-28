//! The MCP server declarations, `mcp.json` in the person's own state directory, and the record of
//! which of them the person approved, `mcp-approved` beside it.
//!
//! A declaration says that a server exists: its alias, its transport, the program and arguments
//! that start it or the url that reaches it, the variables it receives, the files it may read, and
//! the directory it runs in. This module reads and writes that format and computes what an approval
//! binds to. It starts nothing and reaches nothing: launching a server is the client's, and deciding
//! whether one is offered to a session is the agent's.
//!
//! # Why not a settings layer
//!
//! A stdio declaration is a command to run, and BACKEND-1 forbids a settings file naming one. The
//! stronger reason is that two of the settings layers sit in a checkout, and the agent can write
//! files in a checkout, so a declaration read from one would be a road from an ordinary tool call to
//! execution nobody approved. A file in the state directory is outside every directory a session can
//! write. [`crate::Settings::mcp_declared`] is where a layer that tried anyway is reported.
//!
//! # Names, and the values a person gave
//!
//! `variables` lists the names of variables a server receives from the person's own environment
//! at launch. `env` holds the ones the person gave a value when they declared it, in the shape
//! Claude Code's `.mcp.json` uses, and a stored value is handed over as written. The file is the
//! person's own, private to them inside the state directory, which is why a value may be kept in
//! it and in no settings layer. Nothing said about an entry repeats a value: a problem names the
//! variable at most. `reads` lists the files a value or an argument named, one grant each.
//!
//! # Every failure is per entry, except the file's own shape
//!
//! An entry this cannot use is kept, listed with what is wrong with it, and written back unchanged
//! when another entry is added or removed. A file whose root is not the shape below cannot be
//! rewritten without losing what is in it, so it is reported as unreadable and never overwritten.
//!
//! ```json
//! { "servers": {
//!     "weather": { "transport": "stdio", "argv": ["npx", "-y", "weather-mcp"], "variables": ["PATH"],
//!         "env": { "WEATHER_KEY_FILE": "/home/me/keys/weather" }, "reads": ["/home/me/keys/weather"] },
//!     "docs": { "transport": "http", "url": "https://docs.example.com/mcp" } } }
//! ```

use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The declarations, inside the state directory.
const DECLARATIONS_FILE: &str = "mcp.json";

/// The approvals, one digest per line, inside the state directory.
const APPROVALS_FILE: &str = "mcp-approved";

/// The projects answer 2 was given in, one path per line, inside the state directory.
const PROJECTS_FILE: &str = "mcp-projects";

/// The tools answer 2 at a call was given about, one alias, tool and project per line.
const TOOLS_FILE: &str = "mcp-tools";

/// The directory holding one home per declaration a server was started from.
const HOMES_DIRECTORY: &str = "mcp-home";

/// The most of either file worth reading.
///
/// A handful of short argument vectors, or a handful of digests. Bounded so a file that grew by
/// accident, or was replaced by something else entirely, is passed over rather than parsed.
const MAX_BYTES: u64 = 64 * 1024;

/// The longest alias, which is also the first half of every tool name a server offers.
const MAX_ALIAS: usize = 64;

/// What the digested form starts with, so a later change to that form cannot collide with this one.
const DIGEST_FORM: &str = "bravebot-mcp-declaration-1";

/// What a vouched tool list's digested form starts with, so no list digests alike with a declaration.
const LIST_FORM: &str = "bravebot-mcp-tool-list-1";

/// The longest word a server's tool may be offered under.
pub const MAX_TOOL: usize = 64;

/// Where a person's servers are declared.
pub fn declarations_file(directory: &Path) -> PathBuf {
    directory.join(DECLARATIONS_FILE)
}

/// Where a person's approvals are recorded.
pub fn approvals_file(directory: &Path) -> PathBuf {
    directory.join(APPROVALS_FILE)
}

/// Where the projects answer 2 was given in are recorded.
pub fn projects_file(directory: &Path) -> PathBuf {
    directory.join(PROJECTS_FILE)
}

/// Where the tools answer 2 at a call was given about are recorded.
pub fn tools_file(directory: &Path) -> PathBuf {
    directory.join(TOOLS_FILE)
}

/// Where a local server started from the declaration digested as `declared` keeps its own files.
///
/// Keyed by the digest rather than the alias, so a declaration edited to run another program does
/// not inherit what the one before it wrote.
pub fn server_home(directory: &Path, declared: &Digest) -> PathBuf {
    directory.join(HOMES_DIRECTORY).join(declared.to_string())
}

/// Whether `word` may be offered as a tool's name beneath an alias.
///
/// Letters, digits, `-` and `_`, which is what a function name may be on every backend this
/// program speaks to. A server's word outside that is not rewritten into one: two words rewritten
/// alike would be one name for two tools, and a word is the server's own text either way.
pub fn is_tool_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_TOOL
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Whether `alias` may name a server.
///
/// Letters, digits, `-` and `_`, starting with a letter or a digit. The alias is the namespace a
/// server's tools are offered under, `weather:get_forecast`, so a `:` in one would let two servers
/// spell the same tool, and anything wider is a name a person cannot type back on a command line.
pub fn is_alias(alias: &str) -> bool {
    let mut characters = alias.chars();
    alias.len() <= MAX_ALIAS
        && characters.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Whether `name` is a variable name.
///
/// The portable spelling, a letter or `_` and then letters, digits and `_`. Anything else is not
/// taken for a name, which is what keeps a value pasted into the list from reaching a prompt as one.
pub fn is_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// What is wrong with one entry, or with one flag that would have made it.
///
/// Nothing here carries a value. The variants that name a variable name one [`is_variable_name`]
/// accepts, and never what it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The alias is not one [`is_alias`] accepts.
    Alias,
    /// The entry is not a JSON object.
    NotAnObject,
    /// `transport` is missing, or names neither `stdio` nor `http`.
    Transport,
    /// A key this format does not have, as the file spelled it.
    Key(String),
    /// `argv` is missing or empty, names a blank program, or holds something that is not a string.
    Program,
    /// `variables` is not a list of names, or a key of `env` is not a name.
    Name,
    /// `env` is not an object.
    Env,
    /// The value `env` gives this variable is not a string, or holds a NUL no environment can.
    Value(String),
    /// This variable is given two answers: it is in `variables` and in `env`, or given two values.
    Twice(String),
    /// `reads` is not a list of absolute paths.
    Reads,
    /// `directory` is not an absolute path.
    Directory,
    /// `url` is missing, is not a string, is not `http` or `https`, or names no host.
    Url,
    /// `url` carries a user or a password.
    Credentials,
    /// A remote server was given something only a process can use, and this is the key.
    Remote(&'static str),
}

/// One server, as a person declared it.
///
/// Its `Debug` shows a stored value by its name alone, so no value reaches a log or a panic.
#[derive(Clone, PartialEq, Eq)]
pub enum Declaration {
    /// A program this machine starts, spoken to over its standard streams.
    Stdio {
        /// The program and its arguments, never a line. Never empty.
        argv: Vec<String>,
        /// The names of the variables it receives from the environment at launch, each once, in the
        /// order they were written.
        variables: Vec<String>,
        /// The variables it receives with the value the person gave them, by name. None of them is
        /// in `variables`, so the environment at launch is never read for one.
        env: BTreeMap<String, String>,
        /// The files it may read, each once, where the confinement would otherwise refuse them.
        /// Absolute.
        reads: Vec<String>,
        /// Where it runs, where somebody said. Absolute.
        directory: Option<String>,
    },
    /// A server somewhere else, reached over HTTP.
    Http {
        /// Where it is, as somebody wrote it.
        url: String,
    },
}

impl std::fmt::Debug for Declaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio {
                argv,
                variables,
                env,
                reads,
                directory,
            } => f
                .debug_struct("Stdio")
                .field("argv", argv)
                .field("variables", variables)
                .field("env", &env.keys().collect::<Vec<_>>())
                .field("reads", reads)
                .field("directory", directory)
                .finish(),
            Self::Http { url } => f.debug_struct("Http").field("url", url).finish(),
        }
    }
}

/// A field of a declaration, for saying which of them changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Transport,
    Argv,
    Variables,
    /// The stored values, with the name of each variable whose value was added, changed or
    /// removed. Names only: saying which value changed is saying the values.
    Env(Vec<String>),
    Reads,
    Directory,
    Url,
}

impl Field {
    /// The key the file spells it with.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Argv => "argv",
            Self::Variables => "variables",
            Self::Env(_) => "env",
            Self::Reads => "reads",
            Self::Directory => "directory",
            Self::Url => "url",
        }
    }
}

impl Declaration {
    /// A stdio declaration, checked as a file's entry is.
    ///
    /// A name listed twice is kept once, since the set of names is what the server receives.
    pub fn stdio(
        argv: Vec<String>,
        variables: Vec<String>,
        directory: Option<String>,
    ) -> Result<Self, Problem> {
        program(&argv)?;
        let variables = names(variables)?;
        if let Some(directory) = &directory {
            absolute(directory)?;
        }
        Ok(Self::Stdio {
            argv,
            variables,
            env: BTreeMap::new(),
            reads: Vec::new(),
            directory,
        })
    }

    /// This declaration with `env` as the values the person gave, checked as a file's entry is.
    ///
    /// A stored name also in `variables` would be two answers to where its value comes from, so it
    /// is refused rather than one of them winning.
    pub fn storing(self, env: BTreeMap<String, String>) -> Result<Self, Problem> {
        let Self::Stdio {
            argv,
            variables,
            reads,
            directory,
            ..
        } = self
        else {
            return match env.is_empty() {
                true => Ok(self),
                false => Err(Problem::Remote("env")),
            };
        };
        for (name, value) in &env {
            if !is_variable_name(name) {
                return Err(Problem::Name);
            }
            if value.contains('\0') {
                return Err(Problem::Value(name.clone()));
            }
            if variables.contains(name) {
                return Err(Problem::Twice(name.clone()));
            }
        }
        Ok(Self::Stdio {
            argv,
            variables,
            env,
            reads,
            directory,
        })
    }

    /// This declaration with `reads` as the files it may read, each absolute and each kept once.
    pub fn reading(self, reads: Vec<String>) -> Result<Self, Problem> {
        let Self::Stdio {
            argv,
            variables,
            env,
            directory,
            ..
        } = self
        else {
            return match reads.is_empty() {
                true => Ok(self),
                false => Err(Problem::Remote("reads")),
            };
        };
        let mut kept: Vec<String> = Vec::with_capacity(reads.len());
        for file in reads {
            if file.contains('\0') || !Path::new(&file).is_absolute() {
                return Err(Problem::Reads);
            }
            if !kept.contains(&file) {
                kept.push(file);
            }
        }
        Ok(Self::Stdio {
            argv,
            variables,
            env,
            reads: kept,
            directory,
        })
    }

    /// An HTTP declaration, checked as a file's entry is.
    pub fn http(url: String) -> Result<Self, Problem> {
        remote(&url)?;
        Ok(Self::Http { url })
    }

    /// The transport's name, as the file spells it.
    pub fn transport(&self) -> &'static str {
        match self {
            Self::Stdio { .. } => "stdio",
            Self::Http { .. } => "http",
        }
    }

    /// The names of the variables this server receives from the environment at launch. None for a
    /// remote one.
    pub fn variables(&self) -> &[String] {
        match self {
            Self::Stdio { variables, .. } => variables,
            Self::Http { .. } => &[],
        }
    }

    /// The names of the variables this server receives with a stored value, never the values.
    pub fn stored(&self) -> impl Iterator<Item = &str> {
        let env = match self {
            Self::Stdio { env, .. } => Some(env),
            Self::Http { .. } => None,
        };
        env.into_iter()
            .flat_map(|env| env.keys().map(String::as_str))
    }

    /// The files this server may read beyond what its confinement grants. None for a remote one.
    pub fn reads(&self) -> &[String] {
        match self {
            Self::Stdio { reads, .. } => reads,
            Self::Http { .. } => &[],
        }
    }

    /// What an approval of this declaration binds to.
    ///
    /// The transport, the program and every argument or the url, the set of variable names, the
    /// directory, and every stored value and the set of files it may read. Not the alias: an alias
    /// is a label a person chose, and an approval keyed on it would let an edit to the argv inherit
    /// an answer given about a different program. Not the order the names were written in either,
    /// since the set is what the server receives.
    pub fn digest(&self) -> Digest {
        let form = match self {
            Self::Stdio {
                argv,
                variables,
                env,
                reads,
                directory,
            } => {
                let variables: BTreeSet<&String> = variables.iter().collect();
                let mut form = vec![
                    DIGEST_FORM.into(),
                    "stdio".into(),
                    serde_json::json!(argv),
                    serde_json::json!(variables),
                    serde_json::json!(directory),
                ];
                // Only where there is one, so a declaration that stores nothing digests as it did
                // before anything could be stored, and the approval given it still stands.
                if !env.is_empty() || !reads.is_empty() {
                    let reads: BTreeSet<&String> = reads.iter().collect();
                    form.push(serde_json::json!({ "env": env, "reads": reads }));
                }
                serde_json::Value::Array(form)
            }
            Self::Http { url } => serde_json::json!([DIGEST_FORM, "http", url]),
        };
        Digest(Sha256::digest(form.to_string().as_bytes()).into())
    }

    /// The fields that differ between this declaration and `other`, in the file's order.
    ///
    /// A change of transport is that one field, since nothing else of one transport compares with
    /// the other.
    pub fn changes(&self, other: &Self) -> Vec<Field> {
        match (self, other) {
            (
                Self::Stdio {
                    argv,
                    variables,
                    env,
                    reads,
                    directory,
                },
                Self::Stdio {
                    argv: other_argv,
                    variables: other_variables,
                    env: other_env,
                    reads: other_reads,
                    directory: other_directory,
                },
            ) => {
                let set = |names: &[String]| names.iter().cloned().collect::<BTreeSet<_>>();
                let restored: Vec<String> = env
                    .keys()
                    .chain(other_env.keys())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .filter(|name| env.get(*name) != other_env.get(*name))
                    .cloned()
                    .collect();
                [
                    (argv != other_argv, Field::Argv),
                    (set(variables) != set(other_variables), Field::Variables),
                    (!restored.is_empty(), Field::Env(restored)),
                    (set(reads) != set(other_reads), Field::Reads),
                    (directory != other_directory, Field::Directory),
                ]
                .into_iter()
                .filter_map(|(changed, field)| changed.then_some(field))
                .collect()
            }
            (Self::Http { url }, Self::Http { url: other_url }) => match url == other_url {
                true => Vec::new(),
                false => vec![Field::Url],
            },
            _ => vec![Field::Transport],
        }
    }

    /// The entry this is written as.
    fn to_value(&self) -> serde_json::Value {
        let mut entry = serde_json::Map::new();
        entry.insert("transport".into(), self.transport().into());
        match self {
            Self::Stdio {
                argv,
                variables,
                env,
                reads,
                directory,
            } => {
                entry.insert("argv".into(), argv.clone().into());
                if !variables.is_empty() {
                    entry.insert("variables".into(), variables.clone().into());
                }
                if !env.is_empty() {
                    let env = env
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone().into()))
                        .collect();
                    entry.insert("env".into(), serde_json::Value::Object(env));
                }
                if !reads.is_empty() {
                    entry.insert("reads".into(), reads.clone().into());
                }
                if let Some(directory) = directory {
                    entry.insert("directory".into(), directory.clone().into());
                }
            }
            Self::Http { url } => {
                entry.insert("url".into(), url.clone().into());
            }
        }
        serde_json::Value::Object(entry)
    }

    /// The declaration an entry states, or what is wrong with it.
    fn from_value(value: &serde_json::Value) -> Result<Self, Problem> {
        let entry = value.as_object().ok_or(Problem::NotAnObject)?;
        const LOCAL: [&str; 4] = ["variables", "env", "reads", "directory"];
        let allowed: &[&str] = match entry.get("transport").and_then(|word| word.as_str()) {
            Some("stdio") => &[
                "transport",
                "argv",
                "variables",
                "env",
                "reads",
                "directory",
            ],
            Some("http") => &["transport", "url"],
            _ => return Err(Problem::Transport),
        };
        for key in entry.keys() {
            if allowed.contains(&key.as_str()) {
                continue;
            }
            return Err(match LOCAL.into_iter().find(|local| local == key) {
                Some(local) => Problem::Remote(local),
                None => Problem::Key(key.clone()),
            });
        }
        if allowed.contains(&"url") {
            let url = entry.get("url").and_then(|url| url.as_str());
            return Self::http(url.ok_or(Problem::Url)?.to_string());
        }
        let argv = match entry.get("argv") {
            Some(serde_json::Value::Array(words)) => words
                .iter()
                .map(|word| word.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or(Problem::Program)?,
            _ => return Err(Problem::Program),
        };
        let variables = match entry.get("variables") {
            None => Vec::new(),
            Some(serde_json::Value::Array(names)) => names
                .iter()
                .map(|name| name.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or(Problem::Name)?,
            Some(_) => return Err(Problem::Name),
        };
        let env = match entry.get("env") {
            None => BTreeMap::new(),
            Some(serde_json::Value::Object(values)) => values
                .iter()
                .map(
                    |(name, value)| match (is_variable_name(name), value.as_str()) {
                        (false, _) => Err(Problem::Name),
                        (true, None) => Err(Problem::Value(name.clone())),
                        (true, Some(value)) => Ok((name.clone(), value.to_string())),
                    },
                )
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(Problem::Env),
        };
        let reads = match entry.get("reads") {
            None => Vec::new(),
            Some(serde_json::Value::Array(files)) => files
                .iter()
                .map(|file| file.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or(Problem::Reads)?,
            Some(_) => return Err(Problem::Reads),
        };
        let directory = match entry.get("directory") {
            None => None,
            Some(serde_json::Value::String(directory)) => Some(directory.clone()),
            Some(_) => return Err(Problem::Directory),
        };
        Self::stdio(argv, variables, directory)?
            .storing(env)?
            .reading(reads)
    }
}

/// A program to run is a non-empty argument vector whose first word is not blank.
fn program(argv: &[String]) -> Result<(), Problem> {
    match argv.first() {
        Some(program) if !program.trim().is_empty() => Ok(()),
        _ => Err(Problem::Program),
    }
}

/// The names, each checked and each kept once.
///
/// `NAME=value` is not a name, and is refused as one that is not, so nothing repeats its value.
fn names(variables: Vec<String>) -> Result<Vec<String>, Problem> {
    let mut kept: Vec<String> = Vec::with_capacity(variables.len());
    for name in variables {
        if !is_variable_name(&name) {
            return Err(Problem::Name);
        }
        if !kept.contains(&name) {
            kept.push(name);
        }
    }
    Ok(kept)
}

/// A directory to run in is an absolute path.
///
/// Absolute because a relative one would be relative to wherever a session happened to start, and
/// the digest would then bind to a spelling rather than to a place.
fn absolute(directory: &str) -> Result<(), Problem> {
    match Path::new(directory).is_absolute() {
        true => Ok(()),
        false => Err(Problem::Directory),
    }
}

/// A url to reach a server at: `http` or `https`, a host, and no user or password.
///
/// No credential in the url: the url is drawn wherever the declaration is, at the question and in
/// `get`, so a token written into it is a token on the screen. A stored value is drawn only where it
/// names a file the server may read, since that read is a grant.
fn remote(url: &str) -> Result<(), Problem> {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Problem::Url);
    }
    let rest = ["http://", "https://"]
        .iter()
        .find_map(|scheme| {
            url.get(..scheme.len())
                .filter(|head| head.eq_ignore_ascii_case(scheme))
                .map(|_| &url[scheme.len()..])
        })
        .ok_or(Problem::Url)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(Problem::Credentials);
    }
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    match host.is_empty() {
        true => Err(Problem::Url),
        false => Ok(()),
    }
}

/// What an approval binds to: SHA-256 over the digested form of a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    /// The digest a line of the approvals file spells, or `None` for a line that spells none.
    pub fn parse(hex: &str) -> Option<Self> {
        let hex = hex.as_bytes();
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.chunks(2)) {
            let pair = std::str::from_utf8(pair).ok()?;
            if !pair.bytes().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            *byte = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// The first eight hex characters, which is what a prompt shows.
    pub fn short(&self) -> String {
        self.to_string()[..8].to_string()
    }

    /// What a vouch for a tool list binds to: SHA-256 over the list as it was drawn.
    ///
    /// Over the text a person was shown and nothing else, so a list the server words differently
    /// on its next start is a list nobody has seen.
    pub fn of_list(text: &str) -> Self {
        let form = serde_json::json!([LIST_FORM, text]);
        Self(Sha256::digest(form.to_string().as_bytes()).into())
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// Why the declarations file cannot be read, and so cannot be rewritten either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// Larger than a declarations file has any reason to be.
    TooLarge,
    /// There, and not readable as text.
    NotRead,
    /// Not JSON.
    NotJson,
    /// JSON, and the root is not an object.
    NotAnObject,
    /// `servers` is there and is not an object.
    Servers,
    /// The root holds a key other than `servers`, as the file spelled it.
    Key(String),
}

/// One alias and what its entry states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub alias: String,
    pub declaration: Result<Declaration, Problem>,
}

/// The declarations file, as it is written.
///
/// Held as the entries the file had rather than as the declarations read out of them, so an entry
/// this cannot use is written back as it was when another one changes. Its `Debug` names the
/// aliases alone, since an entry holds the values stored for it.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Declarations {
    servers: serde_json::Map<String, serde_json::Value>,
}

impl std::fmt::Debug for Declarations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.servers.keys()).finish()
    }
}

impl Declarations {
    /// Read the declarations in a state directory. No file is no declarations.
    pub fn read(home: &Path) -> Result<Self, Unreadable> {
        let path = declarations_file(home);
        match std::fs::metadata(&path) {
            Ok(found) if found.len() > MAX_BYTES => return Err(Unreadable::TooLarge),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(Unreadable::NotRead),
        }
        let text = std::fs::read_to_string(&path).map_err(|_| Unreadable::NotRead)?;
        Self::parse(&text)
    }

    /// Read declarations out of the file's text.
    pub fn parse(text: &str) -> Result<Self, Unreadable> {
        let root = match serde_json::from_str::<serde_json::Value>(text) {
            Ok(serde_json::Value::Object(root)) => root,
            Ok(_) => return Err(Unreadable::NotAnObject),
            Err(_) => return Err(Unreadable::NotJson),
        };
        if let Some(key) = root.keys().find(|key| *key != "servers") {
            return Err(Unreadable::Key(key.clone()));
        }
        match root.get("servers") {
            None => Ok(Self::default()),
            Some(serde_json::Value::Object(servers)) => Ok(Self {
                servers: servers.clone(),
            }),
            Some(_) => Err(Unreadable::Servers),
        }
    }

    /// Every entry, by alias, with what each states or what is wrong with it.
    pub fn entries(&self) -> Vec<Entry> {
        self.servers
            .iter()
            .map(|(alias, value)| entry(alias, value))
            .collect()
    }

    /// The entry for one alias, where the file has one.
    pub fn get(&self, alias: &str) -> Option<Entry> {
        self.servers.get(alias).map(|value| entry(alias, value))
    }

    /// Declare `alias`, replacing whatever the file said about it.
    pub fn insert(&mut self, alias: &str, declaration: &Declaration) {
        self.servers
            .insert(alias.to_string(), declaration.to_value());
    }

    /// Take `alias` out, whatever its entry says. Whether there was one.
    pub fn remove(&mut self, alias: &str) -> bool {
        self.servers.remove(alias).is_some()
    }

    /// The digest of every entry that states a declaration.
    pub fn digests(&self) -> BTreeSet<Digest> {
        self.entries()
            .into_iter()
            .filter_map(|entry| entry.declaration.ok())
            .map(|declaration| declaration.digest())
            .collect()
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let root = serde_json::json!({ "servers": self.servers });
        let mut text = serde_json::to_string_pretty(&root).unwrap_or_default();
        text.push('\n');
        text
    }
}

/// One alias's entry, read. An alias that is not one is the problem before anything in its entry.
fn entry(alias: &str, value: &serde_json::Value) -> Entry {
    let declaration = match is_alias(alias) {
        true => Declaration::from_value(value),
        false => Err(Problem::Alias),
    };
    Entry {
        alias: alias.to_string(),
        declaration,
    }
}

/// The digests the person approved, each with the aliases it was approved under, and the aliases
/// whose approved declaration has changed since.
///
/// A line that spells neither is passed over rather than refused, and an unreadable file approves
/// nothing, which is the direction to be wrong in: a server nobody can show was approved is asked
/// about again.
///
/// ```text
/// 4f1c9a2e...  weather
/// changed docs
/// tools 4f1c9a2e... 9b03d7c1...
/// ```
///
/// The alias is not what was approved: [`Approvals::approves`] asks about a digest and nothing else,
/// for the reason [`Declaration::digest`] leaves the alias out. It is kept so a session can tell a
/// server nobody has been asked about from one that changed since somebody answered, which a
/// project path recorded by answer 2 may pre-answer and this may not (SERVERS-5).
///
/// A `tools` line is a vouch for a tool list: the declaration it was listed by, and the digest of
/// the list as it was drawn. One list per declaration, since a server started from one declaration
/// has one list to offer at a time, and a list that changed is asked about again (SERVERS-8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Approvals {
    approved: BTreeMap<Digest, BTreeSet<String>>,
    changed: BTreeSet<String>,
    lists: BTreeMap<Digest, Digest>,
}

/// The word a line recording a changed declaration starts with.
const CHANGED: &str = "changed";

/// The word a line recording a vouched tool list starts with.
const TOOLS: &str = "tools";

impl Approvals {
    /// Read the approvals in a state directory.
    pub fn read(home: &Path) -> Self {
        bounded(&approvals_file(home))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Read the approvals to change and write back, refusing a file that is there and unreadable.
    pub fn to_change(home: &Path) -> Result<Self, Unreadable> {
        to_change(&approvals_file(home)).map(|text| Self::parse(&text))
    }

    /// Read approvals out of the file's text.
    ///
    /// A digest alone on its line is an approval under no alias, which is how the file was written
    /// before an alias was kept beside one.
    pub fn parse(text: &str) -> Self {
        let mut approvals = Self::default();
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().take(4).collect();
            match words.as_slice() {
                [CHANGED, alias] if is_alias(alias) => {
                    approvals.changed.insert(alias.to_string());
                }
                [TOOLS, declaration, list] => {
                    if let (Some(declaration), Some(list)) =
                        (Digest::parse(declaration), Digest::parse(list))
                    {
                        approvals.lists.insert(declaration, list);
                    }
                }
                [hex, alias @ ..] if alias.len() <= 1 && alias.iter().all(|a| is_alias(a)) => {
                    let Some(digest) = Digest::parse(hex) else {
                        continue;
                    };
                    let aliases = approvals.approved.entry(digest).or_default();
                    aliases.extend(alias.iter().map(|alias| alias.to_string()));
                }
                _ => {}
            }
        }
        approvals
    }

    /// The digest of the tool list vouched for under this declaration, where one was.
    pub fn vouched_list(&self, declaration: &Digest) -> Option<Digest> {
        self.lists.get(declaration).copied()
    }

    /// Record a vouch for the tool list a server started from `declaration` drew, replacing any
    /// earlier one: a list that changed and was vouched for again is the list now.
    pub fn vouch_list(&mut self, declaration: Digest, list: Digest) {
        self.lists.insert(declaration, list);
    }

    /// Whether this digest was approved.
    pub fn approves(&self, digest: &Digest) -> bool {
        self.approved.contains_key(digest)
    }

    /// Whether `alias` was approved as something other than `digest`, so the declaration it names
    /// now is one the person has not seen.
    pub fn changed(&self, alias: &str, digest: &Digest) -> bool {
        !self.approves(digest)
            && (self.changed.contains(alias)
                || self
                    .approved
                    .values()
                    .any(|aliases| aliases.contains(alias)))
    }

    /// Record an approval of this digest, given about `alias`.
    pub fn approve(&mut self, alias: &str, digest: Digest) {
        self.approved
            .entry(digest)
            .or_default()
            .insert(alias.to_string());
        self.changed.remove(alias);
    }

    /// Keep only the approvals a declaration still resolves to, each under the aliases that resolve
    /// to it, and record every alias whose approved declaration has changed.
    ///
    /// An approval outliving its declaration is a digest nothing resolves, and one that would answer
    /// for a declaration written back to exactly what it was, unseen. So a changed declaration's old
    /// digest is dropped here, and what is kept of it is that its alias changed, which approves
    /// nothing. Called whenever the declarations are rewritten, so the two files agree about what
    /// is approved.
    ///
    /// A vouched list is kept while a declaration still resolves to the digest it was listed by,
    /// approved or not: a server a project path started has a list somebody vouched for too.
    pub fn keep_only(&mut self, declarations: &Declarations) {
        let mut kept = Self::default();
        for Entry { alias, declaration } in declarations.entries() {
            let digest = declaration.as_ref().ok().map(Declaration::digest);
            if let Some(list) = digest.and_then(|digest| self.vouched_list(&digest)) {
                kept.lists.extend(digest.map(|digest| (digest, list)));
            }
            match digest {
                Some(digest) if self.approves(&digest) => {
                    kept.approved.entry(digest).or_default().insert(alias);
                }
                _ if self.changed.contains(&alias)
                    || self
                        .approved
                        .values()
                        .any(|aliases| aliases.contains(&alias)) =>
                {
                    kept.changed.insert(alias);
                }
                _ => {}
            }
        }
        *self = kept;
    }

    /// The file's text, one approval per line, then one changed alias per line, then one vouched
    /// list per line.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for (digest, aliases) in &self.approved {
            match aliases.is_empty() {
                true => text.push_str(&format!("{digest}\n")),
                false => {
                    for alias in aliases {
                        text.push_str(&format!("{digest} {alias}\n"));
                    }
                }
            }
        }
        for alias in &self.changed {
            text.push_str(&format!("{CHANGED} {alias}\n"));
        }
        for (declaration, list) in &self.lists {
            text.push_str(&format!("{TOOLS} {declaration} {list}\n"));
        }
        text
    }
}

/// The projects in which answer 2 said to use every future server, one path per line.
///
/// Pre-answers [SERVERS-4]'s question for a server a checkout in one of these requested, and
/// nothing else: not a capability, not a call, not a server whose declaration changed since it was
/// approved. A line that is not an absolute path is passed over.
///
/// [SERVERS-4]: ../../../docs/specs/mcp-servers.md#SERVERS-4
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projects {
    paths: BTreeSet<PathBuf>,
}

impl Projects {
    /// Read the projects recorded in a state directory.
    pub fn read(home: &Path) -> Self {
        bounded(&projects_file(home))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Read the projects to change and write back, refusing a file that is there and unreadable.
    pub fn to_change(home: &Path) -> Result<Self, Unreadable> {
        to_change(&projects_file(home)).map(|text| Self::parse(&text))
    }

    /// Read projects out of the file's text.
    pub fn parse(text: &str) -> Self {
        Self {
            paths: text
                .lines()
                .map(Path::new)
                .filter(|path| path.is_absolute())
                .map(Path::to_path_buf)
                .collect(),
        }
    }

    /// Whether `project` was recorded, as the path it was recorded as.
    pub fn contains(&self, project: &Path) -> bool {
        self.paths.contains(project)
    }

    /// Record `project`, where it is a path a line can hold. Whether it was recorded.
    pub fn add(&mut self, project: &Path) -> bool {
        let writable = project.is_absolute()
            && project
                .to_str()
                .is_some_and(|text| !text.contains(['\n', '\r']));
        if writable {
            self.paths.insert(project.to_path_buf());
        }
        writable
    }

    /// Drop `project`. Whether it was recorded.
    pub fn remove(&mut self, project: &Path) -> bool {
        self.paths.remove(project)
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        self.paths
            .iter()
            .filter_map(|path| path.to_str())
            .map(|path| format!("{path}\n"))
            .collect()
    }
}

/// One tool answer 2 was given about at a call: the alias, the tool's word, and the project.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StandingAnswer {
    alias: String,
    tool: String,
    project: PathBuf,
}

/// The tools a person said to stop asking about, each in the one project they said it in.
///
/// ```text
/// weather get_current_conditions /Users/someone/work/app
/// ```
///
/// Removes the default question for that one tool of that one server in that one project, and
/// nothing else: not another tool, not the same tool elsewhere, not a rule a person wrote, and not
/// a call carrying private data (SERVERS-7). The path is the rest of the line, so a project whose
/// path has a space in it is recorded whole. A line naming no alias, no tool word or no absolute
/// path is passed over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Standing {
    answers: BTreeSet<StandingAnswer>,
}

impl Standing {
    /// Read the standing answers recorded in a state directory.
    pub fn read(home: &Path) -> Self {
        bounded(&tools_file(home))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Read the standing answers to change and write back, refusing a file that is there and
    /// unreadable.
    pub fn to_change(home: &Path) -> Result<Self, Unreadable> {
        to_change(&tools_file(home)).map(|text| Self::parse(&text))
    }

    /// Read standing answers out of the file's text.
    pub fn parse(text: &str) -> Self {
        let answers = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, ' ');
                let (alias, tool, project) = (parts.next()?, parts.next()?, parts.next()?);
                let project = Path::new(project);
                (is_alias(alias) && is_tool_word(tool) && project.is_absolute()).then(|| {
                    StandingAnswer {
                        alias: alias.to_string(),
                        tool: tool.to_string(),
                        project: project.to_path_buf(),
                    }
                })
            })
            .collect();
        Self { answers }
    }

    /// Whether answer 2 was given about this tool of this server in `project`.
    pub fn covers(&self, alias: &str, tool: &str, project: &Path) -> bool {
        self.answers
            .iter()
            .any(|answer| answer.alias == alias && answer.tool == tool && answer.project == project)
    }

    /// Record answer 2 about this tool in `project`, where a line can hold all three. Whether it
    /// was recorded.
    pub fn add(&mut self, alias: &str, tool: &str, project: &Path) -> bool {
        let writable = is_alias(alias)
            && is_tool_word(tool)
            && project.is_absolute()
            && project
                .to_str()
                .is_some_and(|text| !text.contains(['\n', '\r']));
        if writable {
            self.answers.insert(StandingAnswer {
                alias: alias.to_string(),
                tool: tool.to_string(),
                project: project.to_path_buf(),
            });
        }
        writable
    }

    /// Drop every answer given in `project`. How many there were.
    pub fn forget(&mut self, project: &Path) -> usize {
        let before = self.answers.len();
        self.answers.retain(|answer| answer.project != project);
        before - self.answers.len()
    }

    /// The tools answered for in `project`, as `alias:tool`, in order.
    pub fn in_project(&self, project: &Path) -> Vec<String> {
        self.answers
            .iter()
            .filter(|answer| answer.project == project)
            .map(|answer| format!("{}:{}", answer.alias, answer.tool))
            .collect()
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        self.answers
            .iter()
            .filter_map(|answer| {
                let project = answer.project.to_str()?;
                Some(format!("{} {} {project}\n", answer.alias, answer.tool))
            })
            .collect()
    }
}

/// A small file's text, or `None` for one that is missing, too large, or not text.
fn bounded(path: &Path) -> Option<String> {
    match std::fs::metadata(path) {
        Ok(found) if found.len() <= MAX_BYTES => std::fs::read_to_string(path).ok(),
        _ => None,
    }
}

/// A record's text, to be changed and written back: nothing where there is no file yet, and an
/// error where there is one this cannot read, which written over would lose every line it holds.
fn to_change(path: &Path) -> Result<String, Unreadable> {
    match std::fs::metadata(path) {
        Ok(found) if found.len() > MAX_BYTES => Err(Unreadable::TooLarge),
        Ok(_) => std::fs::read_to_string(path).map_err(|_| Unreadable::NotRead),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(_) => Err(Unreadable::NotRead),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// A directory that is absolute on the platform the test runs on, which `/srv` is not on Windows.
    fn somewhere() -> String {
        env!("CARGO_MANIFEST_DIR").to_string()
    }

    fn weather() -> Declaration {
        Declaration::stdio(
            words(&["npx", "-y", "@dangahagan/weather-mcp@latest"]),
            words(&["PATH"]),
            None,
        )
        .expect("a declaration")
    }

    #[test]
    fn a_digest_covers_the_program_every_argument_the_names_and_the_directory() {
        let approved = weather().digest();
        let changed = [
            Declaration::stdio(
                words(&["npx", "-y", "@dangahagan/weather-mcp@1.2.0"]),
                words(&["PATH"]),
                None,
            ),
            Declaration::stdio(
                words(&["npx", "@dangahagan/weather-mcp@latest"]),
                words(&["PATH"]),
                None,
            ),
            Declaration::stdio(
                words(&["npx", "-y", "@dangahagan/weather-mcp@latest"]),
                words(&["PATH", "HOME"]),
                None,
            ),
            Declaration::stdio(
                words(&["npx", "-y", "@dangahagan/weather-mcp@latest"]),
                words(&["PATH"]),
                Some(somewhere()),
            ),
            Declaration::http("https://weather.example.com/mcp".into()),
        ];
        for declaration in changed {
            let declaration = declaration.expect("a declaration");
            assert_ne!(
                declaration.digest(),
                approved,
                "{declaration:?} digests as the approved declaration does"
            );
        }
    }

    #[test]
    fn a_declaration_storing_no_value_digests_as_it_did_before_values_could_be_stored() {
        assert_eq!(
            weather().digest().to_string(),
            "25edc5e806956f3254b5ff2b116f1c07ae5ad8d4a5df00f6d15c0f8aadfec43f"
        );
    }

    #[test]
    fn two_arguments_digest_apart_from_one_holding_the_same_characters() {
        let two = Declaration::stdio(words(&["run", "a", "b"]), Vec::new(), None).unwrap();
        let one = Declaration::stdio(words(&["run", "a b"]), Vec::new(), None).unwrap();
        assert_ne!(two.digest(), one.digest());
    }

    #[test]
    fn a_digest_is_the_same_whatever_the_alias_or_the_order_of_the_names() {
        let mut first = Declarations::default();
        first.insert(
            "weather",
            &Declaration::stdio(words(&["server"]), words(&["PATH", "HOME"]), None).unwrap(),
        );
        let mut second = Declarations::default();
        second.insert(
            "forecast",
            &Declaration::stdio(words(&["server"]), words(&["HOME", "PATH"]), None).unwrap(),
        );
        assert_eq!(first.digests(), second.digests());
    }

    fn stored(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn a_value_is_stored_under_its_name_and_one_written_as_a_name_is_refused_unrepeated() {
        let declaration = Declaration::stdio(words(&["server"]), words(&["PATH"]), None)
            .unwrap()
            .storing(stored(&[("WEATHER_TOKEN", "sk-live-0123456789")]))
            .unwrap();
        assert_eq!(
            declaration.stored().collect::<Vec<_>>(),
            vec!["WEATHER_TOKEN"]
        );
        assert_eq!(declaration.variables(), ["PATH"]);

        let refused = Declaration::stdio(
            words(&["server"]),
            words(&["WEATHER_TOKEN=sk-live-0123456789"]),
            None,
        );
        assert_eq!(refused, Err(Problem::Name));
        assert!(!format!("{refused:?}").contains("sk-live"));
    }

    #[test]
    fn an_env_block_is_stored_values_and_an_object_of_variables_is_not_a_list_of_names() {
        let text = r#"{"servers": {"weather": {"transport": "stdio", "argv": ["server"], "env": {"TOKEN": "sk-live"}}}}"#;
        let entry = Declarations::parse(text).unwrap().get("weather").unwrap();
        assert_eq!(
            entry.declaration,
            Declaration::stdio(words(&["server"]), Vec::new(), None)
                .unwrap()
                .storing(stored(&[("TOKEN", "sk-live")]))
        );

        let text = r#"{"servers": {"weather": {"transport": "stdio", "argv": ["server"], "variables": {"TOKEN": "sk-live"}}}}"#;
        let entry = Declarations::parse(text).unwrap().get("weather").unwrap();
        assert_eq!(entry.declaration, Err(Problem::Name));
        assert!(!format!("{:?}", entry.declaration).contains("sk-live"));
    }

    #[test]
    fn a_stored_value_no_environment_could_hold_or_a_second_answer_is_refused_by_its_name() {
        let plain = || Declaration::stdio(words(&["server"]), words(&["PATH"]), None).unwrap();
        assert_eq!(
            plain().storing(stored(&[("TOKEN", "sk-live\0tail")])),
            Err(Problem::Value("TOKEN".into()))
        );
        assert_eq!(
            plain().storing(stored(&[("PATH", "/usr/bin")])),
            Err(Problem::Twice("PATH".into()))
        );
        assert_eq!(
            plain().storing(stored(&[("sk-live", "TOKEN")])),
            Err(Problem::Name)
        );
        assert!(plain().storing(stored(&[("EMPTY", "")])).is_ok());

        for (entry, problem) in [
            (r#""env": {"TOKEN": 5}"#, Problem::Value("TOKEN".into())),
            (r#""env": {"sk-live": "x"}"#, Problem::Name),
            (r#""env": ["TOKEN=sk-live"]"#, Problem::Env),
            (r#""reads": ["keys/weather"]"#, Problem::Reads),
            (r#""reads": "/keys/weather""#, Problem::Reads),
            (
                r#""variables": ["TOKEN"], "env": {"TOKEN": "x"}"#,
                Problem::Twice("TOKEN".into()),
            ),
        ] {
            let text = format!(
                r#"{{"servers": {{"weather": {{"transport": "stdio", "argv": ["server"], {entry}}}}}}}"#
            );
            let parsed = Declarations::parse(&text).unwrap();
            assert_eq!(
                parsed.get("weather").unwrap().declaration,
                Err(problem),
                "{entry}"
            );
        }
    }

    /// SERVERS-10: a stored value is debugged by its name and never as itself, as a declaration, as
    /// the entry the file holds and as the file, so none reaches a log or a panic.
    #[test]
    fn a_stored_value_is_debugged_by_its_name_alone() {
        let declaration = weather()
            .storing(stored(&[("WEATHER_TOKEN", "hunter2-token")]))
            .expect("a declaration");
        let mut file = Declarations::default();
        file.insert("weather", &declaration);
        let entry = file.get("weather").expect("the entry");
        for (debugged, named) in [
            (format!("{declaration:?}"), "WEATHER_TOKEN"),
            (format!("{declaration:#?}"), "WEATHER_TOKEN"),
            (format!("{entry:?}"), "WEATHER_TOKEN"),
            (format!("{file:?}"), "weather"),
        ] {
            assert!(debugged.contains(named), "{debugged}");
            assert!(!debugged.contains("hunter2"), "{debugged}");
        }
    }

    #[test]
    fn a_digest_covers_every_stored_value_and_every_file_it_may_read() {
        let storing = |pairs: &[(&str, &str)], reads: &[&str]| {
            weather()
                .storing(stored(pairs))
                .and_then(|declaration| declaration.reading(words(reads)))
                .expect("a declaration")
        };
        let key = somewhere();
        let other = Path::new(&key).join("other").display().to_string();
        let approved = storing(&[("KEY_FILE", &key)], &[&key]).digest();
        for changed in [
            weather(),
            storing(&[("KEY_FILE", &other)], &[&key]),
            storing(&[("KEY_PATH", &key)], &[&key]),
            storing(&[("KEY_FILE", &key), ("REGION", "eu")], &[&key]),
            storing(&[("KEY_FILE", &key)], &[]),
            storing(&[("KEY_FILE", &key)], &[&key, &other]),
        ] {
            assert_ne!(changed.digest(), approved, "{changed:?}");
        }
        assert_eq!(
            storing(&[], &[&key, &other]).digest(),
            storing(&[], &[&other, &key]).digest(),
            "the files are a set, whatever order they were found in"
        );
        assert_ne!(storing(&[], &[&key]).digest(), weather().digest());
    }

    #[test]
    fn a_name_that_is_not_one_is_refused() {
        for name in ["", "1PATH", "MY-NAME", "A B", "=value"] {
            assert_eq!(
                Declaration::stdio(words(&["server"]), words(&[name]), None),
                Err(Problem::Name),
                "{name:?} was taken for a name"
            );
        }
    }

    #[test]
    fn a_url_carrying_a_user_or_a_password_is_refused() {
        for url in [
            "https://token@weather.example.com/mcp",
            "https://user:secret@weather.example.com/mcp",
        ] {
            assert_eq!(
                Declaration::http(url.into()),
                Err(Problem::Credentials),
                "{url}"
            );
        }
        assert!(Declaration::http("https://weather.example.com/mcp?to=a@b".into()).is_ok());
    }

    #[test]
    fn a_url_is_http_or_https_with_a_host() {
        for url in [
            "ftp://weather.example.com",
            "https://",
            "https:///mcp",
            "https://:8080/mcp",
            "weather.example.com",
            "https://weather example.com",
        ] {
            assert_eq!(Declaration::http(url.into()), Err(Problem::Url), "{url}");
        }
        for url in [
            "http://127.0.0.1:8080/mcp",
            "HTTPS://Weather.Example.com",
            "http://[::1]:9000",
        ] {
            assert!(Declaration::http(url.into()).is_ok(), "{url}");
        }
    }

    #[test]
    fn a_directory_is_absolute() {
        assert_eq!(
            Declaration::stdio(words(&["server"]), Vec::new(), Some("srv/weather".into())),
            Err(Problem::Directory)
        );
    }

    #[test]
    fn a_remote_server_takes_no_variables_values_reads_or_directory() {
        let text = r#"{"servers": {
            "a": {"transport": "http", "url": "https://a.example.com", "variables": ["PATH"]},
            "b": {"transport": "http", "url": "https://b.example.com", "directory": "/srv"},
            "c": {"transport": "http", "url": "https://c.example.com", "env": {"TOKEN": "x"}},
            "d": {"transport": "http", "url": "https://d.example.com", "reads": ["/srv/key"]}}}"#;
        let declarations = Declarations::parse(text).unwrap();
        for (alias, key) in [
            ("a", "variables"),
            ("b", "directory"),
            ("c", "env"),
            ("d", "reads"),
        ] {
            assert_eq!(
                declarations.get(alias).unwrap().declaration,
                Err(Problem::Remote(key))
            );
        }
        let remote = || Declaration::http("https://a.example.com".into()).unwrap();
        assert_eq!(
            remote().storing(stored(&[("TOKEN", "x")])),
            Err(Problem::Remote("env"))
        );
        assert_eq!(
            remote().reading(words(&["/srv/key"])),
            Err(Problem::Remote("reads"))
        );
    }

    #[test]
    fn a_key_the_format_does_not_have_makes_that_entry_a_problem_and_no_other() {
        let text = r#"{"servers": {
            "weather": {"transport": "stdio", "argv": ["server"], "enabled": true},
            "docs": {"transport": "http", "url": "https://docs.example.com/mcp"}}}"#;
        let declarations = Declarations::parse(text).unwrap();
        assert_eq!(
            declarations.get("weather").unwrap().declaration,
            Err(Problem::Key("enabled".into()))
        );
        assert!(declarations.get("docs").unwrap().declaration.is_ok());
    }

    #[test]
    fn an_alias_that_could_not_name_a_tool_namespace_is_a_problem() {
        for alias in ["", "weather:get", "-weather", "a b", &"a".repeat(65)] {
            assert!(!is_alias(alias), "{alias:?} was taken for an alias");
        }
        let text = r#"{"servers": {"weather:get": {"transport": "stdio", "argv": ["server"]}}}"#;
        let entry = Declarations::parse(text)
            .unwrap()
            .get("weather:get")
            .unwrap();
        assert_eq!(entry.declaration, Err(Problem::Alias));
    }

    #[test]
    fn a_root_holding_more_than_servers_is_unreadable() {
        assert_eq!(
            Declarations::parse(r#"{"servers": {}, "mcpServers": {}}"#),
            Err(Unreadable::Key("mcpServers".into()))
        );
        assert_eq!(
            Declarations::parse(r#"{"servers": []}"#),
            Err(Unreadable::Servers)
        );
        assert_eq!(Declarations::parse("[]"), Err(Unreadable::NotAnObject));
        assert_eq!(Declarations::parse("{"), Err(Unreadable::NotJson));
        assert_eq!(Declarations::parse("{}"), Ok(Declarations::default()));
    }

    #[test]
    fn an_entry_that_cannot_be_used_is_written_back_as_it_was() {
        let broken = r#"{"transport": "stdio", "argv": "npx -y weather-mcp"}"#;
        let text = format!(r#"{{"servers": {{"broken": {broken}}}}}"#);
        let mut declarations = Declarations::parse(&text).unwrap();
        declarations.insert("weather", &weather());

        let rewritten = Declarations::parse(&declarations.to_text()).unwrap();
        assert_eq!(
            rewritten.get("broken").unwrap().declaration,
            Err(Problem::Program)
        );
        assert_eq!(
            rewritten.servers.get("broken"),
            Some(&serde_json::from_str::<serde_json::Value>(broken).unwrap())
        );
        assert_eq!(rewritten.get("weather").unwrap().declaration, Ok(weather()));
    }

    #[test]
    fn a_written_declaration_reads_back_as_the_same_declaration() {
        let declaration = Declaration::stdio(
            words(&["uvx", "server", "--port", ""]),
            words(&["PATH", "HOME"]),
            Some(somewhere()),
        )
        .unwrap()
        .storing(stored(&[("KEY_FILE", &somewhere()), ("EMPTY", "")]))
        .unwrap()
        .reading(vec![somewhere()])
        .unwrap();
        let mut declarations = Declarations::default();
        declarations.insert("weather", &declaration);
        let read = Declarations::parse(&declarations.to_text()).unwrap();
        assert_eq!(read.get("weather").unwrap().declaration, Ok(declaration));
    }

    #[test]
    fn what_changed_is_named_by_field() {
        let pinned = Declaration::stdio(
            words(&["npx", "-y", "@dangahagan/weather-mcp@1.2.0"]),
            words(&["PATH"]),
            None,
        )
        .unwrap();
        assert_eq!(weather().changes(&pinned), vec![Field::Argv]);
        let naming =
            |names: &[&str]| Declaration::stdio(words(&["server"]), words(names), None).unwrap();
        assert_eq!(
            naming(&["PATH", "HOME"]).changes(&naming(&["HOME", "PATH"])),
            Vec::new()
        );
        assert_eq!(
            naming(&["PATH"]).changes(&naming(&["PATH", "HOME"])),
            vec![Field::Variables]
        );
        let storing = |pairs: &[(&str, &str)]| naming(&[]).storing(stored(pairs)).unwrap();
        assert_eq!(
            storing(&[("KEY", "old"), ("REGION", "eu"), ("GONE", "x")]).changes(&storing(&[
                ("KEY", "new"),
                ("REGION", "eu"),
                ("ADDED", "y")
            ])),
            vec![Field::Env(words(&["ADDED", "GONE", "KEY"]))]
        );
        let reading = |files: &[&str]| {
            let files = files.iter().map(|file| format!("{}{file}", somewhere()));
            naming(&[]).reading(files.collect()).unwrap()
        };
        assert_eq!(
            reading(&["/a", "/b"]).changes(&reading(&["/b", "/a"])),
            Vec::new()
        );
        assert_eq!(
            reading(&["/a"]).changes(&reading(&["/b"])),
            vec![Field::Reads]
        );
        let remote = Declaration::http("https://weather.example.com".into()).unwrap();
        assert_eq!(weather().changes(&remote), vec![Field::Transport]);
    }

    #[test]
    fn a_digest_reads_back_from_the_line_it_is_written_as() {
        let digest = weather().digest();
        let approvals = Approvals::parse(&format!("not a digest\n{digest}\n\n"));
        assert!(approvals.approves(&digest));
        assert_eq!(approvals.to_text(), format!("{digest}\n"));
        assert_eq!(digest.short(), digest.to_string()[..8]);
        assert_eq!(Digest::parse(&digest.short()), None);
    }

    #[test]
    fn an_approval_reads_back_with_the_alias_it_was_given_about() {
        let digest = weather().digest();
        let mut approvals = Approvals::default();
        approvals.approve("weather", digest);
        let text = approvals.to_text();
        assert_eq!(text, format!("{digest} weather\n"));
        assert_eq!(Approvals::parse(&text), approvals);
        let spoiled = Approvals::parse(&format!("{digest} not:an:alias\nchanged\n"));
        assert_eq!(spoiled, Approvals::default());
    }

    #[test]
    fn an_approval_nothing_resolves_to_is_dropped() {
        let other = Declaration::http("https://docs.example.com".into()).unwrap();
        let mut approvals = Approvals::default();
        approvals.approve("weather", weather().digest());
        approvals.approve("docs", other.digest());

        let mut declarations = Declarations::default();
        declarations.insert("docs", &other);
        approvals.keep_only(&declarations);

        assert!(!approvals.approves(&weather().digest()));
        assert!(approvals.approves(&other.digest()));
        assert!(!approvals.changed("weather", &weather().digest()));
        assert_eq!(approvals.to_text(), format!("{} docs\n", other.digest()));
    }

    /// SERVERS-5: a declaration changed since it was approved is recorded as changed, and the
    /// record approves nothing, so writing the declaration back to what it was asks again.
    #[test]
    fn a_declaration_changed_since_its_approval_is_recorded_as_changed_and_approves_nothing() {
        let pinned =
            Declaration::stdio(words(&["npx", "-y", "weather-mcp@1.2.0"]), Vec::new(), None)
                .unwrap();
        let mut approvals = Approvals::default();
        approvals.approve("weather", weather().digest());
        let mut declarations = Declarations::default();
        declarations.insert("weather", &pinned);

        assert!(approvals.changed("weather", &pinned.digest()));
        approvals.keep_only(&declarations);
        assert!(approvals.changed("weather", &pinned.digest()));
        assert!(!approvals.approves(&weather().digest()));
        assert_eq!(approvals.to_text(), "changed weather\n");

        declarations.insert("weather", &weather());
        approvals.keep_only(&declarations);
        assert!(!approvals.approves(&weather().digest()));
        assert!(approvals.changed("weather", &weather().digest()));

        approvals.approve("weather", weather().digest());
        assert!(!approvals.changed("weather", &weather().digest()));
        assert!(!approvals.changed("docs", &pinned.digest()));
    }

    /// A digest written before aliases were kept is still an approval, and takes the alias that
    /// resolves to it the first time the file is rewritten.
    #[test]
    fn a_digest_alone_on_its_line_takes_its_alias_when_rewritten() {
        let digest = weather().digest();
        let mut approvals = Approvals::parse(&format!("{digest}\n"));
        let mut declarations = Declarations::default();
        declarations.insert("weather", &weather());
        approvals.keep_only(&declarations);
        assert_eq!(approvals.to_text(), format!("{digest} weather\n"));
    }

    /// SERVERS-4's answer 2 is recorded as the project's path and read back as it, and a path a
    /// line cannot hold is not recorded as some other path.
    #[test]
    fn a_project_reads_back_as_the_path_it_was_recorded_as() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut projects = Projects::default();
        assert!(projects.add(project));
        assert!(!projects.add(Path::new("relative/checkout")));
        assert!(!projects.add(&project.join("two\nlines")));
        let read = Projects::parse(&format!("{}not absolute\n", projects.to_text()));
        assert_eq!(read, projects);
        assert!(read.contains(project));
        assert!(!read.contains(&project.join("src")));
    }

    #[test]
    fn forgetting_a_project_drops_it_and_no_other() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let other = project.join("src");
        let mut projects = Projects::default();
        assert!(projects.add(project) && projects.add(&other));
        assert!(projects.remove(project));
        assert!(!projects.remove(project), "a project was dropped twice");
        assert!(!projects.contains(project));
        assert!(projects.contains(&other));
    }

    #[test]
    fn a_vouched_list_reads_back_beside_the_approval_it_was_listed_by() {
        let mut approvals = Approvals::default();
        approvals.approve("weather", weather().digest());
        let list = Digest::of_list("[]");
        approvals.vouch_list(weather().digest(), list);
        let read = Approvals::parse(&approvals.to_text());
        assert_eq!(read, approvals);
        assert_eq!(read.vouched_list(&weather().digest()), Some(list));
        assert!(read.approves(&weather().digest()));
        assert_ne!(
            Digest::of_list("[]"),
            Digest::of_list("[ ]"),
            "a list drawn differently digested alike"
        );
    }

    /// A list is vouched for under a declaration, so a declaration rewritten to something else takes
    /// its vouch with it, and one that is still there keeps it whether or not its digest is approved:
    /// a project path starts a server nobody approved by digest, and its list was still vouched for.
    #[test]
    fn a_vouched_list_lasts_as_long_as_its_declaration_resolves() {
        let mut declarations = Declarations::default();
        declarations.insert("weather", &weather());
        let mut approvals = Approvals::default();
        approvals.vouch_list(weather().digest(), Digest::of_list("[]"));
        approvals.keep_only(&declarations);
        assert_eq!(
            approvals.vouched_list(&weather().digest()),
            Some(Digest::of_list("[]")),
            "a vouch was dropped while its declaration still resolved"
        );

        let pinned = Declaration::stdio(
            words(&["npx", "-y", "@dangahagan/weather-mcp@1.0.0"]),
            words(&["PATH"]),
            None,
        )
        .expect("a declaration");
        declarations.insert("weather", &pinned);
        approvals.keep_only(&declarations);
        assert_eq!(approvals.vouched_list(&weather().digest()), None);
        assert_eq!(approvals.vouched_list(&pinned.digest()), None);
    }

    #[test]
    fn a_line_that_is_not_a_vouch_is_passed_over() {
        let digest = weather().digest();
        let read = Approvals::parse(&format!(
            "tools {digest}\ntools {digest} nothing\ntools {digest} {digest} more\n"
        ));
        assert_eq!(read, Approvals::default());
    }

    #[test]
    fn a_standing_answer_reaches_one_tool_of_one_server_in_one_project() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let other = project.join("src");
        let mut standing = Standing::default();
        assert!(standing.add("weather", "get_current_conditions", project));
        assert!(standing.covers("weather", "get_current_conditions", project));
        assert!(!standing.covers("weather", "get_forecast", project));
        assert!(!standing.covers("docs", "get_current_conditions", project));
        assert!(!standing.covers("weather", "get_current_conditions", &other));
    }

    #[test]
    fn a_standing_answer_reads_back_with_a_space_in_its_project() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("a checkout");
        let mut standing = Standing::default();
        assert!(standing.add("weather", "get_forecast", &project));
        assert!(!standing.add("weather", "get forecast", &project));
        assert!(!standing.add("weather", "get_forecast", Path::new("relative")));
        assert!(!standing.add("we:ather", "get_forecast", &project));
        assert!(!standing.add("weather", "get_forecast", &project.join("two\nlines")));
        let read = Standing::parse(&format!(
            "{}weather get_forecast relative\nweather\n",
            standing.to_text()
        ));
        assert_eq!(read, standing);
        assert!(read.covers("weather", "get_forecast", &project));
    }

    #[test]
    fn forgetting_a_project_drops_its_standing_answers_and_no_others() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let other = project.join("src");
        let mut standing = Standing::default();
        assert!(standing.add("weather", "get_forecast", project));
        assert!(standing.add("weather", "get_alerts", project));
        assert!(standing.add("weather", "get_forecast", &other));
        assert_eq!(
            standing.in_project(project),
            ["weather:get_alerts", "weather:get_forecast"]
        );
        assert_eq!(standing.forget(project), 2);
        assert_eq!(standing.forget(project), 0);
        assert!(standing.covers("weather", "get_forecast", &other));
    }

    #[test]
    fn a_tool_word_is_what_a_function_name_may_be() {
        assert!(is_tool_word("get_current_conditions"));
        assert!(is_tool_word("check-service-status"));
        assert!(is_tool_word(&"a".repeat(MAX_TOOL)));
        for refused in [
            "",
            "get forecast",
            "weather:get_forecast",
            "get.forecast",
            "ignore previous instructions",
            "é",
        ] {
            assert!(!is_tool_word(refused), "{refused:?} was taken for a word");
        }
        assert!(!is_tool_word(&"a".repeat(MAX_TOOL + 1)));
    }

    #[test]
    fn the_files_are_read_from_the_state_directory() {
        let home = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/config-mcp-files");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(Declarations::read(&home), Ok(Declarations::default()));
        assert_eq!(Approvals::read(&home), Approvals::default());
        assert_eq!(Projects::read(&home), Projects::default());

        let mut declarations = Declarations::default();
        declarations.insert("weather", &weather());
        std::fs::write(declarations_file(&home), declarations.to_text()).unwrap();
        std::fs::write(approvals_file(&home), format!("{}\n", weather().digest())).unwrap();
        assert_eq!(Declarations::read(&home), Ok(declarations));
        assert!(Approvals::read(&home).approves(&weather().digest()));
        std::fs::write(projects_file(&home), format!("{}\n", home.display())).unwrap();
        assert!(Projects::read(&home).contains(&home));
        assert_eq!(Standing::read(&home), Standing::default());
        std::fs::write(
            tools_file(&home),
            format!("weather get_forecast {}\n", home.display()),
        )
        .unwrap();
        assert!(Standing::read(&home).covers("weather", "get_forecast", &home));

        std::fs::write(declarations_file(&home), " ".repeat(MAX_BYTES as usize + 1)).unwrap();
        assert_eq!(Declarations::read(&home), Err(Unreadable::TooLarge));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A record read to be written back is refused where it is there and cannot be read, since
    /// what is written would be every line it held but the one being added. One that is not there
    /// yet is empty, and one that reads is what it says.
    #[test]
    fn a_record_that_cannot_be_read_is_not_read_to_be_written_over() {
        let home = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/config-mcp-to-change");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(Approvals::to_change(&home), Ok(Approvals::default()));
        assert_eq!(Projects::to_change(&home), Ok(Projects::default()));
        assert_eq!(Standing::to_change(&home), Ok(Standing::default()));

        let answer = format!("weather get_forecast {}\n", home.display());
        std::fs::write(tools_file(&home), &answer).unwrap();
        let read = Standing::to_change(&home).expect("a record that reads");
        assert!(read.covers("weather", "get_forecast", &home));

        let too_large = " ".repeat(MAX_BYTES as usize + 1);
        for file in [
            approvals_file(&home),
            projects_file(&home),
            tools_file(&home),
        ] {
            std::fs::write(&file, &too_large).unwrap();
        }
        assert_eq!(Approvals::to_change(&home), Err(Unreadable::TooLarge));
        assert_eq!(Projects::to_change(&home), Err(Unreadable::TooLarge));
        assert_eq!(Standing::to_change(&home), Err(Unreadable::TooLarge));

        std::fs::write(tools_file(&home), [0xff, 0xfe, b'\n']).unwrap();
        assert_eq!(Standing::to_change(&home), Err(Unreadable::NotRead));
        let _ = std::fs::remove_dir_all(&home);
    }
}
