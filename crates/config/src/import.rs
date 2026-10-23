//! What Claude Code's and opencode's own configuration holds, and what a running Ollama, LM Studio
//! or llama-server serves, that this program can use.
//!
//! Read and mapped here, and never written: the question and the write belong to the front end that
//! asks (`docs/specs/import.md`). Only files in the person's profile directory are opened, because a
//! checkout's files are whatever the repository's author wrote, and a host and a credential name
//! taken from one would let a clone decide where the person's key is sent. The servers are asked by
//! the caller, and only where they listen on this machine.

use crate::Secret;
use crate::bedrock::{Bedrock, Tier};
use crate::env_var;
use crate::provider::{self, ANTHROPIC_ID, AWS_PROVIDER_ID, GOOGLE_VERTEX_ID, Provider};
use serde_json::{Map, Value};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// The most of a source file worth reading.
///
/// Larger than a settings file's bound, since an opencode configuration can list a gateway's whole
/// roster, and still a bound, so a file replaced by something else entirely is not parsed.
const SOURCE_BYTES: u64 = 1024 * 1024;

/// Claude Code's switch for Bedrock, which bravebot spells [`env_var::USE_BEDROCK`].
const CLAUDE_BEDROCK: &str = "CLAUDE_CODE_USE_BEDROCK";
/// Claude Code's switch for Vertex AI, which serves Claude models through Google Cloud credentials.
const CLAUDE_VERTEX: &str = "CLAUDE_CODE_USE_VERTEX";
/// Claude Code's model variable, which it reads before the settings file's `model` key.
const CLAUDE_MODEL: &str = "ANTHROPIC_MODEL";
/// The Bedrock API key Claude Code can sign in with, which bravebot does not: it signs through the
/// AWS credential chain.
const BEDROCK_BEARER: &str = "AWS_BEARER_TOKEN_BEDROCK";
/// The key Claude Code reaches Anthropic's own API with, which an `anthropic` provider block sends in
/// the header that API reads it from.
const ANTHROPIC_KEY: &str = "ANTHROPIC_API_KEY";
/// The host Claude Code sends it to instead of Anthropic's.
const ANTHROPIC_BASE_URL: &str = "ANTHROPIC_BASE_URL";
/// What else Claude Code reaches Anthropic's own API with. The first is sent as a bearer token and
/// the second is a host written without the route this program appends, so neither is carried into
/// a `provider` block.
const ANTHROPIC_NAMES: [&str; 2] = ["ANTHROPIC_AUTH_TOKEN", ANTHROPIC_BASE_URL];
/// The names a Claude Code Bedrock setup states that bravebot reads under the same spelling.
const BEDROCK_NAMES: [&str; 5] = [
    env_var::AWS_REGION,
    env_var::AWS_PROFILE,
    env_var::BEDROCK_OPUS_MODEL,
    env_var::BEDROCK_SONNET_MODEL,
    env_var::BEDROCK_HAIKU_MODEL,
];

/// Where Ollama's own client is told the server is.
pub const OLLAMA_HOST: &str = "OLLAMA_HOST";
/// The port Ollama listens on where `OLLAMA_HOST` names none and no scheme.
const OLLAMA_PORT: u16 = 11434;
/// Where LM Studio is asked, read as [`OLLAMA_HOST`] is.
///
/// LM Studio reads no variable for its own address, so this one is bravebot's and moves only where
/// bravebot asks. A person who changed LM Studio's port in its settings names the new one here.
pub const LM_STUDIO_HOST: &str = "BRAVEBOT_LM_STUDIO_HOST";
/// The port LM Studio's server listens on until it is told another.
const LM_STUDIO_PORT: u16 = 1234;
/// What llama-server is told in order to listen where it does, which is also what asking it needs.
pub const LLAMA_ARG_HOST: &str = "LLAMA_ARG_HOST";
pub const LLAMA_ARG_PORT: &str = "LLAMA_ARG_PORT";
/// The ports llama-server listens on until it is told another: the one it has defaulted to since
/// October 2026, and the one every release before that, and the container image, still use.
const LLAMA_SERVER_PORTS: [u16; 2] = [9931, 8080];
/// The capability a model must report for a session's tools to reach it.
///
/// What a server's reader reports for a model that can call tools, so it is the one name the readers
/// in `bravebot-aichat` and the choice here agree on.
pub const TOOLS: &str = "tools";

/// The two programs whose configuration is read, and the servers that are asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    ClaudeCode,
    Opencode,
    Ollama,
    LmStudio,
    LlamaServer,
}

impl Source {
    /// The program's own name, which is not translated.
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Opencode => "opencode",
            Self::Ollama => "Ollama",
            Self::LmStudio => "LM Studio",
            Self::LlamaServer => "llama-server",
        }
    }

    /// Whether this is a server asked on this machine, rather than files read.
    pub fn is_server(self) -> bool {
        self.gateway_id().is_some()
    }

    /// The id a server's entry is written under, which for Ollama is the one the configuration
    /// guide's block uses. `None` for a source that is files.
    fn gateway_id(self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("ollama"),
            Self::LmStudio => Some("lmstudio"),
            Self::LlamaServer => Some("llama-server"),
            Self::ClaudeCode | Self::Opencode => None,
        }
    }
}

/// Where a server listens, as the variable that names it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listens {
    /// On this machine, at these base URLs, none ending in a slash. A server that has had more than
    /// one default has one URL for each, and the first that answers is the one offered.
    Here(Vec<String>),
    /// On another machine, which is never asked: a start would otherwise reach whatever host the
    /// variable names before the person has agreed to anything. Carries the name of the variable.
    Elsewhere(&'static str),
}

impl Listens {
    /// Where `value` says a server is, read as Ollama's client reads `OLLAMA_HOST`: surrounding
    /// space and quotes dropped, `http` and `bare_port` where it names no scheme, the scheme's own
    /// port where it names one and no port, and any path kept.
    ///
    /// Unset, or set to nothing, is `unset`. `0.0.0.0` and `::` are what a server is told to listen
    /// on, and name this machine to a client. `variable` is what is said where the value names
    /// another machine.
    fn from_value(
        value: Option<&str>,
        variable: &'static str,
        unset: &str,
        bare_port: u16,
    ) -> Self {
        let value = unquoted(value.unwrap_or_default());
        if value.is_empty() {
            return Self::Here(vec![unset.to_string()]);
        }
        // Another scheme is kept, as Ollama's client keeps it, and asking there fails.
        let (scheme, rest, default_port) = match value.split_once("://") {
            None => ("http", value, bare_port),
            Some(("http", rest)) => ("http", rest, 80),
            Some(("https", rest)) => ("https", rest, 443),
            Some((scheme, rest)) => (scheme, rest, bare_port),
        };
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = match split_host_port(authority) {
            Some((host, port)) => (host, port.parse::<u16>().unwrap_or(default_port)),
            None => (authority.trim_matches(['[', ']']), default_port),
        };
        let Some(host) = this_machine(host) else {
            return Self::Elsewhere(variable);
        };
        let path = path.trim_end_matches('/');
        let path = match path.is_empty() {
            true => String::new(),
            false => format!("/{path}"),
        };
        Self::Here(vec![format!("{scheme}://{host}:{port}{path}")])
    }

    /// Where llama-server listens, as the variables it is started with say: `LLAMA_ARG_HOST` is the
    /// address it binds, and `LLAMA_ARG_PORT` the port. A port that is set is the only one asked.
    /// One that is not set, or is not a port, leaves both ports it has defaulted to.
    fn llama_server(host: Option<&str>, port: Option<&str>) -> Self {
        let host = unquoted(host.unwrap_or_default());
        let host = match host.is_empty() {
            true => Some("127.0.0.1".to_string()),
            false => this_machine(host.trim_matches(['[', ']'])),
        };
        let Some(host) = host else {
            return Self::Elsewhere(LLAMA_ARG_HOST);
        };
        let named = port
            .and_then(|port| unquoted(port).parse::<u16>().ok())
            .filter(|port| *port != 0);
        let ports = match named {
            Some(port) => vec![port],
            None => LLAMA_SERVER_PORTS.to_vec(),
        };
        Self::Here(
            ports
                .into_iter()
                .map(|port| format!("http://{host}:{port}"))
                .collect(),
        )
    }
}

/// A variable's value with the space and quotes around it dropped, as a shell leaves them where a
/// value was copied from a file.
fn unquoted(value: &str) -> &str {
    value.trim().trim_matches(['"', '\'']).trim()
}

/// A host and port as Go's `net.SplitHostPort` splits them, or `None` where it would refuse:
/// a bracketed IPv6 address with no port after it, or a value with no colon or more than one.
fn split_host_port(authority: &str) -> Option<(&str, &str)> {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']')?;
        return Some((host, after.strip_prefix(':')?));
    }
    let (host, port) = authority.split_once(':')?;
    (!port.contains(':')).then_some((host, port))
}

/// The host as a URL names it, where it is this machine.
///
/// An empty host is what Ollama's client reaches as `127.0.0.1`. A name other than `localhost` is
/// never taken to be this machine, since what it resolves to is up to whoever answers the lookup.
fn this_machine(host: &str) -> Option<String> {
    if host.is_empty() {
        return Some("127.0.0.1".to_string());
    }
    if host.eq_ignore_ascii_case("localhost") {
        return Some("localhost".to_string());
    }
    match host.parse::<IpAddr>().ok()?.to_canonical() {
        IpAddr::V4(address) if address.is_unspecified() => Some("127.0.0.1".to_string()),
        IpAddr::V6(address) if address.is_unspecified() => Some("[::1]".to_string()),
        IpAddr::V4(address) if address.is_loopback() => Some(address.to_string()),
        IpAddr::V6(address) if address.is_loopback() => Some(format!("[{address}]")),
        _ => None,
    }
}

/// The port an `http` or `https` URL reaches on this machine, or `None` where it names another
/// machine or cannot be read.
pub fn local_port(url: &str) -> Option<u16> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let default_port = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = match split_host_port(authority) {
        Some((host, port)) => (host, port.parse::<u16>().ok()?),
        None => (authority.trim_matches(['[', ']']), default_port),
    };
    this_machine(host)
        .filter(|_| !host.is_empty())
        .map(|_| port)
}

/// The files each source is read from, and where the servers are asked.
///
/// Resolved from the variables each program itself reads, so a person who moved one program's
/// directory is read where they moved it. A relative value is never resolved: against the working
/// directory it would name a file in whatever checkout the program was started in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Places {
    /// Claude Code's `settings.json`.
    pub claude_code: Option<PathBuf>,
    /// opencode's configuration files, weakest first.
    pub opencode: Vec<PathBuf>,
    /// opencode's `auth.json`.
    pub opencode_auth: Option<PathBuf>,
    /// Where each server listens, in the order the questions are asked: Ollama, LM Studio, then
    /// llama-server. Empty where nothing is read at all.
    pub servers: Vec<(Source, Listens)>,
}

impl Places {
    /// Where the sources are for this process.
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Where the sources are, given what the variables hold.
    ///
    /// Nothing where the platform names no profile directory, or names a relative one (STATE-2's
    /// rule): there is then no directory that is the person's own to read from.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let named = |name: &str| lookup(name).filter(|value| !value.is_empty());
        let absolute = |name: &str| named(name).map(PathBuf::from).filter(|p| p.is_absolute());
        let Some(profile) = crate::settings::PROFILE_VARIABLES
            .iter()
            .find_map(|name| named(name))
            .map(PathBuf::from)
            .filter(|profile| profile.is_absolute())
        else {
            return Self::default();
        };

        // A relative CLAUDE_CONFIG_DIR is not replaced by the default: the person pointed Claude
        // Code somewhere, and the default is not where its configuration is.
        let claude_code = match named("CLAUDE_CONFIG_DIR") {
            None => Some(profile.join(".claude")),
            Some(_) => absolute("CLAUDE_CONFIG_DIR"),
        }
        .map(|directory| directory.join("settings.json"));

        // The XDG rule: a relative value is invalid and the default applies.
        let config = absolute("XDG_CONFIG_HOME")
            .unwrap_or_else(|| profile.join(".config"))
            .join("opencode");
        let data = absolute("XDG_DATA_HOME")
            .unwrap_or_else(|| profile.join(".local").join("share"))
            .join("opencode");
        let mut opencode = vec![config.join("opencode.json"), config.join("opencode.jsonc")];
        opencode.extend(absolute("OPENCODE_CONFIG"));

        let servers = vec![
            (
                Source::Ollama,
                Listens::from_value(
                    lookup(OLLAMA_HOST).as_deref(),
                    OLLAMA_HOST,
                    &format!("http://localhost:{OLLAMA_PORT}"),
                    OLLAMA_PORT,
                ),
            ),
            (
                Source::LmStudio,
                Listens::from_value(
                    lookup(LM_STUDIO_HOST).as_deref(),
                    LM_STUDIO_HOST,
                    &format!("http://127.0.0.1:{LM_STUDIO_PORT}"),
                    LM_STUDIO_PORT,
                ),
            ),
            (
                Source::LlamaServer,
                Listens::llama_server(
                    lookup(LLAMA_ARG_HOST).as_deref(),
                    lookup(LLAMA_ARG_PORT).as_deref(),
                ),
            ),
        ];

        Self {
            claude_code,
            opencode,
            opencode_auth: Some(data.join("auth.json")),
            servers,
        }
    }

    /// Every file these places name.
    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.claude_code
            .iter()
            .chain(&self.opencode)
            .chain(&self.opencode_auth)
            .map(PathBuf::as_path)
    }
}

/// What one source holds, in bravebot's own spelling.
#[derive(Debug)]
pub struct Found {
    pub source: Source,
    /// The files that were read, for the line naming where this came from.
    pub read: Vec<PathBuf>,
    /// The server that answered, for that line where no file was read.
    pub running_at: Option<String>,
    /// The context window, in tokens, the running server holds [`Found::model`] loaded with.
    pub window: Option<u64>,
    /// Names under `env`, with their values.
    pub env: Vec<(String, String)>,
    /// The top-level `model` key.
    pub model: Option<String>,
    /// The id of the gateway that serves [`Found::model`], where one of [`Found::gateways`] does, so
    /// the model is written only beside it.
    pub model_gateway: Option<String>,
    /// Entries under `provider`.
    pub gateways: Vec<Gateway>,
    /// What was found and cannot be imported.
    pub left: Vec<Left>,
}

impl Found {
    /// Whether anything here could be written.
    pub fn importable(&self) -> bool {
        !self.env.is_empty() || self.model.is_some() || !self.gateways.is_empty()
    }
}

/// One `provider` entry to write.
#[derive(Debug)]
pub struct Gateway {
    pub id: String,
    /// Where requests go, which is what the person approves: a stated `baseURL`, the one compiled
    /// in for a known id, or the Bedrock host its region names.
    pub endpoint: String,
    /// The entry as it would be written, holding only the fields bravebot reads, with no value of a
    /// credential in it.
    pub entry: Map<String, Value>,
    /// A credential value the source holds, which is asked about on its own.
    pub key: Key,
}

