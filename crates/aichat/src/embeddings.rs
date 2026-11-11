//! Vectors for the text of a bot conversation, from an OpenAI-shaped `/v1/embeddings` endpoint
//! (`docs/specs/retrieval.md`, RETRIEVE-15 and RETRIEVE-16).
//!
//! Passages are embedded off the turn, so a passage request is given seconds and sent again after a
//! failure that another attempt could get past. A query is embedded while the person waits for the
//! turn, so it is given a fraction of that and sent once: a turn whose query failed compacts instead.

use bravebot_config::Secret;
use bravebot_config::embeddings::{Choice, Service};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::delegate::DelegateId;
use bravebot_core::event::{Event, Sink};
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::{Egress, EgressError, Request, Timeouts};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The development deployment of Brave's embeddings endpoint, which takes no key.
pub const DEV_EMBEDDINGS_URL: &str = "https://ai-chat-backend-dev.bsg.brave.software/v1/embeddings";

/// The most inputs one request carries. The endpoint's model is served with a batch of 32 and
/// answers a larger one with a 500.
pub const MAX_BATCH: usize = 32;

/// How many times a passage request is sent before its failure is the answer.
const PASSAGE_ATTEMPTS: u32 = 3;

/// The pause after the first failed passage request, doubled after each one after it.
const PASSAGE_BACKOFF: Duration = Duration::from_millis(500);

/// The bounds on a passage request, which nobody is waiting on.
const PASSAGE_TIMEOUTS: Timeouts = Timeouts {
    resolve: Duration::from_secs(5),
    connect: Duration::from_secs(5),
    send: Duration::from_secs(5),
    reply: Duration::from_secs(15),
    idle: Duration::from_secs(5),
};

/// The whole of a query request, from building it to the last byte of the reply.
const QUERY_DEADLINE: Duration = Duration::from_millis(1500);

/// The bounds on each part of a query request, none past [`QUERY_DEADLINE`]. They end the
/// request a deadline has already given up on.
const QUERY_TIMEOUTS: Timeouts = Timeouts {
    resolve: Duration::from_millis(1500),
    connect: Duration::from_millis(1500),
    send: Duration::from_millis(1500),
    reply: Duration::from_millis(1500),
    idle: Duration::from_millis(1500),
};

/// Where vectors are asked for and what they are expected to be.
///
/// One value, so that where the endpoint is chosen can change without the client changing.
#[derive(Debug, Clone)]
pub struct EmbeddingsConfig {
    pub url: String,
    pub model: String,
    /// The length every vector the endpoint answers with must have.
    pub dimensions: usize,
    /// Put in front of the text of a query.
    pub query_prefix: String,
    /// Put in front of the text of a passage.
    pub passage_prefix: String,
    /// Sent as a bearer token where present, to the configured host only.
    pub token: Option<Secret>,
}

impl EmbeddingsConfig {
    /// `embedding_gemma` at [`DEV_EMBEDDINGS_URL`]: 768 dimensions, with the prefixes the model
    /// was trained to tell a search query from a document by.
    pub fn dev() -> Self {
        Self {
            url: DEV_EMBEDDINGS_URL.to_string(),
            model: "embedding_gemma".to_string(),
            dimensions: 768,
            query_prefix: "task: search result | query: ".to_string(),
            passage_prefix: "title: none | text: ".to_string(),
            token: None,
        }
    }
}

/// The model OpenAI is asked for where the person kept none.
pub const OPENAI_DEFAULT_MODEL: &str = "text-embedding-3-small";

/// The length of the vectors of each OpenAI model whose length is published, so choosing one
/// needs no test call to be used.
const OPENAI_DIMENSIONS: &[(&str, usize)] = &[
    ("text-embedding-3-small", 1536),
    ("text-embedding-3-large", 3072),
    ("text-embedding-ada-002", 1536),
];

/// What the endpoint can be chosen from besides the settings: the gateways the person configured,
/// whether Brave's service is offered, and the key stored for a custom endpoint.
pub struct Sources<'a> {
    pub providers: &'a [bravebot_config::provider::Provider],
    /// The build reaches Brave and its embeddings service was asked for, which it has to be while
    /// that endpoint is not public (RETRIEVE-34).
    pub brave: bool,
    pub custom_key: Option<Secret>,
}

/// Why the settings choose no endpoint retrieval can send to (RETRIEVE-34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unresolved {
    /// No service is selected, and Brave is not there to stand in for one.
    NothingSelected,
    /// The service selected is not configured for inference.
    NotOffered(Service),
    /// The service is configured, and its key is not there.
    NoKey(Service),
    /// No model is kept for a service that has no default.
    NoModel(Service),
    /// The custom endpoint has no URL, or one that is not `http` or `https`.
    NoUrl,
    /// The length of the model's vectors has not been found by a test call yet.
    Untested(Service),
}

impl fmt::Display for Unresolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingSelected => write!(f, "no embeddings service is selected"),
            Self::NotOffered(service) => write!(f, "{} is not set up", name(*service)),
            Self::NoKey(service) => write!(f, "{} has no key", name(*service)),
            Self::NoModel(service) => write!(f, "no {} model is chosen", name(*service)),
            Self::NoUrl => write!(f, "the custom embeddings endpoint has no http(s) URL"),
            Self::Untested(service) => write!(
                f,
                "the {} model has not been tested, so the length of its vectors is not known",
                name(*service)
            ),
        }
    }
}

fn name(service: Service) -> &'static str {
    match service {
        Service::OpenAi => "OpenAI",
        Service::Brave => "Brave",
        Service::OpenRouter => "OpenRouter",
        Service::Custom => "the custom endpoint",
    }
}

/// Whether `service` can be chosen at all: a provider for it is configured, or for Brave, its
/// service is offered. The custom endpoint always can.
pub fn offered(service: Service, sources: &Sources<'_>) -> bool {
    match service {
        Service::Brave => sources.brave,
        Service::Custom => true,
        Service::OpenAi | Service::OpenRouter => provider(service, sources).is_some(),
    }
}

fn provider<'a>(
    service: Service,
    sources: &Sources<'a>,
) -> Option<&'a bravebot_config::provider::Provider> {
    sources
        .providers
        .iter()
        .find(|provider| provider.id == service.word())
}

/// The service in force: the one selected, or Brave where none is and Brave's is offered, which
/// is what retrieval sent to before there was a choice.
pub fn in_force(choice: &Choice, sources: &Sources<'_>) -> Option<Service> {
    choice
        .selected
        .or_else(|| sources.brave.then_some(Service::Brave))
}

/// The endpoint the settings choose, from what was kept for the service in force and where its
/// key is (RETRIEVE-34).
///
/// OpenAI and OpenRouter send to the provider block's own host with the provider's own key, so
/// the key goes nowhere it does not already go for inference. `lookup` reads a variable a block
/// names its key in.
pub fn resolve(
    choice: &Choice,
    sources: &Sources<'_>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<EmbeddingsConfig, Unresolved> {
    let service = in_force(choice, sources).ok_or(Unresolved::NothingSelected)?;
    let config = candidate(service, choice, sources, lookup)?;
    if config.dimensions == 0 {
        return Err(Unresolved::Untested(service));
    }
    Ok(config)
}

