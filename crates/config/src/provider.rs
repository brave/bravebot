//! Configuration for reaching an OpenAI-compatible gateway.
//!
//! The `provider` block in `~/.bravebot/settings.json`, in opencode's shape, so a block copied out
//! of `opencode.json` configures this agent unedited. That constraint decides most of what follows:
//! nothing is required that opencode does not require, no field is added however useful it would be,
//! and a field this crate does not know is read past rather than refused.
//!
//! What this does not hold is a resolved credential. A block says where a token lives, in the
//! variables `env` names or in `options.apiKey`, and it is read when a request needs signing. A block
//! that says neither names no credential and is asked without one, which is what a local Ollama
//! wants.

use crate::Secret;
use std::fmt;

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
const KNOWN_ENDPOINTS: &[(&str, &str)] = &[("openrouter", "https://openrouter.ai/api/v1")];

/// The id naming AWS Bedrock, which is reached by signing rather than by a bearer token.
///
/// The same id the other tool uses, so the block that configures it there configures it here. Its
/// endpoint is not in [`KNOWN_ENDPOINTS`] because there is no one endpoint: the host carries the
/// region, so it is built from `options.region` rather than looked up.
const AWS_PROVIDER_ID: &str = "amazon-bedrock";

/// The endpoint compiled in for `id`, where there is one.
fn known_endpoint(id: &str) -> Option<&'static str> {
    KNOWN_ENDPOINTS
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, url)| *url)
}

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
    /// Whatever `options` held, merged into the request body and never interpreted.
    ///
    /// Opaque on purpose: a gateway's routing controls are its own invention, and a schema
    /// enumerating them is a schema that changes when the gateway adds a field. Trusted exactly as
    /// far as a variable the person exported would be, and no model output can reach it.
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Model {
    /// The window to budget against, stated or assumed.
    pub fn window(&self) -> u64 {
        self.context_window.unwrap_or(CONTEXT_WINDOW)
    }
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
    /// The models this provider offers, in the order the file listed them.
    ///
    /// Possibly empty, because opencode does not require `models`. A provider offering nothing is
    /// reported as such rather than guessed at: a gateway roster is too large and too fluid to
    /// enumerate, so there is nothing to fall back to.
    pub models: Vec<Model>,
    /// The AWS account this entry reaches, where it names Bedrock rather than a gateway.
    ///
    /// Resolved here rather than by the caller because a request to Bedrock needs a region to sign
    /// for and a profile to resolve credentials from, and both are properties of this block.
    /// `None` for every other entry, which is reached as an OpenAI-compatible gateway.
    pub bedrock: Option<crate::bedrock::Bedrock>,
}

impl Provider {
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
        let base_url = options
            .and_then(|options| options.get("baseURL"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
            // A stated endpoint wins, so a block pointing a known name at a proxy or a private
            // deployment reaches the host it named rather than the one compiled in.
            .or_else(|| known_endpoint(id))?;

        Some(Self {
            id: id.to_string(),
            name: string(entry.get("name")),
            base_url: base_url.trim_end_matches('/').to_string(),
            env: names(entry.get("env")),
            api_key: options
                .and_then(|options| string(options.get("apiKey")))
                .map(Secret::new),
            models: models(entry.get("models")),
            bedrock: None,
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

        Some(Self {
            id: id.to_string(),
            name: string(entry.get("name")),
            base_url: format!("https://bedrock-runtime.{region}.amazonaws.com"),
            env: Vec::new(),
            api_key: None,
            models,
            bedrock: Some(crate::bedrock::Bedrock::from_provider(
                region, profile, entries,
            )),
        })
    }

    /// The URL one chat completion goes to.
    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// The URL that answers with what this gateway serves.
    pub fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }

    /// The URL that answers with what the credential in use may reach.
    ///
    /// A narrower roster than [`Provider::models_url`], and the useful one: a model this account
    /// cannot reach is a row that fails the moment somebody picks it. Not part of the shape every
    /// gateway implements, so a caller asks and falls back rather than relying on it.
    pub fn account_models_url(&self) -> String {
        format!("{}/models/user", self.base_url)
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

    /// Whether a name is one of this provider's models.
    pub fn offers(&self, model: &str) -> bool {
        self.model(model).is_some()
    }

    /// The bearer token for this gateway, or why there is none.
    ///
    /// A variable first, so that a file naming one does not have the value read out from under it by
    /// a stale `apiKey`.
    pub fn credential(&self, lookup: impl Fn(&str) -> Option<String>) -> Credential {
        if !self.names_a_credential() {
            return Credential::NotNeeded;
        }
        self.env
            .iter()
            .filter_map(|name| lookup(name))
            .find_map(|mut value| {
                let token = Secret::new(value.trim());
                // The lookup handed over a `String` of its own and the trimmed token is a second
                // copy, so the first is cleared here rather than returned to the allocator holding
                // the token. A buffer that lived for two statements is still one this program owns.
                crate::scrub(&mut value);
                (!token.is_empty()).then_some(token)
            })
            .or_else(|| self.api_key.clone())
            .map_or(Credential::Absent, Credential::Token)
    }

    /// Whether the block says anywhere a token could live.
    pub(crate) fn names_a_credential(&self) -> bool {
        !self.env.is_empty() || self.api_key.is_some()
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

/// A string value with surrounding space removed, or `None` when nothing is left.
fn string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
            provider.model("some/model").expect("offered").window(),
            CONTEXT_WINDOW
        );
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
        assert_eq!(model.window(), 200_000);
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

    /// A known name pointed somewhere else reaches where it was pointed. Otherwise the table would
    /// override the one field that says, unambiguously, which host to use.
    #[test]
    fn a_stated_endpoint_beats_the_one_compiled_in() {
        let provider = one(r#"{"provider": {"openrouter": {
                "options": {"baseURL": "https://proxy.example.invalid/v1"}
            }}}"#);
        assert_eq!(provider.base_url, "https://proxy.example.invalid/v1");
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
}
