//! Configuration for reaching an OpenAI-compatible gateway.
//!
//! The `provider` block in `~/.bravebot/settings.json`, in opencode's shape, so a block copied out
//! of `opencode.json` configures this agent unedited. That constraint decides most of what follows:
//! nothing is required that opencode does not require, no field is added however useful it would be,
//! and a field this crate does not know is read past rather than refused.
//!
//! What this does not hold is a resolved credential. A block says where a token lives, in the
//! variables `env` names or in `options.apiKey`, and it is read when a request needs signing. A key
//! `bravebot auth login gateway` stored for the block's id is a third place. A block with none of the
//! three names no credential and is asked without one, which is what a local Ollama wants.

use crate::Secret;
use std::fmt;
use std::time::Duration;

/// `url` as a report may print it: the userinfo of its authority dropped.
///
/// A `baseURL`, or an endpoint a variable or a settings `env` block names, may state credentials
/// (`https://user:token@host/v1`), and a report is pasted into issues. Only the authority is read
/// for an `@`, so one in a path or a query is kept.
pub fn without_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{tail}")
}

/// The endpoints known by the name a provider block gives them.
///
/// Here because the other tool resolves this from a registry it fetches, and a block copied out of it
/// therefore names an endpoint nowhere. Requiring `baseURL` of such a block is requiring a field that
/// tool does not, which is the one thing the borrowed shape is supposed to rule out.
///
/// Compiled in rather than fetched, and that is not an optimisation. This value is where a bearer
/// credential gets sent, so a service that could edit it could redirect somebody's token by answering
/// a request; see [routing.md](../../../docs/specs/routing.md). A table in the binary is a
/// destination somebody reviewed.
///
/// Short on purpose. An id that is not here is served by naming `baseURL`, which always works, so the
/// cost of an absent entry is a line of configuration rather than a broken gateway.
///
/// The third column is the variable the other tool reads that id's token from. A block copied out of
/// it relies on that variable without naming it, so an import names it for the block.
const KNOWN_ENDPOINTS: &[(&str, &str, &str)] = &[
    (
        "openrouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
    ),
    (OPENAI_ID, "https://api.openai.com/v1", "OPENAI_API_KEY"),
    (
        ANTHROPIC_ID,
        "https://api.anthropic.com/v1",
        "ANTHROPIC_API_KEY",
    ),
];

/// The id naming OpenAI, which is reached at the endpoint compiled in for it.
///
/// Named apart from the table because a reply from it is read differently: see
/// [`Provider::answers_under_the_model_behind_an_alias`].
pub(crate) const OPENAI_ID: &str = "openai";

/// The models a chat completions route at OpenAI's own API cannot reach, because they take requests
/// through the Responses API alone ([BACKEND-19]).
///
/// OpenAI's own listing states no capability for a model, so this list is the only statement of
/// which ids that route cannot serve. On 2026-10-27 the route answered none of its ids with a 200
/// when each was asked once with one short message and a ceiling of 16 tokens, and named the
/// Responses API in its refusal of three of them ([BACKEND-19]). The ids are read from two public
/// documents:
///
/// - the `ResponsesOnlyModel` list under `ModelIdsResponses` in version 2.3.0 of `openai-openapi`;
/// - the model pages at `https://developers.openai.com/api/docs/models/<id>`, read on 2026-10-27
///   for each id of the listing of 2026-10-26. A page's Endpoints block marks an endpoint it does
///   not support in grey. An id is added where its page marks `v1/chat/completions` unsupported and
///   `v1/responses` supported, and marks no endpoint supported beyond those two and `v1/batch`, so a
///   model that the Images or Audio endpoints also serve is not added. A dated snapshot is added
///   where its alias is added, when the alias page lists the snapshot under Snapshots and the
///   snapshot has no page of its own.
///
/// Both change with a release, and the second is read from the markup of a page.
///
/// [BACKEND-19]: ../../../docs/specs/backends.md#BACKEND-19
const RESPONSES_ONLY_MODELS: &[&str] = &[
    "o1-pro",
    "o1-pro-2025-03-19",
    "o3-pro",
    "o3-pro-2025-06-10",
    "o3-deep-research",
    "o3-deep-research-2025-06-26",
    "o4-mini-deep-research",
    "o4-mini-deep-research-2025-06-26",
    "computer-use-preview",
    "computer-use-preview-2025-03-11",
    "gpt-5.5-pro",
    "gpt-5.5-pro-2026-04-23",
    "gpt-5-codex",
    "gpt-5-pro",
    "gpt-5-pro-2025-10-06",
    "gpt-5.1-codex-max",
    "gpt-daybreak-blue-latest",
    "gpt-daybreak-red-latest",
    "gpt-5.6-cyber",
    "gpt-rosalind-research",
    "gpt-5.1-codex",
    "gpt-5.1-codex-mini",
    "gpt-5.2-codex",
    "gpt-5.2-pro",
    "gpt-5.2-pro-2025-12-11",
    "gpt-5.3-codex",
    "gpt-5.4-pro",
    "gpt-5.4-pro-2026-03-05",
];

/// The id a model of the ChatGPT account is qualified by
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// Not the id of a gateway: where a `provider` entry has it, the entry keeps the name and no model
/// is the account's.
pub const CHATGPT_ID: &str = "chatgpt";

/// Where a ChatGPT account's model requests go
/// ([CHATGPT-2](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2)).
///
/// A constant of the source. Nothing a settings file, a variable, a page or a reply says names
/// another, so the account's access token goes to this host and no other.
const CHATGPT_SERVING_HOST: &str = "https://api.openai.com/v1";

/// The variable that names a stand-in for the serving host, at `<base>/v1`, in a build with the
/// `test-hosts` feature. It is the one `bravebot_skus::chatgpt::STAND_IN` reads, so one stand-in
/// answers the sign-in and the requests. A build without the feature holds no code that reads it, and
/// a release build with it does not compile.
#[cfg(all(feature = "test-hosts", debug_assertions))]
pub const CHATGPT_STAND_IN: &str = "BRAVEBOT_TEST_CHATGPT_STAND_IN";

/// Where the account's requests go: [`CHATGPT_SERVING_HOST`], or in a build with the `test-hosts`
/// feature, a stand-in [`CHATGPT_STAND_IN`] names.
fn chatgpt_serving_host() -> String {
    #[cfg(all(feature = "test-hosts", debug_assertions))]
    if let Some(base) = std::env::var(CHATGPT_STAND_IN)
        .ok()
        .filter(|base| !base.is_empty())
    {
        return format!("{base}/v1");
    }
    CHATGPT_SERVING_HOST.to_string()
}

/// The id a model of the xAI account is qualified by
/// ([XAI-13](../../../docs/specs/xai-sign-in.md#XAI-13)).
///
/// Not the id of a gateway: where a `provider` entry has it, the entry keeps the name and no model
/// is the account's.
pub const XAI_ID: &str = "xai";

/// Where an xAI account's model requests go
/// ([XAI-1](../../../docs/specs/xai-sign-in.md#XAI-1)).
///
/// A constant of the source. Nothing a settings file, a variable, a page or a reply says names
/// another, so the account's access token goes to this host and no other.
const XAI_SERVING_HOST: &str = "https://api.x.ai/v1";

/// The variable that names a stand-in for the xAI serving host, at `<base>/v1`, in a build with the
/// `test-hosts` feature. It is the one `bravebot_skus::xai::XAI_STAND_IN` reads, so one stand-in
/// answers the sign-in and the requests. A build without the feature holds no code that reads it, and
/// a release build with it does not compile.
#[cfg(all(feature = "test-hosts", debug_assertions))]
pub const XAI_STAND_IN: &str = "BRAVEBOT_TEST_XAI_STAND_IN";

/// Where the account's requests go: [`XAI_SERVING_HOST`], or in a build with the `test-hosts`
/// feature, a stand-in [`XAI_STAND_IN`] names.
fn xai_serving_host() -> String {
    #[cfg(all(feature = "test-hosts", debug_assertions))]
    if let Some(base) = std::env::var(XAI_STAND_IN)
        .ok()
        .filter(|base| !base.is_empty())
    {
        return format!("{base}/v1");
    }
    XAI_SERVING_HOST.to_string()
}

/// The id naming Anthropic, which is reached at the endpoint compiled in for it, in the Messages
/// protocol rather than the chat completions one: see [`Provider::protocol`].
pub(crate) const ANTHROPIC_ID: &str = "anthropic";

/// The version of Anthropic's API every request to it states.
///
/// A header the service requires, and a date rather than a release: it names the shape of the
/// request and reply, and a change to either is a new date. Compiled in because a request that left
/// it to configuration could be sent in a shape the reader here does not parse.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// The header an Anthropic key travels in, which the service takes in place of `authorization`.
const ANTHROPIC_KEY_HEADER: &str = "x-api-key";

/// How many models Anthropic's listing is asked for in one page, the most it allows.
///
/// Its default is twenty, and the lineup is longer than that once the models still served but no
/// longer current are counted.
const ANTHROPIC_PAGE: u32 = 1000;

/// The id naming AWS Bedrock, which is reached by signing rather than by a bearer token.
///
/// The same id the other tool uses, so the block that configures it there configures it here. Its
/// endpoint is not in [`KNOWN_ENDPOINTS`] because there is no one endpoint: the host carries the
/// region, so it is built from `options.region` rather than looked up.
pub(crate) const AWS_PROVIDER_ID: &str = "amazon-bedrock";

/// The endpoint compiled in for `id`, where there is one.
pub(crate) fn known_endpoint(id: &str) -> Option<&'static str> {
    KNOWN_ENDPOINTS
        .iter()
        .find(|(known, _, _)| *known == id)
        .map(|(_, url, _)| *url)
}

/// The host and port the requests of `url` reach, in the form two spellings of one place share:
/// the host in lower case without one trailing dot, and the scheme's default port where none is
/// written. `None` for a url that is not `http` or `https`, or whose port is not a number.
///
/// The authority is read as [`without_userinfo`] reads it, so a `user:token@` is not part of the
/// host.
fn reached_at(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let default = match scheme.to_ascii_lowercase().as_str() {
        "https" => 443,
        "http" => 80,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = match authority.rsplit_once(':') {
        None => (authority, default),
        Some((host, "")) => (host, default),
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, port.parse().ok()?)
        }
        Some(_) => return None,
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    Some((host.to_ascii_lowercase(), port))
}

/// Every id [`known_endpoint`] has an endpoint for, in the order the table lists them.
///
/// Read from the table itself, so what a caller offers by name and what a session reaches by that
/// name are one list.
pub fn known_ids() -> impl Iterator<Item = &'static str> {
    KNOWN_ENDPOINTS.iter().map(|(id, _, _)| *id)
}

/// The variable the other tool reads `id`'s token from, where the id is one compiled in.
pub(crate) fn known_variable(id: &str) -> Option<&'static str> {
    if id == GOOGLE_VERTEX_ID {
        return Some(crate::env_var::GOOGLE_API_KEY);
    }
    KNOWN_ENDPOINTS
        .iter()
        .find(|(known, _, _)| *known == id)
        .map(|(_, _, variable)| *variable)
}

/// The id naming Google Vertex AI, which is reached through its OpenAI-compatible endpoint.
///
/// Not in [`KNOWN_ENDPOINTS`] for the reason Bedrock is not: the endpoint carries the project and
/// the location, so it is built from `options.project` and `options.location`.
pub(crate) const GOOGLE_VERTEX_ID: &str = "google-vertex";

/// The models a Vertex AI entry naming none is offered, since no listing will name them for a key.
///
/// Each was answered with a tool call, and answered again once the call came back with its result,
/// at the `global` endpoint on 2026-10-02. No preview is here: Google withdraws one without notice,
/// and a row that answers 404 stays in the picker until a release takes it out. Any model is still
/// reachable named qualified.
const VERTEX_MODELS: &[&str] = &[
    "google/gemini-3.1-flash-lite",
    "google/gemini-2.5-pro",
    "google/gemini-2.5-flash",
    "google/gemini-2.5-flash-lite",
];

/// The location Vertex AI is reached in where none is stated.
const VERTEX_DEFAULT_LOCATION: &str = "global";

/// Where Vertex AI express mode is reached: the native API, which takes a key and no project.
///
/// The URL names no region, and Google chooses where the request is served.
pub(crate) const VERTEX_EXPRESS_ENDPOINT: &str = "https://aiplatform.googleapis.com/v1";

/// The SDK an entry names to say that its stated endpoint takes Vertex's own API.
const VERTEX_SDK: &str = "@ai-sdk/google-vertex";

/// The SDK an entry names to say that its endpoint takes the Responses API, as opencode names the
/// package that speaks it.
pub(crate) const RESPONSES_SDK: &str = "@ai-sdk/openai";

/// The SDK an entry names to say that its endpoint takes chat completions, as opencode names the
/// package for a gateway that speaks it.
const CHAT_SDK: &str = "@ai-sdk/openai-compatible";

/// The publisher a model named without one is taken to belong to.
const VERTEX_PUBLISHER: &str = "google";

/// The header a Vertex AI key travels in.
///
/// The service refuses the same key as a bearer token, so this is where it goes instead of
/// `authorization`.
const VERTEX_KEY_HEADER: &str = "x-goog-api-key";

/// The context window a gateway model is assumed to have, in prompt tokens.
///
/// `limit.context` is optional, because opencode does not require it and a window is a property of
/// the model and the upstream serving it rather than of the person writing the file. Asked for, it
/// would be guessed at, and a guess in the file looks authoritative where a default does not.
///
/// Deliberately the same conservative figure Bedrock assumes, and for the same reason: being wrong
/// upward does not make compaction late, it removes it. [`crate::env_var::CONTEXT_BUDGET`] overrides
/// it for anyone who knows better.
pub const CONTEXT_WINDOW: u64 = crate::bedrock::CONTEXT_WINDOW;

/// One model a gateway offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// What the gateway calls it, which is what a request names.
    pub id: String,
    /// The context window, where the file stated one.
    ///
    /// `None` reads as [`CONTEXT_WINDOW`]. Kept distinct from a stated figure so that
    /// `doctor` can say which models are running on the default.
    pub context_window: Option<u64>,
    /// How long a reply this model may write, where the file stated it, out of the same `limit`
    /// block the window comes from and under the same rule.
    ///
    /// Read for the service that states a ceiling in every request, which is Anthropic's. Nothing
    /// else sends one, so for every other gateway it is carried and unused.
    pub output_limit: Option<u64>,
    /// Whatever `options` held, merged into the request body and never interpreted.
    ///
    /// Opaque on purpose: a gateway's routing controls are its own invention, and a schema
    /// enumerating them is a schema that changes when the gateway adds a field. Trusted exactly as
    /// far as a variable the person exported would be, and no model output can reach it.
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}

/// The most times a request is sent again that a block may state.
///
/// The pauses the doubling from one second gives total 127 seconds at the cap, and none is longer
/// than 64, which stays inside the two minutes after which a service's own request to wait is
/// refused rather than waited out. A wait a service asks for is taken in place of the pause, so the
/// whole wait is bounded by this count and by that figure per retry.
pub const MAX_RETRIES: u32 = 7;

/// The longest gap between two pieces of a reply that a block may state, in milliseconds.
///
/// The bound on the whole of a reply of unstated length is ten minutes, so a longer gap could never
/// be the one that ended it.
pub const MAX_IDLE_TIMEOUT_MS: u64 = 600_000;

/// What a block states about repeating a request and about waiting on a reply gone quiet.
///
/// Both are plain numbers a person chose, and only ever set a count or a timer: the decision to send
/// a request again is still taken from the status or the failure that came back, never from this.
/// `None` is nothing stated, which the clients read as the figures they have always used. Read out of
/// the block's `options` for a gateway and for an AWS account alike, and so only from the layers a
/// `provider` block is read from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Patience {
    /// `options.maxRetries`: how many times a request is sent again, at most [`MAX_RETRIES`].
    ///
    /// Zero is a statement, not an absence: it sends each request once.
    pub max_retries: Option<u32>,
    /// `options.idleTimeoutMs`: how long a reply may go quiet before it is given up on, at most
    /// [`MAX_IDLE_TIMEOUT_MS`].
    pub idle_timeout: Option<Duration>,
}

impl Patience {
    /// What a block's `options` state. A figure above its cap is the cap, as Codex reads its own;
    /// anything that is not a whole number, and a zero gap, states nothing.
    pub(crate) fn from_options(
        options: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Self {
        let stated = |key: &str| options.and_then(|options| options.get(key));
        Self {
            max_retries: stated("maxRetries")
                .and_then(serde_json::Value::as_u64)
                .and_then(|retries| u32::try_from(retries.min(u64::from(MAX_RETRIES))).ok()),
            idle_timeout: positive(stated("idleTimeoutMs"))
                .map(|gap| Duration::from_millis(gap.min(MAX_IDLE_TIMEOUT_MS))),
        }
    }

    /// How many requests are sent before a failure is the caller's: `default` where no retry count
    /// was stated, and one more than the count where one was.
    pub fn attempts(&self, default: u32) -> u32 {
        self.max_retries.map_or(default, |retries| retries + 1)
    }
}

/// One header a block asked for, with the value as the file wrote it.
#[derive(Debug, Clone)]
pub struct Header {
    /// The name, lowercased. HTTP names are case-insensitive, so one spelling here makes a name
    /// written two ways one header rather than two.
    pub name: String,
    /// The value as the block wrote it, which may be an `{env:NAME}` reference rather than a value.
    ///
    /// A [`Secret`] for the reason [`Provider::api_key`] is one: the header a gateway wants is
    /// often a second credential, and a block reaches a person's screen whenever a diagnostic
    /// prints one or an assertion about one fails
    /// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)).
    pub value: Secret,
}

/// Why a header a block named is not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFault {
    /// A name this program sets itself, or one that says how the request is framed.
    Reserved,
    /// Not a header name: empty, or holding a character a name may not.
    NotAName,
    /// A value holding a line ending, a NUL or a character that is not sendable, which would end
    /// the header and let what follows start another.
    NotAValue,
    /// A value holding `{env:` or `{file:` as part of something longer, which is a substitution
    /// nothing here makes. Refused rather than sent, the unsubstituted text being what would go.
    Substitution,
}

