//! What `GET /v1/models` offers when called with `Brave-Product: bravebot`.
//!
//! Asked so a person can pick one, and for nothing else. No model reads any of this: the list
//! reaches a picker, the person there chooses, and what comes back is the name sent in the `model`
//! field of a later request. That field is routing, and the endorsement for it is the choice.
//!
//! The endpoint answers with a bare array rather than an OpenAI-style `{"data": [...]}` envelope.
//! [`automatic-bravebot`](bravebot_config::DEFAULT_MODEL) is added here when the listing omits it,
//! so a picker always offers the product's default triage entry.

use bravebot_config::Config;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_net::{Egress, Request};
use serde::Deserialize;

use crate::ChatError;

/// What a service advertised about a model, in the words the service used.
///
/// Read by whoever draws a picker, and by nothing that decides anything: a person's choice off
/// the list is what endorses a request field, and this is only how the list is described to them.
///
/// Empty where nothing said so. The Brave roster describes a model in a different vocabulary and
/// never mentions modality at all, a settings block names models without describing them, and the
/// OpenAI shape a gateway answers in guarantees only the name. Kept as the service spelled it,
/// rather than mapped onto names chosen here, because a picker draws one badge per entry and a
/// modality nobody here has heard of is still one somebody choosing should be able to see.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Advertised {
    /// What the model accepts: `text`, `image`, `audio`, `video`, `file`, and whatever else a
    /// service names.
    pub input_modalities: Vec<String>,
    /// What it produces.
    pub output_modalities: Vec<String>,
    /// The request fields the service says this model reads, or `None` where it did not say.
    ///
    /// The same list [`Model::reads_effort`] is decided from, kept rather than discarded because
    /// that field answers one question about it and a picker asks others: whether the model can
    /// call tools, whether it reasons, whether it can be asked for structured output.
    pub parameters: Option<Vec<String>>,
}

/// A model the user may choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// What goes in a request's `model` field.
    pub key: String,
    /// What to show a person choosing.
    pub display_name: String,
    /// Whether a subscription is needed for it.
    pub premium: bool,
    /// Which service answers for it, where that is not Brave's own endpoint.
    ///
    /// What tells two rows offering the same slug apart, and what a picker files the row under.
    /// `None` is the Brave roster, whose name belongs to the layer that draws it: this field
    /// carries only what a configuration already called a service, never words composed here.
    pub provider: Option<String>,
    /// How large a request may get, in prompt tokens, as the endpoint advertises it.
    ///
    /// `None` for an entry that did not say, and for `automatic`, where the server picks the model
    /// per request and so no one figure describes it.
    ///
    /// Tokens, despite arriving in a field called
    /// `long_conversation_warning_character_limit`. The name is wrong at the source: the endpoint
    /// computes it as `conversation_token_limit * 0.8`, so it is a token count already discounted
    /// by a fifth, and reading it as characters would divide a budget that has already been made
    /// safe. Usable as a budget with no conversion.
    pub conversation_tokens: Option<u64>,
    /// Whether this model reads the effort level a request may carry.
    ///
    /// True where nothing is known, which is the same reading the tool filter gives an absent
    /// capability list: a roster that says nothing is not a roster claiming none, and withholding
    /// somebody's choice on the strength of a listing that never mentioned the subject would be
    /// this program deciding against them from silence. False only where a row states its
    /// parameters and this is not among them, which is the roster saying so.
    pub reads_effort: bool,
    /// What the service said about this model beyond its name, for whoever draws the row.
    pub advertised: Advertised,
}

impl Model {
    /// Let the server decide per request for this product.
    ///
    /// Always offered: the curated bravebot roster may omit it, and it is what an unrecognised
    /// name is reset to anyway, so it is the one choice that cannot fail to work.
    pub fn automatic() -> Self {
        Self {
            key: bravebot_config::DEFAULT_MODEL.to_string(),
            display_name: "Automatic".to_string(),
            premium: false,
            // Brave's own endpoint resolves it per request, and that is the roster it belongs to.
            provider: None,
            // The server chooses per request, so there is no one model whose limit this could be.
            conversation_tokens: None,
            // Brave's endpoint resolves this per request, so no row describes what will answer.
            reads_effort: true,
            // There is no one model behind this name, so there is nothing to describe.
            advertised: Advertised::default(),
        }
    }

    pub fn is_automatic(&self) -> bool {
        self.key == bravebot_config::DEFAULT_MODEL
    }
}

/// One entry as the server reports it.
///
/// Only the fields that matter to a choice, and the limit a budget is worked out from. The endpoint
/// reports a description and a maker too, neither of which decides anything here.
#[derive(Debug, Deserialize)]
struct Listed {
    /// Absent for an entry the server has no name for, which cannot be requested and is dropped.
    key: Option<String>,
    display_name: String,
    /// What the model can do: `chat`, `tools`, `vision`, `files` and others.
    ///
    /// Defaulted rather than required, so an entry omitting it is read as capable of nothing and
    /// dropped. A missing list is not a promise about tools.
    #[serde(default)]
    capabilities: Vec<String>,
    options: ListedOptions,
}

#[derive(Debug, Deserialize)]
struct ListedOptions {
    /// `basic_and_premium` or `premium`.
    access: String,
    /// The window the endpoint will work to, in **tokens**, whatever the name says.
    ///
    /// Computed at the source as `conversation_token_limit * 0.8`, so the fifth held back is
    /// already in it. Nothing is measured in characters here, and a client that divided it into
    /// tokens would compact five times sooner than it had to.
    ///
    /// The figure varies across the roster by a factor of thirty, which is why one constant cannot
    /// stand in for it. Optional, so an entry that stops reporting it falls back rather than
    /// failing to decode.
    #[serde(default)]
    long_conversation_warning_character_limit: Option<u64>,
}

/// The value of `access` that means a subscription is required.
const PREMIUM_ACCESS: &str = "premium";

/// The capability every turn here depends on.
///
/// Not everything the endpoint lists has it. The rest are chat-only models, and choosing one would
/// produce an agent that cannot read or write anything.
const TOOLS_CAPABILITY: &str = "tools";

/// The parameter a gateway names when a model reads an effort level.
///
/// Distinct from the gateway's own `reasoning` parameter, which far more models take: a row
/// advertising that and not this reasons without reading anything sent in this field.
const EFFORT_PARAMETER: &str = "reasoning_effort";

/// Ask the endpoint what it offers, newest answer each time.
///
/// `automatic` comes first and is always present. The rest are whatever the server listed that can
/// call tools, in the order it listed them, since that order is the backend's own preference.
pub fn list<S: Sink>(
    policy: &mut Policy<'_, S>,
    config: &Config,
    egress: &Egress,
) -> Result<Vec<Model>, ChatError> {
    let request =
        crate::as_brave_bot(Request::get(config.models_url()).header("accept", "application/json"));

    // Through the egress gate like everything else, because that gate is the only way out to the
    // network. The label records where the bytes came from; nothing here branches on them beyond
    // the shape the endpoint documents.
    let response = egress.fetch(policy, request, Label::untrusted_public())?;
    let label = response.body.label();
    let (bytes, _) = policy
        .decode_transport("models", label)
        .decode(response.body);

    let listed: Vec<Listed> = serde_json::from_slice(&bytes).map_err(|e| ChatError::Decode {
        detail: format!("{e} (received {} bytes from /v1/models)", bytes.len()),
    })?;

    Ok(usable(&config.chat_completions_url(), listed))
}