/// The endpoint `service` would be, whether or not it is selected, with `dimensions` 0 where no
/// test call has found them yet: what a test call is made against.
pub fn candidate(
    service: Service,
    choice: &Choice,
    sources: &Sources<'_>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<EmbeddingsConfig, Unresolved> {
    if !offered(service, sources) {
        return Err(Unresolved::NotOffered(service));
    }
    let kept = choice.kept(service);
    let model = model_of(service, choice).ok_or(Unresolved::NoModel(service));
    let dimensions = dimensions_of(service, choice).unwrap_or(0);
    match service {
        Service::Brave => Ok(EmbeddingsConfig::dev()),
        Service::OpenAi | Service::OpenRouter => {
            let token = gateway_key(service, sources, lookup)?;
            Ok(EmbeddingsConfig {
                url: url_of(service, sources, choice).ok_or(Unresolved::NotOffered(service))?,
                model: model?,
                dimensions,
                query_prefix: kept.query_prefix.clone().unwrap_or_default(),
                passage_prefix: kept.passage_prefix.clone().unwrap_or_default(),
                token,
            })
        }
        Service::Custom => Ok(EmbeddingsConfig {
            url: url_of(service, sources, choice).ok_or(Unresolved::NoUrl)?,
            model: model?,
            dimensions,
            query_prefix: kept.query_prefix.clone().unwrap_or_default(),
            passage_prefix: kept.passage_prefix.clone().unwrap_or_default(),
            token: sources.custom_key.clone(),
        }),
    }
}

/// The key the provider block of `service`, OpenAI or OpenRouter, holds for inference, or `None`
/// where the block needs none.
pub fn gateway_key(
    service: Service,
    sources: &Sources<'_>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Option<Secret>, Unresolved> {
    let gateway = provider(service, sources).ok_or(Unresolved::NotOffered(service))?;
    match gateway.credential(lookup) {
        bravebot_config::provider::Credential::Token(token) => Ok(Some(token)),
        bravebot_config::provider::Credential::NotNeeded => Ok(None),
        bravebot_config::provider::Credential::Absent => Err(Unresolved::NoKey(service)),
    }
}

/// Where `service` sends, where that is known without its key: the provider block's host, Brave's
/// endpoint, or the custom URL where it is an `http(s)` one.
pub fn url_of(service: Service, sources: &Sources<'_>, choice: &Choice) -> Option<String> {
    match service {
        Service::Brave => Some(DEV_EMBEDDINGS_URL.to_string()),
        Service::OpenAi | Service::OpenRouter => {
            provider(service, sources).map(|gateway| format!("{}/embeddings", gateway.base_url))
        }
        Service::Custom => choice
            .custom
            .url
            .clone()
            .filter(|url| url.starts_with("https://") || url.starts_with("http://")),
    }
}

/// The model `service` asks for: the one kept, or the one it has by default.
pub fn model_of(service: Service, choice: &Choice) -> Option<String> {
    match service {
        Service::Brave => Some(EmbeddingsConfig::dev().model),
        Service::OpenAi => Some(
            choice
                .openai
                .model
                .clone()
                .unwrap_or_else(|| OPENAI_DEFAULT_MODEL.to_string()),
        ),
        Service::OpenRouter | Service::Custom => choice.kept(service).model.clone(),
    }
}

/// The length of `service`'s vectors: published for Brave and for OpenAI's own models, otherwise
/// as a test call found it.
pub fn dimensions_of(service: Service, choice: &Choice) -> Option<usize> {
    let published = match service {
        Service::Brave => Some(EmbeddingsConfig::dev().dimensions),
        Service::OpenAi => model_of(service, choice).and_then(|model| {
            OPENAI_DIMENSIONS
                .iter()
                .find(|(id, _)| *id == model)
                .map(|(_, dimensions)| *dimensions)
        }),
        Service::OpenRouter | Service::Custom => None,
    };
    published.or(choice.kept(service).dimensions)
}

/// What tells `url` apart from another endpoint, and holds nothing secret: the URL with its query,
/// its fragment and any user and password before the host taken off, since a key may be in any of
/// those.
pub fn endpoint_identity(url: &str) -> String {
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{path}")
}

/// The OpenAI models the window offers to choose from.
pub const OPENAI_MODELS: [&str; 2] = ["text-embedding-3-small", "text-embedding-3-large"];

/// One embedding model a listing offers, as OpenRouter's `/embeddings/models` describes it. The
/// listing states no lengths, so a model's is found by a test call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListedModel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub pricing: Option<Pricing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pricing {
    /// Dollars per input token, as a decimal string.
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Deserialize)]
struct Listing {
    data: Vec<ListedModel>,
}

/// The embedding models listed beside an embeddings endpoint, at `embeddings_url` + `/models`,
/// asked for with `token` where there is one.
pub fn list_models(
    embeddings_url: &str,
    token: Option<&Secret>,
) -> Result<Vec<ListedModel>, EmbedError> {
    let url = format!("{embeddings_url}/models");
    let mut sink = Lent(None);
    let mut routing = Routing::new();
    routing.insert_trusted("embeddings", &url);
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .map_err(|denied| EmbedError::Egress(EgressError::Denied(denied)))?;
    policy.before_server_request(&url);
    let mut request = crate::as_brave_bot(
        Request::get(&url)
            .header("accept", "application/json")
            .refusing_redirects(),
    );
    if let Some(token) = token {
        request = request.socket_header("authorization", format!("Bearer {}", token.expose()));
    }
    let response = Egress::with_timeouts(PASSAGE_TIMEOUTS)
        .fetch(&mut policy, request, Label::untrusted_public())
        .map_err(EmbedError::Egress)?;
    if response.truncated {
        return Err(EmbedError::Egress(EgressError::Transport {
            url,
            detail: "the reply was larger than a reply is read to".to_string(),
            transient: false,
        }));
    }
    let label = response.body.label();
    let (bytes, _) = policy
        .decode_transport("embeddings models", label)
        .decode(response.body);
    let listing: Listing = serde_json::from_slice(&bytes).map_err(|e| EmbedError::Reply {
        detail: e.to_string(),
    })?;
    Ok(listing.data)
}

/// What a text is embedded as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// What a turn searches with. Asked while the turn waits.
    Query,
    /// What is searched. Asked off the turn.
    Passage,
}

/// Why no vectors came back. Each one is a turn that compacts instead of retrieving.
#[derive(Debug)]
pub enum EmbedError {
    /// The request could not be serialised.
    Encode(String),
    /// The request never left, or failed in transit, or the endpoint answered with an error.
    Egress(EgressError),
    /// The reply was not an embeddings reply, or not one for the inputs that were sent.
    Reply { detail: String },
}

impl fmt::Display for EmbedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(detail) => {
                write!(f, "the embeddings request could not be built: {detail}")
            }
            Self::Egress(error) => write!(f, "{error}"),
            Self::Reply { detail } => write!(f, "the embeddings reply was not usable: {detail}"),
        }
    }
}

impl std::error::Error for EmbedError {}

