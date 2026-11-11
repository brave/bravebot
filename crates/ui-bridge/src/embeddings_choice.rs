//! The embeddings service the window chooses for retrieval, what is kept for each one, the test
//! call that finds a model's length, and OpenRouter's list of embedding models
//! (`docs/specs/retrieval.md`, RETRIEVE-35 to RETRIEVE-38).

use crate::protocol::{ErrorCode, Failure, Request};
use crate::retrieval::{chosen, host, layered, sourced, state_in, variable};
use bravebot_aichat::embeddings::{
    EmbedError, EmbeddingsClient, EmbeddingsConfig, OPENAI_MODELS, Sources, Unresolved, candidate,
    dimensions_of, gateway_key, list_models, model_of, offered, url_of,
};
use bravebot_config::embeddings::{Choice, Kept, Service};
use bravebot_config::{Managed, Secret, Settings};
use bravebot_net::EgressError;
use bravebot_session::store;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The passage a test call embeds: a constant, so the call sends nothing of the person's.
const TEST_PASSAGE: &str = "bravebot embeddings test";

/// How long a listing of OpenRouter's models is answered from memory.
const LISTING_KEPT: Duration = Duration::from_secs(300);

/// Every service in [`Service::ALL`]'s order, as the window offers it: whether it can be chosen,
/// why it cannot be used as it stands where it cannot, where it sends, and what was kept for it.
/// The custom endpoint's key is never in it, only whether one is stored. Brave's is left out
/// where it is not offered, since nothing the person can set up offers it (RETRIEVE-35).
pub(crate) fn options(choice: &Choice, sources: &Sources<'_>) -> Value {
    Service::ALL
        .into_iter()
        .filter(|service| *service != Service::Brave || sources.brave)
        .map(|service| option(service, choice, sources))
        .collect()
}

fn option(service: Service, choice: &Choice, sources: &Sources<'_>) -> Value {
    let reason = match candidate(service, choice, sources, variable) {
        Err(why) => Some(why.to_string()),
        Ok(config) if config.dimensions == 0 => Some(Unresolved::Untested(service).to_string()),
        Ok(_) => None,
    };
    let kept = choice.kept(service);
    let sends = url_of(service, sources, choice);
    let mut option = json!({
        "service": service.word(),
        "available": offered(service, sources),
        "reason": reason,
        // The custom URL as written, so a field shows what was typed even before it is usable.
        "url": if service == Service::Custom { kept.url.clone() } else { sends.clone() },
        "host": sends.as_deref().map(host),
        "model": model_of(service, choice),
        "dimensions": dimensions_of(service, choice),
        "queryPrefix": kept.query_prefix,
        "passagePrefix": kept.passage_prefix,
    });
    match service {
        Service::Custom => option["keyStored"] = json!(sources.custom_key.is_some()),
        Service::OpenAi => option["models"] = json!(OPENAI_MODELS),
        Service::Brave | Service::OpenRouter => {}
    }
    option
}

/// What fixes the choice so the person's own file cannot change it, where something does: what
/// fixes `retrieval.enabled`, or the file the process was started on stating `retrieval.embeddings`,
/// which stands over the person's own.
pub(crate) fn locked(
    settings: &Settings,
    managed: &Managed,
    own: Option<&Path>,
    selected: Option<&Path>,
) -> Option<Value> {
    crate::retrieval::locked(settings, managed, own, selected).or_else(|| {
        let run = selected.filter(|run| Some(*run) != own)?;
        (Settings::layered(None, None, Some(run)).embeddings() != &Choice::default())
            .then(|| json!({ "by": "run", "path": run }))
    })
}

/// One field of `values`: left as it is, written, or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    Unchanged,
    Set(String),
    Removed,
}

impl Field {
    /// `name` of `values`: a string is written, `null` removes, absence leaves it. An empty model
    /// or URL is no model or URL; an empty prefix is a prefix of nothing, which is a choice.
    fn read(values: &Map<String, Value>, name: &str, empty_is_none: bool) -> Result<Self, Failure> {
        match values.get(name) {
            None => Ok(Self::Unchanged),
            Some(Value::Null) => Ok(Self::Removed),
            Some(Value::String(text)) if empty_is_none && text.trim().is_empty() => {
                Ok(Self::Removed)
            }
            Some(Value::String(text)) if empty_is_none => Ok(Self::Set(text.trim().to_string())),
            Some(Value::String(text)) => Ok(Self::Set(text.clone())),
            Some(_) => Err(Failure::bad_request(format!(
                "`values.{name}` must be a string or null"
            ))),
        }
    }