/// One entry of a gateway's own roster.
///
/// The OpenAI shape is `{"data": [{"id": ...}]}` and says nothing but the name. Everything else here
/// is a field gateways add: read where present, absent without complaint, because a gateway that
/// reports only what the shape requires still has a usable roster.
#[derive(Debug, Default, Deserialize)]
struct ListedByGateway {
    /// What a request names. The only field the shape guarantees.
    id: String,
    /// The window, where the gateway reports one.
    #[serde(default)]
    context_length: Option<u64>,
    /// The window as Anthropic's listing states it, which is the only place it is named this.
    ///
    /// A field of its own rather than a second name for the one above, since a gateway stating both
    /// would fail to decode as a duplicate, and held as it arrived because a gateway may state it in
    /// a form that is not a whole number, and a roster that cannot be read offers nothing.
    #[serde(default)]
    max_input_tokens: Option<serde_json::Value>,
    /// What the model can do, in the shape Anthropic's listing gives: an object with a `supported`
    /// flag under each capability.
    ///
    /// Held as it arrived and read for one capability, because another gateway may put something
    /// else under the same name and a roster that fails to decode offers nothing.
    #[serde(default)]
    capabilities: Option<serde_json::Value>,
    /// What the model accepts, which is where a gateway says whether it can call tools.
    ///
    /// Absent for a gateway that does not report it, and then nothing is filtered: a roster that
    /// says nothing about capabilities is not a roster claiming none, and dropping all of it would
    /// leave a person unable to pick anything.
    #[serde(default)]
    supported_parameters: Option<Vec<String>>,
    /// What the model takes and what it produces, where the gateway reports it.
    ///
    /// Not every gateway does, and none of it decides anything: it is what a picker draws a
    /// badge from, so a person can see that a model takes images before choosing it for a task
    /// that needs one.
    #[serde(default)]
    architecture: Option<Architecture>,
}

impl ListedByGateway {
    /// Whether the listing says this model reads no effort level.
    ///
    /// Only an explicit `false`: a listing that does not mention the capability, or states it in
    /// another shape, is not a listing claiming none.
    fn states_no_effort(&self) -> bool {
        self.capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.get("effort"))
            .and_then(|effort| effort.get("supported"))
            .and_then(serde_json::Value::as_bool)
            == Some(false)
    }
}

/// The modality half of what a gateway reports about one model.
#[derive(Debug, Deserialize, Default)]
struct Architecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

/// Ask one gateway what models it serves.
///
/// The listing is content, and the person's pick off it is what endorses a request field: the same
/// footing the Brave roster arrives on, and the reason a fetched roster may reach a picker at all.
/// The destination is the gateway's own endpoint, which came from configuration rather than from
/// anything fetched.
///
/// Bearer-authenticated where the block named a credential, because some gateways refuse the listing
/// otherwise, and because a roster is a per-account fact wherever a gateway offers different models to
/// different keys. A block naming none is asked without one, which is what a local Ollama wants.
///
/// What this account may reach is asked for first, where the gateway answers such a question, and the
/// service-wide roster is the fallback. The narrower answer is the better one: a model the credential
/// cannot reach is a row that fails the moment somebody picks it, and the wide list is nearly three
/// times the size here. Only the fallback is guaranteed to exist, since the account-scoped route is a
/// gateway's own extension rather than part of the shape.
///
/// With no credential there is no account to scope an answer to, so that request is not made at all.
/// It would spend a round trip to be told the same thing the wide roster says.
///
/// A service that cannot be asked is answered from the list compiled in for it, with no request,
/// and its rows are built as a listing's are, so a pick off them is recorded and routed the same.
///
/// `headers` is what the block's `options.headers` asked for, resolved by the caller from the same
/// environment the token came from. The roster request carries them because a gateway that needs a
/// header to route a request needs it to answer a listing too, and a roster asked without one is a
/// person seeing no models with nothing local to explain it.
pub fn list_from_gateway<S: Sink>(
    policy: &mut Policy<'_, S>,
    provider: &bravebot_config::provider::Provider,
    token: Option<&str>,
    headers: &[(String, bravebot_config::Secret)],
    egress: &Egress,
) -> Result<Vec<Model>, ChatError> {
    // Vertex AI has no listing a key can call, so asking would carry the key to be answered 404.
    if let Some(compiled) = provider.compiled_roster() {
        let listed = compiled
            .iter()
            .map(|id| ListedByGateway {
                id: (*id).to_string(),
                ..ListedByGateway::default()
            })
            .collect();
        return Ok(offered_by_gateway(provider, listed));
    }

    if token.is_some() && provider.answers_for_an_account() {
        // Any failure falls through to the wide roster: a gateway with no such route answers 404,
        // one that has it under another name answers something undecodable, and neither is a reason
        // to offer nothing when a list that does work is one request away.
        if let Ok(listed) = fetch_listing(
            policy,
            provider,
            provider.account_models_url(),
            token,
            headers,
            egress,
        ) {
            return Ok(offered_by_gateway(provider, listed));
        }
    }

    let listed = fetch_listing(
        policy,
        provider,
        provider.models_url(),
        token,
        headers,
        egress,
    )?;
    Ok(offered_by_gateway(provider, listed))
}

/// One roster request, decoded.
fn fetch_listing<S: Sink>(
    policy: &mut Policy<'_, S>,
    provider: &bravebot_config::provider::Provider,
    url: String,
    token: Option<&str>,
    headers: &[(String, bravebot_config::Secret)],
    egress: &Egress,
) -> Result<Vec<ListedByGateway>, ChatError> {
    let mut request = Request::get(&url).header("accept", "application/json");
    for (name, value) in provider.fixed_headers() {
        request = request.header(*name, *value);
    }
    if let Some(token) = token {
        let (name, value) = provider.credential_header(token);
        // An Anthropic or a Google key stays on the socket the request was addressed to, as the
        // request for a reply keeps it. The other credential headers are left as they were.
        request = match provider.protocol() {
            bravebot_config::provider::Protocol::Messages
            | bravebot_config::provider::Protocol::GenerateContent => {
                request.socket_header(name, value)
            }
            bravebot_config::provider::Protocol::ChatCompletions
            | bravebot_config::provider::Protocol::Responses => request.header(name, value),
        };
    }
    // Carried, never read: `Request::header` appends, and a name this request already set is
    // refused where the block is read rather than compared against here.
    for (name, value) in headers {
        request = request.header(name.clone(), value.expose());
    }

    let listed: GatewayListing = read_listing(policy, request, &url, egress)?;
    Ok(listed.data)
}

/// Send one roster request and decode what answers it as `T`.
///
/// The one place a gateway's or the account's reply is decoded, so the roster of every service
/// reaches a picker through the same declassification site.
fn read_listing<S: Sink, T: serde::de::DeserializeOwned>(
    policy: &mut Policy<'_, S>,
    request: Request,
    url: &str,
    egress: &Egress,
) -> Result<T, ChatError> {
    let response = egress
        .fetch(policy, request, Label::untrusted_public())
        .map_err(|error| crate::failure_of_refusal(policy, error))?;
    let label = response.body.label();
    let (bytes, _) = policy
        .decode_transport("gateway models", label)
        .decode(response.body);

    serde_json::from_slice(&bytes).map_err(|e| ChatError::Decode {
        detail: format!("{e} (received {} bytes from {url})", bytes.len()),
    })
}

/// What the serving host of a ChatGPT account answers a listing with
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)): an object whose `models` key
/// holds the rows.
#[derive(Debug, Deserialize)]
struct AccountListing {
    models: Vec<ListedByAccount>,
}

