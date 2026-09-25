//! The MCP server declarations, `mcp.json` in the person's own state directory, and the record of
//! which of them the person approved, `mcp-approved` beside it.
//!
//! A declaration says that a server exists: its alias, its transport, the program and arguments
//! that start it or the url that reaches it, the names of the variables it needs, and the directory
//! it runs in. This module reads and writes that format and computes what an approval binds to. It
//! starts nothing and reaches nothing: launching a server is the client's, and deciding whether one
//! is offered to a session is the agent's.
//!
//! # Why not a settings layer
//!
//! A stdio declaration is a command to run, and BACKEND-1 forbids a settings file naming one. The
//! stronger reason is that two of the settings layers sit in a checkout, and the agent can write
//! files in a checkout, so a declaration read from one would be a road from an ordinary tool call to
//! execution nobody approved. A file in the state directory is outside every directory a session can
//! write. [`crate::Settings::mcp_declared`] is where a layer that tried anyway is reported.
//!
//! # Names, never values
//!
//! `variables` lists the names of variables a server needs. Their values are the person's own
//! environment at launch and are written nowhere, so an entry spelled `NAME=value` is refused, and
//! what is said about it names the variable and never repeats the value.
//!
//! # Every failure is per entry, except the file's own shape
//!
//! An entry this cannot use is kept, listed with what is wrong with it, and written back unchanged
//! when another entry is added or removed. A file whose root is not the shape below cannot be
//! rewritten without losing what is in it, so it is reported as unreadable and never overwritten.
//!
//! ```json
//! { "servers": {
//!     "weather": { "transport": "stdio", "argv": ["npx", "-y", "weather-mcp"], "variables": ["PATH"] },
//!     "docs": { "transport": "http", "url": "https://docs.example.com/mcp" } } }
//! ```

use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The declarations, inside the state directory.
const DECLARATIONS_FILE: &str = "mcp.json";

/// The approvals, one digest per line, inside the state directory.
const APPROVALS_FILE: &str = "mcp-approved";

/// The most of either file worth reading.
///
/// A handful of short argument vectors, or a handful of digests. Bounded so a file that grew by
/// accident, or was replaced by something else entirely, is passed over rather than parsed.
const MAX_BYTES: u64 = 64 * 1024;

/// The longest alias, which is also the first half of every tool name a server offers.
const MAX_ALIAS: usize = 64;

/// What the digested form starts with, so a later change to that form cannot collide with this one.
const DIGEST_FORM: &str = "bravebot-mcp-declaration-1";

/// Where a person's servers are declared.
pub fn declarations_file(directory: &Path) -> PathBuf {
    directory.join(DECLARATIONS_FILE)
}

/// Where a person's approvals are recorded.
pub fn approvals_file(directory: &Path) -> PathBuf {
    directory.join(APPROVALS_FILE)
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
/// Nothing here carries a value. The one variant that names something from a variable list names
/// the part before the `=`, and only where that part is itself a name.
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
    /// Values where the names belong: an `env` block, or an object in place of the list.
    Values,
    /// `argv` is missing or empty, names a blank program, or holds something that is not a string.
    Program,
    /// An entry in `variables` is not a string, or is not a name.
    Name,
    /// An entry in `variables` is `NAME=value`, and this is the name.
    Assignment(String),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    /// A program this machine starts, spoken to over its standard streams.
    Stdio {
        /// The program and its arguments, never a line. Never empty.
        argv: Vec<String>,
        /// The names of the variables it receives, each once, in the order they were written.
        variables: Vec<String>,
        /// Where it runs, where somebody said. Absolute.
        directory: Option<String>,
    },
    /// A server somewhere else, reached over HTTP.
    Http {
        /// Where it is, as somebody wrote it.
        url: String,
    },
}

/// A field of a declaration, for saying which of them changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Transport,
    Argv,
    Variables,
    Directory,
    Url,
}