    fn apply(&self, to: &mut Option<String>) -> bool {
        let was = to.clone();
        match self {
            Self::Unchanged => {}
            Self::Set(value) => *to = Some(value.clone()),
            Self::Removed => *to = None,
        }
        *to != was
    }

    fn written(&self) -> Option<Option<Value>> {
        match self {
            Self::Unchanged => None,
            Self::Set(value) => Some(Some(Value::String(value.clone()))),
            Self::Removed => Some(None),
        }
    }
}

/// What one `retrieval.embeddings.set` asks for.
#[derive(Debug)]
struct Asked {
    select: Option<Service>,
    /// The service `model`, `url` and the prefixes are written for.
    service: Option<Service>,
    model: Field,
    url: Field,
    query_prefix: Field,
    passage_prefix: Field,
    key: Field,
    disclosed: Option<String>,
}

impl Asked {
    fn read(request: &Request) -> Result<Self, Failure> {
        let service = |name: &str| -> Result<Option<Service>, Failure> {
            match request.param(name) {
                Value::Null => Ok(None),
                Value::String(word) => Service::from_word(word).map(Some).ok_or_else(|| {
                    Failure::bad_request(format!("`{name}` is not an embeddings service: {word}"))
                }),
                _ => Err(Failure::bad_request(format!("`{name}` must be a string"))),
            }
        };
        let select = service("select")?;
        let named = service("service")?;
        let values = match request.param("values") {
            Value::Null => Map::new(),
            Value::Object(values) => values.clone(),
            _ => return Err(Failure::bad_request("`values` must be an object")),
        };
        if let Some(unknown) = values.keys().find(|key| {
            !["model", "url", "queryPrefix", "passagePrefix", "key"].contains(&key.as_str())
        }) {
            return Err(Failure::bad_request(format!(
                "`values.{unknown}` is not something kept for a service"
            )));
        }
        let asked = Self {
            select,
            service: named.or(select),
            model: Field::read(&values, "model", true)?,
            url: Field::read(&values, "url", true)?,
            query_prefix: Field::read(&values, "queryPrefix", false)?,
            passage_prefix: Field::read(&values, "passagePrefix", false)?,
            key: Field::read(&values, "key", true)?,
            disclosed: request.optional_string("disclosed"),
        };
        let writes = !values.is_empty();
        match asked.service {
            None if writes => Err(Failure::bad_request(
                "`values` need a `service` to keep them for",
            )),
            None if asked.select.is_none() => Err(Failure::bad_request(
                "nothing to change: no `select` and no `values`",
            )),
            Some(service)
                if service != Service::Custom
                    && (asked.url != Field::Unchanged || asked.key != Field::Unchanged) =>
            {
                Err(Failure::bad_request(
                    "only the custom endpoint keeps a `url` or a `key`",
                ))
            }
            Some(Service::Brave) if asked.model != Field::Unchanged => {
                Err(Failure::bad_request("Brave's model is not chosen"))
            }
            _ => Ok(asked),
        }
    }
}

/// What the request did.
enum Done {
    /// Nothing was written: retrieval is on and would move to an endpoint the person has not been
    /// shown, which the window shows and asks again naming.
    NeedsDisclosure(String),
    /// Written, and `tested` against the service now selected where it was touched and usable.
    Written { tested: Option<Value> },
}

/// Select a service for retrieval and keep values for one, in the person's own settings file, then
/// test the selected service where the request touched it (RETRIEVE-35, RETRIEVE-36).
pub fn set(request: &Request, selected: Option<&Path>) -> Result<Value, Failure> {
    let asked = Asked::read(request)?;
    let home = bravebot_agent::home::directory()
        .ok_or_else(|| Failure::new(ErrorCode::Config, "there is no home directory to write to"))?;
    let managed = Managed::load();
    let done = apply(
        &home,
        selected,
        &managed,
        &asked,
        store::load_retrieval_disclosed().as_deref(),
        store::save_retrieval_disclosed,
    )?;
    let state = state_in(
        Some(&home),
        selected,
        &managed,
        store::load_retrieval_disclosed().as_deref(),
        &chosen(Some(&home), selected, &managed),
    );
    Ok(answer(&state, &done))
}