/// One row of the account's listing.
///
/// Every field but the list is held as it arrived, because the shape is read from another
/// program's source and has not been observed, and a field in a form this code did not expect must
/// cost that row its place and no more: a listing that fails to decode offers nothing.
#[derive(Debug, Default, Deserialize)]
struct ListedByAccount {
    /// What a request names.
    #[serde(default)]
    slug: serde_json::Value,
    /// What to show a person choosing, where the service has a friendlier name than the slug.
    #[serde(default)]
    display_name: serde_json::Value,
    /// `list` for a model the account's own clients show. Any other value, or none, is a model the
    /// service keeps out of its pickers.
    #[serde(default)]
    visibility: serde_json::Value,
    /// Whether a request made as this program makes it can reach the model.
    #[serde(default)]
    supported_in_api: serde_json::Value,
    #[serde(default)]
    context_window: serde_json::Value,
    #[serde(default)]
    input_modalities: serde_json::Value,
    /// The levels of effort the model takes, one entry each. Only whether there are any is read.
    #[serde(default)]
    supported_reasoning_levels: serde_json::Value,
}

/// The models a ChatGPT account may use, from the serving host's listing
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// `provider` is [`bravebot_config::provider::Provider::chatgpt_account`], or a service standing in
/// for it in a test, and `token` is the account's access token. The token goes to the listing's
/// address on the socket the request is addressed to, and a redirect in answer is a failure: the
/// host was compiled in, and a hop is a place a reply named.
///
/// A row is offered when its `visibility` is `list` and `supported_in_api` is true. Nothing is
/// capped, and a row is not dropped for what it says about tools, since the listing says nothing
/// about them. A listing that is read and has no such row offers nothing.
pub fn list_from_account<S: Sink>(
    policy: &mut Policy<'_, S>,
    provider: &bravebot_config::provider::Provider,
    token: &str,
    egress: &Egress,
) -> Result<Vec<Model>, ChatError> {
    let url = provider.models_url();
    let (name, value) = provider.credential_header(token);
    let request = Request::get(&url)
        .header("accept", "application/json")
        .socket_header(name, value)
        .refusing_redirects()
        .reading_a_refusal();

    let listed: AccountListing = read_listing(policy, request, &url, egress)?;
    Ok(offered_by_account(provider, listed.models))
}