impl Gateway {
    /// The variable opencode itself reads this id's token from, where one is known.
    pub fn fallback(&self) -> Option<&'static str> {
        provider::known_variable(&self.id)
    }

    /// The entry as one line of JSON, which is what the question shows.
    ///
    /// Taken before [`Gateway::keep_key`], so the key is never part of it.
    pub fn shown(&self) -> String {
        serde_json::to_string(&self.entry).unwrap_or_default()
    }

    /// The variables the entry reads its token from.
    pub fn variables(&self) -> Vec<String> {
        match self.entry.get("env") {
            Some(Value::Array(names)) => names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Write the key into the entry, as `options.apiKey`.
    pub fn keep_key(&mut self, key: &Secret) {
        if let Value::Object(options) = self
            .entry
            .entry("options")
            .or_insert_with(|| Value::Object(Map::new()))
        {
            options.insert("apiKey".to_string(), Value::from(key.expose()));
        }
    }

    /// Name the variable the token is read from, where the entry names none.
    pub fn name_variable(&mut self, name: &str) {
        if self.variables().is_empty() {
            self.entry
                .insert("env".to_string(), Value::Array(vec![Value::from(name)]));
        }
    }

    /// Take out of an AWS entry every model a tier of `tiers` names, with that tier.
    ///
    /// The tiers answer for a model before any entry does ([`crate::Config::bedrock_for`]), so a
    /// copy here would be a second row in the picker that nothing ever sends to.
    pub fn take_tier_models(&mut self, tiers: &Bedrock) -> Vec<(String, Tier)> {
        if self.id != AWS_PROVIDER_ID {
            return Vec::new();
        }
        let Some(Value::Object(models)) = self.entry.get_mut("models") else {
            return Vec::new();
        };
        let mut taken = Vec::new();
        models.retain(
            |model, _| match tiers.entry(model).and_then(|entry| entry.tier) {
                Some(tier) => {
                    taken.push((model.clone(), tier));
                    false
                }
                None => true,
            },
        );
        if models.is_empty() {
            self.entry.remove("models");
        }
        taken
    }

    /// Whether the entry names any model, which is all an AWS entry offers.
    pub fn names_models(&self) -> bool {
        self.entry.contains_key("models")
    }
}

/// Cleared when it goes, because an approved key is written into the entry before the entry is
/// moved into the file.
impl Drop for Gateway {
    fn drop(&mut self) {
        crate::scrub_document(&mut self.entry);
    }
}

/// A value as JSON writes it, which is how the question shows one: quoted, and with anything that
/// would not survive a line escaped.
pub fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// What a source says about a gateway's credential beyond what the entry names.
#[derive(Debug)]
pub enum Key {
    /// The entry names its variables, or needs no credential, so there is no value to ask about.
    Named,
    /// A key the source holds in plain text.
    Held(Secret),
    /// A `{file:path}` reference, which is not followed.
    File(String),
}

/// One thing found that is not imported, by name only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Left {
    pub name: String,
    pub reason: Reason,
}

/// Why something found is not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A setting for Anthropic's own API that this import does not carry into a `provider` block: a
    /// bearer token, a host, or a key stated for that host.
    AnthropicApi,
    /// Google Vertex AI with no usable project and no key to reach it with, or through Google Cloud
    /// credentials: neither is reached by a service here.
    Vertex,
    /// A Bedrock API key, where bravebot signs through the AWS credential chain.
    BearerToken,
    /// Bedrock with no region to sign for.
    NoRegion,
    /// A sign-in opencode holds, which is opencode's own.
    SignIn,
    /// An entry naming an SDK other than an OpenAI-compatible one.
    AnotherSdk,
    /// An id with no endpoint stated and none compiled in.
    NoEndpoint,
    /// A key built from an opencode substitution inside a longer value, which this program does not
    /// make, so the value as written is not the key.
    Substitution,
    /// The variable that names where a server is asked names another machine, which is not asked.
    Elsewhere,
    /// A running server with no model that can call tools, which a session could not use.
    NoToolModel,
}

/// One model a running server lists, as its listing names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    /// When it was pulled, in RFC 3339, or empty where the server states no time.
    pub modified_at: String,
    /// What it can do, or `None` where the listing reported nothing, which an Ollama older than
    /// the field does, and llama-server always does.
    pub capabilities: Option<Vec<String>>,
    /// The context window it is loaded with, in tokens, or `None` where it is not loaded or the
    /// server did not say.
    pub window: Option<u64>,
}

/// Everything the sources hold, Claude Code first, then opencode, then the servers in the order
/// [`Places::servers`] lists them.
///
/// `environment` is the process environment. A Claude Code Bedrock setup is often exported rather
/// than written down, and a name already there is read from there at run time, so it is not copied.
///
/// `ask` asks the server of a source at a base URL for its listing, and answers `None` where nothing
/// listens there or what came back could not be read. It is called only where the server is on
/// this machine, and after every file has been read. Every address is asked at once, so a server
/// that never answers holds the start for one bound and not one for each.
pub fn found(
    places: &Places,
    environment: impl Fn(&str) -> Option<String>,
    ask: impl Fn(Source, &str) -> Option<Vec<Installed>> + Sync,
) -> Vec<Found> {
    let environment = |name: &str| environment(name).filter(|value| !value.trim().is_empty());
    let mut found = Vec::new();
    if let Some(path) = &places.claude_code {
        found.extend(claude_code(path, &environment));
    }
    found.extend(opencode(&places.opencode, places.opencode_auth.as_deref()));

    let addresses: Vec<(Source, &str)> = places
        .servers
        .iter()
        .flat_map(|(source, listens)| match listens {
            Listens::Here(hosts) => hosts.iter().map(|host| (*source, host.as_str())).collect(),
            Listens::Elsewhere(_) => Vec::new(),
        })
        .collect();
    // A thread the system will not give is no answer, as a server that did not reply is none.
    let answers: Vec<Option<Vec<Installed>>> = std::thread::scope(|scope| {
        let asking: Vec<_> = addresses
            .iter()
            .map(|(source, host)| {
                std::thread::Builder::new()
                    .spawn_scoped(scope, || ask(*source, host))
                    .ok()
            })
            .collect();
        asking
            .into_iter()
            .map(|asked| {
                asked.and_then(|asked| {
                    asked
                        .join()
                        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                })
            })
            .collect()
    });
    let mut answers = answers.into_iter();
    for (source, listens) in &places.servers {
        match listens {
            Listens::Elsewhere(variable) => {
                found.push(server_left(*source, variable, Reason::Elsewhere, None));
            }
            Listens::Here(hosts) => {
                // Taken for every address, so that the answers after this source's own are the
                // ones of the next.
                let theirs: Vec<_> = hosts
                    .iter()
                    .zip(answers.by_ref().take(hosts.len()))
                    .collect();
                if let Some((host, listed)) = theirs
                    .into_iter()
                    .find_map(|(host, listed)| listed.map(|listed| (host, listed)))
                {
                    found.push(running(*source, host, listed));
                }
            }
        }
    }
    found
}

/// What a running server at `host` serves, as the configuration guide's block for it: a gateway
/// needing no key, and one model that can call tools.
///
/// Ollama's is the newest it has pulled. The other two state no time: LM Studio's is the one it has
/// loaded, or the first it lists where none is, and llama-server's is the first it lists.
///
/// Where no model reports what it can do, the model is chosen among them all, since an Ollama older
/// than the field would otherwise be offered nothing at all.
pub fn running(source: Source, host: &str, listed: Vec<Installed>) -> Found {
    let reported = listed.iter().any(|model| model.capabilities.is_some());
    let candidates: Vec<Installed> = listed
        .into_iter()
        .filter(|model| !model.name.trim().is_empty())
        .filter(|model| {
            !reported
                || model
                    .capabilities
                    .as_ref()
                    .is_some_and(|can| can.iter().any(|capability| capability == TOOLS))
        })
        .collect();
    let chosen = match source {
        Source::Ollama => candidates
            .into_iter()
            .max_by_key(|model| crate::time::rfc3339(&model.modified_at)),
        _ => {
            let loaded = candidates.iter().position(|model| model.window.is_some());
            candidates.into_iter().nth(loaded.unwrap_or_default())
        }
    };
    let (Some(chosen), Some(id)) = (chosen, source.gateway_id()) else {
        return server_left(source, host, Reason::NoToolModel, Some(host));
    };

    let endpoint = format!("{host}/v1");
    let mut options = Map::new();
    options.insert("baseURL".to_string(), Value::from(endpoint.as_str()));
    // A window above the assumed one is left unwritten: the assumed one is deliberately low, and
    // a figure read once from a loaded model goes stale when the person reloads it larger.
    if let Some(window) = chosen
        .window
        .filter(|window| *window < crate::provider::CONTEXT_WINDOW)
    {
        options.insert("contextWindow".to_string(), Value::from(window));
    }
    let mut entry = Map::new();
    entry.insert(
        "name".to_string(),
        Value::from(format!("{} (local)", source.name())),
    );
    entry.insert("options".to_string(), Value::Object(options));
    Found {
        source,
        read: Vec::new(),
        running_at: Some(host.to_string()),
        window: chosen.window,
        env: Vec::new(),
        model: Some(format!("{id}/{}", chosen.name)),
        model_gateway: Some(id.to_string()),
        gateways: vec![Gateway {
            id: id.to_string(),
            endpoint,
            entry,
            key: Key::Named,
        }],
        left: Vec::new(),
    }
}

/// A server that offers nothing, and why.
fn server_left(source: Source, name: &str, reason: Reason, running_at: Option<&str>) -> Found {
    Found {
        source,
        read: Vec::new(),
        running_at: running_at.map(str::to_string),
        window: None,
        env: Vec::new(),
        model: None,
        model_gateway: None,
        gateways: Vec::new(),
        left: vec![Left {
            name: name.to_string(),
            reason,
        }],
    }
}

/// A parse of a source file, which clears itself when it goes: a source may hold a key.
#[derive(Default)]
struct Parsed(Map<String, Value>);

impl Drop for Parsed {
    fn drop(&mut self) {
        crate::scrub_document(&mut self.0);
    }
}

/// One source file, or `None` where there is nothing there worth reading.
///
/// `comments` admits opencode's JSONC: comments and trailing commas.
fn read(path: &Path, comments: bool) -> Option<Parsed> {
    match std::fs::metadata(path) {
        Ok(found) if found.is_file() && found.len() <= SOURCE_BYTES => {}
        _ => return None,
    }
    let mut text = std::fs::read_to_string(path).ok()?;
    let parsed = match comments {
        true => {
            let mut strict = without_comments(&text);
            let parsed = parse(&strict);
            crate::scrub(&mut strict);
            parsed
        }
        false => parse(&text),
    };
    crate::scrub(&mut text);
    parsed
}

fn parse(text: &str) -> Option<Parsed> {
    match serde_json::from_str(text) {
        Ok(Value::Object(root)) => Some(Parsed(root)),
        Ok(mut other) => {
            crate::scrub_value(&mut other);
            None
        }
        Err(_) => None,
    }
}

/// JSONC as JSON: comments dropped, and a comma before a closing bracket dropped.
///
/// By bytes, which is sound because every character this looks for is ASCII and no byte of a
/// multi-byte UTF-8 character is.
fn without_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    let mut quoted = false;
    while at < bytes.len() {
        let byte = bytes[at];
        if quoted {
            out.push(byte);
            if byte == b'\\' && at + 1 < bytes.len() {
                out.push(bytes[at + 1]);
                at += 1;
            } else if byte == b'"' {
                quoted = false;
            }
            at += 1;
            continue;
        }
        match (byte, bytes.get(at + 1)) {
            (b'"', _) => {
                quoted = true;
                out.push(byte);
                at += 1;
            }
            (b'/', Some(b'/' | b'*')) => {
                at = past_comment(bytes, at);
                out.push(b' ');
            }
            (b',', _) if matches!(next_significant(bytes, at + 1), Some(b'}' | b']')) => at += 1,
            _ => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|refused| {
        let mut bytes = refused.into_bytes();
        crate::scrub_bytes(&mut bytes);
        String::new()
    })
}

/// Where the comment starting at `at` ends.
fn past_comment(bytes: &[u8], at: usize) -> usize {
    let mut at = at + 2;
    if bytes[at - 1] == b'/' {
        while at < bytes.len() && bytes[at] != b'\n' {
            at += 1;
        }
        return at;
    }
    while at < bytes.len() && !(bytes[at] == b'*' && bytes.get(at + 1) == Some(&b'/')) {
        at += 1;
    }
    at + 2
}

/// The next byte that is neither space nor comment.
fn next_significant(bytes: &[u8], mut at: usize) -> Option<u8> {
    loop {
        match (*bytes.get(at)?, bytes.get(at + 1)) {
            (b' ' | b'\t' | b'\n' | b'\r', _) => at += 1,
            (b'/', Some(b'/' | b'*')) => at = past_comment(bytes, at),
            (byte, _) => return Some(byte),
        }
    }
}

/// A string value with surrounding space removed, or `None` when nothing is left.
fn string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Claude Code's reading of a switch: `1`, `true`, `yes` or `on`.
fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// What Claude Code's settings file and the process environment say about Bedrock.
///
/// Only the `env` block and the `model` key are opened. `apiKeyHelper`, `awsAuthRefresh`,
/// `awsCredentialExport`, `permissions`, `hooks` and `mcpServers` are commands and grants, which a
/// settings file here may not hold.
fn claude_code(path: &Path, environment: &dyn Fn(&str) -> Option<String>) -> Option<Found> {
    let file = read(path, false);
    let exported = environment(CLAUDE_BEDROCK).is_some_and(|value| truthy(&value));
    if file.is_none() && !exported {
        return None;
    }
    let root = file.as_ref().map(|file| &file.0);
    let block = root
        .and_then(|root| root.get("env"))
        .and_then(Value::as_object);
    let stated = |name: &str| string(block.and_then(|block| block.get(name)));

    let mut env = Vec::new();
    let mut left = Vec::new();
    let mut bedrock = false;
    if exported || stated(CLAUDE_BEDROCK).is_some_and(|value| truthy(&value)) {
        let region = stated(env_var::AWS_REGION).or_else(|| environment(env_var::AWS_REGION));
        match region {
            None => left.push(Left {
                name: CLAUDE_BEDROCK.to_string(),
                reason: Reason::NoRegion,
            }),
            Some(_) => {
                bedrock = true;
                if environment(env_var::USE_BEDROCK).is_none() {
                    env.push((env_var::USE_BEDROCK.to_string(), "1".to_string()));
                }
                for name in BEDROCK_NAMES {
                    if let Some(value) = stated(name)
                        && environment(name).is_none()
                    {
                        env.push((name.to_string(), value));
                    }
                }
            }
        }
    }

    // A tier word names a model only where that tier is: a word whose tier nobody named would
    // reach Brave's roster rather than the account being imported.
    let reached = |tier: Tier| {
        env.iter().any(|(name, _)| name == tier.env_var()) || environment(tier.env_var()).is_some()
    };
    let model = stated(CLAUDE_MODEL)
        .or_else(|| string(root.and_then(|root| root.get("model"))))
        .filter(|word| bedrock && Tier::from_alias(word).is_some_and(reached));

    // A key stated beside a host was issued for that host, and the entry written here would send it
    // to Anthropic's, so both are left.
    let mut gateways = Vec::new();
    let proxied = stated(ANTHROPIC_BASE_URL).is_some();
    match (
        stated(ANTHROPIC_KEY),
        provider::known_endpoint(ANTHROPIC_ID),
    ) {
        (Some(key), Some(endpoint)) if !proxied => gateways.push(Gateway {
            id: ANTHROPIC_ID.to_string(),
            endpoint: endpoint.to_string(),
            entry: Map::new(),
            key: Key::Held(Secret::new(key)),
        }),
        (Some(_), _) => left.push(Left {
            name: ANTHROPIC_KEY.to_string(),
            reason: Reason::AnthropicApi,
        }),
        (None, _) => {}
    }
    for name in ANTHROPIC_NAMES {
        if stated(name).is_some() {
            left.push(Left {
                name: name.to_string(),
                reason: Reason::AnthropicApi,
            });
        }
    }
    if stated(CLAUDE_VERTEX).is_some_and(|value| truthy(&value)) {
        left.push(Left {
            name: CLAUDE_VERTEX.to_string(),
            reason: Reason::Vertex,
        });
    }
    if stated(BEDROCK_BEARER).is_some() {
        left.push(Left {
            name: BEDROCK_BEARER.to_string(),
            reason: Reason::BearerToken,
        });
    }

    let found = Found {
        source: Source::ClaudeCode,
        read: file.map(|_| path.to_path_buf()).into_iter().collect(),
        running_at: None,
        window: None,
        env,
        model,
        model_gateway: None,
        gateways,
        left,
    };
    (found.importable() || !found.left.is_empty()).then_some(found)
}