fn answer(state: &Value, done: &Done) -> Value {
    match done {
        Done::NeedsDisclosure(url) => json!({ "state": state, "needsDisclosure": url }),
        Done::Written { tested } => json!({ "state": state, "test": tested }),
    }
}

fn kept_mut(choice: &mut Choice, service: Service) -> &mut Kept {
    match service {
        Service::OpenAi => &mut choice.openai,
        Service::Brave => &mut choice.brave,
        Service::OpenRouter => &mut choice.openrouter,
        Service::Custom => &mut choice.custom,
    }
}

fn apply(
    home: &Path,
    selected: Option<&Path>,
    managed: &Managed,
    asked: &Asked,
    saved: Option<&str>,
    keep: impl FnOnce(&str) -> bool,
) -> Result<Done, Failure> {
    let settings = layered(Some(home), selected, managed);
    refuse_locked(home, &settings, managed, selected)?;

    // The choice as it will be, to be refused before anything is written.
    let mut choice = settings.embeddings().clone();
    if let Some(service) = asked.select {
        choice.selected = Some(service);
    }
    let mut moved = false;
    if let Some(service) = asked.service {
        let kept = kept_mut(&mut choice, service);
        moved |= asked.model.apply(&mut kept.model);
        moved |= asked.url.apply(&mut kept.url);
        asked.query_prefix.apply(&mut kept.query_prefix);
        asked.passage_prefix.apply(&mut kept.passage_prefix);
        // A length found for another model or URL is not this one's.
        if moved {
            kept.dimensions = None;
        }
    }
    let key = match &asked.key {
        Field::Unchanged => None,
        Field::Set(key) => Some(Some(Secret::new(key.clone()))),
        Field::Removed => Some(None),
    };
    let touched = asked.select.is_some() || asked.service == choice.selected;
    // The endpoint the request moves retrieval to, where it touches the one in force.
    let target = sourced(Some(home), &settings, managed, |sources| {
        let sources = Sources {
            providers: sources.providers,
            brave: sources.brave,
            custom_key: key.clone().unwrap_or_else(|| sources.custom_key.clone()),
        };
        bravebot_aichat::embeddings::in_force(&choice, &sources)
            .filter(|_| touched)
            .map(|service| candidate(service, &choice, &sources, variable))
    });
    // While retrieval is on, an endpoint whose length is not known is tested before anything is
    // written, so a failed test cannot leave retrieval on with an endpoint it cannot use.
    let mut pretested = None;
    if settings.retrieval()
        && let Some(target) = &target
    {
        let config = match target {
            Err(why) => {
                return Err(Failure::bad_request(format!(
                    "retrieval is on, so the embeddings service was left as it was: {why}"
                )));
            }
            Ok(config) => config,
        };
        let shown = saved == Some(config.url.as_str());
        if !shown && asked.disclosed.as_deref() != Some(config.url.as_str()) {
            return Ok(Done::NeedsDisclosure(config.url.clone()));
        }
        if config.dimensions == 0 {
            let result = checked(config, &choice);
            if let Err(error) = &result {
                return Err(Failure::bad_request(format!(
                    "retrieval is on, so the embeddings service was left as it was: its test \
                     call failed: {error}"
                )));
            }
            pretested = Some(result);
        }
        if !shown && !keep(&config.url) {
            return Err(Failure::new(
                ErrorCode::Internal,
                "the embeddings service was left as it was: that the person was shown where it \
                 sends could not be kept",
            ));
        }
    }
    let learned = pretested
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|(dimensions, _)| *dimensions)
        .zip(learns(&choice));

    write_choice(home, asked, moved, learned)?;
    if let Some(key) = &key {
        store_key(home, key.as_ref())?;
    }

    let tested = match (pretested, target) {
        (Some(result), _) => Some(tested(result)),
        (None, Some(Ok(config))) => Some(test_and_keep(home, &config, &choice)?),
        _ => None,
    };
    Ok(Done::Written { tested })
}

/// Refuse a change the managed file or the run's settings file fixes.
fn refuse_locked(
    home: &Path,
    settings: &Settings,
    managed: &Managed,
    selected: Option<&Path>,
) -> Result<(), Failure> {
    let own = bravebot_config::user_settings_file(home);
    let Some(lock) = locked(settings, managed, Some(&own), selected) else {
        return Ok(());
    };
    let path = lock["path"].as_str().unwrap_or_default();
    Err(Failure::new(
        ErrorCode::Config,
        match lock["by"].as_str() {
            Some("managed") => {
                format!("retrieval is turned off by the administrator's file {path}")
            }
            _ => format!("the embeddings service is set by {path} for this run"),
        },
    ))
}

