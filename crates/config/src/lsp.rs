//! The language servers a person declares, `lsp.json` in the person's own state directory.
//!
//! A declaration names a command, the arguments it takes, the file extensions it serves (each with
//! the language id the protocol wants), and optionally the variables it receives and the
//! `initializationOptions` it is started with. This module reads that file and computes what an
//! approval binds to. It starts nothing: launching is the language server client's, and asking a
//! person is the agent's.
//!
//! # Why not a settings layer, and why not the checkout
//!
//! A declaration is a command to run, and BACKEND-1 forbids a settings file naming one. Two of the
//! settings layers sit in a checkout and the agent can write files there, so a declaration read from
//! one would be a road from an ordinary tool call to execution nobody approved. SERVERS-1 says the
//! same of an MCP server and this file follows it: only the state directory is read, so a
//! `.github/lsp.json` or `.lsp.json` in a workspace is not looked at.
//!
//! ```json
//! { "servers": {
//!     "clangd": { "command": "clangd", "args": ["--background-index"],
//!         "extensions": { "c": "c", "h": "c", "cpp": "cpp" } } } }
//! ```
//!
//! # Every failure is per entry
//!
//! An entry this cannot use is left out and listed with what is wrong with it, so one mistake does
//! not take the other servers with it. A file that is not the shape above is reported as a whole.

use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The declarations, inside the state directory.
const DECLARATIONS_FILE: &str = "lsp.json";

/// The most of the file worth reading.
const MAX_BYTES: u64 = 64 * 1024;

/// The longest name a declaration may have.
const MAX_NAME: usize = 64;

/// What the digested form starts with, so a later change to that form cannot collide with this one.
const DIGEST_FORM: &str = "bravebot-lsp-declaration-1";

/// Where a person's language servers are declared.
pub fn declarations_file(directory: &Path) -> PathBuf {
    directory.join(DECLARATIONS_FILE)
}

/// What an approval binds to: SHA-256 over the digested form of a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// Why an entry cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The name is not letters, digits, `-` and `_`.
    Name,
    /// The entry is not an object, or holds a key this does not know.
    Shape,
    /// `command` is missing, empty, or a relative path.
    Command,
    /// `args` is not a list of strings.
    Args,
    /// `extensions` is missing, empty, or not an object of strings.
    Extensions,
    /// `env` is not an object of strings, or names something that is not a variable.
    Env,
}

/// One declared server.
///
/// Its `Debug` names the server and not what is in it, since `env` holds values a person gave.
#[derive(Clone, PartialEq, Eq)]
pub struct Declared {
    name: String,
    command: String,
    args: Vec<String>,
    /// File extension, lowercase and without a dot, to the language id sent with `didOpen`.
    extensions: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    initialization_options: Option<serde_json::Value>,
}

impl std::fmt::Debug for Declared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Declared")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Declared {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A bare program name, looked up on `PATH`, or an absolute path.
    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// The extensions this serves, each with its language id, in extension order.
    pub fn extensions(&self) -> &BTreeMap<String, String> {
        &self.extensions
    }

    /// The variables set for it, as the person wrote them.
    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    pub fn initialization_options(&self) -> Option<&serde_json::Value> {
        self.initialization_options.as_ref()
    }

    /// What an approval binds to.
    ///
    /// Over every field that decides what runs and how it is started, so an edit to any of them is
    /// a declaration nobody approved.
    pub fn digest(&self) -> Digest {
        let form = serde_json::json!([
            DIGEST_FORM,
            self.name,
            self.command,
            self.args,
            self.extensions,
            self.env,
            self.initialization_options,
        ]);
        Digest(Sha256::digest(form.to_string().as_bytes()).into())
    }
}

/// Whether `name` may name a declaration.
fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    name.len() <= MAX_NAME
        && characters.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// An extension as it is compared: lowercase, with no leading dot.
fn extension(written: &str) -> String {
    written.trim_start_matches('.').to_ascii_lowercase()
}