/// What opencode's configuration and `auth.json` say about gateways.
///
/// Only `provider`, `model` and the two lists are opened. `permission`, `mcp`, `agent`, `command`
/// and `plugin` are grants and commands, which a settings file here may not hold.
fn opencode(configs: &[PathBuf], auth: Option<&Path>) -> Option<Found> {
    let mut root = Parsed::default();
    let mut read_from = Vec::new();
    for path in configs {
        if let Some(mut file) = read(path, true) {
            merge(&mut root.0, std::mem::take(&mut file.0));
            read_from.push(path.clone());
        }
    }
    let signed = auth.and_then(|path| read(path, true).map(|file| (path, file)));
    if read_from.is_empty() && signed.is_none() {
        return None;
    }
    let signed_in = signed.as_ref().map(|(_, file)| &file.0);
    read_from.extend(signed.as_ref().map(|(path, _)| path.to_path_buf()));
    let root = &root.0;

    let listed = |name: &str| -> Option<Vec<String>> {
        root.get(name).and_then(Value::as_array).map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
    };
    let disabled = listed("disabled_providers").unwrap_or_default();
    let enabled = listed("enabled_providers");
    let considered = |id: &str| {
        !disabled.iter().any(|named| named == id)
            && enabled
                .as_ref()
                .is_none_or(|enabled| enabled.iter().any(|named| named == id))
    };
    let api_key = |id: &str| {
        let entry = signed_in?.get(id)?.as_object()?;
        (entry.get("type")?.as_str()? == "api")
            .then(|| string(entry.get("key")))
            .flatten()
    };

    let mut gateways = Vec::new();
    let mut left = Vec::new();
    let mut configured = Vec::new();
    if let Some(Value::Object(block)) = root.get("provider") {
        for (id, entry) in block {
            let Value::Object(entry) = entry else {
                continue;
            };
            configured.push(id.as_str());
            if !considered(id) {
                continue;
            }
            match gateway(id, entry, || api_key(id)) {
                Ok(gateway) => gateways.push(gateway),
                Err(reason) => left.push(Left {
                    name: id.clone(),
                    reason,
                }),
            }
        }
    }

    for (id, entry) in signed_in.into_iter().flatten() {
        if !considered(id) {
            continue;
        }
        let has_entry = configured.contains(&id.as_str());
        let reason = match entry.get("type").and_then(Value::as_str) {
            Some("api") => {
                let Some(key) = api_key(id).map(Secret::new) else {
                    continue;
                };
                // What opencode keeps for this id is a Bedrock API key, beside an entry or not.
                if id == AWS_PROVIDER_ID {
                    Reason::BearerToken
                } else if has_entry {
                    continue;
                } else if !speaks_the_protocol(id, None) {
                    Reason::AnotherSdk
                } else if id == GOOGLE_VERTEX_ID {
                    // A key with no project is express mode, which needs nothing else.
                    gateways.push(Gateway {
                        id: id.clone(),
                        endpoint: provider::VERTEX_EXPRESS_ENDPOINT.to_string(),
                        entry: Map::new(),
                        key: Key::Held(key),
                    });
                    continue;
                } else if let Some(endpoint) = provider::known_endpoint(id) {
                    gateways.push(Gateway {
                        id: id.clone(),
                        endpoint: endpoint.to_string(),
                        entry: Map::new(),
                        key: Key::Held(key),
                    });
                    continue;
                } else {
                    Reason::NoEndpoint
                }
            }
            Some("oauth" | "wellknown") if !has_entry => Reason::SignIn,
            _ => continue,
        };
        left.push(Left {
            name: id.clone(),
            reason,
        });
    }

    let mut model_gateway = None;
    let model = string(root.get("model")).and_then(|named| {
        let (id, wire) = named.split_once('/')?;
        let gateway = gateways.iter_mut().find(|gateway| gateway.id == id)?;
        model_gateway = Some(id.to_string());
        if id != AWS_PROVIDER_ID {
            return Some(named.clone());
        }
        // An AWS entry answers only for a model it lists, and by the model's own id.
        if let Value::Object(models) = gateway
            .entry
            .entry("models")
            .or_insert_with(|| Value::Object(Map::new()))
        {
            models
                .entry(wire)
                .or_insert_with(|| Value::Object(Map::new()));
        }
        Some(wire.to_string())
    });

    let found = Found {
        source: Source::Opencode,
        read: read_from,
        running_at: None,
        window: None,
        env: Vec::new(),
        model,
        model_gateway,
        gateways,
        left,
    };
    (found.importable() || !found.left.is_empty()).then_some(found)
}

/// opencode's merge of its configuration files: objects deeply, anything else replaced.
fn merge(into: &mut Map<String, Value>, from: Map<String, Value>) {
    for (name, value) in from {
        match (into.get_mut(&name), value) {
            (Some(Value::Object(under)), Value::Object(above)) => merge(under, above),
            (_, value) => {
                if let Some(mut displaced) = into.insert(name, value) {
                    crate::scrub_value(&mut displaced);
                }
            }
        }
    }
}

/// The ids opencode reaches through an SDK of their own where the entry names none, each in a
/// protocol other than OpenAI's and none of them one this program sends.
///
/// `anthropic` is not here: it is reached in the Messages protocol, which this program does send.
const OWN_SDK_IDS: [&str; 4] = ["azure", "cohere", "google", "google-vertex-anthropic"];

/// Whether an entry's `npm` names an SDK speaking the protocol this entry is reached by.
///
/// On top of [`Provider::all`], which ignores the field: an `@ai-sdk/anthropic` entry under any id
/// but `anthropic`, with a `baseURL`, would be kept there and then sent requests in a protocol that
/// id is not reached by. An entry naming no SDK gets the one opencode picks for its id.
fn speaks_the_protocol(id: &str, npm: Option<&str>) -> bool {
    match npm {
        None => !OWN_SDK_IDS.contains(&id),
        // Not for `anthropic`, which an entry written out here would then be read as reaching in the
        // Messages protocol: the field that says otherwise is not written.
        Some("@ai-sdk/openai-compatible") => id != ANTHROPIC_ID,
        Some("@openrouter/ai-sdk-provider") => id == "openrouter",
        Some("@ai-sdk/amazon-bedrock") => id == AWS_PROVIDER_ID,
        Some("@ai-sdk/google-vertex") => id == GOOGLE_VERTEX_ID,
        Some("@ai-sdk/anthropic") => id == ANTHROPIC_ID,
        Some(_) => false,
    }
}

/// A whole-value `{env:NAME}` or `{file:path}` reference.
enum Reference {
    Env(String),
    File(String),
}

fn reference(value: &str) -> Option<Reference> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    if inner.contains(['{', '}']) {
        return None;
    }
    if let Some(name) = inner.strip_prefix("env:") {
        return Some(Reference::Env(name.trim().to_string()))
            .filter(|named| matches!(named, Reference::Env(name) if !name.is_empty()));
    }
    inner
        .strip_prefix("file:")
        .map(|path| Reference::File(path.trim().to_string()))
}

/// One opencode `provider` entry as bravebot would write it, or why it is not offered.
///
/// Offered exactly where [`Provider::all`] keeps it and it speaks the protocol, so one definition
/// of "supported" holds for a copied block and an imported one alike.
fn gateway(
    id: &str,
    entry: &Map<String, Value>,
    signed_in: impl Fn() -> Option<String>,
) -> Result<Gateway, Reason> {
    let npm = entry.get("npm").and_then(Value::as_str).map(str::trim);
    if !speaks_the_protocol(id, npm) {
        return Err(Reason::AnotherSdk);
    }
    if id == GOOGLE_VERTEX_ID {
        // The OpenAI-compatible endpoint exists only for a stated project; a `baseURL` does not
        // substitute for one. An entry naming none is express mode, which needs the key the entry
        // states and the host compiled in: one stating a `baseURL` or no key is left, since
        // opencode serves it through an SDK route this program does not take.
        let options = entry.get("options").and_then(Value::as_object);
        let project = string(options.and_then(|options| options.get("project")));
        let location = string(options.and_then(|options| options.get("location")));
        let named = |name: &str| string(options.and_then(|options| options.get(name)));
        let usable = match project {
            Some(_) => provider::vertex_endpoint(project.as_deref(), location.as_deref()).is_some(),
            None => named("baseURL").is_none() && named("apiKey").is_some(),
        };
        if !usable {
            return Err(Reason::Vertex);
        }
    }
    let aws = id == AWS_PROVIDER_ID;
    let mut probe = Parsed::default();
    let mut block = Map::new();
    block.insert(id.to_string(), Value::Object(entry.clone()));
    probe.0.insert("provider".to_string(), Value::Object(block));
    let Some(kept) = Provider::all(&probe.0).pop() else {
        return Err(if aws {
            Reason::NoRegion
        } else if id == GOOGLE_VERTEX_ID {
            Reason::Vertex
        } else {
            Reason::NoEndpoint
        });
    };
    // A substitution opencode would have made is not made here, so a host still holding one names
    // nowhere.
    let endpoint = kept.base_url.clone();
    let reachable = (endpoint.starts_with("https://") || endpoint.starts_with("http://"))
        && !endpoint.contains(['{', '}', ' ']);
    if !reachable {
        return Err(Reason::NoEndpoint);
    }

    let options = entry.get("options").and_then(Value::as_object);
    let option = |name: &str| string(options.and_then(|options| options.get(name)));
    let mut written = Map::new();
    if let Some(name) = string(entry.get("name")) {
        written.insert("name".to_string(), Value::String(name));
    }

    let mut key = Key::Named;
    if !aws {
        let mut env = kept.env.clone();
        match option("apiKey").map(Secret::new) {
            Some(value) => match reference(value.expose()) {
                Some(Reference::Env(name)) => {
                    if !env.contains(&name) {
                        env.push(name);
                    }
                }
                Some(Reference::File(path)) => key = Key::File(path),
                // Held as it is, the unsubstituted text would be written and sent as the key.
                None if ["{env:", "{file:"]
                    .iter()
                    .any(|opening| value.expose().contains(opening)) =>
                {
                    return Err(Reason::Substitution);
                }
                None => key = Key::Held(value),
            },
            None if env.is_empty() => {
                if let Some(held) = signed_in() {
                    key = Key::Held(Secret::new(held));
                }
            }
            None => {}
        }
        // opencode reads a known id's token from its variable without the entry naming it; this
        // program reads only what the entry names.
        if matches!(key, Key::Named)
            && env.is_empty()
            && let Some(variable) = provider::known_variable(id)
        {
            env.push(variable.to_string());
        }
        if !env.is_empty() {
            written.insert(
                "env".to_string(),
                Value::Array(env.into_iter().map(Value::String).collect()),
            );
        }
    }

    let mut kept_options = Map::new();
    let fields: &[&str] = if aws {
        &["region", "profile"]
    } else if id == GOOGLE_VERTEX_ID {
        &["baseURL", "project", "location"]
    } else {
        &["baseURL"]
    };
    for field in fields {
        if let Some(value) = option(field) {
            kept_options.insert(field.to_string(), Value::String(value));
        }
    }
    if !kept_options.is_empty() {
        written.insert("options".to_string(), Value::Object(kept_options));
    }

    if let Some(Value::Object(models)) = entry.get("models") {
        let models: Map<String, Value> = models
            .iter()
            .map(|(model, stated)| (model.clone(), Value::Object(model_entry(stated, aws))))
            .collect();
        if !models.is_empty() {
            written.insert("models".to_string(), Value::Object(models));
        }
    }

    Ok(Gateway {
        id: id.to_string(),
        endpoint,
        entry: written,
        key,
    })
}

/// One model entry, holding only what bravebot reads of one: `limit` for either kind, `name` for an
/// AWS entry, and `options` for a gateway's, where they are merged into the request.
fn model_entry(stated: &Value, aws: bool) -> Map<String, Value> {
    let mut kept = Map::new();
    let Some(stated) = stated.as_object() else {
        return kept;
    };
    if aws && let Some(name) = string(stated.get("name")) {
        kept.insert("name".to_string(), Value::String(name));
    }
    // Kept where bravebot's own reading takes either half from it: both halves stated, and one of
    // them above zero.
    if let Some(Value::Object(limit)) = stated.get("limit") {
        let figure = |name: &str| limit.get(name).and_then(Value::as_u64);
        if let (Some(context), Some(output)) = (figure("context"), figure("output"))
            && (context > 0 || output > 0)
        {
            let mut both = Map::new();
            both.insert("context".to_string(), Value::from(context));
            both.insert("output".to_string(), Value::from(output));
            kept.insert("limit".to_string(), Value::Object(both));
        }
    }
    if !aws
        && let Some(Value::Object(options)) = stated.get("options")
        && !options.is_empty()
    {
        kept.insert("options".to_string(), Value::Object(options.clone()));
    }
    kept
}

/// A settings file, as a document an import may add names to or `bravebot mcp` may add a request
/// to or take one out of.
///
/// Holds what the file says, so it clears itself when it goes. Nothing here writes: [`text`] is what
/// the caller puts on disk.
///
/// [`text`]: Destination::text
pub struct Destination {
    path: PathBuf,
    root: Parsed,
    /// The file as it was read, or `None` where there was none.
    read: Option<Secret>,
}

/// Why the settings file cannot be added to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unwritable {
    /// It is there and does not hold a settings document, so rewriting it would lose what the
    /// person wrote.
    NotADocument,
    /// It is, or would be with the import in it, past what the settings reader reads, which drops
    /// every name in it.
    TooLarge,
    /// It changed after it was read, so writing the document read then would lose the change.
    Changed,
}