/// Write the selection, the values `asked` sets for its service, and a length `learned`, to the
/// person's own settings file.
fn write_choice(
    home: &Path,
    asked: &Asked,
    moved: bool,
    learned: Option<(usize, Service)>,
) -> Result<(), Failure> {
    let mut file = crate::connectors::user_settings(home)?;
    let mut kept = asked
        .select
        .is_none_or(|service| file.select_embeddings(service));
    if let Some(service) = asked.service {
        for (name, field) in [
            ("model", &asked.model),
            ("url", &asked.url),
            ("queryPrefix", &asked.query_prefix),
            ("passagePrefix", &asked.passage_prefix),
        ] {
            if let Some(value) = field.written() {
                kept &= file.keep_embeddings(service, name, value);
            }
        }
        if moved {
            kept &= file.keep_embeddings(service, "dimensions", None);
        }
    }
    if let Some((dimensions, service)) = learned {
        kept &= file.keep_embeddings(service, "dimensions", Some(json!(dimensions)));
    }
    if !kept {
        return Err(Failure::new(
            ErrorCode::Config,
            format!(
                "{} was left as it is: its `retrieval` or `retrieval.embeddings` is not a block",
                bravebot_config::user_settings_file(home).display()
            ),
        ));
    }
    crate::connectors::write_settings(&file)
}

/// Keep `key` as the custom endpoint's, readable by this user alone, or remove it for `None`.
fn store_key(home: &Path, key: Option<&Secret>) -> Result<(), Failure> {
    bravebot_agent::key_entry::store_embeddings_key(home, key).map_err(|error| {
        Failure::new(
            ErrorCode::Config,
            format!(
                "the key was not kept in {}: {error}",
                bravebot_config::embeddings_key::file(home).display()
            ),
        )
    })
}

/// Test `config`, and keep the length the reply has where the service's length is not published.
fn test_and_keep(
    home: &Path,
    config: &EmbeddingsConfig,
    choice: &Choice,
) -> Result<Value, Failure> {
    let result = checked(config, choice);
    if let (Ok((dimensions, _)), Some(service)) = (&result, learns(choice)) {
        let mut file = crate::connectors::user_settings(home)?;
        if file.keep_embeddings(service, "dimensions", Some(json!(dimensions))) {
            crate::connectors::write_settings(&file)?;
        }
    }
    Ok(tested(result))
}

/// A test call to `config`, a reply of another length than the selected service publishes being
/// a failure.
fn checked(config: &EmbeddingsConfig, choice: &Choice) -> Result<(usize, Duration), EmbedError> {
    let expected = choice
        .selected
        .filter(|service| published(*service, choice))
        .and_then(|service| dimensions_of(service, choice));
    test(config).and_then(|(dimensions, took)| match expected {
        Some(expected) if expected != dimensions => Err(EmbedError::Reply {
            detail: format!("a vector of {dimensions} dimensions where {expected} were expected"),
        }),
        _ => Ok((dimensions, took)),
    })
}

/// The selected service, where the length a test call finds is kept for it.
fn learns(choice: &Choice) -> Option<Service> {
    choice
        .selected
        .filter(|service| !published(*service, choice))
}

/// Whether the length of `service`'s vectors is known without a test call.
fn published(service: Service, choice: &Choice) -> bool {
    let unkept = Choice {
        selected: choice.selected,
        openai: Kept {
            dimensions: None,
            ..choice.openai.clone()
        },
        ..Choice::default()
    };
    dimensions_of(service, &unkept).is_some()
}

/// One passage at `config`, the length its vector has, and how long the answer took.
fn test(config: &EmbeddingsConfig) -> Result<(usize, Duration), EmbedError> {
    let started = Instant::now();
    let dimensions = EmbeddingsClient::new(config.clone()).probe(TEST_PASSAGE)?;
    Ok((dimensions, started.elapsed()))
}

/// A test call as the window is told it (RETRIEVE-36).
fn tested(result: Result<(usize, Duration), EmbedError>) -> Value {
    match result {
        Ok((dimensions, took)) => json!({
            "ok": true,
            "dimensions": dimensions,
            "millis": u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
        }),
        Err(error) => json!({
            "ok": false,
            "kind": kind_of(&error),
            "message": error.to_string(),
        }),
    }
}