/// The variable a whole-value `{env:NAME}` reference names.
///
/// The same form `crate::import` reads, so a value written for one path means the same on the
/// other. Whole-value only: a reference inside a longer string is refused at load rather than
/// sent with the braces still in it.
fn env_reference(value: &str) -> Option<&str> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    if inner.contains(['{', '}']) {
        return None;
    }
    let name = inner.strip_prefix("env:")?.trim();
    (!name.is_empty()).then_some(name)
}

/// Whether `value` is one a header can carry.
///
/// A line ending would end this header and let the rest of the value start another, which is a
/// block choosing headers nobody wrote down. A NUL and the other control characters are refused
/// with it because no service reads one and an intermediary may read several of them differently.
fn sendable(value: &str) -> bool {
    value
        .chars()
        .all(|c| c == '\t' || (c.is_ascii() && !c.is_ascii_control()))
}

/// Whether `name` holds only the characters an HTTP field name may.
fn a_field_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

/// One gateway, and the models it was configured to offer.
///
/// Not comparable, because one field is a credential and `Secret` refuses equality: an operator
/// answering a question about a token's bytes recovers them a guess at a time.
#[derive(Debug, Clone)]
pub struct Provider {
    /// The key this provider had in the block, which is what a picker row names it by.
    ///
    /// A term that cannot collide with a name a service chose for itself, which is what
    /// distinguishing two services offering the same model requires.
    pub id: String,
    /// What to show a person choosing, where the block said something friendlier than the id.
    pub name: Option<String>,
    /// Where to send requests, without a trailing slash.
    pub base_url: String,
    /// Variables that may hold the bearer token, in the order to try them.
    pub env: Vec<String>,
    /// A token written into the file directly.
    ///
    /// Supported because it is opencode's field, not because it is a good idea: a long-lived
    /// credential in a file is a credential in a file people paste into issues. Naming a variable in
    /// `env` keeps it wherever the person already keeps secrets.
    ///
    /// A [`Secret`] rather than a `String` so that the value is redacted wherever this block is
    /// printed, and so that the buffer holding it is cleared when the block goes
    /// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)).
    pub api_key: Option<Secret>,
    /// The key `bravebot auth login gateway` stored for this id, where one is.
    ///
    /// Never read out of the block: the store is a file of its own ([`crate::keys`]), and
    /// configuration fills this in by id once the blocks are read.
    pub stored_key: Option<Secret>,
    /// Whether the block states no endpoint and is reached at the host compiled in for its id, in
    /// [`KNOWN_ENDPOINTS`].
    ///
    /// That service refuses every request without a key, so a block of such an id naming no
    /// credential is not the person saying none is wanted ([BACKEND-16]).
    ///
    /// [BACKEND-16]: ../../../docs/specs/backends.md#BACKEND-16
    pub(crate) endpoint_compiled_in: bool,
    /// The models this provider offers, in the order the file listed them.
    ///
    /// Possibly empty, because opencode does not require `models`. A provider offering nothing is
    /// reported as such rather than guessed at: a gateway roster is too large and too fluid to
    /// enumerate, so the gateway is asked. A service with no listing to ask is offered
    /// [`Self::compiled_roster`] instead, which is never copied in here.
    pub models: Vec<Model>,
    /// The window every model of this provider is assumed to have, where `options.contextWindow`
    /// states one.
    ///
    /// For a server whose window is one setting for all its models, such as a local Ollama, and the
    /// only place to keep a figure where the block lists no models, since a block that lists models
    /// turns off the roster. A model's own `limit.context` outranks it.
    pub context_window: Option<u64>,
    /// How often a request to this gateway is sent again, and how long a reply that has gone quiet
    /// is waited on, where `options` states either.
    ///
    /// Nothing for an AWS entry: its account is what a client reaches and carries its own
    /// ([`crate::bedrock::Bedrock::patience`]), so the figures have one home.
    pub patience: Patience,
    /// A reply ceiling somebody exported, which outranks whatever the models state.
    ///
    /// Set where the account's own copy is, once the environment has been read: a block is parsed
    /// with none to consult. Only a service whose requests state a ceiling reads it.
    pub(crate) output_budget: Option<u64>,
    /// The protocol this entry's requests are written in: see [`Provider::protocol`].
    pub(crate) protocol: Protocol,
    /// The AWS account this entry reaches, where it names Bedrock rather than a gateway.
    ///
    /// Resolved here rather than by the caller because a request to Bedrock needs a region to sign
    /// for and a profile to resolve credentials from, and both are properties of this block.
    /// `None` for every other entry, which is reached as an OpenAI-compatible gateway.
    pub bedrock: Option<crate::bedrock::Bedrock>,
    /// The headers `options.headers` asked for, in the order the block listed them.
    ///
    /// Carried rather than resolved, on [BACKEND-16]'s footing for a token: a value may name a
    /// variable, and one read at startup is stale in a session where somebody exported another.
    /// [`Self::request_headers`] resolves them at the point a request needs them.
    ///
    /// Person-written, like every other name in a settings file, so nothing here is untrusted
    /// content and no decision is made on a byte a turn produced.
    ///
    /// [BACKEND-16]: ../../../docs/specs/backends.md#BACKEND-16
    pub headers: Vec<Header>,
    /// The names under `options.headers` this block will not send, with why.
    ///
    /// Kept rather than dropped for the reason `crate::settings::Settings::references_unread` is
    /// kept: a header somebody wrote and this program refuses is the one worth saying out loud,
    /// since a gateway that answers 401 says nothing about a line in a settings file.
    pub headers_refused: Vec<(String, HeaderFault)>,
}

/// The wire protocol a provider's requests are written in.
///
/// Which one is a fact about the service and not about the model, so it is read from the entry's
/// id: the endpoint compiled in for a name is the only place the protocol is known to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// OpenAI's chat completions, which most gateways speak.
    ChatCompletions,
    /// Anthropic's Messages API.
    Messages,
    /// Google's native `generateContent` API, which Vertex AI express mode speaks.
    GenerateContent,
    /// OpenAI's Responses API, which an entry naming `@ai-sdk/openai` speaks, and an `openai`
    /// entry reached at OpenAI's own API
    /// ([BACKEND-61](../../../docs/specs/backends.md#BACKEND-61)).
    Responses,
}

impl Provider {
    /// The service a stored ChatGPT sign-in reaches, spoken to in the Responses protocol
    /// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
    ///
    /// Built by the same reading as an entry that names `@ai-sdk/openai`, from constants, so its
    /// route, protocol and header are the ones such an entry has. It names no variable, holds no key
    /// and lists no model: its token is the account's and its models are the account's.
    pub fn chatgpt_account() -> Self {
        let entry = serde_json::json!({
            "name": "ChatGPT",
            "npm": RESPONSES_SDK,
            "options": {"baseURL": chatgpt_serving_host()},
        });
        let serde_json::Value::Object(entry) = entry else {
            unreachable!("a JSON object literal");
        };
        match Self::one(CHATGPT_ID, &entry) {
            Some(account) => account,
            None => unreachable!("an entry that states its endpoint is reachable"),
        }
    }

    /// The service a stored xAI sign-in reaches, spoken to in chat completions as every other
    /// gateway is ([XAI-15](../../../docs/specs/xai-sign-in.md#XAI-15)).
    ///
    /// Built by the same reading as an entry that states its endpoint, from constants. It names no
    /// variable, holds no key and lists no model: its token is the account's and its models are
    /// the account's.
    pub fn xai_account() -> Self {
        let entry = serde_json::json!({
            "name": "xAI",
            "options": {"baseURL": xai_serving_host()},
        });
        let serde_json::Value::Object(entry) = entry else {
            unreachable!("a JSON object literal");
        };
        match Self::one(XAI_ID, &entry) {
            Some(account) => account,
            None => unreachable!("an entry that states its endpoint is reachable"),
        }
    }

    /// The service a known id reaches where no block names it, or `None` for an id that is not one
    /// of [`known_ids`].
    ///
    /// The reading of a block that holds no field, so its endpoint is the one a session reaches
    /// for `{"<id>": {}}` and cannot differ from it. `Self::one` alone would also accept an id such
    /// as `google-vertex` that has no entry in the table and builds from an empty entry, so the table
    /// is asked first.
    pub fn known(id: &str) -> Option<Self> {
        known_endpoint(id)?;
        Self::one(id, &serde_json::Map::new())
    }

    /// Every provider in a settings root, in the order the file listed them.
    ///
    /// Empty where the block is absent or shaped differently, on the same footing as the rest of the
    /// file: a half-typed settings file must not stop a session.
    ///
    /// A provider whose endpoint is neither stated nor known is dropped. Unlike a missing window
    /// there is nothing to assume: with no host there is no request to build, and inventing one
    /// produces failures far from the mistake.
    pub fn all(root: &serde_json::Map<String, serde_json::Value>) -> Vec<Self> {
        let Some(serde_json::Value::Object(block)) = root.get("provider") else {
            return Vec::new();
        };
        block
            .iter()
            .filter_map(|(id, entry)| match entry {
                serde_json::Value::Object(entry) => Self::one(id, entry),
                _ => None,
            })
            .collect()
    }

    /// One entry under `provider`, or `None` where it does not describe a reachable service.
    fn one(id: &str, entry: &serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        let options = match entry.get("options") {
            Some(serde_json::Value::Object(options)) => Some(options),
            _ => None,
        };

        if id == AWS_PROVIDER_ID {
            return Self::aws(id, entry, options);
        }
        let stated = stated_endpoint(options);
        let npm = string(entry.get("npm"));
        let mut protocol = protocol_of(id, npm.as_deref(), stated);
        let project = string(options.and_then(|options| options.get("project")));
        // A Vertex entry naming no project is the express mode of the native API, which needs the
        // key alone. One that states an endpoint of its own keeps speaking chat completions there
        // unless it names the SDK that says the endpoint takes Vertex's own API, so a proxy written
        // for the OpenAI-compatible route is not sent a request it cannot read.
        if id == GOOGLE_VERTEX_ID
            && project.is_none()
            && (stated.is_none() || npm.as_deref() == Some(VERTEX_SDK))
        {
            protocol = Protocol::GenerateContent;
        }
        // A stated endpoint wins, so a block pointing a known name at a proxy or a private
        // deployment reaches the host it named rather than the one compiled in.
        // The credential header is this provider's own, whichever one `credential_header` chooses,
        // and a block naming a credential has already said how the request authenticates. Settled
        // here rather than in the parse so that Vertex refuses `x-goog-api-key` and nothing else
        // has to know that is the name.
        let names_a_credential = !names(entry.get("env")).is_empty()
            || options
                .and_then(|options| string(options.get("apiKey")))
                .is_some();
        let credential_name = if id == GOOGLE_VERTEX_ID {
            VERTEX_KEY_HEADER
        } else if protocol == Protocol::Messages {
            ANTHROPIC_KEY_HEADER
        } else {
            "authorization"
        };
        let (headers, headers_refused) = header_entries(
            options.and_then(|options| options.get("headers")),
            credential_name,
            names_a_credential,
            Provider::fixed_header_names(protocol),
        );
        let endpoint_compiled_in = stated.is_none()
            && id != GOOGLE_VERTEX_ID
            && id != AWS_PROVIDER_ID
            && known_endpoint(id).is_some();
        let base_url = match stated {
            Some(url) => url.to_string(),
            None if id == GOOGLE_VERTEX_ID && project.is_none() => {
                VERTEX_EXPRESS_ENDPOINT.to_string()
            }
            None if id == GOOGLE_VERTEX_ID => vertex_endpoint(
                project.as_deref(),
                string(options.and_then(|options| options.get("location"))).as_deref(),
            )?,
            // The table's address for this id is the one its own protocol is spoken at, so an entry
            // that names another protocol has no address here and says where its service is.
            None if id == ANTHROPIC_ID && protocol != Protocol::Messages => return None,
            None => known_endpoint(id)?.to_string(),
        };

        Some(Self {
            id: id.to_string(),
            name: string(entry.get("name")),
            base_url: base_url.trim_end_matches('/').to_string(),
            env: names(entry.get("env")),
            api_key: options
                .and_then(|options| string(options.get("apiKey")))
                .map(Secret::new),
            stored_key: None,
            endpoint_compiled_in,
            models: models(entry.get("models")),
            context_window: options.and_then(|options| positive(options.get("contextWindow"))),
            patience: Patience::from_options(options),
            output_budget: None,
            protocol,
            bedrock: None,
            headers,
            headers_refused,
        })
    }

    /// The Vertex AI service the environment names, where it names one.
    ///
    /// The key has to hold a value, and a project that is set has to be one the service's host can
    /// be built from. A key with no project is express mode, which the native API serves with the
    /// key alone; a project with no key is a service nobody could sign in to. The entry names
    /// `GOOGLE_API_KEY` in `env`, so an exported key is read again when a request needs it, and
    /// holds the value `lookup` found for the case where the key lives only in a settings file.
    pub(crate) fn from_environment(lookup: &impl Fn(&str) -> Option<String>) -> Option<Self> {
        let filled = |name: &str| lookup(name).filter(|value| !value.trim().is_empty());
        let mut key = filled(crate::env_var::GOOGLE_API_KEY)?;
        let token = Secret::new(key.trim());
        crate::scrub(&mut key);
        let (base_url, protocol) = match filled(crate::env_var::GOOGLE_CLOUD_PROJECT) {
            Some(project) => {
                let location = filled(crate::env_var::VERTEX_LOCATION);
                let base_url =
                    vertex_endpoint(Some(project.trim()), location.as_deref().map(str::trim))?;
                (base_url, Protocol::ChatCompletions)
            }
            None => (
                VERTEX_EXPRESS_ENDPOINT.to_string(),
                Protocol::GenerateContent,
            ),
        };

        Some(Self {
            id: GOOGLE_VERTEX_ID.to_string(),
            name: None,
            base_url,
            env: vec![crate::env_var::GOOGLE_API_KEY.to_string()],
            api_key: Some(token),
            stored_key: None,
            endpoint_compiled_in: false,
            models: Vec::new(),
            context_window: None,
            patience: Patience::default(),
            output_budget: None,
            protocol,
            bedrock: None,
            headers: Vec::new(),
            headers_refused: Vec::new(),
        })
    }

    /// One entry naming AWS Bedrock.
    ///
    /// A region is what this requires and all it requires, being what the host is built from and
    /// what a signature is bound to. Credentials come from the AWS chain, so there is no token to
    /// name and nothing here reads one: a `profile` selects which set of them, exactly as the tier
    /// variables' own does.
    fn aws(
        id: &str,
        entry: &serde_json::Map<String, serde_json::Value>,
        options: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Option<Self> {
        let region = string(options?.get("region"))?;
        let profile = string(options.and_then(|options| options.get("profile")));
        let models = models(entry.get("models"));
        let entries = bedrock_entries(entry.get("models"));
        let bedrock = crate::bedrock::Bedrock::from_provider(region, profile, entries)?
            .with_patience(Patience::from_options(options));
        // Bedrock takes a signature over the request and the headers it is computed across, so a
        // header added from a file would either be left out of the signature or change what was
        // signed. Every name an AWS entry writes is refused for that reason rather than some of
        // them.
        let headers_refused = header_names(options.and_then(|options| options.get("headers")))
            .map(|name| (name, HeaderFault::Reserved))
            .collect();

        Some(Self {
            id: id.to_string(),
            name: string(entry.get("name")),
            base_url: format!("https://{}", bedrock.host()),
            env: Vec::new(),
            api_key: None,
            stored_key: None,
            endpoint_compiled_in: false,
            models,
            context_window: None,
            patience: Patience::default(),
            output_budget: None,
            protocol: Protocol::ChatCompletions,
            bedrock: Some(bedrock),
            headers: Vec::new(),
            headers_refused,
        })
    }

    /// The protocol this provider's requests are written in.
    ///
    /// Chat completions, except for an `anthropic` entry, which is reached in the Messages protocol
    /// as opencode reaches it unless the entry names the OpenAI-compatible SDK. A block copied out of
    /// opencode says which it is in the same field, and an entry that was a gateway of its own
    /// under this name keeps that reading by naming the SDK.
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// The URL one chat completion goes to.
    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// The URL one request for a reply goes to, whichever protocol it is written in.
    ///
    /// The one a refusal is remembered against and a diagnostic names, so neither says a route this
    /// service does not have.
    pub fn completion_url(&self) -> String {
        match self.protocol() {
            Protocol::ChatCompletions => self.chat_completions_url(),
            Protocol::Messages => format!("{}/messages", self.base_url),
            Protocol::GenerateContent => {
                format!("{}/publishers/{VERTEX_PUBLISHER}/models", self.base_url)
            }
            Protocol::Responses => format!("{}/responses", self.base_url),
        }
    }

    /// The URL one `generateContent` request for `model` goes to, or `None` where the model's name
    /// holds a character that would move the request.
    ///
    /// `model` is the name the service knows it by, `<publisher>/<model>` or a bare model of the
    /// `google` publisher. Both parts are written into the path, so one holding `/`, `?`, `#`,
    /// `@` or `:` would send the key to another route than the one named; such a name has no URL.
    /// `streaming` picks the route that answers as server-sent events.
    pub fn generate_content_url(&self, model: &str, streaming: bool) -> Option<String> {
        let (publisher, name) = match model.split_once('/') {
            Some((publisher, name)) => (publisher, name),
            None => (VERTEX_PUBLISHER, model),
        };
        // Dots alone are a step up or in place, which a client resolves before it sends.
        let part_ok = |part: &str| {
            !part.chars().all(|c| c == '.')
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !part_ok(publisher) || !part_ok(name) {
            return None;
        }
        let route = if streaming {
            "streamGenerateContent?alt=sse"
        } else {
            "generateContent"
        };
        Some(format!(
            "{}/publishers/{publisher}/models/{name}:{route}",
            self.base_url
        ))
    }

    /// The URL that answers with what this gateway serves.
    ///
    /// Anthropic's states how many it returns in a page, and the page is as long as it allows: its
    /// default would leave the later models out of the picker without a word.
    pub fn models_url(&self) -> String {
        match self.protocol() {
            Protocol::ChatCompletions | Protocol::Responses => format!("{}/models", self.base_url),
            Protocol::Messages => format!("{}/models?limit={ANTHROPIC_PAGE}", self.base_url),
            Protocol::GenerateContent => self.completion_url(),
        }
    }

    /// Whether this service answers a question about what the credential in use may reach.
    ///
    /// Not part of the shape every gateway implements, so [`Provider::account_models_url`] is a
    /// route only a service that has it is asked. Anthropic has none: the request would carry the
    /// key to be answered 404.
    pub fn answers_for_an_account(&self) -> bool {
        matches!(
            self.protocol(),
            Protocol::ChatCompletions | Protocol::Responses
        )
    }

    /// The URL that answers with what the credential in use may reach.
    ///
    /// A narrower roster than [`Provider::models_url`], and the useful one: a model this account
    /// cannot reach is a row that fails the moment somebody picks it. Not part of the shape every
    /// gateway implements, so a caller asks and falls back rather than relying on it.
    pub fn account_models_url(&self) -> String {
        format!("{}/models/user", self.base_url)
    }

    /// The models offered in place of asking this service what it serves, where it cannot be asked.
    ///
    /// Vertex AI only, which has no listing a key can call: the request would carry the key to be
    /// answered 404. `None` for every other gateway, which is asked. Never part of [`Self::models`],
    /// which a bare name is routed by: these are names to choose from, and a choice off a picker is
    /// recorded qualified.
    pub fn compiled_roster(&self) -> Option<&'static [&'static str]> {
        (self.id == GOOGLE_VERTEX_ID).then_some(VERTEX_MODELS)
    }

    /// The header this service takes its credential in, and the value to put there.
    ///
    /// `authorization` with a bearer token everywhere but Vertex AI, which refuses a key sent that
    /// way and takes it in `x-goog-api-key`. The only place either header is chosen, so a request
    /// and a roster cannot disagree about it.
    pub fn credential_header(&self, token: &str) -> (&'static str, String) {
        if self.id == GOOGLE_VERTEX_ID {
            (VERTEX_KEY_HEADER, token.to_string())
        } else if self.protocol == Protocol::Messages {
            (ANTHROPIC_KEY_HEADER, token.to_string())
        } else {
            ("authorization", format!("Bearer {token}"))
        }
    }

    /// The headers this service takes on every request whatever the credential, with their values.
    ///
    /// Empty for every gateway but Anthropic, whose API requires the version of itself the request
    /// is written for. Here beside [`Provider::credential_header`] so a request and a roster state
    /// both and cannot disagree about either.
    pub fn fixed_headers(&self) -> &'static [(&'static str, &'static str)] {
        match self.protocol() {
            Protocol::ChatCompletions | Protocol::GenerateContent | Protocol::Responses => &[],
            Protocol::Messages => &[("anthropic-version", ANTHROPIC_VERSION)],
        }
    }