impl Field {
    /// The key the file spells it with.
    pub fn key(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Argv => "argv",
            Self::Variables => "variables",
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

    /// The names of the variables this server receives. None for a remote one.
    pub fn variables(&self) -> &[String] {
        match self {
            Self::Stdio { variables, .. } => variables,
            Self::Http { .. } => &[],
        }
    }

    /// What an approval of this declaration binds to.
    ///
    /// The transport, the program and every argument or the url, the set of variable names, and
    /// the directory. Not the alias: an alias is a label a person chose, and an approval keyed on
    /// it would let an edit to the argv inherit an answer given about a different program. Not the
    /// order the names were written in either, since the set is what the server receives.
    pub fn digest(&self) -> Digest {
        let form = match self {
            Self::Stdio {
                argv,
                variables,
                directory,
            } => {
                let variables: BTreeSet<&String> = variables.iter().collect();
                serde_json::json!([DIGEST_FORM, "stdio", argv, variables, directory])
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
                    directory,
                },
                Self::Stdio {
                    argv: other_argv,
                    variables: other_variables,
                    directory: other_directory,
                },
            ) => {
                let set = |names: &[String]| names.iter().cloned().collect::<BTreeSet<_>>();
                [
                    (argv != other_argv, Field::Argv),
                    (set(variables) != set(other_variables), Field::Variables),
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
                directory,
            } => {
                entry.insert("argv".into(), argv.clone().into());
                if !variables.is_empty() {
                    entry.insert("variables".into(), variables.clone().into());
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
        let allowed: &[&str] = match entry.get("transport").and_then(|word| word.as_str()) {
            Some("stdio") => &["transport", "argv", "variables", "directory"],
            Some("http") => &["transport", "url"],
            _ => return Err(Problem::Transport),
        };
        for key in entry.keys() {
            if allowed.contains(&key.as_str()) {
                continue;
            }
            return Err(match key.as_str() {
                // Claude Code's spelling, which holds values. Named for what it is rather than as
                // an unknown key, because somebody who wrote it meant to pass those values on.
                "env" => Problem::Values,
                "variables" => Problem::Remote("variables"),
                "directory" => Problem::Remote("directory"),
                other => Problem::Key(other.to_string()),
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
            Some(serde_json::Value::Object(_)) => return Err(Problem::Values),
            Some(_) => return Err(Problem::Name),
        };
        let directory = match entry.get("directory") {
            None => None,
            Some(serde_json::Value::String(directory)) => Some(directory.clone()),
            Some(_) => return Err(Problem::Directory),
        };
        Self::stdio(argv, variables, directory)
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
fn names(variables: Vec<String>) -> Result<Vec<String>, Problem> {
    let mut kept: Vec<String> = Vec::with_capacity(variables.len());
    for name in variables {
        if let Some((before, _)) = name.split_once('=') {
            // The part after the `=` is dropped here, before anything could print it.
            return Err(match is_variable_name(before) {
                true => Problem::Assignment(before.to_string()),
                false => Problem::Name,
            });
        }
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
/// No credential in the url, for the reason no value is in a variable list: this file holds what a
/// server is, and a token written into it is a token kept in plain text beside every server.
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
/// this cannot use is written back as it was when another one changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declarations {
    servers: serde_json::Map<String, serde_json::Value>,
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

/// The digests the person approved.
///
/// A line that spells no digest is passed over rather than refused, and an unreadable file approves
/// nothing, which is the direction to be wrong in: a server nobody can show was approved is asked
/// about again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Approvals {
    digests: BTreeSet<Digest>,
}

impl Approvals {
    /// Read the approvals in a state directory.
    pub fn read(home: &Path) -> Self {
        let path = approvals_file(home);
        match std::fs::metadata(&path) {
            Ok(found) if found.len() <= MAX_BYTES => {}
            _ => return Self::default(),
        }
        std::fs::read_to_string(&path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Read approvals out of the file's text.
    pub fn parse(text: &str) -> Self {
        Self {
            digests: text
                .lines()
                .filter_map(|line| Digest::parse(line.trim()))
                .collect(),
        }
    }

    /// Whether this digest was approved.
    pub fn approves(&self, digest: &Digest) -> bool {
        self.digests.contains(digest)
    }

    /// Record an approval of this digest.
    pub fn approve(&mut self, digest: Digest) {
        self.digests.insert(digest);
    }

    /// Keep only the approvals some declaration still resolves to.
    ///
    /// An approval outliving its declaration is a digest nothing resolves, and one that would answer
    /// for a declaration written back to exactly what it was, unseen. Called whenever the
    /// declarations are rewritten, so the two files agree about what is approved.
    pub fn keep_only(&mut self, live: &BTreeSet<Digest>) {
        self.digests.retain(|digest| live.contains(digest));
    }

    /// The file's text, one digest per line.
    pub fn to_text(&self) -> String {
        self.digests
            .iter()
            .map(|digest| format!("{digest}\n"))
            .collect()
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

    #[test]
    fn a_value_written_in_place_of_a_name_is_refused_without_repeating_it() {
        let refused = Declaration::stdio(
            words(&["server"]),
            words(&["WEATHER_TOKEN=sk-live-0123456789"]),
            None,
        );
        assert_eq!(refused, Err(Problem::Assignment("WEATHER_TOKEN".into())));
        assert!(!format!("{refused:?}").contains("sk-live"));
    }

    #[test]
    fn an_env_block_or_an_object_of_variables_is_values_and_is_refused() {
        for text in [
            r#"{"servers": {"weather": {"transport": "stdio", "argv": ["server"], "env": {"TOKEN": "sk-live"}}}}"#,
            r#"{"servers": {"weather": {"transport": "stdio", "argv": ["server"], "variables": {"TOKEN": "sk-live"}}}}"#,
        ] {
            let entry = Declarations::parse(text).unwrap().get("weather").unwrap();
            assert_eq!(entry.declaration, Err(Problem::Values), "{text}");
        }
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
    fn a_remote_server_takes_no_variables_and_no_directory() {
        let text = r#"{"servers": {
            "a": {"transport": "http", "url": "https://a.example.com", "variables": ["PATH"]},
            "b": {"transport": "http", "url": "https://b.example.com", "directory": "/srv"}}}"#;
        let declarations = Declarations::parse(text).unwrap();
        assert_eq!(
            declarations.get("a").unwrap().declaration,
            Err(Problem::Remote("variables"))
        );
        assert_eq!(
            declarations.get("b").unwrap().declaration,
            Err(Problem::Remote("directory"))
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
        let reordered = Declaration::stdio(
            words(&["npx", "-y", "@dangahagan/weather-mcp@latest"]),
            words(&["PATH", "PATH"]),
            None,
        )
        .unwrap();
        assert_eq!(weather().changes(&reordered), Vec::new());
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
    fn an_approval_nothing_resolves_to_is_dropped() {
        let other = Declaration::http("https://docs.example.com".into()).unwrap();
        let mut approvals = Approvals::default();
        approvals.approve(weather().digest());
        approvals.approve(other.digest());

        let mut declarations = Declarations::default();
        declarations.insert("docs", &other);
        approvals.keep_only(&declarations.digests());

        assert!(!approvals.approves(&weather().digest()));
        assert!(approvals.approves(&other.digest()));
    }

    #[test]
    fn the_files_are_read_from_the_state_directory() {
        let home = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/config-mcp-files");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(Declarations::read(&home), Ok(Declarations::default()));
        assert_eq!(Approvals::read(&home), Approvals::default());

        let mut declarations = Declarations::default();
        declarations.insert("weather", &weather());
        std::fs::write(declarations_file(&home), declarations.to_text()).unwrap();
        std::fs::write(approvals_file(&home), format!("{}\n", weather().digest())).unwrap();
        assert_eq!(Declarations::read(&home), Ok(declarations));
        assert!(Approvals::read(&home).approves(&weather().digest()));

        std::fs::write(declarations_file(&home), " ".repeat(MAX_BYTES as usize + 1)).unwrap();
        assert_eq!(Declarations::read(&home), Err(Unreadable::TooLarge));
        let _ = std::fs::remove_dir_all(&home);
    }
}