/// Why a request had no vectors, as a turn that falls back on it names the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The endpoint refused the key sent.
    Key,
    /// The endpoint could not be reached.
    Unreachable,
    /// The endpoint did not answer in time.
    OutOfTime,
    /// The endpoint answered with an error or with something that is not vectors.
    Failed,
}

impl EmbedError {
    /// Why the request failed, from what the transport decided and never from a reply's text.
    pub fn failure(&self) -> Failure {
        match self {
            Self::Egress(EgressError::Status {
                status: 401 | 403, ..
            }) => Failure::Key,
            Self::Egress(EgressError::OutOfTime { .. }) => Failure::OutOfTime,
            Self::Egress(
                EgressError::Denied(_)
                | EgressError::AddressRefused { .. }
                | EgressError::Transport { .. }
                | EgressError::InvalidUrl { .. },
            ) => Failure::Unreachable,
            Self::Egress(_) | Self::Encode(_) | Self::Reply { .. } => Failure::Failed,
        }
    }
}

/// Something that turns text into vectors, one per text and in the order given.
pub trait Embedder {
    /// The vectors for `texts`, embedded as `kind`. Empty for no texts, which asks nothing.
    fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;

    /// Where the vectors are asked for, as [`endpoint_identity`] names it, for the vectors file's
    /// header.
    fn endpoint(&self) -> String;

    /// The model the vectors are from, for the vectors file's header.
    fn model(&self) -> &str;

    /// The length of every vector.
    fn dimensions(&self) -> usize;
}

/// A trail the gate decisions of every request are written to, shared with whoever reads it.
pub type SharedSink = Arc<Mutex<dyn Sink + Send>>;

/// The [`Embedder`] that asks the endpoint an [`EmbeddingsConfig`] names.
#[derive(Clone)]
pub struct EmbeddingsClient {
    config: EmbeddingsConfig,
    backoff: Duration,
    trail: Option<SharedSink>,
}

impl fmt::Debug for EmbeddingsClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingsClient")
            .field("config", &self.config)
            .field("backoff", &self.backoff)
            .field("trail", &self.trail.is_some())
            .finish()
    }
}

impl EmbeddingsClient {
    pub fn new(config: EmbeddingsConfig) -> Self {
        Self {
            config,
            backoff: PASSAGE_BACKOFF,
            trail: None,
        }
    }

    /// The same client with another first pause between passage attempts.
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    /// The same client writing what the gates decided about each of its requests to `trail`,
    /// which is otherwise told nothing.
    pub fn with_trail(mut self, trail: SharedSink) -> Self {
        self.trail = Some(trail);
        self
    }

    fn batch(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let started = std::time::Instant::now();
        let prefix = match kind {
            Kind::Query => &self.config.query_prefix,
            Kind::Passage => &self.config.passage_prefix,
        };
        let input: Vec<String> = texts.iter().map(|t| format!("{prefix}{t}")).collect();
        let body = serde_json::to_vec(&Body {
            model: &self.config.model,
            input: &input,
        })
        .map_err(|e| EmbedError::Encode(e.to_string()))?;
        if kind == Kind::Query {
            let bytes = self
                .send_by(body, QUERY_DEADLINE.saturating_sub(started.elapsed()))
                .map_err(EmbedError::Egress)?;
            return read_reply(&bytes, texts.len(), Some(self.config.dimensions));
        }
        let mut failures = 0;
        let bytes = loop {
            match self.send(body.clone(), PASSAGE_TIMEOUTS) {
                Ok(bytes) => break bytes,
                Err(error) => {
                    failures += 1;
                    if failures >= PASSAGE_ATTEMPTS || !crate::retryable_within_ceiling(&error) {
                        return Err(EmbedError::Egress(error));
                    }
                    std::thread::sleep(crate::pause_after_failure(self.backoff, failures, &error));
                }
            }
        };
        read_reply(&bytes, texts.len(), Some(self.config.dimensions))
    }

    /// One passage of `text`, sent once, and the length of the vector that answers it, whatever
    /// the configured length is: how a model's length is found before it is used.
    pub fn probe(&self, text: &str) -> Result<usize, EmbedError> {
        let input = [format!("{}{text}", self.config.passage_prefix)];
        let body = serde_json::to_vec(&Body {
            model: &self.config.model,
            input: &input,
        })
        .map_err(|e| EmbedError::Encode(e.to_string()))?;
        let bytes = self
            .send(body, PASSAGE_TIMEOUTS)
            .map_err(EmbedError::Egress)?;
        let vectors = read_reply(&bytes, 1, None)?;
        match vectors.first().map(Vec::len) {
            Some(0) | None => Err(EmbedError::Reply {
                detail: "a vector of no dimensions".to_string(),
            }),
            Some(length) => Ok(length),
        }
    }

    /// One query request, given up on at `deadline` however its time was spent.
    ///
    /// The request runs on a thread of its own so the wait can end while a part of it is still
    /// within its own bound. A request given up on is left to end by [`QUERY_TIMEOUTS`].
    fn send_by(&self, body: Vec<u8>, deadline: Duration) -> Result<Vec<u8>, EgressError> {
        let (sent, answer) = std::sync::mpsc::channel();
        let client = self.clone();
        std::thread::spawn(move || {
            sent.send(client.send(body, QUERY_TIMEOUTS)).ok();
        });
        answer.recv_timeout(deadline).unwrap_or_else(|_| {
            Err(EgressError::OutOfTime {
                url: self.config.url.clone(),
            })
        })
    }

    /// One request to the configured URL and nowhere else.
    fn send(&self, body: Vec<u8>, timeouts: Timeouts) -> Result<Vec<u8>, EgressError> {
        let url = self.config.url.as_str();
        let mut sink = Lent(self.trail.as_ref());
        let mut routing = Routing::new();
        routing.insert_trusted("embeddings", url);
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .map_err(EgressError::Denied)?;
        policy.before_server_request(url);
        let mut request = crate::as_brave_bot(
            Request::post(url, body)
                .header("content-type", "application/json")
                .refusing_redirects(),
        );
        if let Some(token) = &self.config.token {
            request = request.socket_header("authorization", format!("Bearer {}", token.expose()));
        }
        let response = Egress::with_timeouts(timeouts).fetch(
            &mut policy,
            request,
            Label::untrusted_public(),
        )?;
        if response.truncated {
            return Err(EgressError::Transport {
                url: url.to_string(),
                detail: "the reply was larger than a reply is read to".to_string(),
                transient: false,
            });
        }
        let label = response.body.label();
        let (bytes, _) = policy
            .decode_transport("embeddings", label)
            .decode(response.body);
        Ok(bytes)
    }
}

impl Embedder for EmbeddingsClient {
    fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut vectors = Vec::with_capacity(texts.len());
        for batch in texts.chunks(MAX_BATCH) {
            vectors.extend(self.batch(kind, batch)?);
        }
        Ok(vectors)
    }

    fn endpoint(&self) -> String {
        endpoint_identity(&self.config.url)
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn dimensions(&self) -> usize {
        self.config.dimensions
    }
}

/// The client's trail, as the policy writes to it: each event under the lock, and nowhere when
/// there is no trail.
struct Lent<'a>(Option<&'a SharedSink>);