    /// The names `Self::fixed_headers` sets for `protocol`, so `header_entries` can refuse a
    /// block that also names one: attached unconditionally and before a configured header, a
    /// second copy would go out beside it rather than in place of it.
    fn fixed_header_names(protocol: Protocol) -> &'static [&'static str] {
        match protocol {
            Protocol::ChatCompletions | Protocol::GenerateContent | Protocol::Responses => &[],
            Protocol::Messages => &["anthropic-version"],
        }
    }

    /// The headers to put on a request to this service beyond the ones it builds, resolved.
    ///
    /// A value naming a variable is read through `lookup` at this moment, which is the moment and
    /// the reason a token is read ([BACKEND-16]). A reference whose variable holds nothing sends no
    /// header: unlike a credential there is nothing to refuse on its behalf, since nothing here
    /// knows whether the service requires it, and a request that reaches the gateway without it is
    /// answered by the gateway rather than failing for a reason only this file could explain.
    ///
    /// [BACKEND-16]: ../../../docs/specs/backends.md#BACKEND-16
    pub fn request_headers(
        &self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Vec<(String, Secret)> {
        self.headers
            .iter()
            .filter_map(|header| {
                let value = match env_reference(header.value.expose()) {
                    Some(name) => Secret::new(lookup(name)?.trim()),
                    None => header.value.clone(),
                };
                (!value.expose().is_empty() && sendable(value.expose()))
                    .then(|| (header.name.clone(), value))
            })
            .collect()
    }

    /// Whether a tool call's `extra_content` is sent back to this service.
    ///
    /// Vertex AI attaches a thought signature there and refuses the next request without it.
    /// Another service either ignores the member or refuses the request for carrying it.
    pub fn takes_extra_content(&self) -> bool {
        self.id == GOOGLE_VERTEX_ID
    }

    /// Whether this entry's requests go to OpenAI's own API, from the host and port they are sent
    /// to ([`is_openais_endpoint`]).
    ///
    /// The entry's id does not matter: an entry keyed `openai` that states a proxy behaves as that
    /// proxy, and an entry under another id that states OpenAI's endpoint behaves as OpenAI's.
    fn at_openais_api(&self) -> bool {
        is_openais_endpoint(&self.base_url)
    }

    /// Whether a listing of this service states a `shutdown_date` for a model that its requests
    /// are refused after ([BACKEND-19]).
    ///
    /// OpenAI's own API only. Another service that states a field of that name may mean something
    /// else by it, and nothing is known of what its dates are for.
    ///
    /// [BACKEND-19]: ../../../docs/specs/backends.md#BACKEND-19
    pub fn lists_shutdown_dates(&self) -> bool {
        self.at_openais_api()
    }

    /// The ids a listing of this service is not offered, because the models they name take
    /// requests through the Responses API only.
    ///
    /// Empty except for OpenAI's own API reached in the chat completions protocol, which an
    /// `openai` entry is only where it names the OpenAI-compatible SDK. An entry reached in the
    /// Responses protocol serves them, and a proxy at another host may serve them over chat
    /// completions. A block that names `models` is offered those and no others, so a
    /// Responses-only model is still one somebody can name.
    pub fn responses_only_models(&self) -> &'static [&'static str] {
        if self.protocol == Protocol::ChatCompletions && self.at_openais_api() {
            RESPONSES_ONLY_MODELS
        } else {
            &[]
        }
    }

    /// Whether a request to this service carries `cache_control` marks on the prefix it will send
    /// again ([BACKEND-32]).
    ///
    /// False for OpenAI's own API, whose published request schema has no such mark on a content
    /// part. On 2026-10-27 six of the seven models tried read a repeated prompt from the cache with
    /// no mark ([BACKEND-31]), and `gpt-4o-mini` and `gpt-5.4-mini` answered a request that carried
    /// one with 200. Whether a mark changes what is read is not known. None is sent there.
    ///
    /// [BACKEND-31]: ../../../docs/specs/backends.md#BACKEND-31
    /// [BACKEND-32]: ../../../docs/specs/backends.md#BACKEND-32
    pub fn takes_cache_markers(&self) -> bool {
        !self.at_openais_api()
    }

    /// Whether a reply names the model behind the name the request carried, not that name.
    ///
    /// OpenAI and Anthropic only: a name such as `gpt-4o` or `claude-haiku-4-5` is an alias, and the
    /// reply names the dated model it resolved to. Every other gateway answers under the slug it was
    /// asked for, so a name that comes back changed is a substitution there and the alias working
    /// here. Both refuse a name they do not serve, so there is no substitution for the comparison
    /// to find.
    pub fn answers_under_the_model_behind_an_alias(&self) -> bool {
        self.id == OPENAI_ID || self.protocol == Protocol::Messages
    }

    /// The same provider with the reply ceiling somebody exported, which outranks every model's.
    pub fn with_output_budget(mut self, budget: Option<u64>) -> Self {
        self.output_budget = budget;
        self
    }

    /// The reply ceiling somebody stated for `model`, exported or in its block, and `None` where
    /// the one assumed for a request applies.
    ///
    /// An exported budget, then what the model's own block stated, which is the order the account
    /// of AWS resolves its own in ([`crate::bedrock::Bedrock::stated_output_limit`]).
    pub fn stated_output_limit(&self, model: &str) -> Option<u64> {
        self.output_budget
            .or_else(|| self.model(model).and_then(|model| model.output_limit))
    }

    /// What to show a person choosing this provider.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// The host this gateway is reached at, which is what would end a token it carries.
    ///
    /// Recorded beside the credential for [`crate::Held::GatewayToken`]: a bearer token is ended
    /// where it is presented, and the endpoint is the only thing a block states that names that
    /// place. The path is dropped because a report a person acts on names the service rather than
    /// one of its routes.
    ///
    /// Userinfo is dropped with it. A `baseURL` may state credentials, and this string is written
    /// into a diagnostic people paste into issues, so the one part of a URL that can be a secret
    /// does not travel with the host.
    pub fn host(&self) -> &str {
        let authority = self
            .base_url
            .split_once("://")
            .map_or(self.base_url.as_str(), |(_, rest)| rest)
            .split('/')
            .next()
            .unwrap_or_default();
        authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host)
    }

    /// The model this provider offers under `id`, if it offers one.
    ///
    /// Used to check a remembered choice before it becomes a request, and to find the window a
    /// budget is taken from.
    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|model| model.id == id)
    }

    /// The window to budget `model` against: its own, else this provider's, else the assumed one.
    pub fn window_of(&self, model: &Model) -> u64 {
        model
            .context_window
            .or(self.context_window)
            .unwrap_or(CONTEXT_WINDOW)
    }

    /// Whether a name is one of this provider's models.
    pub fn offers(&self, model: &str) -> bool {
        self.model(model).is_some()
    }

    /// The bearer token for this gateway, or why there is none.
    ///
    /// A variable first, so that a file naming one does not have the value read out from under it by
    /// a stale `apiKey`. A stored key next, since storing one is a thing somebody did on purpose and
    /// later than writing a block, and a key left in the block is what it replaces.
    pub fn credential(&self, lookup: impl Fn(&str) -> Option<String>) -> Credential {
        self.credential_and_source(lookup).0
    }

    /// The bearer token with the place it was found, for a report that names the place and never
    /// the value. `None` beside a credential that is not a token.
    pub fn credential_and_source(
        &self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> (Credential, Option<Source>) {
        if !self.names_a_credential() {
            // A service at the host compiled in for a known id refuses a request with no key, so a
            // block naming none is a credential missing, not one that is not wanted.
            // A header named for the credential is how such a block authenticates, so it is one.
            // `authorization` counts under every protocol: an `anthropic` block written with its
            // own `Authorization: Bearer ...` header worked before this rule and keeps working.
            let credential_name = self.credential_header("").0;
            let authenticates_by_header = self
                .headers
                .iter()
                .any(|held| held.name == credential_name || held.name == "authorization");
            return if self.endpoint_compiled_in && !authenticates_by_header {
                (Credential::Absent, None)
            } else {
                (Credential::NotNeeded, None)
            };
        }
        let exported = self
            .env
            .iter()
            .filter_map(|name| lookup(name))
            .find_map(|mut value| {
                let token = Secret::new(value.trim());
                // The lookup handed over a `String` of its own and the trimmed token is a second
                // copy, so the first is cleared here rather than returned to the allocator holding
                // the token. A buffer that lived for two statements is still one this program owns.
                crate::scrub(&mut value);
                (!token.is_empty()).then_some(token)
            });
        let found = exported
            .map(|token| (token, Source::Variable))
            .or_else(|| self.stored_key.clone().map(|token| (token, Source::Stored)))
            .or_else(|| self.api_key.clone().map(|token| (token, Source::File)));
        match found {
            Some((token, source)) => (Credential::Token(token), Some(source)),
            None => (Credential::Absent, None),
        }
    }

    /// Whether anywhere a token could live is named, by the block or by a key stored for its id.
    pub(crate) fn names_a_credential(&self) -> bool {
        !self.env.is_empty() || self.api_key.is_some() || self.stored_key.is_some()
    }

    /// Refuse a header named for the credential now that a stored key makes this block one that
    /// names a credential, which `Self::one` could not know when it built `headers`.
    ///
    /// `stored_key` is filled in after parsing, by whichever caller reads
    /// `~/.bravebot/gateway-keys.json` (`Config::from_sources`), so a block naming no `env` and no
    /// `apiKey` passed `header_entries` its `names_a_credential` as `false` and kept a header named
    /// for the credential as an ordinary one. Left uncorrected, the request this block builds would
    /// carry that header's value and the stored key both: two credentials on one request, with
    /// whichever `Request::header` call ran last deciding which one a reader of the request sees.
    ///
    /// A no-op where `stored_key` is still `None`, or where `self.headers` holds nothing by that
    /// name, so a block already refusing it from `env` or `apiKey` is not reported twice.
    pub(crate) fn reconcile_stored_credential(&mut self) {
        if self.stored_key.is_none() {
            return;
        }
        let credential_name = self.credential_header("").0;
        if let Some(position) = self
            .headers
            .iter()
            .position(|held| held.name == credential_name || held.name == "authorization")
        {
            let header = self.headers.remove(position);
            self.headers_refused
                .push((header.name, HeaderFault::Reserved));
        }
    }
}

/// What a gateway block says about the credential its requests carry.
///
/// Three answers rather than two, because "no token" is two different situations and only one of
/// them is a mistake. A block naming variables or an `apiKey` has said where a credential lives, so
/// nothing holding one is a token that is missing or has gone stale. A block naming neither has said
/// none is needed, which is how the tool this shape is borrowed from configures a local Ollama.
///
/// One answer rather than a pair the callers combine, because there are three of them and each has a
/// different remedy: a request, a roster, and a diagnostic. Read as a bare `Option`, the second
/// situation is indistinguishable from the first, and every caller reports a gateway that needs
/// nothing as one somebody has to go and configure.
/// Not comparable, for the reason [`Provider`] is not: the token it carries is a [`Secret`].
#[derive(Clone)]
pub enum Credential {
    /// The token to attach, from the first variable that held one or from the file.
    Token(Secret),
    /// The block names where a token lives and nothing there holds one.
    Absent,
    /// The block names nowhere for a token to live, so its requests carry none.
    NotNeeded,
}

/// Where a gateway's token was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A variable the block's `env` names.
    Variable,
    /// The key `bravebot auth login gateway` stored for the block's id.
    Stored,
    /// The block's own `options.apiKey`.
    File,
}

/// Redacting rather than derived: the value is a long-lived bearer token, and a `Debug` that printed
/// one would put a live credential in whatever assertion or log first fails.
impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token(_) => f.write_str("Token(<redacted>)"),
            Self::Absent => f.write_str("Absent"),
            Self::NotNeeded => f.write_str("NotNeeded"),
        }
    }
}

/// The protocol an entry's requests are written in, from its id, the SDK it names and the endpoint
/// it states.
///
/// An entry naming the SDK that speaks the Responses API is reached in it, whatever its id but
/// `google-vertex`, whose own protocols, and the header its key goes in, are fixed by the id.
/// Otherwise the `anthropic` id is reached in the Messages protocol, as opencode reaches it, and the
/// `openai` id in the Responses protocol where it is reached at OpenAI's own API. An entry under
/// either id that names the OpenAI-compatible SDK is reached as any other gateway is, and so is an
/// `openai` entry that states a proxy's endpoint, which may speak chat completions alone. Any other
/// SDK an entry names is not read: the id decides.
pub(crate) fn protocol_of(id: &str, npm: Option<&str>, stated: Option<&str>) -> Protocol {
    if npm == Some(RESPONSES_SDK) && id != GOOGLE_VERTEX_ID {
        Protocol::Responses
    } else if id == ANTHROPIC_ID && npm != Some(CHAT_SDK) {
        Protocol::Messages
    } else if id == OPENAI_ID && npm != Some(CHAT_SDK) && stated.is_none_or(is_openais_endpoint) {
        Protocol::Responses
    } else {
        Protocol::ChatCompletions
    }
}

/// Whether `url` reaches the same host and port as the endpoint compiled in for [`OPENAI_ID`].
///
/// Case, a trailing dot on the host and a port that is the scheme's default are spellings of the
/// same place ([`reached_at`]). A port that is not the default is another service listening on
/// OpenAI's name, so it is not OpenAI's own API.
fn is_openais_endpoint(url: &str) -> bool {
    let own = known_endpoint(OPENAI_ID).and_then(reached_at);
    own.is_some() && own == reached_at(url)
}

/// The OpenAI-compatible endpoint of Vertex AI for a project and location, without a trailing slash.
///
/// `None` where there is no project, or where either value holds a character that would move the
/// request: both are written into the URL, and a `/`, `@`, `?` or `#` in one would send the key to
/// another host or another route than the one named. The location picks the host, so it is checked
/// more tightly than the project, which only sits in the path.
pub(crate) fn vertex_endpoint(project: Option<&str>, location: Option<&str>) -> Option<String> {
    let project = project?;
    let location = location.unwrap_or(VERTEX_DEFAULT_LOCATION);
    let project_ok = project.starts_with(|c: char| c.is_ascii_alphanumeric())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'));
    let location_ok = location
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !project_ok || !location_ok {
        return None;
    }
    let host = if location == VERTEX_DEFAULT_LOCATION {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{location}-aiplatform.googleapis.com")
    };
    Some(format!(
        "https://{host}/v1/projects/{project}/locations/{location}/endpoints/openapi"
    ))
}

/// A string value with surrounding space removed, or `None` when nothing is left.
fn string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A whole number above zero, where the value is one.
fn positive(value: Option<&serde_json::Value>) -> Option<u64> {
    value
        .and_then(serde_json::Value::as_u64)
        .filter(|figure| *figure > 0)
}

/// The `env` array: variable names that may hold the token.
fn names(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(serde_json::Value::Array(names)) = value else {
        return Vec::new();
    };
    names.iter().filter_map(|name| string(Some(name))).collect()
}