impl Destination {
    /// The file at `path`, or an empty document where there is none yet.
    pub fn open(path: &Path) -> Result<Self, Unwritable> {
        let (root, read) = match std::fs::metadata(path) {
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {
                (Parsed::default(), None)
            }
            Err(_) => return Err(Unwritable::NotADocument),
            Ok(found) if found.len() > crate::settings::MAX_BYTES => {
                return Err(Unwritable::TooLarge);
            }
            Ok(_) => {
                let text = Secret::new(
                    std::fs::read_to_string(path).map_err(|_| Unwritable::NotADocument)?,
                );
                let root = match text.expose().trim().is_empty() {
                    true => Some(Parsed::default()),
                    false => parse(text.expose()),
                };
                (root.ok_or(Unwritable::NotADocument)?, Some(text))
            }
        };
        Ok(Self {
            path: path.to_path_buf(),
            root,
            read,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the file on disk is no longer the one this was read from.
    pub fn changed(&self) -> bool {
        match (std::fs::read(&self.path), &self.read) {
            (Err(missing), None) => missing.kind() != std::io::ErrorKind::NotFound,
            (Ok(mut now), read) => {
                let same = read
                    .as_ref()
                    .is_some_and(|then| then.expose().as_bytes() == now.as_slice());
                crate::scrub_bytes(&mut now);
                !same
            }
            (Err(_), Some(_)) => true,
        }
    }

    /// Whether `env` already sets `name` to a string, which is the only value the settings reader
    /// takes from it, or is something other than a block a name could go in.
    pub fn holds_env(&self, name: &str) -> bool {
        match self.root.0.get("env") {
            None => false,
            Some(Value::Object(block)) => block.get(name).is_some_and(Value::is_string),
            Some(_) => true,
        }
    }

    /// The string `env` sets `name` to, which is the only value the settings reader takes from it.
    pub fn env(&self, name: &str) -> Option<&str> {
        self.root.0.get("env")?.get(name)?.as_str()
    }

    /// Whether `env` has an entry for `name`, whatever its value, or is something other than a
    /// block.
    pub fn names_env(&self, name: &str) -> bool {
        match self.root.0.get("env") {
            None => false,
            Some(Value::Object(block)) => block.contains_key(name),
            Some(_) => true,
        }
    }

    /// Whether `provider` already has an entry for `id`, or is something other than a block.
    pub fn holds_gateway(&self, id: &str) -> bool {
        match self.root.0.get("provider") {
            None => false,
            Some(Value::Object(block)) => block.contains_key(id),
            Some(_) => true,
        }
    }

    /// The id of an entry under `provider` whose `options.baseURL` reaches the server `endpoint`
    /// does on this machine, whatever the entry is called.
    ///
    /// By port alone, since `localhost`, `127.0.0.1` and `[::1]` are one server, and a second entry
    /// for it would list every one of its models twice.
    pub fn serving_locally(&self, endpoint: &str) -> Option<&str> {
        let port = local_port(endpoint)?;
        let Some(Value::Object(block)) = self.root.0.get("provider") else {
            return None;
        };
        block.iter().find_map(|(id, entry)| {
            let base = entry.get("options")?.get("baseURL")?.as_str()?;
            (local_port(base) == Some(port)).then_some(id.as_str())
        })
    }

    /// Whether the file already names a model, as the settings reader reads one.
    pub fn holds_model(&self) -> bool {
        crate::settings::word(&self.root.0, "model").is_some()
    }

    /// Add one name under `env`, where it is not already set.
    pub fn add_env(&mut self, name: &str, value: &str) {
        if !self.holds_env(name) {
            block(&mut self.root.0, "env").insert(name.to_string(), Value::from(value));
        }
    }

    /// Add one entry under `provider`, where there is none for its id.
    pub fn add_gateway(&mut self, mut gateway: Gateway) {
        if !self.holds_gateway(&gateway.id) {
            let entry = std::mem::take(&mut gateway.entry);
            block(&mut self.root.0, "provider").insert(gateway.id.clone(), Value::Object(entry));
        }
    }

    /// Name the model, where the file names none.
    pub fn add_model(&mut self, model: &str) {
        if !self.holds_model() {
            self.root.0.insert("model".to_string(), Value::from(model));
        }
    }

    /// Whether `mcp.request` names `alias`, as the settings reader reads a request.
    pub fn requests(&self, alias: &str) -> bool {
        match self
            .root
            .0
            .get("mcp")
            .and_then(|block| block.get("request"))
        {
            Some(Value::Array(names)) => names.iter().any(|name| name.as_str() == Some(alias)),
            _ => false,
        }
    }

    /// Add `alias` to `mcp.request`, making the block or the list where there is none, and say
    /// whether it went in.
    ///
    /// Not where `mcp` is something other than a block or `mcp.request` something other than a
    /// list: the settings reader ignores either, and replacing it would lose what the person wrote.
    /// A `null` list says nothing to lose, and is replaced.
    pub fn request(&mut self, alias: &str) -> bool {
        let block = match self
            .root
            .0
            .entry("mcp")
            .or_insert_with(|| Value::Object(Map::new()))
        {
            Value::Object(block) => block,
            _ => return false,
        };
        let names = block.entry("request").or_insert(Value::Null);
        if names.is_null() {
            *names = Value::Array(Vec::new());
        }
        match names {
            Value::Array(names) => {
                if !names.iter().any(|name| name.as_str() == Some(alias)) {
                    names.push(Value::from(alias));
                }
                true
            }
            _ => false,
        }
    }

    /// Take every mention of `alias` out of `mcp.request`, and say whether there was one.
    pub fn withdraw(&mut self, alias: &str) -> bool {
        let Some(Value::Array(names)) = self
            .root
            .0
            .get_mut("mcp")
            .and_then(|block| block.get_mut("request"))
        else {
            return false;
        };
        let before = names.len();
        names.retain(|name| name.as_str() != Some(alias));
        names.len() != before
    }

    /// The whole file as it would be written.
    ///
    /// A [`Secret`], because it holds whatever key the person approved keeping in it.
    pub fn text(&self) -> Result<Secret, Unwritable> {
        let mut text = serde_json::to_string_pretty(&self.root.0).unwrap_or_default();
        text.push('\n');
        let text = Secret::new(text);
        match text.expose().len() as u64 > crate::settings::MAX_BYTES {
            true => Err(Unwritable::TooLarge),
            false => Ok(text),
        }
    }
}

/// The block under `name`, made where there is none. Only called where the destination said a name
/// could go in, so an existing value is an object.
fn block<'a>(root: &'a mut Map<String, Value>, name: &str) -> &'a mut Map<String, Value> {
    let value = root
        .entry(name)
        .or_insert_with(|| Value::Object(Map::new()));
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    match value {
        Value::Object(block) => block,
        _ => unreachable!("made an object above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch profile directory that removes itself.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self { path }
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.path.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::fs::write(&path, text).expect("write");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// The lookup a process with `home` as its profile directory and `set` exported would have.
    fn environment<'a>(
        home: &'a Path,
        set: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            if crate::settings::PROFILE_VARIABLES.first() == Some(&name) {
                return Some(home.display().to_string());
            }
            set.iter()
                .find(|(named, _)| *named == name)
                .map(|(_, value)| value.to_string())
        }
    }

    fn found_in(home: &Path, set: &[(&str, &str)]) -> Vec<Found> {
        let lookup = environment(home, set);
        found(&Places::from_lookup(&lookup), &lookup, |_, _| None)
    }

    fn one(home: &Path, set: &[(&str, &str)], source: Source) -> Found {
        found_in(home, set)
            .into_iter()
            .find(|found| found.source == source)
            .unwrap_or_else(|| panic!("nothing found from {}", source.name()))
    }

    fn env_of(found: &Found) -> Vec<(&str, &str)> {
        found
            .env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect()
    }

    fn ids(found: &Found) -> Vec<&str> {
        found.gateways.iter().map(|it| it.id.as_str()).collect()
    }

    fn gateway<'a>(found: &'a Found, id: &str) -> &'a Gateway {
        found
            .gateways
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("no {id} among {:?}", ids(found)))
    }

    fn left(found: &Found) -> Vec<(&str, Reason)> {
        found
            .left
            .iter()
            .map(|it| (it.name.as_str(), it.reason))
            .collect()
    }

    const BEDROCK: &str = r#"{"env": {
        "CLAUDE_CODE_USE_BEDROCK": "1",
        "AWS_REGION": "us-west-2",
        "AWS_PROFILE": "work-sso",
        "ANTHROPIC_DEFAULT_OPUS_MODEL": "arn:aws:bedrock:us-west-2:1:application-inference-profile/opus",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL": "arn:aws:bedrock:us-west-2:1:application-inference-profile/haiku"
    }, "model": "opus"}"#;

    /// IMPORT-2: a checkout's files hold whatever the repository's author wrote. Every file read is
    /// under the profile directory or a directory a variable named absolutely, and a relative value
    /// is not resolved against the directory the program was started in, which is the checkout.
    #[test]
    fn a_checkouts_opencode_json_is_never_read() {
        let home = Scratch::new("import-checkout-home");
        let named = Scratch::new("import-checkout-named");
        for relative in [
            "opencode.json",
            ".opencode/opencode.json",
            ".claude/settings.json",
        ] {
            assert!(
                Path::new(relative).is_relative(),
                "the fixture names a relative file"
            );
        }
        let relative = [
            ("CLAUDE_CONFIG_DIR", ".claude"),
            ("XDG_CONFIG_HOME", ".config"),
            ("XDG_DATA_HOME", ".local/share"),
            ("OPENCODE_CONFIG", "opencode.json"),
        ];
        let places = Places::from_lookup(environment(&home.path, &relative));
        let files: Vec<&Path> = places.files().collect();
        assert!(!files.is_empty(), "nothing would be read at all");
        for file in &files {
            assert!(
                file.starts_with(&home.path),
                "{} is outside the profile directory",
                file.display()
            );
        }
        assert_eq!(
            places.claude_code, None,
            "a relative CLAUDE_CONFIG_DIR was read somewhere"
        );

        let absolute = named.write("opencode.json", "{}");
        let set = [("OPENCODE_CONFIG", absolute.to_str().expect("utf-8"))];
        let places = Places::from_lookup(environment(&home.path, &set));
        assert!(
            places.files().any(|file| file == absolute),
            "an absolute OPENCODE_CONFIG was not read"
        );

        let nowhere = Places::from_lookup(|name| match name {
            "OPENCODE_CONFIG" => Some(absolute.display().to_string()),
            _ => None,
        });
        assert_eq!(
            nowhere,
            Places::default(),
            "a machine naming no profile directory read something"
        );
    }

    /// IMPORT-2: somebody who moved Claude Code's directory keeps its settings there, and the
    /// default holds whatever an older setup left.
    #[test]
    fn claude_config_dir_moves_where_claude_code_is_read() {
        let home = Scratch::new("import-claude-dir-home");
        let moved = Scratch::new("import-claude-dir-moved");
        home.write(
            ".claude/settings.json",
            r#"{"env": {"AWS_REGION": "eu-west-1"}}"#,
        );
        let file = moved.write("settings.json", BEDROCK);
        let set = [("CLAUDE_CONFIG_DIR", moved.path.to_str().expect("utf-8"))];

        let found = one(&home.path, &set, Source::ClaudeCode);

        assert_eq!(found.read, [file]);
        assert!(env_of(&found).contains(&("AWS_REGION", "us-west-2")));
    }

    /// IMPORT-2: the XDG variables are where opencode keeps its files, so they are where it is read.
    #[test]
    fn xdg_config_home_moves_where_opencode_is_read() {
        let home = Scratch::new("import-xdg-home");
        let config = Scratch::new("import-xdg-config");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"default-place": {"options": {"baseURL": "https://default.invalid/v1"}}}}"#,
        );
        config.write(
            "opencode/opencode.json",
            r#"{"provider": {"moved-place": {"options": {"baseURL": "https://moved.invalid/v1"}}}}"#,
        );
        let set = [("XDG_CONFIG_HOME", config.path.to_str().expect("utf-8"))];

        let found = one(&home.path, &set, Source::Opencode);

        assert_eq!(ids(&found), ["moved-place"]);
    }

    /// IMPORT-3: Claude Code's Bedrock names are bravebot's own apart from the switch, and a tier
    /// word is copied where the tier it names was.
    #[test]
    fn a_claude_code_bedrock_block_becomes_bravebots_own_names() {
        let home = Scratch::new("import-bedrock");
        home.write(".claude/settings.json", BEDROCK);

        let found = one(&home.path, &[], Source::ClaudeCode);

        assert_eq!(
            env_of(&found),
            [
                ("BRAVEBOT_USE_BEDROCK", "1"),
                ("AWS_REGION", "us-west-2"),
                ("AWS_PROFILE", "work-sso"),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL",
                    "arn:aws:bedrock:us-west-2:1:application-inference-profile/opus"
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                    "arn:aws:bedrock:us-west-2:1:application-inference-profile/haiku"
                ),
            ]
        );
        assert_eq!(found.model.as_deref(), Some("opus"));
        assert!(found.left.is_empty(), "{:?}", found.left);

        // The switch exported rather than written, with the region beside it: the region is read
        // from there at run time, so it is not copied, and the switch is still bravebot's own.
        home.write(".claude/settings.json", r#"{"model": "sonnet"}"#);
        let set = [
            ("CLAUDE_CODE_USE_BEDROCK", "true"),
            ("AWS_REGION", "us-east-1"),
        ];
        let found = one(&home.path, &set, Source::ClaudeCode);
        assert_eq!(env_of(&found), [("BRAVEBOT_USE_BEDROCK", "1")]);
        assert_eq!(
            found.model, None,
            "a tier word whose tier nobody named was copied"
        );

        // A name the environment already sets is read from there at run time, so it is not copied.
        home.write(".claude/settings.json", BEDROCK);
        let set = [("AWS_REGION", "us-east-1"), (env_var::USE_BEDROCK, "1")];
        let found = one(&home.path, &set, Source::ClaudeCode);
        let names: Vec<&str> = env_of(&found).into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "AWS_PROFILE",
                "ANTHROPIC_DEFAULT_OPUS_MODEL",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL"
            ]
        );
    }

    /// IMPORT-3: without a region Bedrock is not configured at all, so offering the switch would
    /// write a setup that answers nothing. What was found is still said.
    #[test]
    fn bedrock_without_a_region_is_left_and_said() {
        let home = Scratch::new("import-no-region");
        home.write(
            ".claude/settings.json",
            r#"{"env": {"CLAUDE_CODE_USE_BEDROCK": "1", "AWS_PROFILE": "work"}, "model": "opus"}"#,
        );

        let found = one(&home.path, &[], Source::ClaudeCode);

        assert!(!found.importable(), "{:?}", found.env);
        assert_eq!(
            left(&found),
            [("CLAUDE_CODE_USE_BEDROCK", Reason::NoRegion)]
        );
    }

    /// IMPORT-3: one definition of supported. Every entry `Provider::all` drops is not offered, and
    /// every one it keeps is, where it speaks the protocol and its host names somewhere.
    #[test]
    fn an_opencode_block_is_offered_exactly_when_bravebot_would_keep_it() {
        let home = Scratch::new("import-kept");
        let block = r#"{"provider": {
            "stated": {"options": {"baseURL": "https://gateway.invalid/v1"}},
            "openrouter": {"models": {"z-ai/glm-4.6": {}}},
            "nowhere": {"models": {"m": {}}},
            "blank": {"options": {"baseURL": "  "}},
            "substituted": {"options": {"baseURL": "{env:GATEWAY_HOST}/v1"}},
            "amazon-bedrock": {"options": {"region": "us-west-2"}},
            "not-an-entry": "text"
        }}"#;
        home.write(".config/opencode/opencode.json", block);

        let found = one(&home.path, &[], Source::Opencode);

        let root: Map<String, Value> = serde_json::from_str(block).expect("json");
        let mut kept: Vec<String> = Provider::all(&root).into_iter().map(|it| it.id).collect();
        // Kept by that reading, which makes no substitution, and so sent to no host at all.
        assert!(kept.iter().any(|id| id == "substituted"), "{kept:?}");
        kept.retain(|id| id != "substituted");
        assert_eq!(ids(&found), kept);
        assert_eq!(
            left(&found),
            [
                ("blank", Reason::NoEndpoint),
                ("nowhere", Reason::NoEndpoint),
                ("substituted", Reason::NoEndpoint)
            ]
        );
        assert_eq!(
            gateway(&found, "openrouter").endpoint,
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(
            gateway(&found, "amazon-bedrock").endpoint,
            "https://bedrock-runtime.us-west-2.amazonaws.com"
        );
    }

    /// IMPORT-3: the entry opencode writes for Vertex AI names a project, and it is offered with the
    /// two fields the host is built from and the variable this program reads the key from.
    #[test]
    fn a_google_vertex_entry_naming_a_project_is_offered_with_its_project_and_location() {
        let home = Scratch::new("import-vertex-project");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"google-vertex": {
                "npm": "@ai-sdk/google-vertex",
                "options": {"project": "example-project-1", "location": "us-east5"}
            }}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["google-vertex"]);
        let vertex = gateway(&found, "google-vertex");
        assert_eq!(
            vertex.endpoint,
            "https://us-east5-aiplatform.googleapis.com/v1/projects/example-project-1/locations/us-east5/endpoints/openapi"
        );
        assert_eq!(vertex.variables(), [env_var::GOOGLE_API_KEY]);
        assert_eq!(
            vertex.entry.get("options"),
            Some(&serde_json::json!({"project": "example-project-1", "location": "us-east5"}))
        );
    }

    /// IMPORT-3: the entry opencode writes for Vertex AI express mode names an API key and no
    /// project, and is offered at the host compiled in with the key as IMPORT-6 says.
    #[test]
    fn a_google_vertex_entry_naming_a_key_and_no_project_is_offered_in_express_mode() {
        let home = Scratch::new("import-vertex-express");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"google-vertex": {
                "npm": "@ai-sdk/google-vertex",
                "options": {"apiKey": "{env:GOOGLE_VERTEX_API_KEY}"}
            }}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["google-vertex"]);
        let vertex = gateway(&found, "google-vertex");
        assert_eq!(vertex.endpoint, "https://aiplatform.googleapis.com/v1");
        assert_eq!(vertex.variables(), ["GOOGLE_VERTEX_API_KEY"]);
        assert!(left(&found).is_empty(), "{:?}", left(&found));
    }

    /// IMPORT-3: a key `opencode auth login` keeps for Vertex AI is express mode too, since it has
    /// no project to name.
    #[test]
    fn an_auth_json_key_for_google_vertex_is_offered_in_express_mode() {
        let home = Scratch::new("import-vertex-express-auth");
        home.write(
            ".local/share/opencode/auth.json",
            r#"{"google-vertex": {"type": "api", "key": "a-vertex-key"}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        let vertex = gateway(&found, "google-vertex");
        assert_eq!(vertex.endpoint, "https://aiplatform.googleapis.com/v1");
        match &vertex.key {
            Key::Held(key) => assert_eq!(key.expose(), "a-vertex-key"),
            other => panic!("the key was not held: {other:?}"),
        }
        assert!(left(&found).is_empty(), "{:?}", left(&found));
        assert!(!format!("{found:?}").contains("a-vertex-key"));
    }

    /// IMPORT-3 and IMPORT-4: with no project and no key there is nothing to reach the service
    /// with, and a project that is a reference opencode would have substituted names no project
    /// here.
    #[test]
    fn a_google_vertex_entry_naming_no_project_and_no_key_is_left_and_said() {
        let home = Scratch::new("import-vertex-no-project");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"google-vertex": {"npm": "@ai-sdk/google-vertex"}}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert!(ids(&found).is_empty());
        assert_eq!(left(&found), [("google-vertex", Reason::Vertex)]);

        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"google-vertex": {"options": {"project": "{env:GOOGLE_CLOUD_PROJECT}"}}}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert!(ids(&found).is_empty());
        assert_eq!(left(&found), [("google-vertex", Reason::Vertex)]);
    }

    /// IMPORT-3: a `baseURL` is not a project. opencode still serves this id through its own SDK
    /// when the entry names no project, so a stated proxy host must not make it look like the
    /// OpenAI-compatible gateway `google-vertex` only becomes with one.
    #[test]
    fn a_google_vertex_entry_naming_a_base_url_and_no_project_is_left_and_said() {
        let home = Scratch::new("import-vertex-baseurl-no-project");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"google-vertex": {
                "options": {"baseURL": "https://vertex-proxy.example/v1", "apiKey": "a-key"}
            }}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert!(ids(&found).is_empty());
        assert_eq!(left(&found), [("google-vertex", Reason::Vertex)]);
    }

    /// IMPORT-3: opencode reads an `openai` entry naming no SDK, and bravebot reaches the same host
    /// through its chat-completions route. The entry arrives naming the variable opencode reads the
    /// key from, since this program reads only what an entry names.
    #[test]
    fn an_openai_entry_naming_no_sdk_is_offered_at_openais_endpoint() {
        let home = Scratch::new("import-openai");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"openai": {"name": "OpenAI"}}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["openai"]);
        let openai = gateway(&found, "openai");
        assert_eq!(openai.endpoint, "https://api.openai.com/v1");
        assert_eq!(openai.variables(), ["OPENAI_API_KEY"]);
        assert!(left(&found).is_empty(), "{:?}", left(&found));
    }

    /// IMPORT-3: `@ai-sdk/openai` speaks the Responses API, and the entry import writes names no SDK,
    /// so one written from it would be sent chat completions at a host that takes the other protocol.
    /// An entry naming it is left even though the host it would reach is one this program knows.
    #[test]
    fn an_openai_entry_naming_the_responses_sdk_is_left_as_another_sdk() {
        let home = Scratch::new("import-openai-sdk");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"openai": {"npm": "@ai-sdk/openai"}}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert!(ids(&found).is_empty(), "{:?}", ids(&found));
        assert_eq!(left(&found), [("openai", Reason::AnotherSdk)]);
    }

    /// IMPORT-3: `opencode auth login` keeps an OpenAI key in `auth.json` and writes no config
    /// entry. The endpoint is compiled in, so the key is a gateway rather than a `NoEndpoint`.
    #[test]
    fn an_auth_json_key_for_openai_is_a_provider() {
        let home = Scratch::new("import-openai-auth");
        home.write(
            ".local/share/opencode/auth.json",
            r#"{"openai": {"type": "api", "key": "a-held-key"}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        let openai = gateway(&found, "openai");
        assert_eq!(openai.endpoint, "https://api.openai.com/v1");
        match &openai.key {
            Key::Held(key) => assert_eq!(key.expose(), "a-held-key"),
            other => panic!("the key was not held: {other:?}"),
        }
        assert_eq!(openai.fallback(), Some("OPENAI_API_KEY"));
        assert!(left(&found).is_empty(), "{:?}", left(&found));
    }

    /// IMPORT-3: an entry for another SDK would be kept by `Provider::all` and then sent requests in
    /// a protocol it does not speak.
    #[test]
    fn an_entry_naming_another_sdk_is_not_offered() {
        let home = Scratch::new("import-sdk");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {
                "anthropic-proxy": {"npm": "@ai-sdk/anthropic", "options": {"baseURL": "https://proxy.invalid"}},
                "not-openrouter": {"npm": "@openrouter/ai-sdk-provider", "options": {"baseURL": "https://x.invalid/v1"}},
                "compatible": {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "https://y.invalid/v1"}},
                "openrouter": {"npm": "@openrouter/ai-sdk-provider"},
                "google": {"options": {"baseURL": "https://gemini-proxy.invalid"}}
            }}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["compatible", "openrouter"]);
        // Naming no SDK, `google` gets its own from opencode, which does not speak OpenAI's, and
        // `@ai-sdk/anthropic` is the Messages protocol, which only the `anthropic` id is reached by.
        assert_eq!(
            left(&found),
            [
                ("anthropic-proxy", Reason::AnotherSdk),
                ("google", Reason::AnotherSdk),
                ("not-openrouter", Reason::AnotherSdk)
            ]
        );
    }

    /// IMPORT-3: opencode reads an `anthropic` entry naming no SDK through the Anthropic SDK, and
    /// bravebot reaches the same host through the Messages API. The entry arrives naming the
    /// variable opencode reads the key from, since this program reads only what an entry names.
    #[test]
    fn an_anthropic_entry_naming_no_sdk_is_offered_at_anthropics_endpoint() {
        let home = Scratch::new("import-anthropic");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"anthropic": {"name": "Anthropic"}}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["anthropic"]);
        let anthropic = gateway(&found, "anthropic");
        assert_eq!(anthropic.endpoint, "https://api.anthropic.com/v1");
        assert_eq!(anthropic.variables(), ["ANTHROPIC_API_KEY"]);
        assert!(left(&found).is_empty(), "{:?}", left(&found));
    }

    /// IMPORT-3: naming the Anthropic SDK is how opencode says the same thing, and a `baseURL` the
    /// entry states is the one written, so a proxy in front of the API is reached as it was there.
    #[test]
    fn an_anthropic_entry_naming_its_own_sdk_keeps_the_endpoint_it_states() {
        let home = Scratch::new("import-anthropic-sdk");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"anthropic": {
                "npm": "@ai-sdk/anthropic",
                "options": {"baseURL": "https://proxy.invalid/anthropic/v1", "apiKey": "{env:MY_CLAUDE_KEY}"}
            }}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["anthropic"]);
        let anthropic = gateway(&found, "anthropic");
        assert_eq!(anthropic.endpoint, "https://proxy.invalid/anthropic/v1");
        assert_eq!(anthropic.variables(), ["MY_CLAUDE_KEY"]);
        assert!(left(&found).is_empty(), "{:?}", left(&found));
    }

    /// IMPORT-3: an `anthropic` entry that names the OpenAI-compatible SDK is a gateway that happens to
    /// be called that, and the entry written out would drop the field that says so and be read as
    /// reaching Anthropic's own protocol. It is left rather than imported as something it is not.
    #[test]
    fn an_anthropic_entry_naming_the_openai_compatible_sdk_is_left_as_another_sdk() {
        let home = Scratch::new("import-anthropic-compatible");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"anthropic": {
                "npm": "@ai-sdk/openai-compatible",
                "options": {"baseURL": "https://litellm.invalid/v1"}
            }}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert!(ids(&found).is_empty(), "{:?}", ids(&found));
        assert_eq!(left(&found), [("anthropic", Reason::AnotherSdk)]);
    }

    /// IMPORT-3: only the `anthropic` id is reached in the Messages protocol, so a proxy kept under
    /// another id that names the Anthropic SDK would be written out as a gateway speaking chat
    /// completions, which is not what it was there. It is left, and says why.
    #[test]
    fn an_anthropic_sdk_entry_under_another_id_is_left_as_another_sdk() {
        let home = Scratch::new("import-anthropic-sdk-elsewhere");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"claude-proxy": {
                "npm": "@ai-sdk/anthropic",
                "options": {"baseURL": "https://proxy.invalid/anthropic/v1", "apiKey": "{env:MY_CLAUDE_KEY}"}
            }}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert!(ids(&found).is_empty(), "{:?}", ids(&found));
        assert_eq!(left(&found), [("claude-proxy", Reason::AnotherSdk)]);
    }

    /// IMPORT-3: `opencode auth login` keeps an Anthropic API key in `auth.json` and writes no
    /// config entry. The endpoint is compiled in, so the key is a gateway rather than a
    /// `NoEndpoint`, and a sign-in beside it is still left, since this program takes only a key.
    #[test]
    fn an_auth_json_key_for_anthropic_is_a_provider_and_its_sign_in_is_not() {
        let home = Scratch::new("import-anthropic-auth");
        home.write(
            ".local/share/opencode/auth.json",
            r#"{"anthropic": {"type": "api", "key": "a-held-key"}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        let anthropic = gateway(&found, "anthropic");
        assert_eq!(anthropic.endpoint, "https://api.anthropic.com/v1");
        match &anthropic.key {
            Key::Held(key) => assert_eq!(key.expose(), "a-held-key"),
            other => panic!("the key was not held: {other:?}"),
        }
        assert_eq!(anthropic.fallback(), Some("ANTHROPIC_API_KEY"));
        assert!(left(&found).is_empty(), "{:?}", left(&found));

        home.write(
            ".local/share/opencode/auth.json",
            r#"{"anthropic": {"type": "oauth", "refresh": "r", "access": "a", "expires": 1}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert!(ids(&found).is_empty(), "{:?}", ids(&found));
        assert_eq!(left(&found), [("anthropic", Reason::SignIn)]);
    }

    /// IMPORT-3: opencode's own lists say what the person turned off there.
    #[test]
    fn disabled_providers_are_not_offered() {
        let home = Scratch::new("import-disabled");
        let entries = r#""provider": {
            "one": {"options": {"baseURL": "https://one.invalid/v1"}},
            "two": {"options": {"baseURL": "https://two.invalid/v1"}},
            "three": {"options": {"baseURL": "https://three.invalid/v1"}}
        }"#;
        home.write(
            ".config/opencode/opencode.json",
            &format!(r#"{{"disabled_providers": ["two"], {entries}}}"#),
        );
        assert_eq!(
            ids(&one(&home.path, &[], Source::Opencode)),
            ["one", "three"]
        );

        home.write(
            ".config/opencode/opencode.json",
            &format!(r#"{{"enabled_providers": ["one", "two"], "disabled_providers": ["two"], {entries}}}"#),
        );
        assert_eq!(ids(&one(&home.path, &[], Source::Opencode)), ["one"]);
    }

    /// IMPORT-3: `opencode auth login` keeps a key in `auth.json` and writes no config entry, so a
    /// known id there is a gateway, and an id with no endpoint anywhere is said and left.
    #[test]
    fn an_auth_json_key_for_a_known_id_is_a_provider() {
        let home = Scratch::new("import-auth");
        home.write(
            ".local/share/opencode/auth.json",
            r#"{
                "openrouter": {"type": "api", "key": "sk-or-held"},
                "somewhere-else": {"type": "api", "key": "sk-other"},
                "anthropic": {"type": "oauth", "refresh": "r", "access": "a", "expires": 1},
                "google": {"type": "api", "key": "a-gemini-key"},
                "amazon-bedrock": {"type": "api", "key": "a-bedrock-bearer-token"}
            }"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        let openrouter = gateway(&found, "openrouter");
        assert_eq!(openrouter.endpoint, "https://openrouter.ai/api/v1");
        assert!(openrouter.entry.is_empty(), "{:?}", openrouter.entry);
        match &openrouter.key {
            Key::Held(key) => assert_eq!(key.expose(), "sk-or-held"),
            other => panic!("the key was not held: {other:?}"),
        }
        assert_eq!(openrouter.fallback(), Some("OPENROUTER_API_KEY"));
        assert_eq!(
            left(&found),
            [
                ("amazon-bedrock", Reason::BearerToken),
                ("anthropic", Reason::SignIn),
                ("google", Reason::AnotherSdk),
                ("somewhere-else", Reason::NoEndpoint)
            ]
        );
        assert!(!format!("{found:?}").contains("a-bedrock-bearer-token"));

        // Beside an entry, which is imported, the Bedrock API key is still one bravebot signs
        // without, and is said.
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2"}}}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert!(ids(&found).contains(&"amazon-bedrock"), "{:?}", ids(&found));
        assert!(
            left(&found).contains(&("amazon-bedrock", Reason::BearerToken)),
            "{:?}",
            left(&found)
        );
    }

    /// IMPORT-3: an unknown field copied today could mean something nobody approved once a later
    /// build reads it, and the question shows only what is written.
    #[test]
    fn only_the_fields_bravebot_reads_are_written() {
        let home = Scratch::new("import-fields");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {
                "gw": {
                    "name": "Gateway",
                    "npm": "@ai-sdk/openai-compatible",
                    "api": "https://elsewhere.invalid",
                    "env": ["GW_TOKEN"],
                    "whitelist": ["m"],
                    "options": {"baseURL": "https://gw.invalid/v1", "headers": {"X-Sent": "yes"}, "timeout": 5},
                    "models": {"m": {
                        "name": "Model",
                        "cost": {"input": 1},
                        "limit": {"context": 1000, "output": 100},
                        "options": {"reasoning": {"effort": "high"}}
                    }}
                },
                "amazon-bedrock": {
                    "options": {"region": "us-west-2", "profile": "work", "endpoint": "https://vpc.invalid"},
                    "models": {"arn:m": {"name": "Named", "options": {"x": 1}, "limit": {"context": 5}}}
                }
            }}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(
            Value::Object(gateway(&found, "gw").entry.clone()),
            serde_json::json!({
                "name": "Gateway",
                "env": ["GW_TOKEN"],
                "options": {"baseURL": "https://gw.invalid/v1"},
                "models": {"m": {
                    "limit": {"context": 1000, "output": 100},
                    "options": {"reasoning": {"effort": "high"}}
                }}
            })
        );
        assert_eq!(
            Value::Object(gateway(&found, "amazon-bedrock").entry.clone()),
            serde_json::json!({
                "options": {"region": "us-west-2", "profile": "work"},
                "models": {"arn:m": {"name": "Named"}}
            })
        );
    }

    /// IMPORT-3: a `limit` is written wherever bravebot's own reading takes a figure from it, so the
    /// imported entry budgets and caps each model as the source's entry would.
    #[test]
    fn a_limit_is_written_wherever_bravebot_reads_half_of_it() {
        let home = Scratch::new("import-limit");
        let block = r#"{"provider": {"amazon-bedrock": {
            "options": {"region": "us-west-2"},
            "models": {
                "both": {"limit": {"context": 1000, "output": 100}},
                "window": {"limit": {"context": 2000, "output": 0}},
                "ceiling": {"limit": {"context": 0, "output": 50}},
                "neither": {"limit": {"context": 0, "output": 0}},
                "half": {"limit": {"context": 3000}}
            }
        }}}"#;
        home.write(".config/opencode/opencode.json", block);

        let found = one(&home.path, &[], Source::Opencode);

        let read = |root: &Map<String, Value>| {
            let provider = Provider::all(root).pop().expect("kept");
            let bedrock = provider.bedrock.expect("an AWS entry");
            ["both", "window", "ceiling", "neither", "half"].map(|id| {
                let entry = bedrock.entry(id).expect("offered");
                (entry.context_window, entry.output_limit)
            })
        };
        let source: Map<String, Value> = serde_json::from_str(block).expect("json");
        let Value::Object(written) = serde_json::json!({"provider": {
            "amazon-bedrock": gateway(&found, "amazon-bedrock").entry.clone()
        }}) else {
            unreachable!("an object")
        };
        let stated = read(&source);
        assert_eq!(
            stated[1],
            (Some(2000), None),
            "the fixture states no lone window"
        );
        assert_eq!(
            stated[2],
            (None, Some(50)),
            "the fixture states no lone ceiling"
        );
        assert_eq!(read(&written), stated);
    }

    /// IMPORT-4: a helper is a command, and a settings file here names a destination, never a
    /// command. Nothing it says reaches anything that is written.
    #[test]
    fn an_api_key_helper_is_never_imported() {
        let home = Scratch::new("import-helper");
        home.write(
            ".claude/settings.json",
            r#"{"apiKeyHelper": "/bin/sh -c 'print-a-key'",
                "awsAuthRefresh": "aws sso login",
                "awsCredentialExport": "/usr/local/bin/export-creds",
                "env": {"CLAUDE_CODE_USE_BEDROCK": "1", "AWS_REGION": "us-west-2"}}"#,
        );

        let found = one(&home.path, &[], Source::ClaudeCode);

        let said = format!("{:?}", (&found.env, &found.model, &found.left));
        for command in [
            "print-a-key",
            "aws sso login",
            "export-creds",
            "apiKeyHelper",
        ] {
            assert!(!said.contains(command), "{command} reached {said}");
        }
        assert_eq!(
            env_of(&found),
            [("BRAVEBOT_USE_BEDROCK", "1"), ("AWS_REGION", "us-west-2")]
        );
    }

    /// IMPORT-4: permissions, hooks and servers are grants and commands in both programs, and an
    /// import writes a settings file, which may hold neither.
    #[test]
    fn permissions_hooks_and_mcp_servers_are_never_imported() {
        let home = Scratch::new("import-grants");
        home.write(
            ".claude/settings.json",
            r#"{"permissions": {"allow": ["Bash(rm -rf:*)"]},
                "hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "run-a-hook"}]}]},
                "mcpServers": {"srv": {"command": "start-a-server"}},
                "env": {"CLAUDE_CODE_USE_BEDROCK": "1", "AWS_REGION": "us-west-2"}}"#,
        );
        home.write(
            ".config/opencode/opencode.json",
            r#"{"permission": {"bash": "allow"},
                "mcp": {"srv": {"type": "local", "command": ["start-a-server"]}},
                "agent": {"a": {"prompt": "an-agent-prompt"}},
                "command": {"c": {"template": "a-command-template"}},
                "plugin": ["a-plugin"],
                "provider": {"gw": {"options": {"baseURL": "https://gw.invalid/v1"}}}}"#,
        );

        let found = found_in(&home.path, &[]);

        let said = format!(
            "{:?}",
            found
                .iter()
                .map(|it| (&it.env, &it.model, &it.left, &it.gateways))
                .collect::<Vec<_>>()
        );
        for grant in [
            "rm -rf",
            "run-a-hook",
            "start-a-server",
            "allow",
            "an-agent-prompt",
            "a-command-template",
            "a-plugin",
        ] {
            assert!(!said.contains(grant), "{grant} reached {said}");
        }
        assert_eq!(found.len(), 2, "both sources were read");
    }

    /// IMPORT-4: what Claude Code reaches Anthropic or Vertex with is said by name, never by value.
    #[test]
    fn what_claude_code_uses_and_bravebot_cannot_is_named_and_not_shown() {
        let home = Scratch::new("import-left-claude");
        home.write(
            ".claude/settings.json",
            r#"{"env": {"ANTHROPIC_API_KEY": "sk-ant-secret",
                        "ANTHROPIC_BASE_URL": "https://proxy.invalid",
                        "CLAUDE_CODE_USE_VERTEX": "1",
                        "AWS_BEARER_TOKEN_BEDROCK": "bedrock-secret"}}"#,
        );

        let found = one(&home.path, &[], Source::ClaudeCode);

        assert!(!found.importable());
        assert_eq!(
            left(&found),
            [
                ("ANTHROPIC_API_KEY", Reason::AnthropicApi),
                ("ANTHROPIC_BASE_URL", Reason::AnthropicApi),
                ("CLAUDE_CODE_USE_VERTEX", Reason::Vertex),
                ("AWS_BEARER_TOKEN_BEDROCK", Reason::BearerToken),
            ]
        );
        let said = format!("{found:?}");
        for value in ["sk-ant-secret", "proxy.invalid", "bedrock-secret"] {
            assert!(!said.contains(value), "{value} is in {said}");
        }
    }

    /// IMPORT-3: a key Claude Code reaches Anthropic's API with is an `anthropic` entry at the
    /// endpoint compiled in, holding the key for its own question, and is not also said as left.
    #[test]
    fn a_claude_code_anthropic_key_is_an_anthropic_provider() {
        let home = Scratch::new("import-claude-anthropic-key");
        home.write(
            ".claude/settings.json",
            r#"{"env": {"ANTHROPIC_API_KEY": "sk-ant-secret"}}"#,
        );

        let found = one(&home.path, &[], Source::ClaudeCode);

        assert!(found.importable());
        assert_eq!(ids(&found), ["anthropic"]);
        let anthropic = gateway(&found, "anthropic");
        assert_eq!(anthropic.endpoint, "https://api.anthropic.com/v1");
        match &anthropic.key {
            Key::Held(key) => assert_eq!(key.expose(), "sk-ant-secret"),
            other => panic!("the key was not held: {other:?}"),
        }
        assert_eq!(anthropic.fallback(), Some("ANTHROPIC_API_KEY"));
        assert!(left(&found).is_empty(), "{:?}", left(&found));
        assert!(!anthropic.shown().contains("sk-ant-secret"));
    }

    /// IMPORT-4: a key stated beside a host was issued for that host. Written as an entry it would be
    /// sent to Anthropic's, so the key and the host are both left, and so is a bearer token, which
    /// the header this API reads a key from does not carry.
    #[test]
    fn a_claude_code_anthropic_key_for_another_host_or_a_bearer_token_is_left() {
        let home = Scratch::new("import-claude-anthropic-proxy");
        home.write(
            ".claude/settings.json",
            r#"{"env": {"ANTHROPIC_API_KEY": "sk-ant-secret",
                        "ANTHROPIC_BASE_URL": "https://proxy.invalid"}}"#,
        );
        let found = one(&home.path, &[], Source::ClaudeCode);
        assert!(ids(&found).is_empty(), "{:?}", ids(&found));
        assert_eq!(
            left(&found),
            [
                ("ANTHROPIC_API_KEY", Reason::AnthropicApi),
                ("ANTHROPIC_BASE_URL", Reason::AnthropicApi),
            ]
        );

        home.write(
            ".claude/settings.json",
            r#"{"env": {"ANTHROPIC_AUTH_TOKEN": "a-bearer-token"}}"#,
        );
        let found = one(&home.path, &[], Source::ClaudeCode);
        assert!(ids(&found).is_empty(), "{:?}", ids(&found));
        assert_eq!(
            left(&found),
            [("ANTHROPIC_AUTH_TOKEN", Reason::AnthropicApi)]
        );
    }

    /// IMPORT-6: `{env:VAR}` is opencode's substitution, and bravebot makes none, so copied as it is
    /// the reference would be sent as the token.
    #[test]
    fn an_env_reference_becomes_a_variable_name_not_a_token() {
        let home = Scratch::new("import-env-ref");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"gw": {"env": ["FIRST"], "options": {"baseURL": "https://gw.invalid/v1", "apiKey": "{env:GW_TOKEN}"}}}}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        let gw = gateway(&found, "gw");
        assert!(matches!(gw.key, Key::Named), "{:?}", gw.key);
        assert_eq!(
            gw.entry.get("env"),
            Some(&serde_json::json!(["FIRST", "GW_TOKEN"]))
        );
        assert!(
            !format!("{:?}", gw.entry).contains("apiKey"),
            "{:?}",
            gw.entry
        );
    }

    /// IMPORT-6: a key in plain text is held for its own question, and never becomes part of the
    /// entry the source's question shows.
    #[test]
    fn a_literal_key_is_held_apart_from_the_entry() {
        let home = Scratch::new("import-literal");
        home.write(
            ".config/opencode/opencode.jsonc",
            r#"{
                // a comment, and a trailing comma below
                "provider": {"gw": {"options": {"baseURL": "https://gw.invalid/v1", "apiKey": "sk-literal"},},},
            }"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        let gw = gateway(&found, "gw");
        match &gw.key {
            Key::Held(key) => assert_eq!(key.expose(), "sk-literal"),
            other => panic!("the key was not held: {other:?}"),
        }
        assert!(!format!("{:?}", gw.entry).contains("sk-literal"));
        assert_eq!(
            gw.entry.get("env"),
            None,
            "a variable was named for an unknown id"
        );
    }

    /// IMPORT-6: a file reference is not followed. Following it would read a file nobody was asked
    /// about, wherever it names.
    #[test]
    fn a_file_reference_is_not_followed() {
        let home = Scratch::new("import-file-ref");
        let secret = home.write("elsewhere/key.txt", "sk-in-a-file");
        home.write(
            ".config/opencode/opencode.json",
            &format!(
                r#"{{"provider": {{"openrouter": {{"options": {{"apiKey": "{{file:{}}}"}}}}}}}}"#,
                secret.display().to_string().replace('\\', "\\\\")
            ),
        );

        let found = one(&home.path, &[], Source::Opencode);

        let openrouter = gateway(&found, "openrouter");
        match &openrouter.key {
            Key::File(path) => assert_eq!(Path::new(path), secret),
            other => panic!("the reference was not kept as one: {other:?}"),
        }
        assert!(!format!("{found:?}").contains("sk-in-a-file"));
    }

    /// IMPORT-3: opencode's top-level model is BACKEND-18's form already, and an AWS entry answers
    /// only for a model it lists.
    #[test]
    fn a_top_level_model_is_copied_where_its_provider_is() {
        let home = Scratch::new("import-model");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"model": "openrouter/z-ai/glm-4.6", "provider": {"openrouter": {}}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert_eq!(found.model.as_deref(), Some("openrouter/z-ai/glm-4.6"));
        assert_eq!(found.model_gateway.as_deref(), Some("openrouter"));

        home.write(
            ".config/opencode/opencode.json",
            r#"{"model": "amazon-bedrock/openai.gpt-5.6-sol", "provider": {"amazon-bedrock": {"options": {"region": "us-west-2"}}}}"#,
        );
        let found = one(&home.path, &[], Source::Opencode);
        assert_eq!(found.model.as_deref(), Some("openai.gpt-5.6-sol"));
        assert_eq!(found.model_gateway.as_deref(), Some("amazon-bedrock"));
        assert_eq!(
            gateway(&found, "amazon-bedrock").entry.get("models"),
            Some(&serde_json::json!({"openai.gpt-5.6-sol": {}}))
        );

        home.write(
            ".config/opencode/opencode.json",
            r#"{"model": "anthropic/claude-sonnet-4-5", "provider": {"openrouter": {}}}"#,
        );
        assert_eq!(one(&home.path, &[], Source::Opencode).model, None);
    }

    /// IMPORT-7: a name already set keeps its value, and a file that does not parse is refused
    /// rather than replaced.
    #[test]
    fn a_destination_adds_names_and_replaces_none() {
        let home = Scratch::new("import-destination");
        let path = home.write(
            "settings.json",
            r#"{"env": {"AWS_REGION": "eu-west-1"}, "theme": "dark"}"#,
        );
        let mut destination = Destination::open(&path).expect("a document");
        destination.add_env("AWS_REGION", "us-west-2");
        destination.add_env("AWS_PROFILE", "work");
        destination.add_model("opus");
        let text = destination.text().expect("small");
        let written: Value = serde_json::from_str(text.expose()).expect("json");
        assert_eq!(
            written,
            serde_json::json!({
                "env": {"AWS_REGION": "eu-west-1", "AWS_PROFILE": "work"},
                "theme": "dark",
                "model": "opus"
            })
        );

        // A value the settings reader ignores is not set, so the import's name goes in its place.
        home.write(
            "settings.json",
            r#"{"env": {"AWS_REGION": 5, "AWS_PROFILE": "kept"}, "model": " "}"#,
        );
        let mut destination = Destination::open(&path).expect("a document");
        assert!(!destination.holds_env("AWS_REGION"));
        assert!(destination.holds_env("AWS_PROFILE"));
        assert!(destination.names_env("AWS_REGION"));
        assert!(!destination.names_env("AWS_DEFAULT_REGION"));
        assert!(!destination.holds_model());
        destination.add_env("AWS_REGION", "us-west-2");
        destination.add_model("opus");
        let text = destination.text().expect("small");
        let written: Value = serde_json::from_str(text.expose()).expect("json");
        assert_eq!(
            written,
            serde_json::json!({
                "env": {"AWS_REGION": "us-west-2", "AWS_PROFILE": "kept"},
                "model": "opus"
            })
        );

        // A file changed after it was read is not the document the import would write over.
        assert!(!destination.changed());
        home.write("settings.json", r#"{"theme": "light"}"#);
        assert!(destination.changed());
        let absent = Destination::open(&home.path.join("absent.json")).expect("empty");
        assert!(!absent.changed());
        home.write("absent.json", "{}");
        assert!(absent.changed());

        home.write("settings.json", r#"{"env": "AWS_REGION"}"#);
        let destination = Destination::open(&path).expect("a document");
        assert!(
            destination.names_env("AWS_PROFILE"),
            "an env that is not a block"
        );

        home.write("settings.json", r#"{"env": {"#);
        assert_eq!(
            Destination::open(&path).err(),
            Some(Unwritable::NotADocument)
        );
        // The settings reader drops a file past its limit whole, so an import would add names to a
        // file whose every name is already unread.
        home.write(
            "settings.json",
            &format!(
                r#"{{"theme": "{}"}}"#,
                "x".repeat(crate::settings::MAX_BYTES as usize)
            ),
        );
        assert_eq!(Destination::open(&path).err(), Some(Unwritable::TooLarge));
        home.write("settings.json", "  \n");
        assert!(Destination::open(&path).is_ok(), "an empty file is refused");
    }

    /// SERVERS-2: a request goes in once, beside what the file already requests, and comes out
    /// leaving the rest; a `mcp` or `mcp.request` the settings reader would not take as a list is
    /// not replaced by one.
    #[test]
    fn a_destination_requests_a_server_once_and_withdraws_only_it() {
        let home = Scratch::new("import-destination-request");
        let path = home.write(
            "settings.json",
            r#"{"mcp": {"request": ["docs"], "other": 1}, "theme": "dark"}"#,
        );
        let mut destination = Destination::open(&path).expect("a document");
        assert!(destination.requests("docs"));
        assert!(!destination.requests("weather"));
        assert!(destination.request("weather"));
        assert!(destination.request("weather"));
        assert!(destination.requests("weather"));
        let text = destination.text().expect("small");
        let written: Value = serde_json::from_str(text.expose()).expect("json");
        assert_eq!(
            written,
            serde_json::json!({
                "mcp": {"request": ["docs", "weather"], "other": 1},
                "theme": "dark"
            })
        );
        assert!(destination.withdraw("docs"));
        assert!(!destination.withdraw("docs"));
        let text = destination.text().expect("small");
        let written: Value = serde_json::from_str(text.expose()).expect("json");
        assert_eq!(
            written,
            serde_json::json!({"mcp": {"request": ["weather"], "other": 1}, "theme": "dark"})
        );

        let mut empty = Destination::open(&home.path.join("absent.json")).expect("empty");
        assert!(!empty.withdraw("weather"));
        assert!(empty.request("weather"));
        let text = empty.text().expect("small");
        let written: Value = serde_json::from_str(text.expose()).expect("json");
        assert_eq!(
            written,
            serde_json::json!({"mcp": {"request": ["weather"]}})
        );

        home.write("settings.json", r#"{"mcp": {"request": null, "other": 1}}"#);
        let mut destination = Destination::open(&path).expect("a document");
        assert!(destination.request("weather"));
        let written: Value =
            serde_json::from_str(destination.text().expect("small").expose()).expect("json");
        assert_eq!(
            written,
            serde_json::json!({"mcp": {"request": ["weather"], "other": 1}})
        );

        // A null block is one `doctor` reports, unlike a null list, so it is left for the person.
        for text in [
            r#"{"mcp": {"request": "weather"}}"#,
            r#"{"mcp": ["weather"]}"#,
            r#"{"mcp": null}"#,
        ] {
            home.write("settings.json", text);
            let mut destination = Destination::open(&path).expect("a document");
            assert!(!destination.requests("weather"), "{text}");
            assert!(!destination.request("docs"), "{text}");
            assert!(!destination.withdraw("weather"), "{text}");
            let written: Value =
                serde_json::from_str(destination.text().expect("small").expose()).expect("json");
            let read: Value = serde_json::from_str(text).expect("json");
            assert_eq!(written, read, "{text}");
        }
    }

    /// IMPORT-6: a substitution inside a longer value is one this program does not make, so the text
    /// as written is not the key, and holding it would send and write that text as one.
    #[test]
    fn a_key_built_from_a_substitution_is_left() {
        let home = Scratch::new("import-substitution");
        home.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {
                "prefixed": {"options": {"baseURL": "https://a.invalid/v1", "apiKey": "Bearer {env:GW_TOKEN}"}},
                "empty": {"options": {"baseURL": "https://b.invalid/v1", "apiKey": "{env:}"}},
                "two": {"options": {"baseURL": "https://c.invalid/v1", "apiKey": "{env:A}{env:B}"}},
                "file": {"options": {"baseURL": "https://d.invalid/v1", "apiKey": "prefix-{file:/x}"}},
                "whole": {"options": {"baseURL": "https://e.invalid/v1", "apiKey": "{env:GW_TOKEN}"}}
            }}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["whole"]);
        assert_eq!(
            left(&found),
            [
                ("empty", Reason::Substitution),
                ("file", Reason::Substitution),
                ("prefixed", Reason::Substitution),
                ("two", Reason::Substitution)
            ]
        );
        assert!(!format!("{found:?}").contains("Bearer"), "{found:?}");
    }

    fn installed(name: &str, modified_at: &str, capabilities: Option<&[&str]>) -> Installed {
        Installed {
            name: name.to_string(),
            modified_at: modified_at.to_string(),
            capabilities: capabilities.map(|can| can.iter().map(|it| it.to_string()).collect()),
            window: None,
        }
    }

    /// The listing an Ollama with three models pulled answers, as `/api/tags` reports it.
    fn three_pulled() -> Vec<Installed> {
        vec![
            installed(
                "qwen3-coder-oc:latest",
                "2026-09-16T14:08:37.311476-04:00",
                Some(&["completion", "tools"]),
            ),
            installed(
                "qwen3-coder:30b",
                "2026-09-16T14:07:58.84902-04:00",
                Some(&["completion", "tools"]),
            ),
            installed(
                "llama3:latest",
                "2024-06-05T09:46:42.558935-04:00",
                Some(&["completion"]),
            ),
        ]
    }

    /// What a start finds with `set` exported and `answer` as each server's reply, and every
    /// address each server was asked at, in no order, since the servers are asked together.
    fn with_servers(
        set: &[(&str, &str)],
        answer: impl Fn(Source, &str) -> Option<Vec<Installed>> + Sync,
    ) -> (Vec<Found>, Vec<(Source, String)>) {
        // Never made, so no file is found and the servers are the only sources.
        let home = crate::testutil::scratch_dir("import-servers-no-home");
        let lookup = environment(&home, set);
        let asked = std::sync::Mutex::new(Vec::new());
        let found = found(&Places::from_lookup(&lookup), &lookup, |source, host| {
            asked
                .lock()
                .expect("asked")
                .push((source, host.to_string()));
            answer(source, host)
        });
        (found, asked.into_inner().expect("asked"))
    }

    /// What a start finds with `set` exported and `listed` as Ollama's answer, and every base URL
    /// Ollama was asked at. Nothing else answers.
    fn with_ollama(
        set: &[(&str, &str)],
        listed: Option<Vec<Installed>>,
    ) -> (Vec<Found>, Vec<String>) {
        let (found, asked) = with_servers(set, |source, _| match source {
            Source::Ollama => listed.clone(),
            _ => None,
        });
        let asked = asked
            .into_iter()
            .filter(|(source, _)| *source == Source::Ollama)
            .map(|(_, host)| host)
            .collect();
        (found, asked)
    }

    /// IMPORT-10: the window shown is the one of the model that is written, not of another model
    /// the server happens to have loaded.
    #[test]
    fn the_window_is_the_one_of_the_model_chosen() {
        let mut listed = three_pulled();
        let newest = listed
            .iter()
            .position(|model| model.name == "qwen3-coder-oc:latest")
            .expect("the newest");
        listed[newest].window = Some(4096);
        for (index, model) in listed.iter_mut().enumerate() {
            if index != newest {
                model.window = Some(777);
            }
        }

        let found = running(Source::Ollama, "http://localhost:11434", listed);

        assert_eq!(found.model.as_deref(), Some("ollama/qwen3-coder-oc:latest"));
        assert_eq!(found.window, Some(4096));
    }

    /// IMPORT-3: a window below the assumed one is written as the entry's `contextWindow`, because
    /// the entry lists no models to carry it. A window at or above the assumed one, and none at all,
    /// write the documented block alone, so a figure that later grows is not held down. The
    /// regressions rejected are writing nothing, and writing every figure.
    #[test]
    fn a_window_below_the_assumed_one_is_written_into_the_entry() {
        let options = |window: Option<u64>| {
            let mut listed = three_pulled();
            listed[0].window = window;
            let found = running(Source::Ollama, "http://localhost:11434", listed);
            let chosen = found.model.clone();
            assert_eq!(chosen.as_deref(), Some("ollama/qwen3-coder-oc:latest"));
            found.gateways[0].entry["options"].clone()
        };

        assert_eq!(
            options(Some(4096)),
            serde_json::json!({"baseURL": "http://localhost:11434/v1", "contextWindow": 4096})
        );
        let plain = serde_json::json!({"baseURL": "http://localhost:11434/v1"});
        assert_eq!(options(Some(crate::provider::CONTEXT_WINDOW)), plain);
        assert_eq!(options(Some(1_000_000)), plain);
        assert_eq!(options(None), plain);
    }

    /// IMPORT-10: nothing configured and Ollama running is the case the configuration guide
    /// answers with a block to paste, so the import writes that block rather than one of its own:
    /// the same id, the same name, the same `baseURL`, and no key, which Ollama does not want.
    #[test]
    fn a_running_ollama_is_offered_as_the_documented_block() {
        let (found, asked) = with_ollama(&[], Some(three_pulled()));

        assert_eq!(asked, ["http://localhost:11434"]);
        let [found] = found.as_slice() else {
            panic!("not one source: {found:?}");
        };
        assert_eq!(found.source, Source::Ollama);
        assert!(found.read.is_empty(), "{:?}", found.read);
        assert_eq!(found.running_at.as_deref(), Some("http://localhost:11434"));
        assert!(found.env.is_empty() && found.left.is_empty(), "{found:?}");
        // The block under "A local Ollama, or another gateway that wants no key" in
        // docs/website/docs/customize/configuration.md.
        let documented: Value = serde_json::from_str(
            r#"{"name": "Ollama (local)", "options": {"baseURL": "http://localhost:11434/v1"}}"#,
        )
        .expect("json");
        let gateway = gateway(found, "ollama");
        assert_eq!(Value::Object(gateway.entry.clone()), documented);
        assert_eq!(gateway.endpoint, "http://localhost:11434/v1");
        assert!(matches!(gateway.key, Key::Named), "{:?}", gateway.key);
        assert_eq!(found.model.as_deref(), Some("ollama/qwen3-coder-oc:latest"));
        assert_eq!(found.model_gateway.as_deref(), Some("ollama"));
    }

    /// IMPORT-10: a session's tools reach only a model that can call them, and each listed time
    /// carries its own offset, so the newest is found by comparing times. Compared as text, the
    /// model pulled at 20:00 in Paris would beat the one pulled eight minutes later in New York.
    #[test]
    fn the_default_model_is_the_newest_that_can_call_tools() {
        let listed = vec![
            installed(
                "unreadable-time",
                "yesterday",
                Some(&["completion", "tools"]),
            ),
            installed(
                "new-york",
                "2026-09-16T14:08:37-04:00",
                Some(&["completion", "tools"]),
            ),
            installed(
                "paris",
                "2026-09-16T20:00:00.5+02:00",
                Some(&["tools", "completion"]),
            ),
            installed(
                "newest-without-tools",
                "2026-09-17T00:00:00Z",
                Some(&["completion"]),
            ),
            installed("", "2026-09-18T00:00:00Z", Some(&["tools"])),
        ];

        let (found, _) = with_ollama(&[], Some(listed));

        let found = found.first().expect("a source");
        assert_eq!(found.model.as_deref(), Some("ollama/new-york"), "{found:?}");
    }

    /// IMPORT-10: an Ollama older than the capabilities field reports none for any model, and
    /// reading that as "none can call tools" would offer such a person nothing.
    #[test]
    fn an_ollama_reporting_no_capabilities_defaults_to_its_newest_model() {
        let listed = vec![
            installed("older", "2026-01-01T00:00:00Z", None),
            installed("newer", "2026-02-01T00:00:00Z", None),
        ];

        let (found, _) = with_ollama(&[], Some(listed));

        let found = found.first().expect("a source");
        assert_eq!(found.model.as_deref(), Some("ollama/newer"), "{found:?}");
        assert_eq!(ids(found), ["ollama"]);
    }

    /// IMPORT-4: a gateway whose every model fails the session's first tool call is not a working
    /// configuration, so it is not offered, and the start says why rather than saying nothing.
    #[test]
    fn an_ollama_with_no_model_that_can_call_tools_is_left_and_said() {
        let without_tools = vec![
            installed(
                "llama3:latest",
                "2024-06-05T09:46:42Z",
                Some(&["completion"]),
            ),
            installed(
                "nomic-embed-text",
                "2024-06-06T09:46:42Z",
                Some(&["embedding"]),
            ),
        ];
        for listed in [without_tools, Vec::new()] {
            let (found, _) = with_ollama(&[], Some(listed.clone()));

            let [found] = found.as_slice() else {
                panic!("not one source for {listed:?}: {found:?}");
            };
            assert!(!found.importable(), "{found:?}");
            assert_eq!(
                left(found),
                [("http://localhost:11434", Reason::NoToolModel)],
                "{listed:?}"
            );
            assert_eq!(found.running_at.as_deref(), Some("http://localhost:11434"));
        }
    }

    /// IMPORT-10: Ollama's own client reads `OLLAMA_HOST`, so a person who moved the server told
    /// both programs in one place. Each value is read as Ollama reads it, and the written
    /// `baseURL` is the address that answered.
    #[test]
    fn ollama_host_moves_where_ollama_is_asked() {
        for (value, host) in [
            ("", "http://localhost:11434"),
            ("  ", "http://localhost:11434"),
            ("127.0.0.1", "http://127.0.0.1:11434"),
            ("127.0.0.1:8080", "http://127.0.0.1:8080"),
            ("127.0.0.2", "http://127.0.0.2:11434"),
            (":1234", "http://127.0.0.1:1234"),
            ("0.0.0.0", "http://127.0.0.1:11434"),
            ("0.0.0.0:9000", "http://127.0.0.1:9000"),
            ("::1", "http://[::1]:11434"),
            ("[::1]", "http://[::1]:11434"),
            ("[::1]:9000", "http://[::1]:9000"),
            ("::", "http://[::1]:11434"),
            ("[::ffff:127.0.0.1]:7", "http://127.0.0.1:7"),
            ("http://localhost", "http://localhost:80"),
            ("https://localhost", "https://localhost:443"),
            ("http://127.0.0.1:11434/", "http://127.0.0.1:11434"),
            ("localhost:11434/ollama/", "http://localhost:11434/ollama"),
            (" 'LOCALHOST:7' ", "http://localhost:7"),
            ("\"localhost\"", "http://localhost:11434"),
            ("localhost:99999", "http://localhost:11434"),
        ] {
            let (found, asked) = with_ollama(&[(OLLAMA_HOST, value)], Some(three_pulled()));

            assert_eq!(asked, [host], "{value:?}");
            let found = found.first().expect("a source");
            assert_eq!(found.running_at.as_deref(), Some(host), "{value:?}");
            assert_eq!(
                gateway(found, "ollama").endpoint,
                format!("{host}/v1"),
                "{value:?}"
            );
        }

        let (_, asked) = with_ollama(&[], None);
        assert_eq!(asked, ["http://localhost:11434"], "unset");
    }

    /// IMPORT-10: a start asks before anyone has agreed to anything, so it reaches no machine but
    /// this one: another host would learn that this program started, and could choose the gateway
    /// the person is offered. What `OLLAMA_HOST` names is said instead.
    #[test]
    fn an_ollama_host_off_this_machine_is_left_and_not_asked() {
        for value in [
            "192.168.1.5",
            "10.0.0.1:11434",
            "example.com",
            "ollama.com",
            "https://example.com/ollama",
            "localhost.example.com",
            "[2001:db8::1]:11434",
            "::ffff:8.8.8.8",
            "http://[fe80::1]:11434",
        ] {
            let (found, asked) = with_ollama(&[(OLLAMA_HOST, value)], Some(three_pulled()));

            assert!(asked.is_empty(), "{value:?} was asked at {asked:?}");
            let [found] = found.as_slice() else {
                panic!("not one source for {value:?}: {found:?}");
            };
            assert!(!found.importable(), "{value:?}: {found:?}");
            assert_eq!(left(found), [(OLLAMA_HOST, Reason::Elsewhere)], "{value:?}");
        }
    }

    /// IMPORT-10: an Ollama that is not running is the ordinary case, and a start that said so
    /// every time would be noise about a program the person may never have installed.
    #[test]
    fn an_ollama_that_does_not_answer_is_no_source() {
        let (found, asked) = with_ollama(&[], None);

        assert_eq!(asked.len(), 1);
        assert!(found.is_empty(), "{found:?}");
    }

    /// An LM Studio with three language models and an embedding model, as `/api/v1/models` reports
    /// them: the first is trained for tool use, the second is loaded, and the third is neither.
    fn lm_studio_listing() -> Vec<Installed> {
        let model = |name: &str, tools: bool, window: Option<u64>| Installed {
            name: name.to_string(),
            modified_at: String::new(),
            capabilities: Some(match tools {
                true => vec!["completion".to_string(), "tools".to_string()],
                false => vec!["completion".to_string()],
            }),
            window,
        };
        vec![
            model("qwen/qwen3-coder-30b", true, None),
            model("google/gemma-4-26b-a4b", true, Some(4096)),
            model("google/gemma-3-4b", false, Some(8192)),
        ]
    }

    /// What a start finds where only `source` answers, with `listed`.
    fn with_only(
        source: Source,
        set: &[(&str, &str)],
        listed: Vec<Installed>,
    ) -> (Vec<Found>, Vec<(Source, String)>) {
        with_servers(set, |asked, _| (asked == source).then(|| listed.clone()))
    }

    /// IMPORT-12: nothing configured and LM Studio running is offered as the block a person would
    /// write by hand for it: a gateway needing no key, at the address that answered, named for the
    /// program, with a model LM Studio serves.
    #[test]
    fn a_running_lm_studio_is_offered_as_a_gateway_needing_no_key() {
        let (found, asked) = with_only(Source::LmStudio, &[], lm_studio_listing());

        assert_eq!(
            asked
                .iter()
                .filter(|(source, _)| *source == Source::LmStudio)
                .collect::<Vec<_>>(),
            [&(Source::LmStudio, "http://127.0.0.1:1234".to_string())]
        );
        let [found] = found.as_slice() else {
            panic!("not one source: {found:?}");
        };
        assert_eq!(found.source, Source::LmStudio);
        assert!(found.read.is_empty() && found.env.is_empty() && found.left.is_empty());
        assert_eq!(found.running_at.as_deref(), Some("http://127.0.0.1:1234"));
        let documented: Value = serde_json::from_str(
            r#"{"name": "LM Studio (local)", "options": {"baseURL": "http://127.0.0.1:1234/v1", "contextWindow": 4096}}"#,
        )
        .expect("json");
        let gateway = gateway(found, "lmstudio");
        assert_eq!(Value::Object(gateway.entry.clone()), documented);
        assert_eq!(gateway.endpoint, "http://127.0.0.1:1234/v1");
        assert!(matches!(gateway.key, Key::Named), "{:?}", gateway.key);
        assert_eq!(
            found.model.as_deref(),
            Some("lmstudio/google/gemma-4-26b-a4b")
        );
        assert_eq!(found.model_gateway.as_deref(), Some("lmstudio"));
    }

    /// IMPORT-12: LM Studio states no time, so no model is the newest. The one written is a model it
    /// has loaded and can call tools with, since a request for one that is not loaded waits on LM
    /// Studio loading it, and otherwise the first it lists that can. A loaded model without tools,
    /// a model that is not first only because it was listed last, and a blank name are not it.
    #[test]
    fn the_model_chosen_from_lm_studio_is_one_it_has_loaded_that_can_call_tools() {
        let chosen = |listed: Vec<Installed>| {
            let (found, _) = with_only(Source::LmStudio, &[], listed);
            found.first().expect("a source").model.clone()
        };

        assert_eq!(
            chosen(lm_studio_listing()).as_deref(),
            Some("lmstudio/google/gemma-4-26b-a4b")
        );
        let mut none_loaded = lm_studio_listing();
        none_loaded.iter_mut().for_each(|model| model.window = None);
        assert_eq!(
            chosen(none_loaded).as_deref(),
            Some("lmstudio/qwen/qwen3-coder-30b")
        );
        let mut blank_first = lm_studio_listing();
        blank_first[0].name = "  ".to_string();
        blank_first[1].window = None;
        assert_eq!(
            chosen(blank_first).as_deref(),
            Some("lmstudio/google/gemma-4-26b-a4b")
        );
    }

    /// IMPORT-12: a session's tools reach only a model that can call them, so an LM Studio that
    /// reports none trained for tool use, or holds no language model at all, is left and said, as
    /// an Ollama with none is.
    #[test]
    fn an_lm_studio_with_no_model_trained_for_tool_use_is_left_and_said() {
        let mut without_tools = lm_studio_listing();
        without_tools.retain(|model| model.capabilities.as_ref().unwrap().len() == 1);
        for listed in [without_tools, Vec::new()] {
            let (found, _) = with_only(Source::LmStudio, &[], listed.clone());

            let [found] = found.as_slice() else {
                panic!("not one source for {listed:?}: {found:?}");
            };
            assert!(!found.importable(), "{found:?}");
            assert_eq!(
                left(found),
                [("http://127.0.0.1:1234", Reason::NoToolModel)],
                "{listed:?}"
            );
        }
    }

    /// IMPORT-5 and IMPORT-12: the window LM Studio has a model loaded with is written where it is
    /// below the assumed one, as it is for Ollama, and not where it is at or above it.
    #[test]
    fn an_lm_studios_window_below_the_assumed_one_is_written_into_the_entry() {
        let options = |window: Option<u64>| {
            let mut listed = lm_studio_listing();
            listed[1].window = window;
            let (found, _) = with_only(Source::LmStudio, &[], listed);
            let found = found.into_iter().next().expect("a source");
            assert_eq!(found.window, window);
            found.gateways[0].entry["options"].clone()
        };

        assert_eq!(
            options(Some(4096)),
            serde_json::json!({"baseURL": "http://127.0.0.1:1234/v1", "contextWindow": 4096})
        );
        let plain = serde_json::json!({"baseURL": "http://127.0.0.1:1234/v1"});
        assert_eq!(options(Some(crate::provider::CONTEXT_WINDOW)), plain);
        assert_eq!(options(None), plain);
    }

    fn llama_server_listing() -> Vec<Installed> {
        vec![installed(
            "../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
            "",
            None,
        )]
    }

    /// IMPORT-12: a running llama-server is offered as a gateway needing no key, with the model it
    /// lists. It reports no capabilities and no loaded window, so none is claimed for it.
    #[test]
    fn a_running_llama_server_is_offered_as_a_gateway_needing_no_key() {
        let (found, _) = with_only(
            Source::LlamaServer,
            &[(LLAMA_ARG_PORT, "9931")],
            llama_server_listing(),
        );

        let [found] = found.as_slice() else {
            panic!("not one source: {found:?}");
        };
        assert_eq!(found.source, Source::LlamaServer);
        assert_eq!(found.running_at.as_deref(), Some("http://127.0.0.1:9931"));
        let documented: Value = serde_json::from_str(
            r#"{"name": "llama-server (local)", "options": {"baseURL": "http://127.0.0.1:9931/v1"}}"#,
        )
        .expect("json");
        let gateway = gateway(found, "llama-server");
        assert_eq!(Value::Object(gateway.entry.clone()), documented);
        assert!(matches!(gateway.key, Key::Named), "{:?}", gateway.key);
        assert_eq!(
            found.model.as_deref(),
            Some("llama-server/../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf")
        );
        assert_eq!(found.window, None);
    }

    /// IMPORT-2 and IMPORT-12: LM Studio reads no variable for its address, so the one that moves
    /// where it is asked is bravebot's own, read as `OLLAMA_HOST` is, and the default is the address
    /// LM Studio's server starts on.
    #[test]
    fn the_lm_studio_host_moves_where_lm_studio_is_asked() {
        for (value, host) in [
            ("", "http://127.0.0.1:1234"),
            ("localhost", "http://localhost:1234"),
            ("localhost:4321", "http://localhost:4321"),
            ("127.0.0.1:4321/lm", "http://127.0.0.1:4321/lm"),
            ("0.0.0.0:4321", "http://127.0.0.1:4321"),
            ("[::1]", "http://[::1]:1234"),
            ("http://localhost", "http://localhost:80"),
            ("localhost:99999", "http://localhost:1234"),
        ] {
            let (found, asked) = with_only(
                Source::LmStudio,
                &[(LM_STUDIO_HOST, value)],
                lm_studio_listing(),
            );

            assert!(
                asked.contains(&(Source::LmStudio, host.to_string())),
                "{value:?} was asked at {asked:?}"
            );
            let found = found.first().expect("a source");
            assert_eq!(found.running_at.as_deref(), Some(host), "{value:?}");
            assert_eq!(
                gateway(found, "lmstudio").endpoint,
                format!("{host}/v1"),
                "{value:?}"
            );
        }
    }

    /// IMPORT-2 and IMPORT-12: llama-server is asked at the port it was told to listen on, and
    /// where it was told none, at both ports it has defaulted to. Its host is the address it binds,
    /// which for `0.0.0.0` and `::` is this machine to a client.
    #[test]
    fn llama_args_move_where_llama_server_is_asked() {
        for (set, hosts) in [
            (
                vec![],
                vec!["http://127.0.0.1:9931", "http://127.0.0.1:8080"],
            ),
            (
                vec![(LLAMA_ARG_PORT, "7777")],
                vec!["http://127.0.0.1:7777"],
            ),
            (
                vec![(LLAMA_ARG_PORT, " 7777 ")],
                vec!["http://127.0.0.1:7777"],
            ),
            (
                vec![(LLAMA_ARG_PORT, "not-a-port")],
                vec!["http://127.0.0.1:9931", "http://127.0.0.1:8080"],
            ),
            (
                vec![(LLAMA_ARG_PORT, "0")],
                vec!["http://127.0.0.1:9931", "http://127.0.0.1:8080"],
            ),
            (
                vec![(LLAMA_ARG_PORT, "70000")],
                vec!["http://127.0.0.1:9931", "http://127.0.0.1:8080"],
            ),
            (
                vec![(LLAMA_ARG_HOST, "0.0.0.0"), (LLAMA_ARG_PORT, "7777")],
                vec!["http://127.0.0.1:7777"],
            ),
            (
                vec![(LLAMA_ARG_HOST, "::"), (LLAMA_ARG_PORT, "7777")],
                vec!["http://[::1]:7777"],
            ),
            (
                vec![(LLAMA_ARG_HOST, "127.0.0.2"), (LLAMA_ARG_PORT, "7777")],
                vec!["http://127.0.0.2:7777"],
            ),
            (
                vec![(LLAMA_ARG_HOST, "localhost")],
                vec!["http://localhost:9931", "http://localhost:8080"],
            ),
        ] {
            let (_, asked) = with_only(Source::LlamaServer, &set, llama_server_listing());

            let mut asked: Vec<_> = asked
                .into_iter()
                .filter(|(source, _)| *source == Source::LlamaServer)
                .map(|(_, host)| host)
                .collect();
            asked.sort();
            let mut hosts = hosts;
            hosts.sort();
            assert_eq!(asked, hosts, "{set:?}");
        }
    }

    /// IMPORT-12: llama-server has listened on 8080 in every release before October 2026 and in
    /// its container image, and on 9931 since. One that answers at either is found, the current
    /// default is preferred where both answer, and only one entry is offered either way.
    #[test]
    fn llama_server_is_found_at_either_port_it_has_defaulted_to() {
        let answering = |ports: &'static [&'static str]| {
            let (found, _) = with_servers(&[], move |source, host| {
                (source == Source::LlamaServer && ports.iter().any(|port| host.ends_with(port)))
                    .then(llama_server_listing)
            });
            found
                .into_iter()
                .map(|found| found.running_at.unwrap_or_default())
                .collect::<Vec<_>>()
        };

        assert_eq!(answering(&[":9931"]), ["http://127.0.0.1:9931"]);
        assert_eq!(answering(&[":8080"]), ["http://127.0.0.1:8080"]);
        assert_eq!(answering(&[":9931", ":8080"]), ["http://127.0.0.1:9931"]);
        assert!(answering(&[]).is_empty());
    }

    /// IMPORT-2 and IMPORT-12: a variable naming another machine is said and nothing is asked of
    /// it, for each server by the variable that named it, and the other servers are asked as before.
    /// For llama-server that is the address it binds when it is not this machine's loopback.
    #[test]
    fn a_server_address_off_this_machine_is_left_and_not_asked() {
        for (variable, source, value) in [
            (LM_STUDIO_HOST, Source::LmStudio, "192.168.1.5"),
            (LM_STUDIO_HOST, Source::LmStudio, "https://example.com/lm"),
            (LLAMA_ARG_HOST, Source::LlamaServer, "192.168.1.5"),
            (LLAMA_ARG_HOST, Source::LlamaServer, "example.com"),
            (LLAMA_ARG_HOST, Source::LlamaServer, "127.0.0.1,192.168.1.5"),
            (LLAMA_ARG_HOST, Source::LlamaServer, "/tmp/llama.sock"),
        ] {
            let (found, asked) =
                with_servers(&[(variable, value)], |_, _| Some(lm_studio_listing()));

            assert!(
                asked.iter().all(|(asked, _)| *asked != source),
                "{value:?} was asked at {asked:?}"
            );
            let this = found
                .iter()
                .find(|found| found.source == source)
                .unwrap_or_else(|| panic!("nothing said of {value:?}: {found:?}"));
            assert!(!this.importable(), "{value:?}: {this:?}");
            assert_eq!(left(this), [(variable, Reason::Elsewhere)], "{value:?}");
            let others: Vec<_> = found.iter().map(|found| found.source).collect();
            assert_eq!(others.len(), 3, "{value:?}: {others:?}");
        }
    }

    /// IMPORT-12: every server is asked at once, so a start where several hold the connection and
    /// never answer waits for one bound, not for the sum of them. Each question waits for the others
    /// to have been put, which a start that asked one after another never lets it see, and stops
    /// waiting at a deadline so that such a start fails here and does not hang.
    #[test]
    fn every_server_is_asked_at_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};
        let asking = AtomicUsize::new(0);
        let seen_together = std::sync::Mutex::new(Vec::new());

        let (_, asked) = with_servers(&[], |source, host| {
            asking.fetch_add(1, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(5);
            while asking.load(Ordering::SeqCst) < 4 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            seen_together.lock().expect("seen").push((
                source,
                host.to_string(),
                asking.load(Ordering::SeqCst),
            ));
            None
        });

        assert_eq!(asked.len(), 4, "{asked:?}");
        let seen = seen_together.into_inner().expect("seen");
        assert!(
            seen.iter().all(|(_, _, together)| *together == 4),
            "a question was put before the others: {seen:?}"
        );
    }

    /// IMPORT-12: the servers are offered in the order Ollama, LM Studio, llama-server, whatever
    /// order they answered in, so the one asked about first, and the one a second answer at the same
    /// port finds already written, is always the same. They answer in the reverse order here.
    #[test]
    fn the_servers_are_offered_in_a_fixed_order() {
        let (found, _) = with_servers(&[], |source, _| {
            let (listed, after) = match source {
                Source::Ollama => (three_pulled(), 200),
                Source::LmStudio => (lm_studio_listing(), 100),
                Source::LlamaServer => (llama_server_listing(), 0),
                Source::ClaudeCode | Source::Opencode => return None,
            };
            std::thread::sleep(std::time::Duration::from_millis(after));
            Some(listed)
        });

        let sources: Vec<_> = found.iter().map(|found| found.source).collect();
        assert_eq!(
            sources,
            [Source::Ollama, Source::LmStudio, Source::LlamaServer]
        );
    }
}