fn is_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn strings(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn string_map(value: &serde_json::Value) -> Option<BTreeMap<String, String>> {
    value
        .as_object()?
        .iter()
        .map(|(key, item)| Some((key.clone(), item.as_str()?.to_string())))
        .collect()
}

fn declared(name: &str, value: &serde_json::Value) -> Result<Declared, Problem> {
    if !is_name(name) {
        return Err(Problem::Name);
    }
    let entry = value.as_object().ok_or(Problem::Shape)?;
    const KEYS: [&str; 5] = [
        "command",
        "args",
        "extensions",
        "env",
        "initializationOptions",
    ];
    if entry.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err(Problem::Shape);
    }

    // A relative path would resolve against the working directory, which is the workspace, and a
    // program inside a checkout is exactly what a declaration outside it exists to keep out.
    let command = entry
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.is_empty())
        .filter(|command| {
            Path::new(command).is_absolute() || !command.contains(['/', std::path::MAIN_SEPARATOR])
        })
        .ok_or(Problem::Command)?
        .to_string();

    let args = match entry.get("args") {
        None => Vec::new(),
        Some(args) => strings(args).ok_or(Problem::Args)?,
    };

    let extensions: BTreeMap<String, String> = entry
        .get("extensions")
        .and_then(string_map)
        .ok_or(Problem::Extensions)?
        .into_iter()
        .map(|(written, id)| (extension(&written), id))
        .collect();
    if extensions.is_empty()
        || extensions
            .keys()
            .any(|key| key.is_empty() || key.contains(['/', '.']))
    {
        return Err(Problem::Extensions);
    }

    let env = match entry.get("env") {
        None => BTreeMap::new(),
        Some(env) => string_map(env).ok_or(Problem::Env)?,
    };
    if !env.keys().all(|key| is_variable_name(key)) {
        return Err(Problem::Env);
    }

    Ok(Declared {
        name: name.to_string(),
        command,
        args,
        extensions,
        env,
        initialization_options: entry.get("initializationOptions").cloned(),
    })
}

/// Why the file cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// Larger than a declarations file has any reason to be.
    TooLarge,
    /// There, and not readable as text.
    NotRead,
    /// Not JSON, or not an object holding only `servers`, which is an object.
    NotDeclarations,
}

/// What the file declares, and the entries it holds that cannot be used.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Declarations {
    /// In name order.
    pub servers: Vec<Declared>,
    /// The name of each entry left out, and why.
    pub problems: Vec<(String, Problem)>,
}

impl Declarations {
    /// Read the declarations in a state directory. No file is no declarations.
    pub fn read(state: &Path) -> Result<Self, Unreadable> {
        let path = declarations_file(state);
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
        let Ok(serde_json::Value::Object(root)) = serde_json::from_str(text) else {
            return Err(Unreadable::NotDeclarations);
        };
        if root.keys().any(|key| key != "servers") {
            return Err(Unreadable::NotDeclarations);
        }
        let servers = match root.get("servers") {
            None => return Ok(Self::default()),
            Some(serde_json::Value::Object(servers)) => servers,
            Some(_) => return Err(Unreadable::NotDeclarations),
        };
        let mut found = Self::default();
        for (name, value) in servers {
            match declared(name, value) {
                Ok(server) => found.servers.push(server),
                Err(problem) => found.problems.push((name.clone(), problem)),
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Declarations {
        Declarations::parse(text).expect("a declarations file")
    }

    const CLANGD: &str = r#"{ "servers": { "clangd": { "command": "clangd",
        "args": ["--background-index"], "extensions": { ".C": "c", "h": "c" } } } }"#;

    /// SERVERS-1's file, for a language server: read from the state directory and from nowhere else.
    #[test]
    fn the_file_is_read_from_the_state_directory() {
        let state = crate::testutil::scratch_dir("lsp-declarations-state");
        let _ = std::fs::remove_dir_all(&state);
        std::fs::create_dir_all(&state).expect("a state directory");
        std::fs::write(declarations_file(&state), CLANGD).expect("write");
        let read = Declarations::read(&state).expect("readable");
        assert_eq!(read.servers.len(), 1);
        assert_eq!(read.servers[0].command(), "clangd");
        assert_eq!(read.servers[0].args(), ["--background-index"]);
        assert_eq!(declarations_file(&state).file_name().unwrap(), "lsp.json");

        let elsewhere = crate::testutil::scratch_dir("lsp-declarations-elsewhere");
        let _ = std::fs::remove_dir_all(&elsewhere);
        std::fs::create_dir_all(&elsewhere).expect("another directory");
        assert!(
            Declarations::read(&elsewhere)
                .expect("no file is no declarations")
                .servers
                .is_empty()
        );
    }

    /// An extension is compared the way a path's is: lowercase, with no dot.
    #[test]
    fn an_extension_is_lowercase_and_has_no_dot() {
        let read = parse(CLANGD);
        let extensions: Vec<_> = read.servers[0].extensions().keys().cloned().collect();
        assert_eq!(extensions, ["c", "h"]);
    }