/// The `models` block as Bedrock entries, in the order the file listed it.
///
/// The same block the gateway path reads, taken into the shape that backend speaks. `name` is read
/// here and not there because an inference-profile ARN is unreadable and a gateway's model id is
/// not, so this is the one place the friendly name is worth more than the id.
fn bedrock_entries(value: Option<&serde_json::Value>) -> Vec<crate::bedrock::Entry> {
    let Some(serde_json::Value::Object(block)) = value else {
        return Vec::new();
    };
    block
        .iter()
        .map(|(id, entry)| {
            let entry = entry.as_object();
            crate::bedrock::Entry {
                tier: None,
                id: id.to_string(),
                name: entry.and_then(|entry| string(entry.get("name"))),
                context_window: entry.and_then(|entry| window(entry.get("limit"))),
                output_limit: entry.and_then(|entry| ceiling(entry.get("limit"))),
            }
        })
        .collect()
}

/// The names one `headers` block is keyed by.
fn header_names(value: Option<&serde_json::Value>) -> impl Iterator<Item = String> + '_ {
    let block = match value {
        Some(serde_json::Value::Object(block)) => Some(block),
        _ => None,
    };
    block.into_iter().flat_map(|block| block.keys().cloned())
}

/// The `headers` block as headers to send and names refused.
///
/// `credential_name` is the header this provider's credential goes in, refused where the block
/// named somewhere for a credential to live: two things authenticating one request is a request
/// nobody can read the authentication of, and which of them won would depend on the order headers
/// were attached in.
///
/// `fixed_names` is `Provider::fixed_header_names` for this entry's protocol, refused the same
/// way and for the same reason: those are attached before a configured header regardless of a
/// credential, and a block naming one would go out beside it rather than in its place.
fn header_entries(
    value: Option<&serde_json::Value>,
    credential_name: &str,
    names_a_credential: bool,
    fixed_names: &[&str],
) -> (Vec<Header>, Vec<(String, HeaderFault)>) {
    // `content-type` and the framing names are this program's own: a body is encoded here and its
    // length and destination are properties of the request being sent, so a block restating any of
    // them describes a request other than the one that goes.
    const FRAMING: &[&str] = &[
        "host",
        "content-length",
        "content-type",
        "transfer-encoding",
        "connection",
    ];

    let Some(serde_json::Value::Object(block)) = value else {
        return (Vec::new(), Vec::new());
    };
    let mut kept: Vec<Header> = Vec::new();
    let mut refused = Vec::new();
    for (written, value) in block {
        let name = written.trim().to_ascii_lowercase();
        let fault = |fault| Some((written.clone(), fault));
        let entry = if !a_field_name(&name) {
            fault(HeaderFault::NotAName)
        } else if FRAMING.contains(&name.as_str())
            || fixed_names.contains(&name.as_str())
            || (names_a_credential && name == credential_name.to_ascii_lowercase())
            || kept.iter().any(|held| held.name == name)
        {
            fault(HeaderFault::Reserved)
        } else {
            match string(Some(value)) {
                // A name with nothing under it is a line somebody has not finished writing, on the
                // same footing as every other empty value in this file: absence, not a refusal.
                None => None,
                Some(value) if !sendable(&value) => fault(HeaderFault::NotAValue),
                Some(value)
                    if env_reference(&value).is_none()
                        && ["{env:", "{file:"]
                            .iter()
                            .any(|opening| value.contains(opening)) =>
                {
                    fault(HeaderFault::Substitution)
                }
                Some(value) => {
                    kept.push(Header {
                        name,
                        value: Secret::new(&value),
                    });
                    None
                }
            }
        };
        refused.extend(entry);
    }
    (kept, refused)
}