impl Sink for Lent<'_> {
    fn emit(&mut self, event: Event) {
        if let Some(mut trail) = self.0.and_then(|trail| trail.lock().ok()) {
            trail.emit(event);
        }
    }

    fn recording_for(&mut self, delegate: Option<DelegateId>) {
        if let Some(mut trail) = self.0.and_then(|trail| trail.lock().ok()) {
            trail.recording_for(delegate);
        }
    }
}

#[derive(Serialize)]
struct Body<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct Reply {
    data: Vec<Datum>,
}

#[derive(Deserialize)]
struct Datum {
    index: usize,
    embedding: Vec<f32>,
}

/// The vectors of a reply in the order of the inputs, where it holds exactly one of `dimensions`
/// for each of `count` inputs, or of any one length where `dimensions` is `None`.
fn read_reply(
    bytes: &[u8],
    count: usize,
    dimensions: Option<usize>,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let reply: Reply = serde_json::from_slice(bytes).map_err(|e| EmbedError::Reply {
        detail: e.to_string(),
    })?;
    if reply.data.len() != count {
        return Err(EmbedError::Reply {
            detail: format!("{} vectors for {count} inputs", reply.data.len()),
        });
    }
    let mut vectors: Vec<Option<Vec<f32>>> = vec![None; count];
    let mut expected = dimensions;
    for datum in reply.data {
        let dimensions = *expected.get_or_insert(datum.embedding.len());
        if datum.embedding.len() != dimensions {
            return Err(EmbedError::Reply {
                detail: format!(
                    "a vector of {} dimensions where {dimensions} were expected",
                    datum.embedding.len()
                ),
            });
        }
        // A number past what an f32 holds is read as an infinity, which no distance survives.
        if !datum.embedding.iter().all(|value| value.is_finite()) {
            return Err(EmbedError::Reply {
                detail: "a vector holding something other than a finite number".to_string(),
            });
        }
        match vectors.get_mut(datum.index) {
            Some(slot @ None) => *slot = Some(datum.embedding),
            _ => {
                return Err(EmbedError::Reply {
                    detail: format!("index {} is repeated or out of range", datum.index),
                });
            }
        }
    }
    // Every slot is filled: there were `count` data, each in range and none repeated.
    Ok(vectors.into_iter().flatten().collect())
}

/// An [`Embedder`] that asks nothing of the network: the vector of a text is a unit-length hash of
/// its words, so texts sharing words are near each other and a text always gets the same vector.
#[cfg(any(test, feature = "fake-embeddings"))]
#[derive(Debug, Clone)]
pub struct FakeEmbedder {
    pub dimensions: usize,
    /// When set, every call fails with this status, as an endpoint that is down would.
    pub failing: Option<u16>,
}

#[cfg(any(test, feature = "fake-embeddings"))]
impl FakeEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            failing: None,
        }
    }
}

/// The errors a request that never got an answer comes back as, for a stand-in endpoint to give.
#[cfg(any(test, feature = "fake-embeddings"))]
impl EmbedError {
    pub fn fake_transport() -> Self {
        Self::Egress(EgressError::Transport {
            url: "fake".into(),
            detail: "refused".into(),
            transient: true,
        })
    }

    pub fn fake_address_refused() -> Self {
        Self::Egress(EgressError::AddressRefused { url: "fake".into() })
    }

    pub fn fake_out_of_time() -> Self {
        Self::Egress(EgressError::OutOfTime { url: "fake".into() })
    }
}