    /// A command that is a relative path would run a program out of the checkout.
    #[test]
    fn a_relative_command_is_not_a_declaration() {
        for command in ["./clangd", "tools/clangd", "../clangd", ""] {
            let text = format!(
                r#"{{ "servers": {{ "x": {{ "command": "{command}", "extensions": {{ "c": "c" }} }} }} }}"#
            );
            let read = parse(&text);
            assert!(read.servers.is_empty(), "{command}");
            assert_eq!(read.problems, [("x".to_string(), Problem::Command)]);
        }
        let absolute = r#"{ "servers": { "x": { "command": "/usr/bin/clangd",
            "extensions": { "c": "c" } } } }"#;
        assert_eq!(parse(absolute).servers.len(), 1);
    }

    /// `Path::extension` yields what follows the last dot, so a key with a dot inside it could never
    /// match a file, and a person who wrote one believes it works.
    #[test]
    fn an_extension_with_a_dot_inside_it_is_a_problem() {
        let read = parse(
            r#"{ "servers": { "x": { "command": "y", "extensions": { "d.ts": "typescript" } } } }"#,
        );
        assert!(read.servers.is_empty());
        assert_eq!(read.problems, [("x".to_string(), Problem::Extensions)]);
    }

    /// One bad entry does not take the others with it.
    #[test]
    fn a_bad_entry_is_listed_and_the_others_are_kept() {
        let read = parse(
            r#"{ "servers": {
                "bad": { "command": "x" },
                "good": { "command": "y", "extensions": { "rb": "ruby" } } } }"#,
        );
        assert_eq!(read.servers.len(), 1);
        assert_eq!(read.servers[0].name(), "good");
        assert_eq!(read.problems, [("bad".to_string(), Problem::Extensions)]);
    }

    /// An unknown key is a mistake a person believes worked.
    #[test]
    fn an_unknown_key_is_a_problem() {
        let read = parse(
            r#"{ "servers": { "x": { "command": "y", "extensions": { "c": "c" }, "shell": true } } }"#,
        );
        assert!(read.servers.is_empty());
        assert_eq!(read.problems, [("x".to_string(), Problem::Shape)]);
    }

    /// An edit to any field that decides what runs is a declaration nobody approved.
    #[test]
    fn the_digest_covers_everything_that_decides_what_runs() {
        let entry = |command: &str, args: &str, extensions: &str, more: &str| {
            let text = format!(
                r#"{{ "servers": {{ "clangd": {{ "command": "{command}", "args": [{args}],
                    "extensions": {{ {extensions} }}{more} }} }} }}"#
            );
            parse(&text).servers[0].digest()
        };
        let base = entry("clangd", "\"--a\"", "\"c\": \"c\"", "");
        assert_eq!(base, entry("clangd", "\"--a\"", "\"c\": \"c\"", ""));
        for changed in [
            entry("other", "\"--a\"", "\"c\": \"c\"", ""),
            entry("clangd", "\"--b\"", "\"c\": \"c\"", ""),
            entry("clangd", "\"--a\"", "\"c\": \"cpp\"", ""),
            entry("clangd", "\"--a\"", "\"c\": \"c\", \"h\": \"c\"", ""),
            entry(
                "clangd",
                "\"--a\"",
                "\"c\": \"c\"",
                ", \"env\": { \"A\": \"1\" }",
            ),
            entry(
                "clangd",
                "\"--a\"",
                "\"c\": \"c\"",
                ", \"initializationOptions\": { \"a\": 1 }",
            ),
        ] {
            assert_ne!(base, changed);
        }
    }

    /// A file that is not the shape is reported whole, and none of it is used.
    #[test]
    fn a_file_of_the_wrong_shape_is_unreadable() {
        for text in [
            "[]",
            "not json",
            r#"{ "other": {} }"#,
            r#"{ "servers": [] }"#,
        ] {
            assert_eq!(
                Declarations::parse(text).unwrap_err(),
                Unreadable::NotDeclarations,
                "{text}"
            );
        }
    }

    /// The values a person gave are not repeated by a log line.
    #[test]
    fn debug_names_the_server_and_not_its_variables() {
        let read =
            parse(&CLANGD.replace("\"args\"", "\"env\": { \"KEY\": \"hunter2\" }, \"args\""));
        let shown = format!("{:?}", read.servers[0]);
        assert!(shown.contains("clangd"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}