/// The endpoint an entry's `options` state, where they state one.
fn stated_endpoint(options: Option<&serde_json::Map<String, serde_json::Value>>) -> Option<&str> {
    options
        .and_then(|options| options.get("baseURL"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
}

/// One model a `provider` entry with the id [`CHATGPT_ID`] names where it states no endpoint
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// A name and a window and nothing else. A request to the account carries nothing a settings file
/// says ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)), so the model's `options`
/// and its output ceiling have no reader and are not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountModel {
    /// What a request names, and what follows the identifier in a picker row's key.
    pub id: String,
    /// The context window the model's own `limit` states, under the rule a gateway's model follows:
    /// `context` and `output` both have to be there, though the output is read for nothing. The
    /// entry's own `options.contextWindow` is not read.
    ///
    /// `None` reads as [`CONTEXT_WINDOW`], as it does for a gateway's model.
    pub context_window: Option<u64>,
}

/// The models a `provider` entry with the id [`CHATGPT_ID`] names, where it states no endpoint
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// Such an entry is read for its `models` and for nothing else, and it is not a gateway: it names
/// no endpoint, so [`Provider::all`] drops it and the account stays reachable. An entry that states
/// an endpoint is a gateway and keeps the id, so none of its models is the account's. A name is
/// trimmed, as the other strings an entry states are, and one with nothing left is dropped, since a
/// row for it could not be picked. Two names that trim to the same text make one row, because a row
/// is chosen by its key.
pub(crate) fn account_models(
    root: &serde_json::Map<String, serde_json::Value>,
) -> Vec<AccountModel> {
    let Some(serde_json::Value::Object(block)) = root.get("provider") else {
        return Vec::new();
    };
    let Some(serde_json::Value::Object(entry)) = block.get(CHATGPT_ID) else {
        return Vec::new();
    };
    let options = entry.get("options").and_then(serde_json::Value::as_object);
    if stated_endpoint(options).is_some() {
        return Vec::new();
    }
    let Some(serde_json::Value::Object(named)) = entry.get("models") else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    named
        .iter()
        .filter_map(|(id, model)| {
            let id = id.trim();
            (!id.is_empty() && seen.insert(id)).then(|| AccountModel {
                id: id.to_string(),
                context_window: window(model.get("limit")),
            })
        })
        .collect()
}

/// The `models` block, in the order the file listed it.
fn models(value: Option<&serde_json::Value>) -> Vec<Model> {
    let Some(serde_json::Value::Object(block)) = value else {
        return Vec::new();
    };
    block
        .iter()
        .map(|(id, entry)| {
            let entry = entry.as_object();
            Model {
                id: id.to_string(),
                context_window: entry.and_then(|entry| window(entry.get("limit"))),
                output_limit: entry.and_then(|entry| ceiling(entry.get("limit"))),
                options: entry.and_then(|entry| match entry.get("options") {
                    Some(serde_json::Value::Object(options)) if !options.is_empty() => {
                        Some(options.clone())
                    }
                    _ => None,
                }),
            }
        })
        .collect()
}

/// The window a `limit` block states, where it states a usable one.
///
/// opencode requires `context` and `output` together once `limit` is present, so a block naming only
/// one of them is not a `limit` and its figure is not read. Honoured because a copied block that
/// opencode rejects should not be read here as though it said something.
fn window(limit: Option<&serde_json::Value>) -> Option<u64> {
    let serde_json::Value::Object(limit) = limit? else {
        return None;
    };
    let context = limit.get("context")?.as_u64()?;
    limit.get("output")?.as_u64()?;
    (context > 0).then_some(context)
}

/// The reply ceiling a `limit` block states, where it states a usable one.
///
/// Read out of the same block and under the same rule as [`window`]: both halves have to be there
/// for either to be one, so a block opencode would reject says nothing here either. The figure it
/// states is the one it already means in that tool, which is what makes the block copyable.
fn ceiling(limit: Option<&serde_json::Value>) -> Option<u64> {
    let serde_json::Value::Object(limit) = limit? else {
        return None;
    };
    limit.get("context")?.as_u64()?;
    let output = limit.get("output")?.as_u64()?;
    (output > 0).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CLI-7: only the authority's userinfo goes. The regression it rejects is cutting at the last
    /// `@` of the whole string, which would eat a path or query that holds one.
    #[test]
    fn userinfo_is_dropped_from_the_authority_alone() {
        for (stated, shown) in [
            ("https://u:p@host.invalid/v1", "https://host.invalid/v1"),
            ("https://u:p@host.invalid:8443", "https://host.invalid:8443"),
            ("https://u:p@w@host.invalid/v1", "https://host.invalid/v1"),
            ("https://host.invalid/v1/@x", "https://host.invalid/v1/@x"),
            ("https://host.invalid?a=b@c", "https://host.invalid?a=b@c"),
            ("https://u@host.invalid#f", "https://host.invalid#f"),
            ("host.invalid/v1", "host.invalid/v1"),
        ] {
            assert_eq!(without_userinfo(stated), shown, "{stated}");
        }
    }

    fn parsed(text: &str) -> Vec<Provider> {
        let serde_json::Value::Object(root) = serde_json::from_str(text).expect("json") else {
            panic!("not an object");
        };
        Provider::all(&root)
    }

    /// The token a credential carries, for an assertion that has to say which one it is.
    ///
    /// A `Secret` answers no comparison, which is the point of it, so a test says what it wanted
    /// by reading the value out here rather than by putting an operator on the type.
    fn token(credential: &Credential) -> Option<&str> {
        match credential {
            Credential::Token(token) => Some(token.expose()),
            Credential::Absent | Credential::NotNeeded => None,
        }
    }

    fn one(text: &str) -> Provider {
        let mut all = parsed(text);
        assert_eq!(all.len(), 1, "expected exactly one provider");
        all.remove(0)
    }

    const PLACEHOLDER_PROJECT: &str = "example-project-1";

    /// The `google-vertex` entry written as `entry`, which holds the whole block.
    fn one_entry(entry: &str) -> Provider {
        let text = format!(r#"{{"provider": {{"google-vertex": {entry}}}}}"#);
        let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
            panic!("not an object");
        };
        Provider::all(&root).pop().expect("one provider")
    }

    fn vertex(options: &str) -> Vec<Provider> {
        parsed(&format!(
            r#"{{"provider": {{"google-vertex": {{"options": {options}}}}}}}"#
        ))
    }

    /// The default location is `global`, whose host carries no location prefix.
    #[test]
    fn a_google_vertex_entry_is_reached_at_the_host_its_project_and_location_build() {
        let all = vertex(&format!(r#"{{"project": "{PLACEHOLDER_PROJECT}"}}"#));
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].chat_completions_url(),
            "https://aiplatform.googleapis.com/v1/projects/example-project-1/locations/global/endpoints/openapi/chat/completions"
        );
        assert_eq!(all[0].host(), "aiplatform.googleapis.com");
    }

    #[test]
    fn a_google_vertex_entry_in_a_region_is_reached_at_that_regions_host() {
        let all = vertex(&format!(
            r#"{{"project": "{PLACEHOLDER_PROJECT}", "location": "us-central1"}}"#
        ));
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].base_url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/example-project-1/locations/us-central1/endpoints/openapi"
        );
        assert_eq!(all[0].host(), "us-central1-aiplatform.googleapis.com");
    }

    #[test]
    fn a_google_vertex_entry_without_a_project_is_reached_in_express_mode() {
        for options in [
            "{}",
            r#"{"location": "us-central1"}"#,
            r#"{"project": "   "}"#,
        ] {
            let all = vertex(options);
            assert_eq!(all.len(), 1, "{options}");
            assert_eq!(all[0].protocol(), Protocol::GenerateContent, "{options}");
            assert_eq!(
                all[0].base_url, "https://aiplatform.googleapis.com/v1",
                "{options}"
            );
            assert_eq!(all[0].host(), "aiplatform.googleapis.com", "{options}");
        }
        let with_project = vertex(&format!(r#"{{"project": "{PLACEHOLDER_PROJECT}"}}"#));
        assert_eq!(with_project[0].protocol(), Protocol::ChatCompletions);
    }

    #[test]
    fn an_express_request_goes_to_the_publisher_and_model_it_names() {
        let express = vertex("{}");
        assert_eq!(
            express[0].generate_content_url("google/gemini-2.5-flash", false),
            Some(
                "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:generateContent"
                    .to_string()
            )
        );
        assert_eq!(
            express[0].generate_content_url("gemini-2.5-flash", true),
            Some(
                "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
                    .to_string()
            )
        );
    }

    /// The model is written into the path, so a name that could change the route has no URL.
    #[test]
    fn a_model_name_that_would_move_an_express_request_has_no_url() {
        let express = vertex("{}");
        for model in [
            "",
            "google/",
            "/gemini",
            "google/gemini/extra",
            "google/gemini?x=1",
            "google/gemini#frag",
            "google/gem@ini",
            "google/gemini:other",
            "evil.invalid/../x",
            "../gemini",
            "./gemini",
            "google/..",
            "google/.",
            "google/ge mini",
        ] {
            assert_eq!(
                express[0].generate_content_url(model, false),
                None,
                "{model:?}"
            );
        }
    }

    /// An entry stating an endpoint and no project keeps the route it had unless it names the SDK
    /// that says the endpoint takes Vertex's own API.
    #[test]
    fn a_stated_endpoint_with_no_project_keeps_chat_completions_unless_the_sdk_says_otherwise() {
        let plain = vertex(r#"{"baseURL": "https://proxy.example.invalid/v1"}"#);
        assert_eq!(plain[0].protocol(), Protocol::ChatCompletions);
        assert_eq!(
            plain[0].completion_url(),
            "https://proxy.example.invalid/v1/chat/completions"
        );
        let other_sdk = one_entry(
            r#"{"npm": "@ai-sdk/openai-compatible",
                "options": {"baseURL": "https://proxy.example.invalid/v1"}}"#,
        );
        assert_eq!(other_sdk.protocol(), Protocol::ChatCompletions);
    }

    #[test]
    fn a_stated_endpoint_is_where_an_express_entry_is_reached() {
        let all = [one_entry(
            r#"{"npm": "@ai-sdk/google-vertex",
                "options": {"baseURL": "https://proxy.example.invalid/v1/"}}"#,
        )];
        assert_eq!(all[0].protocol(), Protocol::GenerateContent);
        assert_eq!(
            all[0].generate_content_url("google/gemini-2.5-flash", false),
            Some(
                "https://proxy.example.invalid/v1/publishers/google/models/gemini-2.5-flash:generateContent"
                    .to_string()
            )
        );
    }

    /// Both values are written into the URL, so one that could change the host or the route
    /// would carry the key somewhere the person did not name.
    #[test]
    fn a_project_or_location_that_would_move_the_request_configures_nothing() {
        for project in [
            "p/../other",
            "p@evil.invalid",
            "p?x=1",
            "p#frag",
            "-leading",
            "has space",
            "p\\q",
        ] {
            let options = serde_json::json!({"project": project}).to_string();
            assert!(
                vertex(&options).is_empty(),
                "project {project:?} was accepted"
            );
        }
        for location in [
            "evil.invalid/",
            "x@y",
            "US-CENTRAL1",
            "a.b",
            "us central",
            "a/b",
            "a:443",
        ] {
            let options = serde_json::json!({"project": PLACEHOLDER_PROJECT, "location": location})
                .to_string();
            assert!(
                vertex(&options).is_empty(),
                "location {location:?} was accepted"
            );
        }
    }

    /// The compiled list is exactly the four ids the `global` endpoint answered, and a preview is
    /// not among them. Compared with literals rather than with the constant, which would pass
    /// whatever the constant held.
    #[test]
    fn the_compiled_vertex_list_is_the_four_answered_ids_and_no_preview() {
        let all = vertex(&format!(r#"{{"project": "{PLACEHOLDER_PROJECT}"}}"#));
        let roster = all[0].compiled_roster().expect("offered");
        assert_eq!(
            roster,
            [
                "google/gemini-3.1-flash-lite",
                "google/gemini-2.5-pro",
                "google/gemini-2.5-flash",
                "google/gemini-2.5-flash-lite",
            ]
        );
        assert!(
            roster.iter().all(|id| !id.contains("preview")),
            "a preview model was compiled in: {roster:?}"
        );
    }

    /// Every character Google allows is accepted, so a check tightened past the clause does not
    /// quietly configure nothing for a real project.
    #[test]
    fn a_project_and_location_holding_only_allowed_characters_are_accepted() {
        let options =
            serde_json::json!({"project": "a_b.c:d-9", "location": "us-central1"}).to_string();
        let all = vertex(&options);
        assert_eq!(all.len(), 1, "an allowed project or location was refused");
        assert_eq!(
            all[0].base_url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/a_b.c:d-9/locations/us-central1/endpoints/openapi"
        );
        let digit_first = serde_json::json!({"project": "9project"}).to_string();
        assert_eq!(
            vertex(&digit_first).len(),
            1,
            "a project starting with a digit was refused"
        );
    }

    #[test]
    fn a_stated_endpoint_beats_the_google_vertex_host() {
        let all = vertex(&format!(
            r#"{{"project": "{PLACEHOLDER_PROJECT}", "baseURL": "https://proxy.example.invalid/v1"}}"#
        ));
        assert_eq!(all[0].base_url, "https://proxy.example.invalid/v1");
        let no_project = vertex(r#"{"baseURL": "https://proxy.example.invalid/v1"}"#);
        assert_eq!(no_project[0].base_url, "https://proxy.example.invalid/v1");
    }

    #[test]
    fn a_google_vertex_key_is_sent_in_its_own_header() {
        let all = vertex(&format!(r#"{{"project": "{PLACEHOLDER_PROJECT}"}}"#));
        assert_eq!(
            all[0].credential_header("a-key"),
            ("x-goog-api-key", "a-key".to_string())
        );
        let other =
            one(r#"{"provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1"}}}}"#);
        assert_eq!(
            other.credential_header("a-token"),
            ("authorization", "Bearer a-token".to_string())
        );
        assert_eq!(all[0].compiled_roster(), Some(VERTEX_MODELS));
        assert_eq!(other.compiled_roster(), None);
        assert!(all[0].takes_extra_content());
        assert!(!other.takes_extra_content());
    }

    /// The point of the block: a `provider` entry copied out of `opencode.json` configures this
    /// agent without being rewritten first.
    #[test]
    fn a_provider_block_is_read() {
        let provider = one(r#"{"provider": {"openrouter": {
                "name": "OpenRouter",
                "env": ["OPENROUTER_API_KEY"],
                "options": {"baseURL": "https://openrouter.ai/api/v1"},
                "models": {"z-ai/glm-4.6": {}}
            }}}"#);
        assert_eq!(provider.id, "openrouter");
        assert_eq!(provider.display_name(), "OpenRouter");
        assert_eq!(provider.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(provider.env, ["OPENROUTER_API_KEY"]);
        assert!(provider.offers("z-ai/glm-4.6"));
    }

    /// The block that configures this account in the other tool, copied across unedited. Its models
    /// are keyed by inference-profile ARN, and the credential is the AWS chain rather than a token,
    /// so nothing here names one.
    #[test]
    fn an_aws_block_configures_bedrock_rather_than_a_gateway() {
        let provider = one(r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2", "profile": "claude-code-bedrock-sso"},
                "models": {
                    "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc": {
                        "name": "GPT-5.6 Sol (Bedrock)",
                        "limit": {"context": 1050000, "output": 128000}
                    }
                }
            }}}"#);

        let bedrock = provider.bedrock.as_ref().expect("an AWS account");
        assert_eq!(bedrock.region, "us-west-2");
        assert_eq!(bedrock.profile.as_deref(), Some("claude-code-bedrock-sso"));
        assert_eq!(
            provider.base_url,
            "https://bedrock-runtime.us-west-2.amazonaws.com"
        );
        assert!(
            provider.env.is_empty() && provider.api_key.is_none(),
            "an AWS entry named a bearer token"
        );

        let entry = bedrock
            .entry("arn:aws:bedrock:us-west-2:1:application-inference-profile/abc")
            .expect("the model the block named");
        assert_eq!(entry.tier, None, "a block names a model, not a tier");
        assert_eq!(entry.display_name(), "GPT-5.6 Sol (Bedrock)");
        assert_eq!(entry.window(), 1_050_000);
        assert_eq!(entry.output(), 128_000);
    }

    /// An ARN is not a name anybody reads, so a block that named nothing friendlier leaves the id
    /// standing rather than inventing a word for it.
    #[test]
    fn an_aws_model_the_block_did_not_name_is_shown_by_its_id() {
        let provider = one(r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2"},
                "models": {"openai.gpt-5.6-sol": {}}
            }}}"#);
        let bedrock = provider.bedrock.as_ref().expect("an AWS account");
        assert_eq!(bedrock.profile, None);
        let entry = bedrock.entry("openai.gpt-5.6-sol").expect("the model");
        assert_eq!(entry.display_name(), "openai.gpt-5.6-sol");
        assert_eq!(
            entry.window(),
            super::CONTEXT_WINDOW,
            "a window nobody stated was guessed at"
        );
        assert_eq!(
            entry.output(),
            crate::bedrock::OUTPUT_LIMIT,
            "a ceiling nobody stated was guessed at"
        );
    }

    /// The figure the block already carries and this used to throw away. A ceiling is a property
    /// of the model, and one compiled-in number for every model Bedrock fronts cuts a reply off
    /// far below what most of them allow.
    #[test]
    fn a_stated_reply_ceiling_is_read_per_model() {
        let provider = one(r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2"},
                "models": {
                    "anthropic.claude-sonnet-4-5": {"limit": {"context": 200000, "output": 64000}},
                    "anthropic.claude-opus-4-1": {"limit": {"context": 200000, "output": 32000}},
                    "openai.gpt-5.6-sol": {}
                }
            }}}"#);
        let bedrock = provider.bedrock.as_ref().expect("an AWS account");
        // Two stated figures rather than one, and neither equal to the other or to the assumed
        // value: a reading that took any single number for every model passes with one.
        assert_eq!(bedrock.output_limit("anthropic.claude-sonnet-4-5"), 64_000);
        assert_eq!(bedrock.output_limit("anthropic.claude-opus-4-1"), 32_000);
        assert_eq!(
            bedrock.output_limit("openai.gpt-5.6-sol"),
            crate::bedrock::OUTPUT_LIMIT,
            "a model that stated no ceiling took another model's"
        );
    }

    /// The region is the host and what a signature is bound to, so an entry without one names no
    /// service. Guessing a region produces requests that fail somewhere far from the mistake, which
    /// is the same reason the tier variables refuse a block that omits it.
    #[test]
    fn an_aws_block_without_a_region_configures_nothing() {
        let root: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"provider": {"amazon-bedrock": {"models": {"openai.gpt-5.6-sol": {}}}}}"#,
        )
        .expect("parses");
        assert!(Provider::all(&root).is_empty());
    }

    /// The block's region is written into the host the entry is reached at, so one that would move
    /// the request names no service, and the entry is dropped with it rather than reached
    /// somewhere else.
    #[test]
    fn an_aws_block_whose_region_would_move_the_request_configures_nothing() {
        for &region in crate::testutil::REGIONS_THAT_WOULD_MOVE_THE_REQUEST {
            let root = serde_json::json!({"provider": {"amazon-bedrock": {
                "options": {"region": region},
                "models": {"openai.gpt-5.6-sol": {}}
            }}});
            let serde_json::Value::Object(root) = root else {
                unreachable!("a json object literal")
            };
            assert!(
                Provider::all(&root).is_empty(),
                "region {region:?} was accepted"
            );
        }
    }

    /// opencode requires nothing of a model entry, so neither may this. An empty entry is a legal
    /// opencode model and has to stay a legal one here, or a copied block stops working.
    #[test]
    fn a_model_entry_may_be_empty() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1"},
                "models": {"some/model": {}}
            }}}"#);
        let model = provider.model("some/model").expect("offered");
        assert_eq!(model.context_window, None);
        assert_eq!(model.options, None);
    }

    /// A window nobody stated is the conservative default rather than a refusal. Requiring the
    /// number asks for one the person does not have, and a guess typed to satisfy a requirement
    /// looks authoritative where a default does not.
    #[test]
    fn a_model_without_a_stated_window_gets_the_assumed_one() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1"},
                "models": {"some/model": {}}
            }}}"#);
        assert_eq!(
            provider.window_of(provider.model("some/model").expect("offered")),
            CONTEXT_WINDOW
        );
    }

    /// IMPORT-5: a window the block states for the provider is every model's, where the model
    /// states none, and a model's own `limit` outranks it. The regressions rejected are ignoring
    /// the provider figure, and letting it override a model that pinned its own.
    #[test]
    fn a_provider_window_is_the_default_for_models_that_state_none() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1", "contextWindow": 4096},
                "models": {"plain": {}, "pinned": {"limit": {"context": 8192, "output": 1024}}}
            }}}"#);
        assert_eq!(provider.context_window, Some(4096));
        assert_eq!(
            provider.window_of(provider.model("plain").expect("offered")),
            4096
        );
        assert_eq!(
            provider.window_of(provider.model("pinned").expect("offered")),
            8192
        );
    }

    /// A provider window that is not a whole number above zero says nothing, as an unusable model
    /// `limit` does.
    #[test]
    fn an_unusable_provider_window_is_not_read() {
        for stated in ["0", "-5", "\"4096\"", "40.5", "null"] {
            let provider = one(&format!(
                r#"{{"provider": {{"gw": {{"options": {{"baseURL": "https://e.invalid/v1", "contextWindow": {stated}}}}}}}}}"#
            ));
            assert_eq!(provider.context_window, None, "{stated}");
        }
    }

    /// The case `limit` exists for: an upstream pinned to a smaller window than the model's own.
    /// A default above the real window would not delay compaction, it would remove it.
    #[test]
    fn a_stated_window_is_read() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1"},
                "models": {"anthropic/claude-sonnet-4.5": {"limit": {"context": 200000, "output": 64000}}}
            }}}"#);
        let model = provider
            .model("anthropic/claude-sonnet-4.5")
            .expect("offered");
        assert_eq!(model.context_window, Some(200_000));
        assert_eq!(provider.window_of(model), 200_000);
    }

    /// opencode requires `context` and `output` together once `limit` is present. A block naming
    /// only one of them is one opencode rejects, so reading its figure here would honour a shape
    /// the block is supposed to share.
    #[test]
    fn a_limit_missing_either_half_states_no_window() {
        for text in [
            r#"{"provider": {"gw": {"options": {"baseURL": "https://e.invalid/v1"},
                "models": {"m": {"limit": {"context": 200000}}}}}}"#,
            r#"{"provider": {"gw": {"options": {"baseURL": "https://e.invalid/v1"},
                "models": {"m": {"limit": {"output": 64000}}}}}}"#,
            r#"{"provider": {"gw": {"options": {"baseURL": "https://e.invalid/v1"},
                "models": {"m": {"limit": {"context": 0, "output": 64000}}}}}}"#,
            r#"{"provider": {"gw": {"options": {"baseURL": "https://e.invalid/v1"},
                "models": {"m": {"limit": "wide"}}}}}"#,
        ] {
            let provider = one(text);
            assert_eq!(
                provider.model("m").expect("offered").context_window,
                None,
                "{text:?} stated a window"
            );
        }
        // The ceiling comes out of the same block under the same rule, so a half-typed `limit`
        // states neither figure. A zero is absence for the same reason it is for the window: it
        // is not a ceiling a reply could be written under.
        for text in [
            r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2"},
                "models": {"m": {"limit": {"context": 200000}}}}}}"#,
            r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2"},
                "models": {"m": {"limit": {"output": 64000}}}}}}"#,
            r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2"},
                "models": {"m": {"limit": {"context": 200000, "output": 0}}}}}}"#,
            r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2"},
                "models": {"m": {"limit": "wide"}}}}}"#,
        ] {
            let bedrock = one(text).bedrock.expect("an AWS account");
            assert_eq!(
                bedrock.entry("m").expect("offered").output_limit,
                None,
                "{text:?} stated a ceiling"
            );
        }
    }

    /// Reading past a field is what makes a copied block work. opencode's own fields mean nothing
    /// here, and refusing them would defeat the reason the shape was borrowed.
    #[test]
    fn fields_this_crate_does_not_know_are_read_past() {
        let provider = one(r#"{"provider": {"gw": {
                "npm": "@ai-sdk/openai-compatible",
                "api": "https://example.invalid",
                "options": {"baseURL": "https://example.invalid/v1", "timeout": 30000},
                "models": {"some/model": {
                    "name": "Some Model",
                    "family": "glm",
                    "release_date": "2026-01-01",
                    "cost": {"input": 0.1, "output": 0.2},
                    "modalities": {"input": ["text"], "output": ["text"]},
                    "limit": {"context": 131072, "output": 8192}
                }}
            }}}"#);
        let model = provider.model("some/model").expect("offered");
        assert_eq!(model.context_window, Some(131_072));
    }

    /// The two figures a person states are what the clients read, and nothing stated reads as the
    /// figures they have always used, which is how a block written before they existed is read.
    #[test]
    fn a_block_states_how_often_a_request_is_sent_again_and_how_long_a_quiet_reply_is_waited_on() {
        let stated = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1",
                            "maxRetries": 5, "idleTimeoutMs": 300000}
            }}}"#);
        assert_eq!(stated.patience.max_retries, Some(5));
        assert_eq!(
            stated.patience.idle_timeout,
            Some(Duration::from_millis(300_000))
        );
        assert_eq!(stated.patience.attempts(3), 6);

        let silent =
            one(r#"{"provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1"}}}}"#);
        assert_eq!(silent.patience, Patience::default());
        assert_eq!(
            silent.patience.attempts(3),
            3,
            "nothing stated took a figure"
        );
    }

    /// Zero retries sends each request once. Reading it as "nothing stated" would hand a person who
    /// asked for no repeats the default two, and they would find out from a bill.
    #[test]
    fn zero_retries_is_a_statement_and_zero_milliseconds_is_not() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1",
                            "maxRetries": 0, "idleTimeoutMs": 0}
            }}}"#);
        assert_eq!(provider.patience.max_retries, Some(0));
        assert_eq!(provider.patience.attempts(3), 1);
        assert_eq!(
            provider.patience.idle_timeout, None,
            "a gap of nothing would end every reply the moment it began"
        );
    }

    /// A figure above the cap is the cap, as Codex reads its own. Left as written, seven doublings
    /// of a one second pause is the most that stays inside the two minutes a service's own request
    /// to wait is held to, and a hundred is a turn that sits unattended for longer than a person
    /// would wait.
    #[test]
    fn a_figure_above_the_cap_is_the_cap() {
        let provider = one(
            r#"{"provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1",
                "maxRetries": 100, "idleTimeoutMs": 86400000}}}}"#,
        );
        assert_eq!(provider.patience.max_retries, Some(MAX_RETRIES));
        assert_eq!(
            provider.patience.idle_timeout,
            Some(Duration::from_millis(MAX_IDLE_TIMEOUT_MS))
        );

        let at_the_cap = one(&format!(
            r#"{{"provider": {{"gw": {{"options": {{"baseURL": "https://example.invalid/v1",
                "maxRetries": {MAX_RETRIES}, "idleTimeoutMs": {MAX_IDLE_TIMEOUT_MS}}}}}}}}}"#
        ));
        assert_eq!(at_the_cap.patience, provider.patience);

        let below = one(&format!(
            r#"{{"provider": {{"gw": {{"options": {{"baseURL": "https://example.invalid/v1",
                "maxRetries": {}, "idleTimeoutMs": {}}}}}}}}}"#,
            MAX_RETRIES - 1,
            MAX_IDLE_TIMEOUT_MS - 1
        ));
        assert_eq!(below.patience.max_retries, Some(MAX_RETRIES - 1));
        assert_eq!(
            below.patience.idle_timeout,
            Some(Duration::from_millis(MAX_IDLE_TIMEOUT_MS - 1))
        );
    }

    /// A value that is not a whole number states nothing, on the footing `contextWindow` is read:
    /// a mistyped figure costs the setting and leaves the entry reachable.
    #[test]
    fn a_figure_that_is_not_a_whole_number_states_nothing() {
        for spelling in [r#""3""#, "-1", "1.5", "true", "null", "[]", r#"{"n": 2}"#] {
            let provider = one(&format!(
                r#"{{"provider": {{"gw": {{"options": {{"baseURL": "https://example.invalid/v1",
                    "maxRetries": {spelling}, "idleTimeoutMs": {spelling}}}}}}}}}"#
            ));
            assert_eq!(
                provider.patience,
                Patience::default(),
                "{spelling} stated something"
            );
            assert_eq!(provider.base_url, "https://example.invalid/v1");
        }
    }

    /// An AWS account is stated the same two keys, read from the block's `options` beside the
    /// region, and the entry keeps them where a client reaching the account can read them.
    #[test]
    fn an_aws_entry_states_the_same_two_figures() {
        let provider = one(r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2", "maxRetries": 4, "idleTimeoutMs": 240000},
                "models": {"arn:aws:bedrock:us-west-2:111122223333:inference-profile/x": {}}
            }}}"#);
        let account = provider.bedrock.expect("an account");
        assert_eq!(account.patience().max_retries, Some(4));
        assert_eq!(
            account.patience().idle_timeout,
            Some(Duration::from_millis(240_000))
        );

        let silent =
            one(r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2"}}}}"#);
        assert_eq!(
            silent.bedrock.expect("an account").patience(),
            Patience::default()
        );
    }

    /// `options` reaches the request body whole and is never parsed. A gateway's routing controls
    /// are its own invention, so a schema enumerating them is one that changes when it adds a field.
    #[test]
    fn model_options_are_carried_without_being_interpreted() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1"},
                "models": {"m": {"options": {"provider": {"order": ["amazon-bedrock"], "allow_fallbacks": false}}}}
            }}}"#);
        let options = provider
            .model("m")
            .expect("offered")
            .options
            .clone()
            .expect("options");
        assert_eq!(
            options.get("provider").and_then(|it| it.get("order")),
            Some(&serde_json::json!(["amazon-bedrock"]))
        );
    }

    /// With no host there is no request to build, and inventing one produces a failure far from the
    /// mistake. Unlike a window, there is nothing conservative to assume.
    #[test]
    fn a_provider_without_a_base_url_is_not_offered() {
        for text in [
            r#"{"provider": {"gw": {"models": {"m": {}}}}}"#,
            r#"{"provider": {"gw": {"options": {}}}}"#,
            r#"{"provider": {"gw": {"options": {"baseURL": ""}}}}"#,
            r#"{"provider": {"gw": {"options": {"baseURL": "   "}}}}"#,
            r#"{"provider": {"gw": {"options": {"baseURL": 1}}}}"#,
            r#"{"provider": {"gw": {"options": "not a block"}}}"#,
        ] {
            assert!(parsed(text).is_empty(), "{text:?} was offered");
        }
    }

    /// The models of the entry a `chatgpt` block names, whatever else it says.
    fn account_models_of(text: &str) -> Vec<AccountModel> {
        let serde_json::Value::Object(root) = serde_json::from_str(text).expect("json") else {
            panic!("not an object");
        };
        account_models(&root)
    }

    fn named(id: &str, context_window: Option<u64>) -> AccountModel {
        AccountModel {
            id: id.to_string(),
            context_window,
        }
    }

    /// CHATGPT-9: an entry with the account's id that states no endpoint is the account's list of
    /// models and is not a gateway, so a person can pin a short list out of an account that lists
    /// many and the account stays reachable. A window is read by the rule a gateway's model follows,
    /// and nothing else in the entry is: not the entry's own `options`, and not a model's `options`,
    /// which no request to the account carries (CHATGPT-10). A window is the model's own `limit`,
    /// so the entry's `contextWindow` is not one. A name is trimmed so that a stray space does not
    /// make a row the service would refuse, a name with nothing left is dropped, and two names that
    /// trim to the same text are one row, since a row is chosen by its key.
    #[test]
    fn a_chatgpt_entry_without_an_endpoint_names_the_accounts_models_and_is_not_a_gateway() {
        let text = r#"{"provider": {"chatgpt": {
            "npm": "@ai-sdk/openai",
            "options": {"contextWindow": 999, "headers": {"x-extra": "1"}},
            "models": {
                "gpt-z": {"limit": {"context": 400000, "output": 128000}, "options": {"store": true}},
                "gpt-a": {},
                "gpt-half": {"limit": {"context": 200000}},
                "": {},
                "  ": {},
                " gpt-edge ": {"limit": {"context": 300000, "output": 64000}},
                "gpt-a ": {}
            }}}}"#;
        assert!(parsed(text).is_empty(), "the entry was read as a gateway");
        let mut offered = account_models_of(text);
        offered.sort_by(|left, right| left.id.cmp(&right.id));
        assert_eq!(
            offered,
            [
                named("gpt-a", None),
                named("gpt-edge", Some(300_000)),
                named("gpt-half", None),
                named("gpt-z", Some(400_000)),
            ]
        );
    }

    /// CHATGPT-9: an endpoint that is blank states none, as it does for any entry, so the entry is
    /// still the account's list.
    #[test]
    fn a_chatgpt_entry_with_a_blank_endpoint_still_names_the_accounts_models() {
        let text = r#"{"provider": {"chatgpt": {
            "options": {"baseURL": "   "}, "models": {"gpt-a": {}}}}}"#;
        assert!(parsed(text).is_empty());
        assert_eq!(account_models_of(text), [named("gpt-a", None)]);
    }

    /// CHATGPT-9: an entry that states an endpoint is a gateway and keeps the id, so none of its
    /// models is the account's, and an entry with any other id is read for the account never.
    #[test]
    fn only_a_chatgpt_entry_that_states_no_endpoint_names_the_accounts_models() {
        let gateway = r#"{"provider": {"chatgpt": {
            "options": {"baseURL": "https://gateway.example.invalid/v1"},
            "models": {"gpt-a": {}}}}}"#;
        assert_eq!(parsed(gateway).len(), 1, "a gateway keeps its id");
        assert!(account_models_of(gateway).is_empty());

        for text in [
            r#"{"provider": {"openai": {"models": {"gpt-a": {}}}}}"#,
            r#"{"provider": {"chatgpt-mirror": {"models": {"gpt-a": {}}}}}"#,
            r#"{"provider": {"chatgpt": {"models": ["gpt-a"]}}}"#,
            r#"{"provider": {"chatgpt": "gpt-a"}}"#,
            r#"{"provider": {"chatgpt": {}}}"#,
            r#"{"models": {"gpt-a": {}}}"#,
            r#"{}"#,
        ] {
            assert!(account_models_of(text).is_empty(), "{text}");
        }
    }

    /// The case the block is copied for. The other tool resolves an endpoint for a name it knows, so
    /// a block that names one and nothing else is complete there and has to be complete here.
    #[test]
    fn a_known_provider_name_supplies_its_own_endpoint() {
        let provider = one(r#"{"provider": {"openrouter": {
                "options": {"apiKey": "a-token"}
            }}}"#);
        assert_eq!(provider.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(token(&provider.credential(|_| None)), Some("a-token"));
    }

    /// What a caller offers by name has to be what a session reaches by that name, so an id is known
    /// exactly where an endpoint is compiled in for it. The ids that have a reading of their own and
    /// no endpoint are the ones a list built from another table, such as the variables, would add:
    /// Vertex AI builds its endpoint from a project, and neither Bedrock nor the ChatGPT account is a
    /// gateway reached by a name alone.
    #[test]
    fn the_known_ids_are_the_ids_an_endpoint_is_compiled_in_for_and_no_other() {
        let ids: Vec<&str> = known_ids().collect();
        let candidates = [
            "openrouter",
            "openai",
            "anthropic",
            GOOGLE_VERTEX_ID,
            AWS_PROVIDER_ID,
            CHATGPT_ID,
            "work",
            "",
        ];
        let with_an_endpoint: Vec<&str> = candidates
            .into_iter()
            .filter(|id| known_endpoint(id).is_some())
            .collect();
        let listed: Vec<&str> = candidates
            .into_iter()
            .filter(|id| ids.contains(id))
            .collect();
        assert_eq!(listed, with_an_endpoint);
        assert!(!ids.is_empty(), "no id is known");
        for id in &ids {
            assert!(known_endpoint(id).is_some(), "{id} has no endpoint");
        }
        assert!(
            known_variable(GOOGLE_VERTEX_ID).is_some(),
            "the variables name an id the endpoints do not"
        );
    }

    /// A known id asked for by name alone is read as `{"<id>": {}}` is, so the host a picker shows
    /// for it is the host a session sends the key to, whichever protocol the id is spoken in.
    #[test]
    fn a_known_id_asked_for_alone_is_read_as_an_entry_that_names_nothing() {
        for id in known_ids() {
            let alone = Provider::known(id).unwrap_or_else(|| panic!("{id} is not known"));
            let entry = one(&format!(r#"{{"provider": {{"{id}": {{}}}}}}"#));

            assert_eq!(alone.id, id);
            assert_eq!(Some(alone.base_url.as_str()), known_endpoint(id), "{id}");
            assert_eq!(alone.base_url, entry.base_url, "{id}");
            assert_eq!(alone.host(), entry.host(), "{id}");
            assert_eq!(alone.protocol(), entry.protocol(), "{id}");
            assert!(
                alone.env.is_empty() && alone.api_key.is_none() && alone.stored_key.is_none(),
                "{id} names somewhere for a credential to live"
            );
            assert!(alone.models.is_empty(), "{id} lists models");
        }
        assert_eq!(
            Provider::known("openai").map(|provider| provider.host().to_string()),
            Some("api.openai.com".to_string())
        );
    }

    /// An id with no row is not a known provider, though the reading of an empty entry would build
    /// one for some of them: Vertex AI's has an express endpoint, which is not a name's endpoint.
    #[test]
    fn an_id_with_no_row_in_the_table_is_not_a_known_provider() {
        for id in [GOOGLE_VERTEX_ID, AWS_PROVIDER_ID, CHATGPT_ID, "work", ""] {
            assert!(Provider::known(id).is_none(), "{id:?} was taken as known");
        }
        assert!(
            one(r#"{"provider": {"google-vertex": {}}}"#).base_url == VERTEX_EXPRESS_ENDPOINT,
            "the empty Vertex entry builds the express endpoint, so `known` has to check the table first"
        );
    }

    /// A known name pointed somewhere else reaches where it was pointed. Otherwise the table would
    /// override the one field that says, unambiguously, which host to use.
    #[test]
    fn a_stated_endpoint_beats_the_one_compiled_in() {
        let provider = one(r#"{"provider": {"openrouter": {
                "options": {"baseURL": "https://proxy.example.invalid/v1"}
            }}}"#);
        assert_eq!(provider.base_url, "https://proxy.example.invalid/v1");
    }

    /// OpenAI is a gateway like any other once its host is known: the key goes in `authorization`,
    /// nothing is compiled in to offer, and no tool call carries anything back that this program
    /// keeps. A name that changed any of that would be a second code path for a service that
    /// speaks the protocol everything else here speaks. The protocol is the Responses one
    /// ([BACKEND-17]), the one every model OpenAI serves takes with a tool.
    ///
    /// [BACKEND-17]: ../../../docs/specs/backends.md#BACKEND-17
    #[test]
    fn an_openai_entry_is_reached_at_openais_host_like_any_other_gateway() {
        let provider = one(r#"{"provider": {"openai": {"env": ["OPENAI_API_KEY"]}}}"#);
        assert_eq!(provider.base_url, "https://api.openai.com/v1");
        assert_eq!(provider.protocol(), Protocol::Responses);
        assert_eq!(
            provider.completion_url(),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(provider.models_url(), "https://api.openai.com/v1/models");
        assert_eq!(provider.host(), "api.openai.com");
        assert_eq!(
            provider.credential_header("a-key"),
            ("authorization", "Bearer a-key".to_string())
        );
        assert_eq!(provider.compiled_roster(), None);
        assert!(!provider.takes_extra_content());

        let exported = |name: &str| (name == "OPENAI_API_KEY").then(|| "a-key".to_string());
        assert_eq!(token(&provider.credential(exported)), Some("a-key"));
    }

    /// CHATGPT-2 and CHATGPT-10: the account is reached at the serving host in the Responses
    /// protocol, and it names no variable to read a key from, so `OPENAI_API_KEY` and a gateway's
    /// key are never what a request to it carries.
    #[test]
    fn the_chatgpt_account_is_reached_at_the_serving_host_and_is_given_no_key_of_its_own() {
        let account = Provider::chatgpt_account();
        assert_eq!(account.id, "chatgpt");
        assert_eq!(account.display_name(), "ChatGPT");
        assert_eq!(account.protocol(), Protocol::Responses);
        assert_eq!(
            account.completion_url(),
            "https://api.openai.com/v1/responses"
        );
        assert!(account.models.is_empty());
        assert!(account.headers.is_empty());

        let everything = |name: &str| Some(format!("{name}-value"));
        assert!(
            matches!(account.credential(everything), Credential::NotNeeded),
            "a variable was read for the account"
        );
    }

    /// XAI-1 and XAI-15: the account is reached at the serving host in chat completions, and it names
    /// no variable to read a key from, so `XAI_API_KEY` and a gateway's key are never what a request
    /// to it carries.
    #[test]
    fn the_xai_account_is_reached_at_the_serving_host_and_is_given_no_key_of_its_own() {
        let account = Provider::xai_account();
        assert_eq!(account.id, "xai");
        assert_eq!(account.display_name(), "xAI");
        assert_eq!(account.protocol(), Protocol::ChatCompletions);
        assert_eq!(account.host(), "api.x.ai");
        assert_eq!(
            account.completion_url(),
            "https://api.x.ai/v1/chat/completions"
        );
        assert_eq!(account.models_url(), "https://api.x.ai/v1/models");
        assert!(account.models.is_empty());
        assert!(account.headers.is_empty());

        let everything = |name: &str| Some(format!("{name}-value"));
        assert!(
            matches!(account.credential(everything), Credential::NotNeeded),
            "a variable was read for the account"
        );
    }

    /// A proxy, or an Azure-style deployment of the same protocol, is reached by stating where it
    /// is; the host compiled in for the name is only what an absent `baseURL` falls back to.
    #[test]
    fn a_stated_endpoint_beats_the_one_compiled_in_for_openai() {
        let provider = one(r#"{"provider": {"openai": {
                "env": ["OPENAI_API_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/openai/v1"}
            }}}"#);
        assert_eq!(provider.base_url, "https://proxy.example.invalid/openai/v1");
        assert_eq!(provider.host(), "proxy.example.invalid");
    }

    /// BACKEND-19 and BACKEND-32 treat OpenAI's own API differently from any other chat
    /// completions service: its listing leaves out the models only its Responses API takes, and its
    /// requests carry no cache marks. Both follow the host a request goes to. Keying them by the id
    /// `openai` would take the marks from a proxy that reads them and the models from a proxy that
    /// serves them over chat completions, and keying them by a stated `baseURL` alone would miss an
    /// entry that states OpenAI's host.
    #[test]
    fn openais_own_api_follows_the_host_a_request_goes_to_whatever_the_entry_is_called() {
        let own = one(r#"{"provider": {"openai": {
                "npm": "@ai-sdk/openai-compatible",
                "env": ["OPENAI_API_KEY"]
            }}}"#);
        assert_eq!(own.protocol(), Protocol::ChatCompletions);
        assert_eq!(own.host(), "api.openai.com");
        assert!(!own.takes_cache_markers());
        assert_eq!(own.responses_only_models(), RESPONSES_ONLY_MODELS);
        assert!(own.responses_only_models().contains(&"o3-pro"));
        assert!(own.responses_only_models().contains(&"gpt-5-pro"));
        assert!(!own.responses_only_models().contains(&"gpt-4o-mini"));

        let stated = one(r#"{"provider": {"work": {
                "options": {"baseURL": "https://API.OpenAI.com/v1"}
            }}}"#);
        assert!(!stated.takes_cache_markers(), "{}", stated.host());
        assert_eq!(stated.responses_only_models(), RESPONSES_ONLY_MODELS);

        let proxy = one(r#"{"provider": {"openai": {
                "env": ["OPENAI_API_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/openai/v1"}
            }}}"#);
        assert!(proxy.takes_cache_markers());
        assert!(proxy.responses_only_models().is_empty());

        let responses = one(r#"{"provider": {"openai": {
                "npm": "@ai-sdk/openai",
                "env": ["OPENAI_API_KEY"]
            }}}"#);
        assert_eq!(responses.protocol(), Protocol::Responses);
        assert_eq!(responses.host(), "api.openai.com");
        assert!(
            responses.responses_only_models().is_empty(),
            "the Responses route serves those models"
        );

        let bare = one(r#"{"provider": {"openai": {"env": ["OPENAI_API_KEY"]}}}"#);
        assert_eq!(bare.protocol(), Protocol::Responses);
        assert!(!bare.takes_cache_markers());
        assert!(bare.lists_shutdown_dates());
        assert!(
            bare.responses_only_models().is_empty(),
            "an openai entry is reached in the Responses protocol, which serves those models"
        );

        let other = one(r#"{"provider": {"openrouter": {"env": ["OPENROUTER_API_KEY"]}}}"#);
        assert!(other.takes_cache_markers());
        assert!(other.responses_only_models().is_empty());
    }

    /// OpenAI's own API is told from a proxy by the host and port the entry's `baseURL` reaches, so
    /// the spellings of that place are OpenAI's and any other host or port is not. A comparison with
    /// the text `api.openai.com` as the entry spells it takes `:443` and a trailing dot for another
    /// service, and one that drops whatever port is written takes a tunnel on another port for
    /// OpenAI. The scheme gives the default port, so `http` reaches port 80.
    #[test]
    fn the_spellings_of_openais_host_are_its_own_api_and_another_host_or_port_is_not() {
        let at = |url: &str| {
            one(&format!(
                r#"{{"provider": {{"work": {{"options": {{"baseURL": "{url}"}}}}}}}}"#
            ))
        };
        for url in [
            "https://api.openai.com/v1",
            "https://API.OpenAI.com/v1",
            "https://api.openai.com:443/v1",
            "https://api.openai.com./v1",
            "https://API.OPENAI.COM.:443/v1/",
            "https://api.openai.com:/v1",
            "https://api.openai.com",
            "https://api.openai.com/v1?api-version=1",
            "https://user:token@api.openai.com/v1",
        ] {
            let provider = at(url);
            assert!(
                !provider.takes_cache_markers(),
                "{url} was taken for a proxy"
            );
            assert_eq!(
                provider.responses_only_models(),
                RESPONSES_ONLY_MODELS,
                "{url}"
            );
        }
        for url in [
            "https://api.openai.com:8443/v1",
            "https://api.openai.com:444/v1",
            "https://api.openai.com:80/v1",
            "http://api.openai.com/v1",
            "https://api.openai.com:abc/v1",
            "https://api.openai.com:+443/v1",
            "https://api.openai.com.proxy.example.invalid/v1",
            "https://notapi.openai.com/v1",
            "https://proxy.example.invalid/api.openai.com/v1",
            "https://proxy.example.invalid/v1?host=api.openai.com",
        ] {
            let provider = at(url);
            assert!(provider.takes_cache_markers(), "{url} was taken for OpenAI");
            assert!(provider.responses_only_models().is_empty(), "{url}");
        }
    }

    /// The list is the models that take requests through the Responses API alone. It holds each id
    /// OpenAI's model pages mark as unsupported by chat completions and supported by the Responses
    /// API, read on 2026-10-27, beside the ones its OpenAPI description lists. The first loop checks
    /// that the ids the pages added are present. The second is a spot check, not the whole of the
    /// rest: it names ids that must stay offered, models a page marks as taking chat completions
    /// and models that are not Responses-only: `gpt-image-1` is served by the Images endpoints
    /// beside the Responses API, and `sora-2-pro` and `text-embedding-3-small` are served by the
    /// Videos and Embeddings endpoints and not by the Responses API. A list that gained any other
    /// id passes.
    #[test]
    fn the_responses_only_list_holds_the_marked_ids_and_leaves_out_a_sample_that_stays_offered() {
        let own = one(r#"{"provider": {"openai": {
                "npm": "@ai-sdk/openai-compatible",
                "env": ["OPENAI_API_KEY"]
            }}}"#);
        let listed = own.responses_only_models();
        for id in [
            "gpt-5.1-codex",
            "gpt-5.1-codex-mini",
            "gpt-5.2-codex",
            "gpt-5.3-codex",
            "gpt-5.2-pro",
            "gpt-5.2-pro-2025-12-11",
            "gpt-5.4-pro",
            "gpt-5.4-pro-2026-03-05",
        ] {
            assert!(listed.contains(&id), "{id} is Responses-only on its page");
        }
        for id in [
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.2",
            "gpt-5.2-chat-latest",
            "gpt-4o-mini",
            "o3",
            "gpt-image-1",
            "sora-2-pro",
            "text-embedding-3-small",
        ] {
            assert!(!listed.contains(&id), "{id} takes another route");
        }
    }

    /// The list is a set of ids that each name one model once.
    #[test]
    fn the_responses_only_list_names_each_model_once() {
        let mut seen = std::collections::HashSet::new();
        for id in RESPONSES_ONLY_MODELS {
            assert!(
                !id.is_empty() && id.trim() == *id && seen.insert(*id),
                "{id:?}"
            );
        }
    }

    /// Anthropic is the one known name reached in another protocol, so everything that differs
    /// from a gateway is stated by the entry: the route, the header the key is in, and the version
    /// header the service requires beside it.
    #[test]
    fn an_anthropic_entry_is_reached_through_the_messages_api_at_anthropics_host() {
        let provider = one(r#"{"provider": {"anthropic": {"env": ["ANTHROPIC_API_KEY"]}}}"#);
        assert_eq!(provider.base_url, "https://api.anthropic.com/v1");
        assert_eq!(provider.protocol(), Protocol::Messages);
        assert_eq!(
            provider.completion_url(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            provider.models_url(),
            "https://api.anthropic.com/v1/models?limit=1000"
        );
        assert_eq!(provider.host(), "api.anthropic.com");
        assert_eq!(
            provider.credential_header("a-key"),
            ("x-api-key", "a-key".to_string())
        );
        assert_eq!(
            provider.fixed_headers(),
            [("anthropic-version", "2023-06-01")]
        );
        assert_eq!(provider.compiled_roster(), None);
        assert!(!provider.takes_extra_content());
        assert!(!provider.answers_for_an_account());
        assert!(provider.answers_under_the_model_behind_an_alias());

        let exported = |name: &str| (name == "ANTHROPIC_API_KEY").then(|| "a-key".to_string());
        assert_eq!(token(&provider.credential(exported)), Some("a-key"));

        let other =
            one(r#"{"provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1"}}}}"#);
        assert_eq!(other.protocol(), Protocol::ChatCompletions);
        assert_eq!(
            other.completion_url(),
            "https://example.invalid/v1/chat/completions"
        );
        assert_eq!(other.models_url(), "https://example.invalid/v1/models");
        assert!(other.fixed_headers().is_empty());
        assert!(other.answers_for_an_account());
        assert!(!other.answers_under_the_model_behind_an_alias());
    }

    /// A proxy in front of the API is reached by stating where it is, and is still spoken to in the
    /// protocol the id names, since the table supplies only an address.
    #[test]
    fn a_stated_endpoint_beats_the_one_compiled_in_for_anthropic() {
        let provider = one(r#"{"provider": {"anthropic": {
                "env": ["ANTHROPIC_API_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/anthropic/v1"}
            }}}"#);
        assert_eq!(
            provider.base_url,
            "https://proxy.example.invalid/anthropic/v1"
        );
        assert_eq!(provider.host(), "proxy.example.invalid");
        assert_eq!(provider.protocol(), Protocol::Messages);
        assert_eq!(
            provider.completion_url(),
            "https://proxy.example.invalid/anthropic/v1/messages"
        );
    }

    /// The id alone says which protocol an entry is reached in, except that an `anthropic` entry may
    /// name the SDK opencode would use for it. One that names the OpenAI-compatible SDK is a gateway of
    /// that kind which happens to be called this, as every such block was before the id was known: its
    /// requests go to its own chat completions route with a bearer token, and with no address of its
    /// own it has no service, since the address compiled in is the one the other protocol is spoken at.
    #[test]
    fn an_anthropic_entry_naming_the_openai_compatible_sdk_is_a_gateway_like_any_other() {
        let provider = one(r#"{"provider": {"anthropic": {
                "npm": "@ai-sdk/openai-compatible",
                "env": ["A_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/v1"}
            }}}"#);
        assert_eq!(provider.protocol(), Protocol::ChatCompletions);
        assert_eq!(
            provider.completion_url(),
            "https://proxy.example.invalid/v1/chat/completions"
        );
        assert_eq!(
            provider.credential_header("a-key"),
            ("authorization", "Bearer a-key".to_string())
        );
        assert!(provider.fixed_headers().is_empty());
        assert!(provider.answers_for_an_account());
        assert!(!provider.answers_under_the_model_behind_an_alias());

        assert!(
            parsed(r#"{"provider": {"anthropic": {"npm": "@ai-sdk/openai-compatible"}}}"#)
                .is_empty(),
            "an entry that names no address of its own was given Anthropic's"
        );

        for sdk in [
            r#""npm": "@ai-sdk/anthropic""#,
            r#""npm": "something-else""#,
        ] {
            let provider = one(&format!(
                r#"{{"provider": {{"anthropic": {{{sdk}, "env": ["A_KEY"]}}}}}}"#
            ));
            assert_eq!(provider.protocol(), Protocol::Messages, "{sdk}");
        }
    }

    /// A block written while the id was an ordinary name for a gateway, with an endpoint and no SDK,
    /// is read in the protocol the id has now, as opencode reads the same block, and the SDK it names
    /// is the way back. An `@ai-sdk/anthropic` entry under another id is not reached in that protocol:
    /// only the id says so, and the entry is a gateway like any other. Both are costs the spec records,
    /// held here so a change to either shows in a test.
    #[test]
    fn what_decides_the_protocol_of_an_entry_is_its_id_and_the_sdk_it_names() {
        let old_form = one(r#"{"provider": {"anthropic": {
                "env": ["A_KEY"],
                "options": {"baseURL": "https://litellm.example.invalid/v1"},
                "models": {"claude-via-the-gateway": {}}
            }}}"#);
        assert_eq!(old_form.protocol(), Protocol::Messages);

        let opted_out = one(r#"{"provider": {"anthropic": {
                "npm": "@ai-sdk/openai-compatible",
                "env": ["A_KEY"],
                "options": {"baseURL": "https://litellm.example.invalid/v1"}
            }}}"#);
        assert_eq!(opted_out.protocol(), Protocol::ChatCompletions);

        let elsewhere = one(r#"{"provider": {"claude-proxy": {
                "npm": "@ai-sdk/anthropic",
                "env": ["A_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/v1"}
            }}}"#);
        assert_eq!(elsewhere.protocol(), Protocol::ChatCompletions);
    }

    /// BACKEND-61: an entry naming `@ai-sdk/openai` is reached in the Responses protocol, whatever its
    /// id, at the `/responses` route, with its key as a bearer token and no header of the protocol's
    /// own. The SDK it names decides over the id, so an `anthropic` entry naming it is Responses and
    /// must say where its service is, since the address compiled in is the Messages API's.
    #[test]
    fn an_entry_naming_the_openai_sdk_is_reached_through_the_responses_api() {
        let own_id = one(r#"{"provider": {"a-gateway": {
                "npm": "@ai-sdk/openai",
                "env": ["A_KEY"],
                "options": {"baseURL": "https://gateway.example.invalid/v1/"}
            }}}"#);
        assert_eq!(own_id.protocol(), Protocol::Responses);
        assert_eq!(
            own_id.completion_url(),
            "https://gateway.example.invalid/v1/responses"
        );
        assert_eq!(
            own_id.models_url(),
            "https://gateway.example.invalid/v1/models"
        );
        assert!(own_id.fixed_headers().is_empty());
        assert_eq!(
            own_id.credential_header("secret"),
            ("authorization", "Bearer secret".to_string())
        );
        // A gateway that takes this protocol is asked what an account may reach as any other is.
        assert!(own_id.answers_for_an_account());

        let known = one(r#"{"provider": {"openai": {"npm": "@ai-sdk/openai", "env": ["A_KEY"]}}}"#);
        assert_eq!(known.protocol(), Protocol::Responses);
        assert_eq!(
            known.completion_url(),
            "https://api.openai.com/v1/responses"
        );

        let plain = one(r#"{"provider": {"openrouter": {"env": ["A_KEY"]}}}"#);
        assert_eq!(plain.protocol(), Protocol::ChatCompletions);
        assert_eq!(
            plain.completion_url(),
            "https://openrouter.ai/api/v1/chat/completions"
        );

        let stated = one(r#"{"provider": {"anthropic": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": "https://proxy.example.invalid/v1"}
            }}}"#);
        assert_eq!(stated.protocol(), Protocol::Responses);
        assert!(
            parsed(r#"{"provider": {"anthropic": {"npm": "@ai-sdk/openai"}}}"#).is_empty(),
            "the address compiled in for `anthropic` is not one this protocol is spoken at"
        );

        // An id that fixes its own protocol and the header its key goes in keeps both.
        let vertex = one(r#"{"provider": {"google-vertex": {
                "npm": "@ai-sdk/openai",
                "options": {"project": "an-example-project", "apiKey": "a-key"}
            }}}"#);
        assert_eq!(vertex.protocol(), Protocol::ChatCompletions);
    }

    /// BACKEND-17 and BACKEND-61: an `openai` entry is reached in the Responses protocol where it is
    /// reached at OpenAI's own API. On its chat completions route OpenAI refuses a request that
    /// offers a function tool to `gpt-6-sol` or `gpt-6.1-sol`, and every request from this program
    /// offers tools. A proxy keyed `openai` may speak chat completions alone, so
    /// it keeps that route, as does an entry that names the OpenAI-compatible SDK, which is how a
    /// person asks for chat completions at OpenAI's own API. An entry under another id keeps
    /// chat completions whatever endpoint it states, so a block written for another service is not
    /// moved to a route its service may not have.
    #[test]
    fn an_openai_entry_is_reached_in_the_responses_protocol_at_openais_own_api_only() {
        let at = |block: &str| one(&format!(r#"{{"provider": {block}}}"#)).protocol();

        for (block, expected) in [
            (r#"{"openai": {"env": ["A_KEY"]}}"#, Protocol::Responses),
            (r#"{"openai": {}}"#, Protocol::Responses),
            (
                r#"{"openai": {"options": {"baseURL": "https://api.openai.com/v1"}}}"#,
                Protocol::Responses,
            ),
            (
                r#"{"openai": {"options": {"baseURL": "HTTPS://API.OpenAI.com.:443/v1/"}}}"#,
                Protocol::Responses,
            ),
            (
                r#"{"openai": {"npm": "@ai-sdk/openai", "env": ["A_KEY"]}}"#,
                Protocol::Responses,
            ),
            (
                r#"{"openai": {"npm": "an-sdk-nobody-reads", "env": ["A_KEY"]}}"#,
                Protocol::Responses,
            ),
            (
                r#"{"openai": {"npm": "@ai-sdk/openai-compatible", "env": ["A_KEY"]}}"#,
                Protocol::ChatCompletions,
            ),
            (
                r#"{"openai": {"options": {"baseURL": "https://proxy.example.invalid/v1"}}}"#,
                Protocol::ChatCompletions,
            ),
            (
                r#"{"openai": {"options": {"baseURL": "https://api.openai.com:8443/v1"}}}"#,
                Protocol::ChatCompletions,
            ),
            (
                r#"{"openai": {"options": {"baseURL": "http://localhost:11434/v1"}}}"#,
                Protocol::ChatCompletions,
            ),
            (
                r#"{"openai": {"npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://proxy.example.invalid/v1"}}}"#,
                Protocol::ChatCompletions,
            ),
            (
                r#"{"work": {"options": {"baseURL": "https://api.openai.com/v1"}}}"#,
                Protocol::ChatCompletions,
            ),
        ] {
            assert_eq!(at(block), expected, "{block}");
        }

        // The entry a session reaches for the name alone, which is what a stored key is sent to.
        assert_eq!(
            Provider::known("openai").map(|provider| provider.protocol()),
            Some(Protocol::Responses)
        );
        // The other known names keep their own.
        assert_eq!(at(r#"{"openrouter": {}}"#), Protocol::ChatCompletions);
        assert_eq!(at(r#"{"anthropic": {}}"#), Protocol::Messages);
    }

    /// The table supplies an address and nothing else. A block naming no credential does not have an
    /// exported `ANTHROPIC_API_KEY` meant for another tool picked up on the strength of the id: its
    /// key is missing, as BACKEND-16 says of a known id at the host compiled in for it.
    #[test]
    fn an_anthropic_block_naming_no_key_names_no_credential() {
        let provider = one(r#"{"provider": {"anthropic": {}}}"#);
        let exported = |name: &str| (name == "ANTHROPIC_API_KEY").then(|| "a-key".to_string());
        assert!(matches!(provider.credential(exported), Credential::Absent));
    }

    /// An exported budget outranks a model's own, which outranks the figure a request assumes. The
    /// same order the AWS account resolves its ceiling in, so one setting means one thing.
    #[test]
    fn an_anthropic_reply_ceiling_is_the_exported_budget_then_the_models_own() {
        let provider = one(r#"{"provider": {"anthropic": {"models": {
                "claude-stated": {"limit": {"context": 200000, "output": 64000}},
                "claude-half": {"limit": {"output": 64000}},
                "claude-silent": {}
            }}}}"#);
        assert_eq!(provider.stated_output_limit("claude-stated"), Some(64_000));
        // A `limit` naming only one half is not a `limit`, as opencode reads it.
        assert_eq!(provider.stated_output_limit("claude-half"), None);
        assert_eq!(provider.stated_output_limit("claude-silent"), None);
        assert_eq!(provider.stated_output_limit("claude-unlisted"), None);

        let exported = provider.with_output_budget(Some(48_000));
        for model in ["claude-stated", "claude-half", "claude-unlisted"] {
            assert_eq!(exported.stated_output_limit(model), Some(48_000), "{model}");
        }
    }

    /// A trailing slash on the endpoint would otherwise produce a double slash in the path, which
    /// some gateways route differently and others reject.
    #[test]
    fn a_trailing_slash_on_the_endpoint_is_dropped() {
        let provider =
            one(r#"{"provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1/"}}}}"#);
        assert_eq!(
            provider.chat_completions_url(),
            "https://example.invalid/v1/chat/completions"
        );
    }

    /// CRED-25: the host is what would end a token this block carries, and it is written into a
    /// diagnostic people paste into issues. The scheme and the path say nothing about which
    /// service revokes it, and userinfo is the one part of a URL that can be a credential, so a
    /// record built by naming the endpoint would carry a second secret out of the same block that
    /// carried the first.
    #[test]
    fn the_host_a_token_would_be_ended_at_carries_no_other_part_of_the_endpoint() {
        for (stated, host) in [
            ("https://gateway.invalid/v1", "gateway.invalid"),
            ("http://localhost:11434/v1", "localhost:11434"),
            (
                "https://sk-live-secret@gateway.invalid/v1",
                "gateway.invalid",
            ),
            (
                "https://user:sk-live-secret@gateway.invalid",
                "gateway.invalid",
            ),
        ] {
            let provider = one(&format!(
                r#"{{"provider": {{"gw": {{"options": {{"baseURL": "{stated}"}}}}}}}}"#
            ));
            assert_eq!(provider.host(), host, "the host read from {stated}");
        }
    }

    /// A variable is preferred to a value in the file, so that naming one does not have the value
    /// read out from under it by an `apiKey` left behind.
    #[test]
    fn a_named_variable_holds_the_token_before_the_file_does() {
        let provider = one(r#"{"provider": {"gw": {
                "env": ["ABSENT_ONE", "PRESENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "in-the-file"}
            }}}"#);
        let credential = provider.credential(|name| match name {
            "PRESENT_ONE" => Some("from-the-environment".to_string()),
            _ => None,
        });
        assert_eq!(token(&credential), Some("from-the-environment"));
    }

    /// BACKEND-16: a variable the block names is read first, a stored key before a value written
    /// into the block, and the place each was found is what is reported, never the value.
    #[test]
    fn a_stored_key_is_read_after_a_variable_and_before_the_file() {
        let mut provider = one(r#"{"provider": {"gw": {
                "env": ["PRESENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "in-the-file"}
            }}}"#);
        provider.stored_key = Some(Secret::new("stored-placeholder"));
        let exported = |name: &str| (name == "PRESENT_ONE").then(|| "from-the-environment".into());

        let (credential, source) = provider.credential_and_source(exported);
        assert_eq!(token(&credential), Some("from-the-environment"));
        assert_eq!(source, Some(Source::Variable));

        let (credential, source) = provider.credential_and_source(|_| None);
        assert_eq!(token(&credential), Some("stored-placeholder"));
        assert_eq!(source, Some(Source::Stored));

        provider.stored_key = None;
        let (credential, source) = provider.credential_and_source(|_| None);
        assert_eq!(token(&credential), Some("in-the-file"));
        assert_eq!(source, Some(Source::File));
    }

    /// BACKEND-16: a block naming nowhere for a token wants none until a key is stored for its id,
    /// which is somebody saying it does.
    #[test]
    fn a_stored_key_is_sent_to_a_block_naming_no_credential() {
        let mut provider = one(r#"{"provider": {"ollama": {
                "options": {"baseURL": "http://localhost:11434/v1"}
            }}}"#);
        provider.stored_key = Some(Secret::new("stored-placeholder"));
        assert_eq!(
            token(&provider.credential(|_| None)),
            Some("stored-placeholder")
        );
    }

    /// Supported because it is opencode's field. A copied block that authenticates this way has to
    /// keep working, whatever this file recommends instead.
    #[test]
    fn a_token_written_into_the_file_is_still_read() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "in-the-file"}
            }}}"#);
        assert_eq!(token(&provider.credential(|_| None)), Some("in-the-file"));
    }

    /// A block reaches a person's screen whenever a diagnostic prints one or an assertion about
    /// one fails, and a token somebody wrote into their settings file is a live credential. It
    /// went out in plain text while this field was a `String`, since the derived `Debug` prints
    /// whatever a field holds.
    #[test]
    fn printing_a_block_does_not_print_the_token_written_into_it() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "in-the-file"}
            }}}"#);

        let printed = format!("{provider:?}");

        assert!(
            !printed.contains("in-the-file"),
            "the token is in what was printed: {printed}"
        );
        assert!(
            printed.contains("<redacted>"),
            "the field is not there at all, so nothing says a token was withheld: {printed}"
        );
    }

    /// A request this configuration cannot sign is reported rather than sent unauthenticated.
    #[test]
    fn a_provider_with_nothing_holding_a_token_has_none() {
        let provider = one(r#"{"provider": {"gw": {
                "env": ["ABSENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1"}
            }}}"#);
        assert!(matches!(provider.credential(|_| None), Credential::Absent));
        assert!(matches!(
            provider.credential(|_| Some("   ".to_string())),
            Credential::Absent
        ));
    }

    /// The block a local Ollama is configured with in the tool this shape is borrowed from: an
    /// endpoint and nothing else. Read as a token that is missing, every caller refuses a gateway
    /// that wants no token, and the copied block reaches the model picker and then cannot answer.
    ///
    /// Distinct from the case above, which names a variable and is still refused: a block saying
    /// where a credential lives and finding nothing there is a stale or missing token.
    #[test]
    fn a_provider_naming_no_credential_needs_none() {
        let provider = one(r#"{"provider": {"ollama": {
                "name": "Ollama (local)",
                "options": {"baseURL": "http://localhost:11434/v1"}
            }}}"#);
        assert!(matches!(
            provider.credential(|_| None),
            Credential::NotNeeded
        ));
        // Nothing is named, so nothing in the environment is consulted to decide it either.
        assert!(matches!(
            provider.credential(|_| Some("in-the-environment".to_string())),
            Credential::NotNeeded
        ));
    }

    /// BACKEND-16: a block of a known id that states no endpoint is reached at the host compiled in
    /// for it, which refuses a request with no key, so naming no credential there is one missing. A
    /// block stating its own endpoint under the same id keeps the rule, and a stored key is a key.
    #[test]
    fn a_known_id_at_its_compiled_in_host_naming_no_credential_is_missing_one() {
        for id in ["openai", "openrouter", "anthropic"] {
            let bare = one(&format!(r#"{{"provider": {{"{id}": {{}}}}}}"#));
            assert!(
                matches!(bare.credential(|_| None), Credential::Absent),
                "{id}: a block naming no credential at the compiled-in host was read as needing none"
            );
            let mut stored = bare;
            stored.stored_key = Some(Secret::new("stored-placeholder"));
            assert_eq!(
                token(&stored.credential(|_| None)),
                Some("stored-placeholder"),
                "{id}"
            );
        }
        // A header carrying the credential is how that block authenticates.
        let by_header = one(r#"{"provider": {"openai": {
                "options": {"headers": {"Authorization": "Bearer a-placeholder"}}
            }}}"#);
        assert!(matches!(
            by_header.credential(|_| None),
            Credential::NotNeeded
        ));
        // An anthropic block written before this rule with its own bearer header keeps working.
        let bearer = one(r#"{"provider": {"anthropic": {
                "options": {"headers": {"Authorization": "Bearer a-placeholder"}}
            }}}"#);
        assert!(matches!(bearer.credential(|_| None), Credential::NotNeeded));
        let local = one(r#"{"provider": {"openai": {
                "options": {"baseURL": "http://localhost:8080/v1"}
            }}}"#);
        assert!(matches!(local.credential(|_| None), Credential::NotNeeded));
        // Another id at the same host is not a known id.
        let other = one(r#"{"provider": {"mine": {
                "options": {"baseURL": "https://api.openai.com/v1"}
            }}}"#);
        assert!(matches!(other.credential(|_| None), Credential::NotNeeded));
    }

    /// Two gateways are ordinary. The block being a map is what makes a duplicate id
    /// unrepresentable, rather than a rule something has to remember to enforce.
    #[test]
    fn more_than_one_provider_may_be_configured() {
        let all = parsed(
            r#"{"provider": {
                "openrouter": {"options": {"baseURL": "https://openrouter.ai/api/v1"}},
                "together": {"options": {"baseURL": "https://api.together.xyz/v1"}}
            }}"#,
        );
        let ids: Vec<&str> = all.iter().map(|it| it.id.as_str()).collect();
        assert_eq!(ids, ["openrouter", "together"]);
    }

    /// Where the block said nothing friendlier, the id is what a picker row names it by: a term
    /// that cannot collide with a name a service chose for itself.
    #[test]
    fn a_provider_with_no_name_is_shown_by_its_id() {
        let provider = one(
            r#"{"provider": {"openrouter": {"options": {"baseURL": "https://e.invalid/v1"}}}}"#,
        );
        assert_eq!(provider.display_name(), "openrouter");
    }

    /// A provider that turns nothing on is still a provider, and "no models configured" is a better
    /// thing to report than a guessed roster. A gateway's is too large and too fluid to enumerate.
    #[test]
    fn a_provider_may_offer_no_models() {
        let provider =
            one(r#"{"provider": {"gw": {"options": {"baseURL": "https://e.invalid/v1"}}}}"#);
        assert!(provider.models.is_empty());
        assert!(!provider.offers("anything"));
    }

    /// Every failure reads as absence, on the same footing as the `env` block: a half-typed
    /// settings file must not stop a session.
    #[test]
    fn a_malformed_provider_block_configures_nothing() {
        for text in [
            r#"{}"#,
            r#"{"provider": {}}"#,
            r#"{"provider": "not a block"}"#,
            r#"{"provider": []}"#,
            r#"{"provider": {"gw": "not a block"}}"#,
            r#"{"provider": {"gw": []}}"#,
            r#"{"providers": {"gw": {"options": {"baseURL": "https://e.invalid/v1"}}}}"#,
        ] {
            assert!(parsed(text).is_empty(), "{text:?} configured something");
        }
    }

    /// The case the block exists for: a gateway that routes on a header of its own. Both spellings
    /// of one name are asserted, because a reading that kept the name as written would send two
    /// headers where the person wrote one.
    #[test]
    fn a_header_a_block_asked_for_is_read() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"X-Tenant-Id": "team-7", "x-route": "eu"}
                }
            }}}"#);
        let mut seen: Vec<(String, String)> = provider
            .request_headers(|_| None)
            .into_iter()
            .map(|(name, value)| (name, value.expose().to_string()))
            .collect();
        seen.sort();
        assert_eq!(
            seen,
            [
                ("x-route".to_string(), "eu".to_string()),
                ("x-tenant-id".to_string(), "team-7".to_string())
            ]
        );
        assert!(
            provider.headers_refused.is_empty(),
            "a header nobody objected to was refused: {:?}",
            provider.headers_refused
        );
    }

    /// `Request::header` appends, so a name this program already set would go twice and an
    /// intermediary would choose between them. The name is reported as the file wrote it, because
    /// a person looking for the line they typed is looking for their own spelling.
    #[test]
    fn a_header_that_would_change_framing_is_refused() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {
                        "Host": "elsewhere.invalid",
                        "content-length": "3",
                        "Content-Type": "text/plain",
                        "transfer-encoding": "chunked",
                        "connection": "close",
                        "x-route": "eu"
                    }
                }
            }}}"#);
        let kept: Vec<(String, String)> = provider
            .request_headers(|_| None)
            .into_iter()
            .map(|(name, value)| (name, value.expose().to_string()))
            .collect();
        assert_eq!(
            kept,
            [("x-route".to_string(), "eu".to_string())],
            "a reserved name was sent"
        );
        let mut refused: Vec<(String, HeaderFault)> = provider.headers_refused.clone();
        refused.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            refused,
            [
                ("Content-Type".to_string(), HeaderFault::Reserved),
                ("Host".to_string(), HeaderFault::Reserved),
                ("connection".to_string(), HeaderFault::Reserved),
                ("content-length".to_string(), HeaderFault::Reserved),
                ("transfer-encoding".to_string(), HeaderFault::Reserved)
            ],
            "a refused name was not reported as the file wrote it, or was refused for another reason"
        );
    }

    /// One name written twice in two spellings is one header. `Request::header` appends, so
    /// keeping both would send two values and let the service pick, which is not a thing the
    /// person asked for in either line.
    #[test]
    fn one_name_written_twice_sends_one_header() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"x-route": "eu", "X-Route": "us"}
                }
            }}}"#);
        let sent = provider.request_headers(|_| None);
        assert_eq!(sent.len(), 1, "one name produced two headers: {sent:?}");
        assert_eq!(sent[0].0, "x-route");
        assert_eq!(
            provider.headers_refused.len(),
            1,
            "the spelling that lost was not reported: {:?}",
            provider.headers_refused
        );
        assert_eq!(provider.headers_refused[0].1, HeaderFault::Reserved);
    }

    /// Two things authenticating one request is a request nobody can read the authentication of,
    /// and which won would depend on the order headers were attached in. The same name is kept
    /// where the block names no credential, because then nothing else is putting it there.
    #[test]
    fn the_header_a_credential_goes_in_is_refused_only_where_one_is_named() {
        let with_key = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "apiKey": "in-the-file",
                    "headers": {"Authorization": "Bearer another"}
                }
            }}}"#);
        assert!(
            with_key.request_headers(|_| None).is_empty(),
            "a second credential was sent beside the configured one"
        );
        assert_eq!(
            with_key.headers_refused,
            [("Authorization".to_string(), HeaderFault::Reserved)]
        );

        let named_variable = one(r#"{"provider": {"gw": {
                "env": ["GW_KEY"],
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"authorization": "Bearer another"}
                }
            }}}"#);
        assert!(
            named_variable.request_headers(|_| None).is_empty(),
            "a block naming a variable for its token still sent a written authorization"
        );

        let no_credential = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"authorization": "Basic abc"}
                }
            }}}"#);
        let sent = no_credential.request_headers(|_| None);
        assert_eq!(sent.len(), 1, "nothing else was going to set that header");
        assert_eq!(sent[0].0, "authorization");
    }

    /// A block naming no `env` and no `apiKey` still names a credential once a key is stored for
    /// its id, but `Self::one` cannot know that: `stored_key` is filled in later, by
    /// `Config::from_sources`. Left alone, the written `authorization` would reach the request
    /// beside the stored key `credential_header` adds, carrying two credentials on one request.
    /// `reconcile_stored_credential` is what `Config::from_sources` calls to settle this once the
    /// key is known.
    #[test]
    fn a_stored_key_refuses_a_written_authorization_once_it_is_known() {
        let mut provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"authorization": "Bearer another"}
                }
            }}}"#);
        // Before the key is stored, nothing named a credential, so the header was kept.
        assert_eq!(
            provider.request_headers(|_| None).len(),
            1,
            "a block naming no credential yet should still send its own header"
        );
        assert!(provider.headers_refused.is_empty());

        provider.stored_key = Some(Secret::new("stored-placeholder"));
        provider.reconcile_stored_credential();

        assert!(
            provider.request_headers(|_| None).is_empty(),
            "a second credential was sent beside the stored key"
        );
        assert_eq!(
            provider.headers_refused,
            [("authorization".to_string(), HeaderFault::Reserved)]
        );
        let (credential, source) = provider.credential_and_source(|_| None);
        assert_eq!(token(&credential), Some("stored-placeholder"));
        assert_eq!(source, Some(Source::Stored));
    }

    /// Nothing to do where the block already refused the header from `env` or `apiKey`: a second
    /// pass over a name no longer in `headers` must not report it twice.
    #[test]
    fn reconciling_a_stored_key_is_a_no_op_once_a_credential_was_already_named() {
        let mut provider = one(r#"{"provider": {"gw": {
                "env": ["GW_KEY"],
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"authorization": "Bearer another"}
                }
            }}}"#);
        assert_eq!(provider.headers_refused.len(), 1);

        provider.stored_key = Some(Secret::new("stored-placeholder"));
        provider.reconcile_stored_credential();

        assert_eq!(
            provider.headers_refused.len(),
            1,
            "a header already refused was reported a second time: {:?}",
            provider.headers_refused
        );
    }

    /// Vertex AI takes its key in a header of its own, so the name to refuse is that one and not
    /// `authorization`. A gateway that is not Vertex has nothing putting `x-goog-api-key` on a
    /// request, so a block that wants one there gets one.
    #[test]
    fn a_vertex_block_refuses_the_header_its_key_goes_in() {
        let all = vertex(&format!(
            r#"{{"project": "{PLACEHOLDER_PROJECT}", "apiKey": "in-the-file",
                "headers": {{"x-goog-api-key": "another", "authorization": "Bearer another"}}}}"#
        ));
        let provider = &all[0];
        let sent = provider.request_headers(|_| None);
        assert_eq!(
            sent.len(),
            1,
            "the key's own header was sent twice or the other was dropped: {sent:?}"
        );
        assert_eq!(sent[0].0, "authorization");
        assert_eq!(
            provider.headers_refused,
            [(VERTEX_KEY_HEADER.to_string(), HeaderFault::Reserved)]
        );

        let elsewhere = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "apiKey": "in-the-file",
                    "headers": {"x-goog-api-key": "a-value"}
                }
            }}}"#);
        assert_eq!(elsewhere.request_headers(|_| None).len(), 1);
    }

    /// An `anthropic` entry takes its key in `x-api-key`, so that is the name to refuse there and
    /// not `authorization`, which is where every gateway but Vertex and Anthropic takes one.
    #[test]
    fn an_anthropic_block_refuses_the_header_its_key_goes_in() {
        let provider = one(r#"{"provider": {"anthropic": {
                "env": ["ANTHROPIC_API_KEY"],
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"x-api-key": "another", "authorization": "Bearer another"}
                }
            }}}"#);
        let sent = provider.request_headers(|_| None);
        assert_eq!(
            sent.len(),
            1,
            "the key's own header was sent twice or the other was dropped: {sent:?}"
        );
        assert_eq!(sent[0].0, "authorization");
        assert_eq!(
            provider.headers_refused,
            [("x-api-key".to_string(), HeaderFault::Reserved)]
        );
    }

    /// `Provider::fixed_headers` attaches `anthropic-version` to every Messages request before a
    /// configured header is, whatever the block names: a second value under that name would go
    /// out beside the first rather than in place of it, since `Request::header` appends.
    #[test]
    fn an_anthropic_block_refuses_the_version_header_the_api_requires() {
        let provider = one(r#"{"provider": {"anthropic": {
                "env": ["ANTHROPIC_API_KEY"],
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"Anthropic-Version": "2020-01-01"}
                }
            }}}"#);
        assert!(
            provider.request_headers(|_| None).is_empty(),
            "a configured version header was sent beside the one this program sets"
        );
        assert_eq!(
            provider.headers_refused,
            [("Anthropic-Version".to_string(), HeaderFault::Reserved)]
        );

        let elsewhere = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"anthropic-version": "a-value"}
                }
            }}}"#);
        let sent: Vec<(String, String)> = elsewhere
            .request_headers(|_| None)
            .into_iter()
            .map(|(name, value)| (name, value.expose().to_string()))
            .collect();
        assert_eq!(
            sent,
            [("anthropic-version".to_string(), "a-value".to_string())],
            "a name only Messages sets fixed was refused on a gateway that does not set it"
        );
    }

    /// A header a gateway wants is often a second credential, and a block reaches a person's
    /// screen whenever a diagnostic prints one. The value went out in plain text while this field
    /// was a `String`, since the derived `Debug` prints whatever a field holds.
    #[test]
    fn printing_a_block_does_not_print_a_header_value() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"x-second-key": "in-the-file"}
                }
            }}}"#);

        let printed = format!("{provider:?}");

        assert!(
            !printed.contains("in-the-file"),
            "the header value is in what was printed: {printed}"
        );
        assert!(
            printed.contains("x-second-key"),
            "the name is withheld too, so nothing says which header was configured: {printed}"
        );
    }

    /// A value naming a variable is read where a token is read, at the point of use: one read at
    /// startup is stale in a session where somebody exported another. A variable holding nothing
    /// sends no header, because nothing here knows whether the service requires it and the gateway
    /// is the one that can say.
    #[test]
    fn a_header_value_naming_a_variable_is_resolved_at_the_point_of_use() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"x-tenant-id": "{env:GW_TENANT}"}
                }
            }}}"#);
        assert_eq!(
            provider.headers[0].value.expose(),
            "{env:GW_TENANT}",
            "the reference was substituted at load"
        );

        let first = provider.request_headers(|name| match name {
            "GW_TENANT" => Some("team-7".to_string()),
            _ => None,
        });
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].1.expose(), "team-7");

        // The same block, a different environment: a value cached at load would answer "team-7".
        let later = provider.request_headers(|_| Some("team-8".to_string()));
        assert_eq!(later[0].1.expose(), "team-8");

        assert!(
            provider.request_headers(|_| None).is_empty(),
            "a variable holding nothing sent the reference as a value"
        );
        assert!(
            provider
                .request_headers(|_| Some("   ".to_string()))
                .is_empty(),
            "a variable holding only spaces sent an empty header"
        );
    }

    /// Nothing here substitutes, so a reference inside a longer value would go out with the braces
    /// still in it and the service would route on the literal text. Refused at load, where the key
    /// that wrote it can be named.
    #[test]
    fn a_reference_inside_a_longer_header_value_is_refused() {
        let provider = one(r#"{"provider": {"gw": {
                "options": {
                    "baseURL": "https://example.invalid/v1",
                    "headers": {"x-tenant-id": "tenant-{env:GW_TENANT}", "x-key": "{file:/tmp/k}"}
                }
            }}}"#);
        assert!(
            provider
                .request_headers(|_| Some("team-7".to_string()))
                .is_empty(),
            "an unsubstituted reference was sent"
        );
        let mut refused = provider.headers_refused.clone();
        refused.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            refused,
            [
                ("x-key".to_string(), HeaderFault::Substitution),
                ("x-tenant-id".to_string(), HeaderFault::Substitution)
            ]
        );
    }

    /// A line ending in a value would end the header and let the rest start another, which is a
    /// settings file choosing headers nobody wrote down. A name that is not a field name is
    /// refused with it, because what it would produce is a request no service parses.
    #[test]
    fn a_header_that_is_not_sendable_is_refused() {
        let provider = one(&serde_json::json!({"provider": {"gw": {"options": {
            "baseURL": "https://example.invalid/v1",
            "headers": {
                "x-split": "eu\r\nx-admin: yes",
                "x-nul": "a\u{0}b",
                "x bad": "eu",
                "": "eu",
                "x-ok": "eu"
            }
        }}}})
        .to_string());
        let sent = provider.request_headers(|_| None);
        assert_eq!(sent.len(), 1, "something unsendable went out: {sent:?}");
        assert_eq!(sent[0].0, "x-ok");
        let mut refused: Vec<(String, HeaderFault)> = provider.headers_refused.clone();
        refused.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            refused,
            [
                (String::new(), HeaderFault::NotAName),
                ("x bad".to_string(), HeaderFault::NotAName),
                ("x-nul".to_string(), HeaderFault::NotAValue),
                ("x-split".to_string(), HeaderFault::NotAValue)
            ]
        );
    }

    /// Every failure reads as absence, on the same footing as the rest of the block: a half-typed
    /// settings file must not stop a session. A name with nothing under it is a line somebody has
    /// not finished writing, so it is neither sent nor reported.
    #[test]
    fn a_malformed_headers_block_configures_no_headers() {
        for headers in [
            "\"not a block\"",
            "[]",
            "{}",
            r#"{"x-route": null}"#,
            r#"{"x-route": 7}"#,
            r#"{"x-route": {"a": "b"}}"#,
            r#"{"x-route": ""}"#,
            r#"{"x-route": "   "}"#,
        ] {
            let provider = one(&format!(
                r#"{{"provider": {{"gw": {{"options": {{
                    "baseURL": "https://example.invalid/v1", "headers": {headers}
                }}}}}}}}"#
            ));
            assert!(
                provider
                    .request_headers(|_| Some("a-value".to_string()))
                    .is_empty(),
                "{headers} configured a header"
            );
            assert!(
                provider.headers_refused.is_empty(),
                "{headers} was reported as a refusal rather than read as absence"
            );
        }
    }

    /// Bedrock signs over the headers it sends, so one added here either breaks the signature or
    /// is not covered by it. Refused by name so the block says why rather than failing at the
    /// service.
    #[test]
    fn an_aws_block_sends_no_configured_header() {
        let provider = one(r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2", "headers": {"x-tenant-id": "team-7"}}
            }}}"#);
        assert!(provider.bedrock.is_some());
        assert!(provider.request_headers(|_| None).is_empty());
        assert_eq!(
            provider.headers_refused,
            [("x-tenant-id".to_string(), HeaderFault::Reserved)]
        );
    }
}