/// The picker rows an account's listing becomes, in the order it listed them.
///
/// Separated from the request so the filtering is testable without a server. Every key is
/// qualified by the account's id, which is what a choice is remembered as and what later selects
/// this service rather than the `openai` gateway offering a model of the same name.
fn offered_by_account(
    provider: &bravebot_config::provider::Provider,
    listed: Vec<ListedByAccount>,
) -> Vec<Model> {
    // One row to a name: the picker remembers a choice by its key, and a second row for the same
    // key would be one nobody could tell from the first.
    let mut seen = std::collections::HashSet::new();
    listed
        .into_iter()
        .filter(|row| {
            row.visibility.as_str() == Some("list") && row.supported_in_api.as_bool() == Some(true)
        })
        .filter_map(|row| {
            let slug = row.slug.as_str().filter(|slug| !slug.trim().is_empty())?;
            if !seen.insert(slug.to_string()) {
                return None;
            }
            let strings = |value: &serde_json::Value| -> Vec<String> {
                value
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Some(account_row(
                provider,
                slug,
                row.display_name
                    .as_str()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or(slug)
                    .to_string(),
                row.context_window
                    .as_u64()
                    .filter(|window| *window > 0)
                    .unwrap_or(bravebot_config::provider::CONTEXT_WINDOW),
                // A listing that names the levels is the service saying what it takes, and one
                // that names none for a model says it takes none. A row with no such field, or one
                // in a form other than a list, did not say, which is not a row claiming none: the
                // answer is then the one the service gives a request that carries a level.
                match row.supported_reasoning_levels.as_array() {
                    Some(levels) => !levels.is_empty(),
                    None => true,
                },
                Advertised {
                    input_modalities: strings(&row.input_modalities),
                    output_modalities: Vec::new(),
                    parameters: None,
                },
            ))
        })
        .collect()
}

/// The picker rows a `chatgpt` entry's models become, one for each
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// The person wrote the list, so nothing is asked of the serving host and no row is dropped for
/// what a listing would say: the service refuses at the first request a model the account cannot
/// reach, as a gateway does for one a configured block names. Keyed by the account's id for the
/// reason [`offered_by_account`] gives. A window the entry did not state is the assumed one, as it
/// is for a gateway's model, and whether the model reads an effort level is what a request has
/// already learned from the service, since an entry describes no model.
pub fn named_by_entry(
    provider: &bravebot_config::provider::Provider,
    named: &[bravebot_config::provider::AccountModel],
) -> Vec<Model> {
    named
        .iter()
        .map(|model| {
            account_row(
                provider,
                &model.id,
                model.id.clone(),
                model
                    .context_window
                    .unwrap_or(bravebot_config::provider::CONTEXT_WINDOW),
                true,
                Advertised::default(),
            )
        })
        .collect()
}

/// The model names offered where the account's listing cannot be read
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// Names only, so a row takes the window and the label a model the listing did not describe has.
/// They are the seven rows the service's listing offered a signed-in account, each of which answered
/// a request. A release changes this list; no reply, model or settings file does. The `-fast` names
/// opencode offers are not here, since the service refuses them as models. It accepts the
/// `service_tier` `priority`, which this program does not send.
const COMPILED_ACCOUNT_MODELS: &[&str] = &[
    "gpt-5.6-luna",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-6-astra",
    "gpt-6-luna",
    "gpt-6-sol",
    "gpt-6.1-sol",
];

/// The picker rows of the compiled list, under the account's identifier for the reason
/// [`offered_by_account`] gives.
pub fn compiled_for_account(provider: &bravebot_config::provider::Provider) -> Vec<Model> {
    COMPILED_ACCOUNT_MODELS
        .iter()
        .map(|slug| {
            account_row(
                provider,
                slug,
                (*slug).to_string(),
                bravebot_config::provider::CONTEXT_WINDOW,
                true,
                Advertised::default(),
            )
        })
        .collect()
}

/// One row of the account's roster, whichever way the model came to be named.
///
/// The key carries the account's identifier, since that is what selects the account over the
/// `openai` gateway offering a model of the same name, and the row is the account's own
/// subscription, which is not Brave's. `listed_to_read_effort` is what the source of the row said
/// about the level, and the answer a request has already learned from the service is applied to it.
fn account_row(
    provider: &bravebot_config::provider::Provider,
    slug: &str,
    display_name: String,
    window: u64,
    listed_to_read_effort: bool,
    advertised: Advertised,
) -> Model {
    Model {
        key: format!("{}/{slug}", provider.id),
        display_name,
        premium: false,
        provider: Some(provider.display_name().to_string()),
        conversation_tokens: Some(window),
        reads_effort: listed_to_read_effort
            && crate::reads_effort(&provider.completion_url(), slug),
        advertised,
    }
}

/// The envelope a gateway's roster arrives in.
#[derive(Debug, Deserialize)]
struct GatewayListing {
    data: Vec<ListedByGateway>,
}

/// The picker rows one gateway's roster becomes, in the order it listed them.
///
/// Separated from the request so the filtering is testable without a server.
///
/// Every key is qualified by the provider's id, because that is what a choice is remembered as and
/// what later selects this gateway rather than another service offering the same name.
///
/// Nothing is capped. A gateway's roster runs to hundreds, and the picker filters as a person types,
/// so a limit here would be this program deciding somebody may not choose a model their gateway
/// serves.
fn offered_by_gateway(
    provider: &bravebot_config::provider::Provider,
    listed: Vec<ListedByGateway>,
) -> Vec<Model> {
    listed
        .into_iter()
        .filter(|entry| match entry.supported_parameters.as_ref() {
            Some(parameters) => parameters.iter().any(|it| it == TOOLS_CAPABILITY),
            None => true,
        })
        .filter(|entry| !entry.id.trim().is_empty())
        .map(|entry| Model {
            key: format!("{}/{}", provider.id, entry.id),
            // The bare name the gateway knows it by. What a person reads is composed by whoever
            // draws the picker, which is the layer that owns the words shown to somebody.
            display_name: entry.id.clone(),
            // A gateway is reached with the person's own bearer token, so a Leo subscription has
            // nothing to do with it.
            premium: false,
            // The name the block gave this service, which is what a person recognises it by and
            // what a configured model from the same gateway is filed under.
            provider: Some(provider.display_name().to_string()),
            // What the block said about this model outranks what the gateway reports, because a
            // stated window is somebody pinning a figure they know better than the roster does.
            // Failing that the reported one, and failing that the conservative default: a window
            // above the real one does not delay compaction, it removes it.
            conversation_tokens: Some(
                provider
                    .model(&entry.id)
                    .and_then(|model| model.context_window)
                    .or(provider.context_window)
                    .or(entry.context_length)
                    // Zero is a placeholder rather than a window, as it is on the Brave roster.
                    .or(entry
                        .max_input_tokens
                        .as_ref()
                        .and_then(serde_json::Value::as_u64)
                        .filter(|window| *window > 0))
                    .unwrap_or(bravebot_config::provider::CONTEXT_WINDOW),
            ),
            // A gateway states its parameters per model, and reasoning is not one field but two:
            // many rows take the gateway's own reasoning parameter and not this one, so a model
            // that reasons is not thereby a model that reads a level sent this way.
            // A roster that says nothing about the field describes it no better than a settings
            // block does, so the answer is the one this service has already given: a model whose
            // service refused the field reads no level, and one that has refused nothing takes
            // the level to be read until it says otherwise.
            // A refusal outranks the row either way round. A listing is what the gateway says it
            // would do and a refusal is what it did, so a row still advertising the parameter it
            // has already turned a request down for describes nothing that will be sent again.
            reads_effort: crate::reads_effort(&provider.completion_url(), &entry.id)
                && !entry.states_no_effort()
                && entry
                    .supported_parameters
                    .as_ref()
                    .is_none_or(|parameters| parameters.iter().any(|it| it == EFFORT_PARAMETER)),
            // Everything the roster said, carried through for whoever draws the row. A picker
            // that wanted this used to decode the envelope a second time to get at it, which
            // made a second declassification site out of a field nothing decides anything on.
            advertised: Advertised {
                input_modalities: entry
                    .architecture
                    .as_ref()
                    .map(|a| a.input_modalities.clone())
                    .unwrap_or_default(),
                output_modalities: entry
                    .architecture
                    .as_ref()
                    .map(|a| a.output_modalities.clone())
                    .unwrap_or_default(),
                parameters: entry.supported_parameters.clone(),
            },
        })
        .collect()
}

/// The models worth offering, `automatic` first.
///
/// Separated from the request so the filtering is testable without a server. `service` is the
/// endpoint that answered, which is what a refusal is remembered against.
fn usable(service: &str, listed: Vec<Listed>) -> Vec<Model> {
    // The row naming no model included: what this endpoint refused a request carrying that name is
    // an answer about the name, whatever the server resolves it to per request.
    let mut models = vec![Model {
        reads_effort: crate::reads_effort(service, bravebot_config::DEFAULT_MODEL),
        ..Model::automatic()
    }];
    models.extend(
        listed
            .into_iter()
            .filter(|entry| entry.capabilities.iter().any(|c| c == TOOLS_CAPABILITY))
            .filter_map(|entry| {
                let key = entry.key?;
                // This roster describes what a model can do and never which request fields it
                // reads, so nothing here states the subject either way and the only answer there
                // has ever been is the one this endpoint gave a request that carried a level
                // (BACKEND-22).
                let reads_effort = crate::reads_effort(service, &key);
                Some(Model {
                    key,
                    display_name: entry.display_name,
                    premium: entry.options.access == PREMIUM_ACCESS,
                    provider: None,
                    conversation_tokens: entry
                        .options
                        .long_conversation_warning_character_limit
                        // The endpoint sends 1 where a model reports no window at all, and zero is
                        // not a budget either: both mean "it did not say" rather than "no room".
                        .filter(|limit| *limit > 1),
                    reads_effort,
                    // Its vocabulary is capabilities rather than modalities, and it names no
                    // request field, so there is nothing here it could be read as saying.
                    advertised: Advertised::default(),
                })
            })
            // The endpoint has no reason to list one name twice, but the choice is a person's and
            // two identical rows would leave them unable to tell which they picked.
            .filter(|model| !model.is_automatic()),
    );
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(body: &str) -> Vec<Model> {
        decoded_from("https://roster.example.invalid/v1/chat/completions", body)
    }

    fn decoded_from(service: &str, body: &str) -> Vec<Model> {
        usable(
            service,
            serde_json::from_str(body).expect("the test body parses"),
        )
    }

    /// The shape the deployed endpoint answers with: a bare array, the name to send back in `key`
    /// rather than in `display_name`, and what it can do in `capabilities`.
    #[test]
    fn a_bare_array_decodes_and_keeps_the_requestable_name() {
        let models = decoded(
            r#"[{"key":"claude-3-sonnet","display_name":"Claude Sonnet",
                 "capabilities":["chat","tools","vision"],"options":{"access":"premium"}}]"#,
        );
        assert_eq!(models.len(), 2, "{models:?}");
        assert_eq!(models[1].key, "claude-3-sonnet");
        assert_eq!(models[1].display_name, "Claude Sonnet");
        assert!(models[1].premium);
    }

    #[test]
    fn a_free_model_is_not_premium() {
        let models = decoded(
            r#"[{"key":"claude-3-haiku","display_name":"Claude Haiku",
                 "capabilities":["chat","tools"],"options":{"access":"basic_and_premium"}}]"#,
        );
        assert!(!models[1].premium);
    }

    /// The window is the one thing here worth having that a person does not choose, and it arrives
    /// under a name that says characters while holding tokens.
    #[test]
    fn the_advertised_window_is_kept_as_tokens() {
        let models = decoded(
            r#"[{"key":"claude-opus","display_name":"Claude Opus","capabilities":["chat","tools"],
                 "options":{"access":"premium",
                            "long_conversation_warning_character_limit":102400}}]"#,
        );
        assert_eq!(models[1].conversation_tokens, Some(102_400));
    }

    /// An entry that says nothing about its window is not saying it has none. The caller falls back
    /// rather than compacting at whatever a missing field reads as.
    #[test]
    fn an_entry_that_advertises_no_window_reports_none() {
        let models = decoded(
            r#"[{"key":"mystery","display_name":"Mystery","capabilities":["chat","tools"],
                 "options":{"access":"premium"}}]"#,
        );
        assert_eq!(models[1].conversation_tokens, None);
    }

    /// What the endpoint sends for a model whose window it does not know. Taken as "did not say"
    /// rather than as a budget, which at one token would compact before a turn could start.
    #[test]
    fn the_placeholder_window_is_read_as_nothing_said() {
        let models = decoded(
            r#"[{"key":"mystery","display_name":"Mystery","capabilities":["chat","tools"],
                 "options":{"access":"premium",
                            "long_conversation_warning_character_limit":1}}]"#,
        );
        assert_eq!(models[1].conversation_tokens, None);
    }

    /// The server picks the model per request, so no one window describes it.
    #[test]
    fn automatic_advertises_no_window() {
        assert_eq!(Model::automatic().conversation_tokens, None);
    }

    /// Every turn here works by calling tools, so a chat-only model would be an agent that can read
    /// and write nothing. A good fraction of what the endpoint lists is chat-only, so this is the
    /// common case rather than a hypothetical one.
    #[test]
    fn a_model_that_cannot_call_tools_is_not_offered() {
        let models = decoded(
            r#"[{"key":"llama-3-8b-instruct","display_name":"Llama 3 8B",
                 "capabilities":["chat","files"],"options":{"access":"basic_and_premium"}}]"#,
        );
        assert_eq!(models, vec![Model::automatic()], "{models:?}");
    }

    /// An entry claiming nothing is not claiming tools. Read as capable of nothing rather than
    /// waved through, since the whole point of the check is that a chat-only model cannot work.
    #[test]
    fn an_entry_with_no_capabilities_is_not_offered() {
        let models = decoded(
            r#"[{"key":"mystery","display_name":"Mystery","options":{"access":"premium"}}]"#,
        );
        assert_eq!(models, vec![Model::automatic()], "{models:?}");
    }

    /// The endpoint lists concrete models and never this one, so it has to be added or there would
    /// be no way back to letting the server choose.
    #[test]
    fn automatic_is_offered_first_even_when_the_server_lists_nothing() {
        let models = decoded("[]");
        assert_eq!(models, vec![Model::automatic()]);
        assert!(models[0].is_automatic());
    }

    /// An entry with no key cannot be put in a request's `model` field, so it is not a choice.
    #[test]
    fn an_entry_without_a_key_is_dropped() {
        let models = decoded(
            r#"[{"key":null,"display_name":"Nameless","capabilities":["chat","tools"],
                 "options":{"access":"premium"}}]"#,
        );
        assert_eq!(models, vec![Model::automatic()], "{models:?}");
    }

    /// Added unconditionally, so a server that did list it must not produce two identical rows: a
    /// person cannot tell which of them they chose.
    #[test]
    fn automatic_is_offered_once_even_if_the_server_lists_it_too() {
        let models = decoded(
            r#"[{"key":"automatic-bravebot","display_name":"Automatic","capabilities":["chat","tools"],
                 "options":{"access":"basic_and_premium"}}]"#,
        );
        assert_eq!(models, vec![Model::automatic()], "{models:?}");
    }

    /// A field this agent does not use must not make an entry undecodable. The endpoint grows
    /// fields without asking: it reported `supports_tools` once and reports `capabilities` now.
    #[test]
    fn unknown_fields_do_not_break_decoding() {
        let models = decoded(
            r#"[{"key":"claude-3-sonnet","display_name":"Claude Sonnet",
                 "capabilities":["chat","tools"],"is_near_model":false,"is_suggested_model":true,
                 "options":{"access":"premium","description":"whatever","display_maker":"Anthropic",
                            "max_associated_content_length":16000}}]"#,
        );
        assert_eq!(models.len(), 2, "{models:?}");
    }

    /// The shape of a real reply, field for field, including the ones this ignores: the array is
    /// bare, tool support lives in `capabilities`, and access lives under `options`.
    ///
    /// The shape is what is pinned, not the roster. This decoded `supports_tools` until the
    /// deployed endpoint was actually asked, and every listing would have failed; the fixture
    /// exists so the next such change fails here rather than at a user's prompt. The entries are
    /// stand-ins deliberately, since a snapshot of the live roster in a public repository would
    /// publish which models are deployed and go stale besides.
    #[test]
    fn a_real_response_shape_decodes() {
        let models = decoded(include_str!("../tests/models_response.json"));

        assert!(models[0].is_automatic(), "automatic was not offered first");
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert!(keys.contains(&"claude-3-sonnet"), "{keys:?}");
        assert!(
            !keys.contains(&"llama-3-8b-instruct"),
            "a chat-only model was offered: {keys:?}"
        );

        let sonnet = models
            .iter()
            .find(|m| m.key == "claude-3-sonnet")
            .expect("the premium entry survived");
        assert_eq!(sonnet.display_name, "Claude Sonnet");
        assert!(sonnet.premium);
    }

    /// A gateway configured with no models at all, which is the case its roster is fetched for.
    fn gateway() -> bravebot_config::provider::Provider {
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"openrouter": {
                "options": {"baseURL": "https://openrouter.example.invalid/api/v1"}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider")
    }

    fn from_gateway(provider: &bravebot_config::provider::Provider, body: &str) -> Vec<Model> {
        let listed: GatewayListing = serde_json::from_str(body).expect("the test body parses");
        offered_by_gateway(provider, listed.data)
    }

    /// The shape a gateway answers with: an envelope around entries whose only certain field is the
    /// name. Every key is qualified by the provider's id, since that is what selects this service
    /// later and a fetched roster is not written down anywhere that could say so instead.
    #[test]
    fn a_gateway_roster_is_offered_under_names_that_say_which_gateway_serves_them() {
        let provider = gateway();
        let models = from_gateway(
            &provider,
            r#"{"data": [{"id": "z-ai/glm-4.6"}, {"id": "moonshot/kimi-k2"}]}"#,
        );
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            ["openrouter/z-ai/glm-4.6", "openrouter/moonshot/kimi-k2"]
        );
        // The bare name, for whoever composes the row a person reads.
        assert_eq!(models[0].display_name, "z-ai/glm-4.6");
    }

    /// The same slug is reachable through more than one service, billed and credentialled
    /// differently. A fetched row is the one place nothing written down could say which is which,
    /// so the gateway it came from travels with it.
    #[test]
    fn a_fetched_row_carries_the_gateway_that_serves_it() {
        let models = from_gateway(&gateway(), r#"{"data": [{"id": "z-ai/glm-4.6"}]}"#);
        assert_eq!(models[0].provider.as_deref(), Some("openrouter"));
        assert_eq!(
            Model::automatic().provider,
            None,
            "the Brave roster names no service of its own"
        );
    }

    /// A window the gateway reports is worth having: it is the one fact about a fetched model that
    /// nobody can type, and the default is deliberately far below what most of them offer.
    #[test]
    fn a_window_a_gateway_reports_is_taken_from_the_listing() {
        let models = from_gateway(
            &gateway(),
            r#"{"data": [{"id": "z-ai/glm-4.6", "context_length": 262144}]}"#,
        );
        assert_eq!(models[0].conversation_tokens, Some(262_144));
    }

    /// A gateway saying nothing about a window is not saying there is none. The conservative figure
    /// stands in, because a budget above the real window does not delay compaction but removes it.
    #[test]
    fn a_fetched_model_with_no_reported_window_gets_the_assumed_one() {
        let models = from_gateway(&gateway(), r#"{"data": [{"id": "z-ai/glm-4.6"}]}"#);
        assert_eq!(
            models[0].conversation_tokens,
            Some(bravebot_config::provider::CONTEXT_WINDOW)
        );
    }

    /// Where the block pinned a window for a model, that figure is the person's own and outranks
    /// what the roster reports, which is the whole reason stating one stays possible.
    #[test]
    fn a_window_the_block_stated_outranks_the_one_reported() {
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"openrouter": {
                "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                "models": {"z-ai/glm-4.6": {"limit": {"context": 32000, "output": 8192}}}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        let provider = bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider");
        let models = from_gateway(
            &provider,
            r#"{"data": [{"id": "z-ai/glm-4.6", "context_length": 262144}]}"#,
        );
        assert_eq!(models[0].conversation_tokens, Some(32_000));
    }

    /// A block listing no models keeps the window of a local server in `options.contextWindow`.
    /// It outranks the window a roster reports, and a roster that reports none gets it instead of
    /// the assumed one, which is the regression an Ollama import's figure would otherwise suffer.
    #[test]
    fn a_provider_window_is_taken_for_fetched_models() {
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"ollama": {
                "options": {"baseURL": "http://localhost:11434/v1", "contextWindow": 4096}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        let provider = bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider");
        let models = from_gateway(
            &provider,
            r#"{"data": [{"id": "plain"}, {"id": "reported", "context_length": 262144}]}"#,
        );
        assert_eq!(models[0].conversation_tokens, Some(4096));
        assert_eq!(models[1].conversation_tokens, Some(4096));
    }

    /// Every turn here calls tools, so a chat-only model is a choice that produces an agent which
    /// cannot read or write anything. Dropped on the same grounds as on the Brave roster.
    #[test]
    fn a_gateway_model_that_cannot_call_tools_is_not_offered() {
        let models = from_gateway(
            &gateway(),
            r#"{"data": [
                {"id": "can/talk", "supported_parameters": ["temperature"]},
                {"id": "can/work", "supported_parameters": ["temperature", "tools"]}
            ]}"#,
        );
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["openrouter/can/work"]);
    }

    /// A gateway that reports no capabilities at all is not claiming its models have none. Filtering
    /// on a field the shape does not require would empty the picker for every gateway but one.
    /// A gateway states its parameters per model, and many rows reason through a parameter this
    /// does not send. Sending a level to one of those is a field dropped at the far end.
    #[test]
    fn a_gateway_model_that_does_not_take_the_effort_parameter_says_so() {
        let models = from_gateway(
            &gateway(),
            r#"{"data": [
                {"id": "takes-it", "supported_parameters": ["tools", "reasoning", "reasoning_effort"]},
                {"id": "reasons-only", "supported_parameters": ["tools", "reasoning"]}
            ]}"#,
        );

        assert!(
            models[0].reads_effort,
            "a row advertising it was not believed"
        );
        assert!(
            !models[1].reads_effort,
            "a row that reasons without the parameter was treated as reading a level"
        );
    }

    /// A roster that says nothing about parameters is not a roster claiming none, which is the
    /// same reading the tool filter already gives an absent list.
    #[test]
    fn a_gateway_that_states_no_parameters_is_not_taken_to_read_no_level() {
        let models = from_gateway(&gateway(), r#"{"data": [{"id": "unstated"}]}"#);
        assert!(models[0].reads_effort);
    }

    fn anthropic() -> bravebot_config::provider::Provider {
        let serde_json::Value::Object(root) =
            serde_json::from_str(r#"{"provider": {"anthropic": {"env": ["ANTHROPIC_API_KEY"]}}}"#)
                .expect("json")
        else {
            panic!("not an object");
        };
        bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider")
    }

    /// The shape Anthropic's listing answers with, as its reference gives it: the window under
    /// `max_input_tokens` and what the model can do as an object with a `supported` flag under each
    /// capability. The names are the provider's id in front of what a request carries, and the
    /// window is the one the model states, since the default is far below it.
    #[test]
    fn an_anthropic_listing_is_offered_under_the_providers_id_with_the_window_it_states() {
        let models = from_gateway(
            &anthropic(),
            r#"{"data": [
                {"type": "model", "id": "claude-opus-5-5", "display_name": "Claude Opus 5.5",
                 "created_at": "2026-09-21T00:00:00Z", "max_input_tokens": 1000000,
                 "max_tokens": 128000,
                 "capabilities": {"effort": {"supported": true, "low": {"supported": true}}}},
                {"type": "model", "id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5",
                 "max_input_tokens": 200000, "max_tokens": 64000}
            ], "has_more": false, "first_id": "claude-opus-5-5", "last_id": "claude-haiku-4-5"}"#,
        );

        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            ["anthropic/claude-opus-5-5", "anthropic/claude-haiku-4-5"]
        );
        assert_eq!(models[0].conversation_tokens, Some(1_000_000));
        assert_eq!(models[1].conversation_tokens, Some(200_000));
        assert_eq!(models[0].provider.as_deref(), Some("anthropic"));
    }

    /// A zero where a window belongs is a placeholder, and a budget taken from it would compact
    /// before a turn could start.
    #[test]
    fn an_anthropic_window_of_zero_or_nothing_is_the_assumed_one() {
        let models = from_gateway(
            &anthropic(),
            r#"{"data": [
                {"id": "claude-zero", "max_input_tokens": 0},
                {"id": "claude-null", "max_input_tokens": null},
                {"id": "claude-absent"}
            ]}"#,
        );
        for model in &models {
            assert_eq!(
                model.conversation_tokens,
                Some(bravebot_config::provider::CONTEXT_WINDOW),
                "{}",
                model.key
            );
        }
    }

    /// Anthropic's listing says per model whether an effort level is read, and a model it says reads
    /// none is not offered one. Only an explicit `false` says so: a row that does not mention the
    /// capability, or that states it in another shape, is not a row claiming none.
    #[test]
    fn an_anthropic_model_the_listing_says_reads_no_effort_says_so() {
        let models = from_gateway(
            &anthropic(),
            r#"{"data": [
                {"id": "reads", "capabilities": {"effort": {"supported": true}}},
                {"id": "reads-none", "capabilities": {"effort": {"supported": false}}},
                {"id": "silent"},
                {"id": "no-effort-key", "capabilities": {"batch": {"supported": true}}}
            ]}"#,
        );
        let reads: Vec<(&str, bool)> = models
            .iter()
            .map(|model| (model.key.as_str(), model.reads_effort))
            .collect();
        assert_eq!(
            reads,
            [
                ("anthropic/reads", true),
                ("anthropic/reads-none", false),
                ("anthropic/silent", true),
                ("anthropic/no-effort-key", true),
            ]
        );
    }

    /// A window stated as a float, a string or a negative number is not a window, and does not stop the
    /// rest of the roster being read: a roster that fails to decode offers nothing at all.
    #[test]
    fn a_window_in_a_form_that_is_not_a_whole_number_leaves_the_roster_readable() {
        let models = from_gateway(
            &anthropic(),
            r#"{"data": [
                {"id": "as-a-float", "max_input_tokens": 200000.5},
                {"id": "as-text", "max_input_tokens": "200000"},
                {"id": "as-negative", "max_input_tokens": -1},
                {"id": "as-a-whole-number", "max_input_tokens": 200000}
            ]}"#,
        );
        let windows: Vec<Option<u64>> = models.iter().map(|m| m.conversation_tokens).collect();
        let assumed = Some(bravebot_config::provider::CONTEXT_WINDOW);
        assert_eq!(windows, [assumed, assumed, assumed, Some(200_000)]);
    }

    /// Another gateway may put something else under `capabilities`, and a roster that fails to
    /// decode offers nothing at all, so no shape of the field may stop the rest being read.
    #[test]
    fn a_gateway_stating_capabilities_in_another_shape_still_has_its_roster_read() {
        let models = from_gateway(
            &gateway(),
            r#"{"data": [
                {"id": "as-a-list", "capabilities": ["chat", "tools"]},
                {"id": "as-text", "capabilities": "chat"},
                {"id": "as-nothing", "capabilities": null}
            ]}"#,
        );
        assert_eq!(models.len(), 3);
        assert!(models.iter().all(|model| model.reads_effort));
    }

    /// A roster describing no parameters leaves the level to be judged by the service, and the
    /// judgment is the only description of the field there is. A row that went on claiming a level
    /// is read after the service refused it would show somebody a charge they chose and stopped
    /// getting, which is what consulting a listing is for in the first place.
    #[test]
    fn a_row_whose_service_refused_a_level_reads_none_however_silent_the_roster() {
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"refuser": {
                "options": {"baseURL": "https://refuser.example.invalid/v1"}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        let provider = bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider");
        crate::remember(
            &crate::learned_key(&provider.chat_completions_url(), "refused-the-field"),
            crate::Refusals {
                caching: true,
                cache_ttl: false,
                effort: true,
                ..crate::Refusals::default()
            },
        );

        let models = from_gateway(
            &provider,
            r#"{"data": [{"id": "refused-the-field"}, {"id": "refused-nothing"}]}"#,
        );

        assert!(
            !models[0].reads_effort,
            "a model whose service refused the field was still offered as reading a level"
        );
        assert!(
            models[1].reads_effort,
            "one model's refusal was read as the whole gateway refusing"
        );
    }

    /// This roster names no request field at all, so the only description of the field there has
    /// ever been is what the endpoint answered a request that carried one. A row that went on
    /// offering the level after that would put the interface back to reporting a charge somebody
    /// chose and stopped getting, on the model most sessions are served by.
    #[test]
    fn a_brave_roster_row_the_endpoint_refused_reads_none() {
        let service = "https://curated.example.invalid/v1/chat/completions";
        crate::remember(
            &crate::learned_key(service, "refused-the-field"),
            crate::Refusals {
                caching: false,
                cache_ttl: false,
                effort: true,
                ..crate::Refusals::default()
            },
        );

        let models = decoded_from(
            service,
            r#"[{"key":"refused-the-field","display_name":"One","capabilities":["tools"],
                 "options":{"access":"basic_and_premium"}},
                {"key":"refused-nothing","display_name":"Two","capabilities":["tools"],
                 "options":{"access":"basic_and_premium"}}]"#,
        );

        let refused = models
            .iter()
            .find(|model| model.key == "refused-the-field")
            .expect("the refused model is offered");
        assert!(
            !refused.reads_effort,
            "a row the endpoint had already refused the field for was offered as reading a level"
        );
        let other = models
            .iter()
            .find(|model| model.key == "refused-nothing")
            .expect("the other model is offered");
        assert!(
            other.reads_effort,
            "a refusal for one model took the field away from another on the same endpoint"
        );
    }

    /// A listing is what a gateway says it would do; a refusal is what it did. A row still
    /// advertising the parameter a request was just turned down for describes nothing that will be
    /// sent again, and offering it as reading a level would put the interface back to reporting a
    /// charge somebody chose and stopped getting.
    #[test]
    fn a_row_the_service_refused_reads_none_however_loudly_the_roster_advertises_it() {
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"advertiser": {
                "options": {"baseURL": "https://advertiser.example.invalid/v1"}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        let provider = bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider");
        crate::remember(
            &crate::learned_key(&provider.chat_completions_url(), "advertised-and-refused"),
            crate::Refusals {
                caching: false,
                cache_ttl: false,
                effort: true,
                ..crate::Refusals::default()
            },
        );

        let models = from_gateway(
            &provider,
            r#"{"data": [
                {"id": "advertised-and-refused",
                 "supported_parameters": ["tools", "reasoning_effort"]},
                {"id": "advertised-and-taken",
                 "supported_parameters": ["tools", "reasoning_effort"]}
            ]}"#,
        );

        assert!(
            !models[0].reads_effort,
            "a row the service had already refused the field for was offered as reading a level"
        );
        assert!(
            models[1].reads_effort,
            "a refusal for one model took the field away from another the roster describes"
        );
    }

    #[test]
    fn a_gateway_that_reports_no_capabilities_still_offers_its_models() {
        let models = from_gateway(&gateway(), r#"{"data": [{"id": "z-ai/glm-4.6"}]}"#);
        assert_eq!(models.len(), 1);
    }

    /// A name is what a request carries, so an entry without one cannot be asked for. Nothing else
    /// on the entry could stand in for it.
    #[test]
    fn a_fetched_entry_with_no_usable_name_is_dropped() {
        let models = from_gateway(
            &gateway(),
            r#"{"data": [{"id": "  "}, {"id": "real/one"}]}"#,
        );
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["openrouter/real/one"]);
    }

    /// A gateway is reached with somebody's own bearer token, so a Leo subscription has nothing to
    /// do with it. Marked premium, every row would ask them to import one to use what they pay for.
    #[test]
    fn fetched_gateway_models_are_not_marked_premium() {
        let models = from_gateway(&gateway(), r#"{"data": [{"id": "z-ai/glm-4.6"}]}"#);
        assert!(!models[0].premium);
    }

    /// What a picker draws a badge from, carried through rather than left behind.
    ///
    /// A front-end that wanted it decoded the same envelope a second time to get at it, which
    /// made a second declassification site out of a field nothing decides anything on. The two
    /// directions are kept apart: a model that reads an image and a model that draws one are
    /// different rows to somebody choosing, and a listing that collapsed them would say the
    /// second of these accepts pictures.
    #[test]
    fn what_a_model_takes_and_produces_is_carried_through_to_the_picker() {
        let models = from_gateway(
            &gateway(),
            r#"{"data": [
                {"id": "reads/pictures", "supported_parameters": ["tools", "reasoning"],
                 "architecture": {"input_modalities": ["text", "image"],
                                  "output_modalities": ["text"]}},
                {"id": "draws/pictures", "supported_parameters": ["tools"],
                 "architecture": {"input_modalities": ["text"],
                                  "output_modalities": ["text", "image"]}}
            ]}"#,
        );
        assert_eq!(
            models[0].advertised.input_modalities,
            ["text".to_string(), "image".to_string()]
        );
        assert_eq!(models[0].advertised.output_modalities, ["text".to_string()]);
        assert_eq!(models[1].advertised.input_modalities, ["text".to_string()]);
        assert_eq!(
            models[1].advertised.output_modalities,
            ["text".to_string(), "image".to_string()]
        );
        assert_eq!(
            models[0].advertised.parameters.as_deref(),
            Some(["tools".to_string(), "reasoning".to_string()].as_slice())
        );
    }

    /// A gateway reporting only the name says nothing about modality, which is different from
    /// saying a model has none. Both are empty here, and the parameter list stays `None` so the
    /// distinction survives for whoever reads it.
    #[test]
    fn a_roster_that_describes_nothing_claims_nothing() {
        let models = from_gateway(&gateway(), r#"{"data": [{"id": "z-ai/glm-4.6"}]}"#);
        assert!(models[0].advertised.input_modalities.is_empty());
        assert!(models[0].advertised.output_modalities.is_empty());
        assert_eq!(models[0].advertised.parameters, None);
    }

    fn from_account(body: &str) -> Vec<Model> {
        let listed: AccountListing = serde_json::from_str(body).expect("the test body parses");
        offered_by_account(
            &bravebot_config::provider::Provider::chatgpt_account(),
            listed.models,
        )
    }

    /// CHATGPT-9: the account's own picker shows a model only when its visibility is `list`, and a
    /// request made as this program makes it reaches only a model with `supported_in_api`, so a row
    /// short of either is a row that fails when picked or one the service keeps out of its own.
    /// A row stating neither is not stating both, and a key carries the account's identifier,
    /// without which the name would select the `openai` gateway's model of that name.
    #[test]
    fn only_a_row_the_account_lists_and_takes_in_the_api_is_offered_under_the_accounts_identifier()
    {
        let models = from_account(
            r#"{"models": [
                {"slug": "gpt-a", "display_name": "GPT A", "visibility": "list", "supported_in_api": true},
                {"slug": "gpt-hidden", "visibility": "hide", "supported_in_api": true},
                {"slug": "gpt-app-only", "visibility": "list", "supported_in_api": false},
                {"slug": "gpt-silent"},
                {"slug": "gpt-no-visibility", "supported_in_api": true},
                {"slug": "gpt-no-api-flag", "visibility": "list"},
                {"slug": "gpt-b", "visibility": "list", "supported_in_api": true}
            ]}"#,
        );
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["chatgpt/gpt-a", "chatgpt/gpt-b"]);
        assert_eq!(models[0].display_name, "GPT A");
        assert_eq!(
            models[1].display_name, "gpt-b",
            "a row naming no friendlier name is shown by its slug"
        );
        assert_eq!(models[0].provider.as_deref(), Some("ChatGPT"));
        assert!(
            !models[0].premium,
            "the account's subscription is not Leo's"
        );
    }

    /// CHATGPT-9: the window a row states is the budget a turn compacts against, and a row stating
    /// none or a figure that is not a whole number leaves the assumed one, since a roster that
    /// failed to decode would offer nothing and a window above the real one removes compaction.
    #[test]
    fn a_window_the_account_states_is_taken_and_a_missing_or_malformed_one_is_the_assumed_one() {
        let models = from_account(
            r#"{"models": [
                {"slug": "stated", "visibility": "list", "supported_in_api": true, "context_window": 272000},
                {"slug": "silent", "visibility": "list", "supported_in_api": true},
                {"slug": "zero", "visibility": "list", "supported_in_api": true, "context_window": 0},
                {"slug": "words", "visibility": "list", "supported_in_api": true, "context_window": "large"},
                {"slug": "fraction", "visibility": "list", "supported_in_api": true, "context_window": 1.5}
            ]}"#,
        );
        assert_eq!(models[0].conversation_tokens, Some(272_000));
        for model in &models[1..] {
            assert_eq!(
                model.conversation_tokens,
                Some(bravebot_config::provider::CONTEXT_WINDOW),
                "{}",
                model.key
            );
        }
    }

    /// CHATGPT-9 and BACKEND-22: a row that lists the effort levels it takes reads one, a row whose
    /// list is empty says it takes none, and a row that does not mention the subject is not a row
    /// claiming none.
    #[test]
    fn an_account_row_reads_an_effort_level_only_where_the_listing_does_not_say_it_takes_none() {
        let models = from_account(
            r#"{"models": [
                {"slug": "levels", "visibility": "list", "supported_in_api": true,
                 "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}]},
                {"slug": "none", "visibility": "list", "supported_in_api": true,
                 "supported_reasoning_levels": []},
                {"slug": "silent", "visibility": "list", "supported_in_api": true}
            ]}"#,
        );
        let reads: Vec<(&str, bool)> = models
            .iter()
            .map(|m| (m.key.as_str(), m.reads_effort))
            .collect();
        assert_eq!(
            reads,
            [
                ("chatgpt/levels", true),
                ("chatgpt/none", false),
                ("chatgpt/silent", true)
            ]
        );
    }

    /// CHATGPT-9: a row in a form this code did not expect costs that row its place and no more. A
    /// name that is not a string, or is blank, is no model; a name listed twice is one row; the
    /// modalities a row names are the strings in its list; and no other row is lost to either.
    #[test]
    fn an_account_row_in_an_unexpected_form_is_dropped_alone_and_the_rest_is_still_offered() {
        let models = from_account(
            r#"{"models": [
                {"slug": 7, "visibility": "list", "supported_in_api": true},
                {"slug": "  ", "visibility": "list", "supported_in_api": true},
                {"visibility": "list", "supported_in_api": true},
                {"slug": "kept", "display_name": 4, "visibility": "list", "supported_in_api": true,
                 "input_modalities": ["text", 3, "image"]},
                {"slug": "kept", "display_name": "Again", "visibility": "list", "supported_in_api": true}
            ]}"#,
        );
        assert_eq!(models.len(), 1, "{models:?}");
        assert_eq!(models[0].key, "chatgpt/kept");
        assert_eq!(models[0].display_name, "kept");
        assert_eq!(
            models[0].advertised.input_modalities,
            ["text".to_string(), "image".to_string()]
        );
    }

    /// CHATGPT-9: a model an entry names is a row under the account's identifier, since a bare name
    /// would select the `openai` gateway's model of that name. It carries the window the entry
    /// stated and the assumed one where it stated none, which is the budget a turn compacts against,
    /// and it is the account's own subscription and not Leo's. No listing describes it, so it is
    /// not one that says it reads no effort level (BACKEND-22): a level goes out to be judged at the
    /// far end.
    #[test]
    fn the_models_an_entry_names_are_rows_under_the_accounts_identifier_with_the_window_it_states()
    {
        let named = |id: &str, window: Option<u64>| bravebot_config::provider::AccountModel {
            id: id.to_string(),
            context_window: window,
        };
        let rows = named_by_entry(
            &bravebot_config::provider::Provider::chatgpt_account(),
            &[named("gpt-big", Some(400_000)), named("gpt-plain", None)],
        );

        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, ["chatgpt/gpt-big", "chatgpt/gpt-plain"]);
        assert_eq!(rows[0].display_name, "gpt-big");
        assert_eq!(rows[0].conversation_tokens, Some(400_000));
        assert_eq!(
            rows[1].conversation_tokens,
            Some(bravebot_config::provider::CONTEXT_WINDOW),
            "a window the entry does not state is the assumed one"
        );
        for row in &rows {
            assert_eq!(row.provider.as_deref(), Some("ChatGPT"));
            assert!(!row.premium, "the account's subscription is not Leo's");
            assert_eq!(row.advertised, Advertised::default());
            assert!(
                row.reads_effort,
                "{} was taken to read no effort level, which no listing said of it",
                row.key
            );
        }
    }

    /// CHATGPT-9: the compiled list is rows under the account's identifier, one to a name, each
    /// with the assumed window and no advertised capability, and it is the account's own
    /// subscription and not Leo's. A row keyed by the bare name would select the `openai` gateway's
    /// model of that name, and a repeated key is a row nobody could tell from the first.
    #[test]
    fn the_compiled_list_is_rows_under_the_accounts_identifier_one_to_a_name() {
        let rows = compiled_for_account(&bravebot_config::provider::Provider::chatgpt_account());

        assert!(
            !rows.is_empty(),
            "a picker with no model offers nothing to pick"
        );
        let mut seen = std::collections::HashSet::new();
        for row in &rows {
            let name = row
                .key
                .strip_prefix("chatgpt/")
                .unwrap_or_else(|| panic!("{} is not under the account's identifier", row.key));
            assert!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-.".contains(c)),
                "{name} is not a name a request can carry as the model"
            );
            assert!(seen.insert(row.key.clone()), "{} appears twice", row.key);
            assert_eq!(row.display_name, name);
            assert_eq!(
                row.conversation_tokens,
                Some(bravebot_config::provider::CONTEXT_WINDOW),
                "a name carries the assumed window"
            );
            assert_eq!(row.provider.as_deref(), Some("ChatGPT"));
            assert!(!row.premium, "the account's subscription is not Leo's");
            assert!(
                row.reads_effort,
                "{} was said to take no effort level, which no source says",
                row.key
            );
            assert_eq!(row.advertised, Advertised::default());
        }
    }

    /// CHATGPT-9: the compiled list is the seven models the service listed for a signed-in account
    /// and accepted. The list opencode offers has sixteen names, and the service refuses nine of
    /// them as models (`gpt-5.5` and every `-fast` name), so a picker built on it offers rows that
    /// fail at the first request. An exact list fails for a restored name and for a `-fast` name
    /// added again.
    #[test]
    fn the_compiled_list_is_the_seven_models_the_service_offered() {
        let rows = compiled_for_account(&bravebot_config::provider::Provider::chatgpt_account());

        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "chatgpt/gpt-5.6-luna",
                "chatgpt/gpt-5.6-sol",
                "chatgpt/gpt-5.6-terra",
                "chatgpt/gpt-6-astra",
                "chatgpt/gpt-6-luna",
                "chatgpt/gpt-6-sol",
                "chatgpt/gpt-6.1-sol",
            ]
        );
    }
}
