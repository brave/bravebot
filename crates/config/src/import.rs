//! What Claude Code's and opencode's own configuration holds that this program can use.
//!
//! Read and mapped here, and never written: the question and the write belong to the front end that
//! asks (`docs/specs/import.md`). Only files in the person's profile directory are opened, because a
//! checkout's files are whatever the repository's author wrote, and a host and a credential name
//! taken from one would let a clone decide where the person's key is sent.

use crate::Secret;
use crate::bedrock::{Bedrock, Tier};
use crate::env_var;
use crate::provider::{self, AWS_PROVIDER_ID, Provider};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The most of a source file worth reading.
///
/// Larger than a settings file's bound, since an opencode configuration can list a gateway's whole
/// roster, and still a bound, so a file replaced by something else entirely is not parsed.
const SOURCE_BYTES: u64 = 1024 * 1024;

/// Claude Code's switch for Bedrock, which bravebot spells [`env_var::USE_BEDROCK`].
const CLAUDE_BEDROCK: &str = "CLAUDE_CODE_USE_BEDROCK";
/// Claude Code's switch for Vertex AI, which no service here reaches.
const CLAUDE_VERTEX: &str = "CLAUDE_CODE_USE_VERTEX";
/// Claude Code's model variable, which it reads before the settings file's `model` key.
const CLAUDE_MODEL: &str = "ANTHROPIC_MODEL";
/// The Bedrock API key Claude Code can sign in with, which bravebot does not: it signs through the
/// AWS credential chain.
const BEDROCK_BEARER: &str = "AWS_BEARER_TOKEN_BEDROCK";
/// What Claude Code reaches Anthropic's own API with, in a wire format no service here speaks.
const ANTHROPIC_NAMES: [&str; 3] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
];
/// The names a Claude Code Bedrock setup states that bravebot reads under the same spelling.
const BEDROCK_NAMES: [&str; 5] = [
    env_var::AWS_REGION,
    env_var::AWS_PROFILE,
    env_var::BEDROCK_OPUS_MODEL,
    env_var::BEDROCK_SONNET_MODEL,
    env_var::BEDROCK_HAIKU_MODEL,
];

/// The two programs whose configuration is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    ClaudeCode,
    Opencode,
}

impl Source {
    /// The program's own name, which is not translated.
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Opencode => "opencode",
        }
    }
}

/// The files each source is read from.
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

        Self {
            claude_code,
            opencode,
            opencode_auth: Some(data.join("auth.json")),
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
    /// Anthropic's own API, whose wire format no service here speaks.
    AnthropicApi,
    /// Google Vertex AI, which no service here reaches.
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
}

/// Everything the sources hold, Claude Code first.
///
/// `environment` is the process environment. A Claude Code Bedrock setup is often exported rather
/// than written down, and a name already there is read from there at run time, so it is not copied.
pub fn found(places: &Places, environment: impl Fn(&str) -> Option<String>) -> Vec<Found> {
    let environment = |name: &str| environment(name).filter(|value| !value.trim().is_empty());
    let mut found = Vec::new();
    if let Some(path) = &places.claude_code {
        found.extend(claude_code(path, &environment));
    }
    found.extend(opencode(&places.opencode, places.opencode_auth.as_deref()));
    found
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
        env,
        model,
        model_gateway: None,
        gateways: Vec::new(),
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
/// protocol other than OpenAI's.
const OWN_SDK_IDS: [&str; 6] = [
    "anthropic",
    "azure",
    "cohere",
    "google",
    "google-vertex",
    "google-vertex-anthropic",
];

/// Whether an entry's `npm` names an SDK speaking the protocol this entry is reached by.
///
/// On top of [`Provider::all`], which ignores the field: an `@ai-sdk/anthropic` entry with a
/// `baseURL` would be kept there and then sent requests in a protocol it does not speak. An entry
/// naming no SDK gets the one opencode picks for its id.
fn speaks_the_protocol(id: &str, npm: Option<&str>) -> bool {
    match npm {
        None => !OWN_SDK_IDS.contains(&id),
        Some("@ai-sdk/openai-compatible") => true,
        Some("@openrouter/ai-sdk-provider") => id == "openrouter",
        Some("@ai-sdk/amazon-bedrock") => id == AWS_PROVIDER_ID,
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
    let aws = id == AWS_PROVIDER_ID;
    let mut probe = Parsed::default();
    let mut block = Map::new();
    block.insert(id.to_string(), Value::Object(entry.clone()));
    probe.0.insert("provider".to_string(), Value::Object(block));
    let Some(kept) = Provider::all(&probe.0).pop() else {
        return Err(if aws {
            Reason::NoRegion
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

/// The user's own settings file, as a document an import may add names to.
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

    /// Whether `provider` already has an entry for `id`, or is something other than a block.
    pub fn holds_gateway(&self, id: &str) -> bool {
        match self.root.0.get("provider") {
            None => false,
            Some(Value::Object(block)) => block.contains_key(id),
            Some(_) => true,
        }
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
        found(&Places::from_lookup(&lookup), &lookup)
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
                "anthropic": {"options": {"baseURL": "https://anthropic-proxy.invalid"}},
                "google": {"options": {"baseURL": "https://gemini-proxy.invalid"}}
            }}"#,
        );

        let found = one(&home.path, &[], Source::Opencode);

        assert_eq!(ids(&found), ["compatible", "openrouter"]);
        // Naming no SDK, the two known ids get their own from opencode, and neither speaks OpenAI's.
        assert_eq!(
            left(&found),
            [
                ("anthropic", Reason::AnotherSdk),
                ("anthropic-proxy", Reason::AnotherSdk),
                ("google", Reason::AnotherSdk),
                ("not-openrouter", Reason::AnotherSdk)
            ]
        );
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
}