/// What went wrong, from what the transport decided and never from a reply's text.
fn kind_of(error: &EmbedError) -> &'static str {
    use bravebot_aichat::embeddings::Failure as Why;
    match error {
        EmbedError::Egress(EgressError::Status {
            status: 401 | 403, ..
        }) => "key",
        EmbedError::Egress(EgressError::Status { status: 404, .. }) => "not-found",
        EmbedError::Reply { .. } => "shape",
        _ if matches!(error.failure(), Why::Unreachable | Why::OutOfTime) => "network",
        _ => "other",
    }
}

/// The last listing of OpenRouter's models: the address it was asked at, a fingerprint of the key
/// it was asked with, when, and the rows. A changed key asks again.
static LISTED: Mutex<Option<(String, u64, Instant, Value)>> = Mutex::new(None);

/// A fingerprint of `token`, seeded afresh in each process, so the key itself is not kept.
fn fingerprint(token: Option<&Secret>) -> u64 {
    use std::hash::BuildHasher;
    static SEED: std::sync::OnceLock<std::collections::hash_map::RandomState> =
        std::sync::OnceLock::new();
    SEED.get_or_init(Default::default)
        .hash_one(token.map(Secret::expose))
}

/// OpenRouter's embedding models, asked with its inference key, and answered from memory for a
/// few minutes unless `refresh` is given (RETRIEVE-37).
pub fn models(request: &Request, selected: Option<&Path>) -> Result<Value, Failure> {
    let refresh = request.param("refresh").as_bool().unwrap_or(false);
    let home = bravebot_agent::home::directory();
    models_in(home.as_deref(), selected, &Managed::load(), refresh)
}

fn models_in(
    home: Option<&Path>,
    selected: Option<&Path>,
    managed: &Managed,
    refresh: bool,
) -> Result<Value, Failure> {
    let settings = layered(home, selected, managed);
    let (url, token) = sourced(home, &settings, managed, |sources| {
        let unusable = |why: Unresolved| Failure::new(ErrorCode::Config, why.to_string());
        if !offered(Service::OpenRouter, sources) {
            return Err(unusable(Unresolved::NotOffered(Service::OpenRouter)));
        }
        let token = gateway_key(Service::OpenRouter, sources, variable).map_err(unusable)?;
        let url = url_of(Service::OpenRouter, sources, settings.embeddings())
            .ok_or_else(|| unusable(Unresolved::NotOffered(Service::OpenRouter)))?;
        Ok((url, token))
    })?;
    listed(&url, token.as_ref(), refresh, Instant::now())
}

fn listed(
    url: &str,
    token: Option<&Secret>,
    refresh: bool,
    now: Instant,
) -> Result<Value, Failure> {
    let mut memory = LISTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !refresh
        && let Some((at, keyed, when, rows)) = memory.as_ref()
        && at == url
        && *keyed == fingerprint(token)
        && now.saturating_duration_since(*when) < LISTING_KEPT
    {
        return Ok(json!({ "models": rows }));
    }
    let rows: Value = list_models(url, token)
        .map_err(|error| Failure::new(ErrorCode::Config, unlisted(&error)))?
        .into_iter()
        .map(|model| {
            json!({
                "name": model.name.unwrap_or_else(|| model.id.clone()),
                "id": model.id,
                "contextLength": model.context_length,
                "pricing": { "prompt": model.pricing.and_then(|pricing| pricing.prompt) },
            })
        })
        .collect();
    *memory = Some((url.to_string(), fingerprint(token), now, rows.clone()));
    Ok(json!({ "models": rows }))
}

/// Why OpenRouter's models could not be listed, in a sentence the window shows.
fn unlisted(error: &EmbedError) -> String {
    match error {
        EmbedError::Egress(EgressError::Status {
            status: status @ (401 | 403),
            ..
        }) => format!("OpenRouter refused its key (HTTP {status})"),
        EmbedError::Egress(EgressError::Status { status, .. }) => {
            format!("OpenRouter answered the list of embedding models with HTTP {status}")
        }
        EmbedError::Reply { detail } => {
            format!("OpenRouter's list of embedding models was not usable: {detail}")
        }
        other => format!("OpenRouter's embedding models could not be listed: {other}"),
    }
}

#[cfg(test)]
#[path = "embeddings_choice_tests.rs"]
mod tests;