#[cfg(any(test, feature = "fake-embeddings"))]
impl Embedder for FakeEmbedder {
    fn embed(&self, _kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if let Some(status) = self.failing {
            return Err(EmbedError::Egress(EgressError::Status {
                url: "fake".to_string(),
                status,
                retry_after: None,
                body: None,
            }));
        }
        Ok(texts
            .iter()
            .map(|text| {
                let mut vector = vec![0.0f32; self.dimensions];
                for word in text
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                {
                    let hash = word
                        .to_lowercase()
                        .bytes()
                        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
                        });
                    let cell = u64::try_from(self.dimensions)
                        .ok()
                        .and_then(|dimensions| hash.checked_rem(dimensions))
                        .and_then(|cell| usize::try_from(cell).ok());
                    if let Some(cell) = cell.and_then(|cell| vector.get_mut(cell)) {
                        *cell += 1.0;
                    }
                }
                let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    vector.iter_mut().for_each(|x| *x /= norm);
                }
                vector
            })
            .collect())
    }

    fn endpoint(&self) -> String {
        "https://fake.invalid/v1/embeddings".to_string()
    }

    fn model(&self) -> &str {
        "fake"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Instant;

    /// One request as the server saw it.
    #[derive(Debug, Clone)]
    struct Seen {
        headers: Vec<(String, String)>,
        body: serde_json::Value,
    }

    impl Seen {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(found, _)| found.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        }

        fn inputs(&self) -> Vec<String> {
            self.body["input"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    /// What the server answers a request with, from the inputs it carried.
    type Answer = Box<dyn Fn(usize, &[String]) -> (u16, String) + Send>;

    /// What the server answers a request with, headers besides.
    type HeaderedAnswer =
        Box<dyn Fn(usize, &[String]) -> (u16, Vec<(String, String)>, String) + Send>;

    /// A server on this machine answering every request by `answer`, given the request's number
    /// from zero and its inputs, and what each request was.
    fn serve(answer: Answer) -> (String, Arc<Mutex<Vec<Seen>>>) {
        serve_headed(Box::new(move |number, inputs| {
            let (status, reply) = answer(number, inputs);
            (status, Vec::new(), reply)
        }))
    }

    /// [`serve`], with headers of the answer's own on each reply.
    fn serve_headed(answer: HeaderedAnswer) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!(
            "http://{}/v1/embeddings",
            listener.local_addr().expect("addr")
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        thread::spawn(move || {
            let mut number = 0;
            while let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let mut headers = Vec::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.trim_end().split_once(':') {
                        headers.push((name.trim().to_string(), value.trim().to_string()));
                    }
                }
                let length = headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0);
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let seen = Seen {
                    headers,
                    body: serde_json::from_slice(&body).unwrap_or_default(),
                };
                let (status, extra, reply) = answer(number, &seen.inputs());
                record.lock().expect("lock").push(seen);
                number += 1;
                let extra: String = extra
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect();
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        (url, seen)
    }

    /// A reply giving input `i` a vector of `dimensions` whose first value is the input's length
    /// and second its place, listed last input first.
    fn reply_for(inputs: &[String], dimensions: usize) -> String {
        let data: Vec<serde_json::Value> = inputs
            .iter()
            .enumerate()
            .rev()
            .map(|(index, input)| {
                let mut embedding = vec![0.0f32; dimensions];
                embedding[0] = input.len() as f32;
                embedding[1] = index as f32;
                serde_json::json!({"object": "embedding", "index": index, "embedding": embedding})
            })
            .collect();
        serde_json::json!({"object": "list", "data": data, "model": "m"}).to_string()
    }

    fn answering(dimensions: usize) -> Answer {
        Box::new(move |_, inputs| (200, reply_for(inputs, dimensions)))
    }

    fn client(url: &str, dimensions: usize) -> EmbeddingsClient {
        EmbeddingsClient::new(EmbeddingsConfig {
            url: url.to_string(),
            model: "test-model".to_string(),
            dimensions,
            query_prefix: "Q: ".to_string(),
            passage_prefix: "P: ".to_string(),
            token: None,
        })
        .with_backoff(Duration::from_millis(1))
    }

    fn texts(count: usize) -> Vec<String> {
        (0..count).map(|i| "x".repeat(i + 1)).collect()
    }

    /// RETRIEVE-7: a refused key, an endpoint down or slow, and one that errs are each named as
    /// such.
    #[test]
    fn a_failed_request_is_named_by_why_it_failed() {
        let url = || "https://embeddings.invalid/".to_string();
        let status = |status| {
            EmbedError::Egress(EgressError::Status {
                url: url(),
                status,
                retry_after: None,
                body: None,
            })
        };
        assert_eq!(status(401).failure(), Failure::Key);
        assert_eq!(status(403).failure(), Failure::Key);
        assert_eq!(status(503).failure(), Failure::Failed);
        assert_eq!(
            EmbedError::Egress(EgressError::AddressRefused { url: url() }).failure(),
            Failure::Unreachable
        );
        assert_eq!(
            EmbedError::Egress(EgressError::OutOfTime { url: url() }).failure(),
            Failure::OutOfTime
        );
        assert_eq!(
            EmbedError::Reply {
                detail: "no vectors".to_string()
            }
            .failure(),
            Failure::Failed
        );
    }

    fn status_of(error: &EmbedError) -> Option<u16> {
        match error {
            EmbedError::Egress(EgressError::Status { status, .. }) => Some(*status),
            _ => None,
        }
    }

    /// Forty passages go as a request of 32 and one of 8, each input behind the passage prefix and
    /// naming the model, and come back one vector per text in the order given, though the reply
    /// lists them last first.
    #[test]
    fn passages_go_in_batches_of_32_behind_their_prefix_and_come_back_in_order() {
        let (url, seen) = serve(answering(4));
        let given = texts(40);
        let vectors = client(&url, 4)
            .embed(Kind::Passage, &given)
            .expect("vectors");

        assert_eq!(vectors.len(), 40);
        for (text, vector) in given.iter().zip(&vectors) {
            assert_eq!(vector[0] as usize, "P: ".len() + text.len());
        }
        let seen = seen.lock().expect("lock");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].inputs().len(), 32);
        assert_eq!(seen[1].inputs().len(), 8);
        assert_eq!(seen[0].inputs()[0], "P: x");
        assert_eq!(seen[0].body["model"], "test-model");
        assert_eq!(seen[0].header("content-type"), Some("application/json"));
        assert_eq!(seen[0].header("authorization"), None);
    }

    /// A query goes behind the query prefix, and a configured token goes as a bearer token.
    #[test]
    fn a_query_goes_behind_its_prefix_with_the_token_where_one_is_set() {
        let (url, seen) = serve(answering(4));
        let mut config = client(&url, 4).config;
        config.token = Some(Secret::new("k"));
        EmbeddingsClient::new(config)
            .embed(Kind::Query, &["where".to_string()])
            .expect("vector");

        let seen = seen.lock().expect("lock");
        assert_eq!(seen[0].inputs(), ["Q: where"]);
        assert_eq!(seen[0].header("authorization"), Some("Bearer k"));
    }

    /// A client given a trail writes the gates' decisions about each request it sends there, and
    /// one given none keeps nothing.
    #[test]
    fn a_client_given_a_trail_writes_what_the_gates_decided_there() {
        let (url, _seen) = serve(answering(4));
        let trail = Arc::new(Mutex::new(bravebot_core::event::RecordingSink::default()));
        let shared: SharedSink = trail.clone();

        client(&url, 4)
            .with_trail(shared)
            .embed(Kind::Passage, &texts(2))
            .expect("vectors");

        let trail = trail.lock().expect("lock");
        assert!(
            trail
                .recorded()
                .any(|(_, event)| matches!(event, Event::GatePassed { .. })),
            "nothing reached the trail"
        );
    }

    /// No texts ask nothing.
    #[test]
    fn no_texts_send_no_request() {
        let (url, seen) = serve(answering(4));
        assert!(
            client(&url, 4)
                .embed(Kind::Passage, &[])
                .expect("none")
                .is_empty()
        );
        assert!(seen.lock().expect("lock").is_empty());
    }

    /// RETRIEVE-15: a reply with a vector of another length, a missing index, a repeated one, one
    /// past the inputs, or a number too large for a vector to hold is refused rather than read as
    /// the vectors of the texts sent.
    #[test]
    fn a_reply_that_does_not_match_the_inputs_is_refused() {
        let vector = |n: usize| serde_json::json!(vec![0.5f32; n]);
        let replies = [
            serde_json::json!({"data": [{"index": 0, "embedding": vector(4)}, {"index": 1, "embedding": vector(3)}]}),
            serde_json::json!({"data": [{"index": 0, "embedding": vector(4)}]}),
            serde_json::json!({"data": [{"index": 0, "embedding": vector(4)}, {"index": 0, "embedding": vector(4)}]}),
            serde_json::json!({"data": [{"index": 0, "embedding": vector(4)}, {"index": 2, "embedding": vector(4)}]}),
            serde_json::json!({"error": "no"}),
            serde_json::json!({"data": [{"index": 0, "embedding": vector(4)}, {"index": 1, "embedding": [0.5, 1e40, 0.5, 0.5]}]}),
        ];
        for reply in replies {
            let text = reply.to_string();
            let (url, _) = serve(Box::new(move |_, _| (200, text.clone())));
            let outcome = client(&url, 4).embed(Kind::Passage, &texts(2));
            assert!(
                matches!(outcome, Err(EmbedError::Reply { .. })),
                "{reply} was read as {outcome:?}"
            );
        }
    }

    /// A passage request the endpoint answers with a status worth another attempt is sent again,
    /// up to three times in all.
    #[test]
    fn a_passage_request_is_sent_again_after_a_passing_failure_three_times_at_most() {
        let (url, seen) = serve(Box::new(|number, inputs| {
            if number < 2 {
                (503, "{}".to_string())
            } else {
                (200, reply_for(inputs, 4))
            }
        }));
        assert_eq!(
            client(&url, 4)
                .embed(Kind::Passage, &texts(1))
                .expect("third")
                .len(),
            1
        );
        assert_eq!(seen.lock().expect("lock").len(), 3);

        let (url, seen) = serve(Box::new(|_, _| (503, "{}".to_string())));
        let error = client(&url, 4)
            .embed(Kind::Passage, &texts(1))
            .expect_err("down");
        assert_eq!(status_of(&error), Some(503));
        assert_eq!(seen.lock().expect("lock").len(), 3);
    }

    /// A refusal that would be the same the next time is not sent again, and neither is a query,
    /// whatever failed.
    #[test]
    fn a_refusal_and_a_query_are_sent_once() {
        let (url, seen) = serve(Box::new(|_, _| (400, "{}".to_string())));
        let error = client(&url, 4)
            .embed(Kind::Passage, &texts(1))
            .expect_err("refused");
        assert_eq!(status_of(&error), Some(400));
        assert_eq!(seen.lock().expect("lock").len(), 1);

        let (url, seen) = serve(Box::new(|_, _| (503, "{}".to_string())));
        let error = client(&url, 4)
            .embed(Kind::Query, &texts(1))
            .expect_err("down");
        assert_eq!(status_of(&error), Some(503));
        assert_eq!(seen.lock().expect("lock").len(), 1);
    }

    /// RETRIEVE-16: a passage refused with a 429 is sent again after the wait the service asked
    /// for, and one asking for a wait past the ceiling is sent once.
    #[test]
    fn a_rate_limited_passage_waits_as_asked_unless_the_wait_is_past_the_ceiling() {
        let (url, seen) = serve_headed(Box::new(|number, inputs| {
            if number == 0 {
                (429, vec![("Retry-After".into(), "1".into())], "{}".into())
            } else {
                (200, Vec::new(), reply_for(inputs, 4))
            }
        }));
        let started = Instant::now();
        assert_eq!(
            client(&url, 4)
                .embed(Kind::Passage, &texts(1))
                .expect("second")
                .len(),
            1
        );
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "sent again before the wait asked for"
        );
        assert_eq!(seen.lock().expect("lock").len(), 2);

        let past = (crate::LONGEST_WAIT.as_secs() + 1).to_string();
        let (url, seen) = serve_headed(Box::new(move |_, _| {
            (429, vec![("Retry-After".into(), past.clone())], "{}".into())
        }));
        let started = Instant::now();
        let error = client(&url, 4)
            .embed(Kind::Passage, &texts(1))
            .expect_err("limited");
        assert_eq!(status_of(&error), Some(429));
        assert_eq!(seen.lock().expect("lock").len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the wait was sat through"
        );
    }

    /// RETRIEVE-15: a redirect is not followed, so the token goes to the configured endpoint
    /// alone and the request fails.
    #[test]
    fn a_redirect_is_not_followed_and_the_token_goes_nowhere_else() {
        let (elsewhere, followed) = serve(answering(4));
        let (url, seen) = serve_headed(Box::new(move |_, _| {
            (
                302,
                vec![("Location".into(), elsewhere.clone())],
                "{}".into(),
            )
        }));
        let mut config = client(&url, 4).config;
        config.token = Some(Secret::new("k"));
        let client = EmbeddingsClient::new(config).with_backoff(Duration::from_millis(1));

        for kind in [Kind::Passage, Kind::Query] {
            assert!(
                client.embed(kind, &texts(1)).is_err(),
                "a redirect was read as vectors"
            );
        }
        assert!(
            followed.lock().expect("lock").is_empty(),
            "the redirect was followed"
        );
        let seen = seen.lock().expect("lock");
        assert_eq!(seen.len(), 2, "a redirect was sent again");
        assert!(
            seen.iter()
                .all(|s| s.header("authorization") == Some("Bearer k"))
        );
    }

    /// A query to an endpoint that takes the connection and never answers fails in under the
    /// three seconds a turn could notice, rather than holding the turn for a model reply's bound.
    #[test]
    fn a_query_that_is_never_answered_gives_up_within_its_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!(
            "http://{}/v1/embeddings",
            listener.local_addr().expect("addr")
        );
        let started = Instant::now();
        let outcome = client(&url, 4).embed(Kind::Query, &texts(1));
        assert!(matches!(outcome, Err(EmbedError::Egress(_))), "{outcome:?}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        drop(listener);
    }

    /// A query whose request is slow to be taken and whose reply is slow to come, each under the
    /// bound of its own part and together past a second and a half, fails at the deadline. The
    /// request is larger than the socket buffers, so sending it waits on the server reading it.
    #[test]
    fn a_query_slow_to_start_and_slow_to_finish_fails_at_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!(
            "http://{}/v1/embeddings",
            listener.local_addr().expect("addr")
        );
        let query = vec!["x".repeat(16 * 1024 * 1024)];
        let reply = reply_for(&query, 4);
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                thread::sleep(Duration::from_millis(900));
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut length = 0;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 && !line.trim().is_empty() {
                    if let Some((name, value)) = line.trim_end().split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    line.clear();
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                thread::sleep(Duration::from_millis(900));
                let mut stream = stream;
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(500));
            }
        });
        let started = Instant::now();
        let outcome = client(&url, 4).embed(Kind::Query, &query);
        let took = started.elapsed();
        assert!(
            matches!(
                outcome,
                Err(EmbedError::Egress(EgressError::OutOfTime { .. }))
            ),
            "{outcome:?}"
        );
        assert!(
            took >= QUERY_DEADLINE && took < Duration::from_millis(1750),
            "{took:?}"
        );
    }

    /// The stand-in gives the same text the same unit vector of the length asked for, puts texts
    /// sharing words nearer than texts sharing none, and fails as told.
    #[test]
    fn the_fake_is_deterministic_unit_length_and_can_fail() {
        let fake = FakeEmbedder::new(64);
        let texts = [
            "the deploy key rotated".to_string(),
            "rotated the deploy key".to_string(),
            "lunch on friday".to_string(),
        ];
        let vectors = fake.embed(Kind::Passage, &texts).expect("vectors");
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        assert_eq!(vectors[0].len(), 64);
        assert!((dot(&vectors[0], &vectors[0]) - 1.0).abs() < 1e-5);
        assert!(dot(&vectors[0], &vectors[1]) > dot(&vectors[0], &vectors[2]));
        assert_eq!(vectors, fake.embed(Kind::Passage, &texts).expect("again"));

        let down = FakeEmbedder {
            failing: Some(503),
            ..fake
        };
        assert_eq!(
            status_of(&down.embed(Kind::Query, &texts).expect_err("down")),
            Some(503)
        );
    }

    /// The development endpoint answers 768 unit-length dimensions per input. Reaches the network,
    /// so it runs only when asked: `cargo test -p bravebot-aichat -- --ignored embeddings`.
    #[test]
    #[ignore = "reaches the development embeddings endpoint"]
    fn the_development_endpoint_answers_unit_vectors_of_768() {
        let client = EmbeddingsClient::new(EmbeddingsConfig::dev());
        let vectors = client.embed(Kind::Passage, &texts(33)).expect("passages");
        assert_eq!(vectors.len(), 33);
        for vector in vectors.iter().chain(
            &client
                .embed(Kind::Query, &["hello".to_string()])
                .expect("query"),
        ) {
            assert_eq!(vector.len(), 768);
            let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-2, "norm {norm}");
        }
    }

    fn providers(json: &str) -> Vec<bravebot_config::provider::Provider> {
        let root: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(json).expect("json");
        bravebot_config::provider::Provider::all(&root)
    }

    fn keyed(name: &str) -> Option<String> {
        (name == "OPENAI_API_KEY" || name == "OPENROUTER_API_KEY").then(|| format!("key-{name}"))
    }

    fn selecting(service: Service, kept: bravebot_config::embeddings::Kept) -> Choice {
        let mut choice = Choice {
            selected: Some(service),
            ..Choice::default()
        };
        match service {
            Service::OpenAi => choice.openai = kept,
            Service::Brave => choice.brave = kept,
            Service::OpenRouter => choice.openrouter = kept,
            Service::Custom => choice.custom = kept,
        }
        choice
    }

    const GATEWAYS: &str = r#"{"provider": {
        "openai": {"env": ["OPENAI_API_KEY"]},
        "openrouter": {"env": ["OPENROUTER_API_KEY"]}
    }}"#;

    /// RETRIEVE-34: OpenAI and OpenRouter send to the provider's own host with its own key, and a
    /// published OpenAI model needs no test call.
    #[test]
    fn a_gateway_service_resolves_to_its_providers_host_and_key() {
        use bravebot_config::embeddings::Kept;
        let gateways = providers(GATEWAYS);
        let sources = Sources {
            providers: &gateways,
            brave: true,
            custom_key: None,
        };

        let openai = resolve(
            &selecting(Service::OpenAi, Kept::default()),
            &sources,
            keyed,
        )
        .expect("openai");
        assert_eq!(openai.url, "https://api.openai.com/v1/embeddings");
        assert_eq!(openai.model, OPENAI_DEFAULT_MODEL);
        assert_eq!(openai.dimensions, 1536);
        assert_eq!(openai.query_prefix, "");
        assert_eq!(
            openai.token.as_ref().map(Secret::expose),
            Some("key-OPENAI_API_KEY")
        );

        let router = selecting(
            Service::OpenRouter,
            Kept {
                model: Some("cohere/embed-5-fast".to_string()),
                ..Kept::default()
            },
        );
        assert_eq!(
            resolve(&router, &sources, keyed).err(),
            Some(Unresolved::Untested(Service::OpenRouter))
        );
        let mut tested = router.clone();
        tested.openrouter.dimensions = Some(1024);
        let resolved = resolve(&tested, &sources, keyed).expect("openrouter");
        assert_eq!(resolved.url, "https://openrouter.ai/api/v1/embeddings");
        assert_eq!(resolved.dimensions, 1024);
        assert_eq!(
            resolve(&tested, &sources, |_| None).err(),
            Some(Unresolved::NoKey(Service::OpenRouter))
        );
    }

    /// RETRIEVE-34: a published OpenAI length is used over a kept one, an unpublished model needs
    /// its length kept, OpenRouter needs a model, kept prefixes go as written, and a block that
    /// names no key sends none.
    #[test]
    fn each_gateway_rule_holds_for_lengths_models_prefixes_and_keys() {
        use bravebot_config::embeddings::Kept;
        let gateways = providers(GATEWAYS);
        let sources = Sources {
            providers: &gateways,
            brave: false,
            custom_key: None,
        };
        let openai = |model: &str, dimensions| {
            selecting(
                Service::OpenAi,
                Kept {
                    model: Some(model.to_string()),
                    dimensions,
                    query_prefix: Some("query: ".to_string()),
                    passage_prefix: Some(" passage ".to_string()),
                    ..Kept::default()
                },
            )
        };

        let large = resolve(
            &openai("text-embedding-3-large", Some(999)),
            &sources,
            keyed,
        )
        .expect("large");
        assert_eq!(large.dimensions, 3072);
        assert_eq!(large.query_prefix, "query: ");
        assert_eq!(large.passage_prefix, " passage ");
        assert_eq!(
            resolve(&openai("a-later-model", Some(256)), &sources, keyed)
                .expect("kept length")
                .dimensions,
            256
        );
        assert_eq!(
            resolve(&openai("a-later-model", None), &sources, keyed).err(),
            Some(Unresolved::Untested(Service::OpenAi))
        );
        assert_eq!(
            resolve(
                &selecting(
                    Service::OpenRouter,
                    Kept {
                        dimensions: Some(8),
                        ..Kept::default()
                    }
                ),
                &sources,
                keyed
            )
            .err(),
            Some(Unresolved::NoModel(Service::OpenRouter))
        );

        let local = providers(
            r#"{"provider": {"openai": {"options": {"baseURL": "http://localhost:4000/v1"}}}}"#,
        );
        let keyless = resolve(
            &openai("text-embedding-3-small", None),
            &Sources {
                providers: &local,
                brave: false,
                custom_key: None,
            },
            keyed,
        )
        .expect("a block that names no key");
        assert_eq!(keyless.url, "http://localhost:4000/v1/embeddings");
        assert!(keyless.token.is_none());
    }

    /// RETRIEVE-33: a ChatGPT sign-in is not OpenAI configured with a key, since its account's
    /// tokens cannot call `/v1/embeddings`.
    #[test]
    fn a_chatgpt_sign_in_does_not_offer_openai() {
        use bravebot_config::embeddings::Kept;
        let signed_in = [bravebot_config::provider::Provider::chatgpt_account()];
        let sources = Sources {
            providers: &signed_in,
            brave: false,
            custom_key: None,
        };
        assert!(!offered(Service::OpenAi, &sources));
        assert_eq!(
            resolve(
                &selecting(Service::OpenAi, Kept::default()),
                &sources,
                keyed
            )
            .err(),
            Some(Unresolved::NotOffered(Service::OpenAi))
        );
    }

    /// RETRIEVE-33: a service whose provider is not configured is not used, and nothing stands in
    /// for it, while no selection at all is Brave where this build reaches it.
    #[test]
    fn a_service_not_set_up_is_refused_and_no_selection_is_brave() {
        use bravebot_config::embeddings::Kept;
        let sources = Sources {
            providers: &[],
            brave: true,
            custom_key: None,
        };
        assert_eq!(
            resolve(
                &selecting(Service::OpenAi, Kept::default()),
                &sources,
                keyed
            )
            .err(),
            Some(Unresolved::NotOffered(Service::OpenAi))
        );
        assert!(!offered(Service::OpenRouter, &sources));
        assert!(offered(Service::Custom, &sources));
        assert_eq!(
            resolve(&Choice::default(), &sources, keyed)
                .expect("brave")
                .url,
            DEV_EMBEDDINGS_URL
        );
        let without_brave = Sources {
            brave: false,
            ..sources
        };
        assert_eq!(
            resolve(&Choice::default(), &without_brave, keyed).err(),
            Some(Unresolved::NothingSelected)
        );
        assert_eq!(
            resolve(
                &selecting(Service::Brave, Kept::default()),
                &without_brave,
                keyed
            )
            .err(),
            Some(Unresolved::NotOffered(Service::Brave))
        );
    }

    /// RETRIEVE-34: the custom endpoint needs an http(s) URL, a model and a tested length, and
    /// carries the key stored for it and the prefixes as written.
    #[test]
    fn the_custom_endpoint_needs_its_url_model_and_length() {
        use bravebot_config::embeddings::Kept;
        let sources = Sources {
            providers: &[],
            brave: false,
            custom_key: Some(Secret::new("custom-key")),
        };
        let full = Kept {
            url: Some("https://embed.example/v1/embeddings".to_string()),
            model: Some("m".to_string()),
            dimensions: Some(384),
            query_prefix: Some("query: ".to_string()),
            passage_prefix: None,
        };
        let resolved =
            resolve(&selecting(Service::Custom, full.clone()), &sources, keyed).expect("custom");
        assert_eq!(resolved.url, "https://embed.example/v1/embeddings");
        assert_eq!(resolved.query_prefix, "query: ");
        assert_eq!(resolved.passage_prefix, "");
        assert_eq!(
            resolved.token.as_ref().map(Secret::expose),
            Some("custom-key")
        );
        let plain = Kept {
            url: Some("http://localhost:8080/v1/embeddings".to_string()),
            ..full.clone()
        };
        assert!(resolve(&selecting(Service::Custom, plain), &sources, keyed).is_ok());
        for (kept, why) in [
            (
                Kept {
                    url: Some("ftp://embed.example".to_string()),
                    ..full.clone()
                },
                Unresolved::NoUrl,
            ),
            (
                Kept {
                    model: None,
                    ..full.clone()
                },
                Unresolved::NoModel(Service::Custom),
            ),
            (
                Kept {
                    dimensions: None,
                    ..full.clone()
                },
                Unresolved::Untested(Service::Custom),
            ),
        ] {
            assert_eq!(
                resolve(&selecting(Service::Custom, kept), &sources, keyed).err(),
                Some(why)
            );
        }
    }

    /// RETRIEVE-36: a test call sends one passage with its prefix, once, and takes whatever length
    /// the vector has; an empty vector, one holding a number too large, or a failed answer is not
    /// retried and is an error.
    #[test]
    fn a_probe_sends_one_passage_once_and_takes_its_length() {
        let (url, seen) = serve(Box::new(|number, _| match number {
            0 => (
                200,
                r#"{"data": [{"index": 0, "embedding": [0.1, 0.2, 0.3]}]}"#.to_string(),
            ),
            1 => (
                200,
                r#"{"data": [{"index": 0, "embedding": []}]}"#.to_string(),
            ),
            2 => (
                200,
                r#"{"data": [{"index": 0, "embedding": [0.1, 1e40, 0.3]}]}"#.to_string(),
            ),
            _ => (500, "{}".to_string()),
        }));
        let probing = client(&url, 768);

        assert_eq!(probing.probe("hello").expect("probed"), 3);
        assert_eq!(seen.lock().expect("lock")[0].inputs(), ["P: hello"]);
        assert!(
            probing.probe("hello").is_err(),
            "an empty vector was a length"
        );
        assert!(
            probing.probe("hello").is_err(),
            "a vector past what a number holds was a length"
        );
        assert!(
            probing.probe("hello").is_err(),
            "a failed answer was a length"
        );
        assert_eq!(seen.lock().expect("lock").len(), 4, "a probe was retried");
    }

    /// RETRIEVE-5: an endpoint is named by its URL without anything a key could hide in.
    #[test]
    fn an_endpoint_s_identity_holds_no_query_fragment_or_user() {
        for (url, identity) in [
            (DEV_EMBEDDINGS_URL, DEV_EMBEDDINGS_URL),
            (
                "https://h.example/v1/embeddings?key=s3cret#top",
                "https://h.example/v1/embeddings",
            ),
            (
                "https://me:s3cret@h.example:8443/embed",
                "https://h.example:8443/embed",
            ),
            ("http://h.example?k=s3cret", "http://h.example"),
            ("http://me@h.example#s3cret", "http://h.example"),
        ] {
            assert_eq!(endpoint_identity(url), identity);
        }
        let client = EmbeddingsClient::new(EmbeddingsConfig {
            url: "https://h.example/e?api_key=s3cret".to_string(),
            ..EmbeddingsConfig::dev()
        });
        assert_eq!(client.endpoint(), "https://h.example/e");
    }

    /// RETRIEVE-37: the models beside an embeddings URL are asked for with its key and read with
    /// the fields left out absent, and a reply that is no listing is an error.
    #[test]
    fn the_models_beside_an_embeddings_url_are_listed() {
        let (url, seen) = serve(Box::new(|number, _| match number {
            0 => (
                200,
                r#"{"data": [{"id": "a/e", "name": "A", "context_length": 512,
                    "pricing": {"prompt": "0.1", "completion": "0"}}, {"id": "b/e"}]}"#
                    .to_string(),
            ),
            _ => (200, "[]".to_string()),
        }));
        let key = Secret::new("list-key");

        let models = list_models(&url, Some(&key)).expect("listed");

        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["a/e", "b/e"]
        );
        assert_eq!(models[0].name.as_deref(), Some("A"));
        assert_eq!(models[0].context_length, Some(512));
        assert_eq!(
            models[0].pricing.as_ref().and_then(|p| p.prompt.as_deref()),
            Some("0.1")
        );
        assert!(models[1].name.is_none() && models[1].pricing.is_none());
        assert_eq!(
            seen.lock().expect("lock")[0].header("authorization"),
            Some("Bearer list-key")
        );
        assert!(matches!(
            list_models(&url, None),
            Err(EmbedError::Reply { .. })
        ));
    }

    /// RETRIEVE-37: a body cut off at the read limit is not the whole listing, and what parses of
    /// it would be shown as the models there are, with some silently missing.
    #[test]
    fn a_listing_cut_off_at_the_reply_limit_is_no_listing() {
        let (url, _) = serve(Box::new(|_, _| {
            let padding = "x".repeat(bravebot_net::MAX_RESPONSE_BYTES);
            (
                200,
                format!(r#"{{"data": [{{"id": "a/e", "name": "{padding}"}}]}}"#),
            )
        }));

        assert!(matches!(
            list_models(&url, None),
            Err(EmbedError::Egress(EgressError::Transport { .. }))
        ));
    }

    /// RETRIEVE-36: a candidate is what a test call is made to, so a length not yet known is no
    /// refusal there, while a missing key is named before a missing model.
    #[test]
    fn a_candidate_may_have_no_length_yet() {
        use bravebot_config::embeddings::Kept;
        let sources = Sources {
            providers: &[],
            brave: false,
            custom_key: None,
        };
        let custom = selecting(
            Service::Custom,
            Kept {
                url: Some("https://embed.example/v1/embeddings".to_string()),
                model: Some("m".to_string()),
                ..Kept::default()
            },
        );
        let config = candidate(Service::Custom, &custom, &sources, keyed).expect("a candidate");
        assert_eq!(config.dimensions, 0);
        assert!(config.token.is_none());

        let gateways = providers(GATEWAYS);
        let sources = Sources {
            providers: &gateways,
            brave: false,
            custom_key: None,
        };
        let openrouter = selecting(Service::OpenRouter, Kept::default());
        assert_eq!(
            candidate(Service::OpenRouter, &openrouter, &sources, |_| None).err(),
            Some(Unresolved::NoKey(Service::OpenRouter))
        );
        assert_eq!(
            candidate(Service::OpenRouter, &openrouter, &sources, keyed).err(),
            Some(Unresolved::NoModel(Service::OpenRouter))
        );
        assert_eq!(
            candidate(Service::Brave, &openrouter, &sources, keyed).err(),
            Some(Unresolved::NotOffered(Service::Brave))
        );
    }
}
