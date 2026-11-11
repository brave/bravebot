//! The window's switch for `retrieval.enabled`, written to the person's own settings file
//! (`docs/specs/retrieval.md`, RETRIEVE-24 and RETRIEVE-25).

use crate::protocol::{ErrorCode, Failure, Request};
use bravebot_aichat::embeddings::{
    Embedder, EmbeddingsClient, EmbeddingsConfig, Kind, Sources, Unresolved,
};
use bravebot_config::embeddings::Service;
use bravebot_config::{Managed, Settings};
use bravebot_session::store;
use serde_json::{Value, json};
use std::path::Path;

/// What the window shows: whether retrieval is on, the file the switch writes, the file that
/// decided it, what fixes it where something does, whether the person was told where it sends,
/// and where that is.
pub fn state(selected: Option<&Path>) -> Value {
    let home = bravebot_agent::home::directory();
    let managed = Managed::load();
    let disclosed = store::load_retrieval_disclosed();
    let chosen = chosen(home.as_deref(), selected, &managed);
    state_in(
        home.as_deref(),
        selected,
        &managed,
        disclosed.as_deref(),
        &chosen,
    )
}

/// Whether the settings in force turn retrieval on now.
pub fn on(selected: Option<&Path>) -> bool {
    let home = bravebot_agent::home::directory();
    layered(home.as_deref(), selected, &Managed::load()).retrieval()
}

/// The endpoint the settings in force choose for retrieval, with its key (RETRIEVE-34).
pub fn resolved(selected: Option<&Path>) -> Result<EmbeddingsConfig, Unresolved> {
    let home = bravebot_agent::home::directory();
    chosen(home.as_deref(), selected, &Managed::load()).endpoint
}

/// The service the settings put in force, the endpoint it comes to, and what the window is shown
/// of every service.
pub(crate) struct Chosen {
    pub(crate) service: Option<Service>,
    pub(crate) endpoint: Result<EmbeddingsConfig, Unresolved>,
    /// The service the settings name, which may not be the one in force.
    pub(crate) selected: Option<Service>,
    pub(crate) options: Value,
}

pub(crate) fn chosen(home: Option<&Path>, selected: Option<&Path>, managed: &Managed) -> Chosen {
    chosen_with(home, selected, managed, brave_flagged())
}

/// [`chosen`], with Brave's service asked for or not.
pub(crate) fn chosen_with(
    home: Option<&Path>,
    selected: Option<&Path>,
    managed: &Managed,
    brave: bool,
) -> Chosen {
    let settings = layered(home, selected, managed);
    sourced_with(home, &settings, managed, brave, |sources| {
        let choice = settings.embeddings();
        Chosen {
            service: bravebot_aichat::embeddings::in_force(choice, sources),
            endpoint: bravebot_aichat::embeddings::resolve(choice, sources, variable),
            selected: choice.selected,
            options: crate::embeddings_choice::options(choice, sources),
        }
    })
}

/// What the endpoint is chosen from besides the settings, for `then`.
pub(crate) fn sourced<T>(
    home: Option<&Path>,
    settings: &Settings,
    managed: &Managed,
    then: impl FnOnce(&Sources<'_>) -> T,
) -> T {
    sourced_with(home, settings, managed, brave_flagged(), then)
}

fn sourced_with<T>(
    home: Option<&Path>,
    settings: &Settings,
    managed: &Managed,
    brave: bool,
    then: impl FnOnce(&Sources<'_>) -> T,
) -> T {
    // Settings that configure no inference still choose an endpoint: the custom one needs none.
    let config = bravebot_config::Config::from_env_and_settings(settings, managed).ok();
    then(&Sources {
        providers: config
            .as_ref()
            .map_or(&[], |config| config.providers.as_slice()),
        brave: offers_brave(reaches_brave(config.as_ref()), brave),
        custom_key: home
            .and_then(|home| bravebot_config::embeddings_key::read(home).ok().flatten()),
    })
}

/// Whether Brave's embeddings service is offered: only to a build that reaches Brave, and only
/// where [`env_var::BRAVE_EMBEDDINGS`](bravebot_config::env_var::BRAVE_EMBEDDINGS) asks for it,
/// since its endpoint is not public yet (RETRIEVE-34).
fn offers_brave(reaches_brave: bool, flagged: bool) -> bool {
    flagged && reaches_brave
}

fn reaches_brave(config: Option<&bravebot_config::Config>) -> bool {
    config.is_some_and(|config| config.premium_endpoint.is_some())
}

fn brave_flagged() -> bool {
    flag_set(std::env::var(bravebot_config::env_var::BRAVE_EMBEDDINGS).ok())
}

fn flag_set(value: Option<String>) -> bool {
    value.is_some_and(|value| value.trim() == "1")
}

/// A variable a provider block names its key in.
pub(crate) fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Turn retrieval on or off in the person's own settings file.
///
/// Refused where the managed file or the file the process was started on fixes the key, and,
/// turning it on, unless the request carries the endpoint the window told the person about and it
/// is the one retrieval sends to, or the person was told about that endpoint before.
pub fn set(request: &Request, selected: Option<&Path>) -> Result<Value, Failure> {
    let enabled = request
        .param("enabled")
        .as_bool()
        .ok_or_else(|| Failure::bad_request("`enabled` must be true or false"))?;
    let home = bravebot_agent::home::directory()
        .ok_or_else(|| Failure::new(ErrorCode::Config, "there is no home directory to write to"))?;
    let managed = Managed::load();
    let shown = request.optional_string("disclosed");
    let saved = store::load_retrieval_disclosed();
    let chosen = chosen(Some(&home), selected, &managed);
    change(
        &home,
        selected,
        &managed,
        Change {
            enabled,
            url: chosen
                .endpoint
                .as_ref()
                .ok()
                .map(|config| config.url.as_str()),
            unresolved: chosen.endpoint.as_ref().err(),
            shown: shown.as_deref(),
            saved: saved.as_deref(),
        },
        store::save_retrieval_disclosed,
    )?;
    let saved = store::load_retrieval_disclosed();
    Ok(state_in(
        Some(&home),
        selected,
        &managed,
        saved.as_deref(),
        &chosen,
    ))
}

/// How far a bot session's vectors have caught up with its record, in messages (RETRIEVE-29).
///
/// `off` where the setting is off for the session's project; otherwise the sidecar's standing,
/// with why the endpoint gave the last pass nothing where `indexer` knows it. Counts only, so
/// nothing of the conversation crosses the pipe.
pub fn status(
    request: &Request,
    selected: Option<&Path>,
    indexer: Option<&bravebot_session::indexer::Indexer>,
) -> Result<Value, Failure> {
    let directory = std::path::PathBuf::from(request.string("directory")?);
    let id = session_name(request)?;
    if !crate::settings::layers(Some(&directory), selected).retrieval() {
        return Ok(json!({ "state": "off", "indexed": 0, "eligible": 0 }));
    }
    let record = bravebot_session::sessions::load(&directory, &id)
        .ok_or_else(|| Failure::new(ErrorCode::NoSuchSession, format!("no session {id}")))?;
    let path = bravebot_session::vectors::path_in(&directory, &id)
        .ok_or_else(|| Failure::bad_request("not a session name"))?;
    let failed = |endpoint: &str| indexer.and_then(|indexer| indexer.failure(&path, endpoint));
    Ok(standing(
        &path,
        &record.conversation,
        &resolved(selected),
        &failed,
    ))
}

/// The request's `id`, refused unless it is a session's name, since it is joined into a path.
fn session_name(request: &Request) -> Result<String, Failure> {
    let id = request.string("id")?;
    if !bravebot_session::sessions::is_a_session_name(&id) {
        return Err(Failure::bad_request("not a session name"));
    }
    Ok(id)
}

/// The vectors' standing for the endpoint in force, or `unconfigured` where there is none.
fn standing(
    path: &Path,
    snapshot: &bravebot_agent::conversation::Snapshot,
    endpoint: &Result<EmbeddingsConfig, Unresolved>,
    failed: &dyn Fn(&str) -> Option<bravebot_aichat::embeddings::Failure>,
) -> Value {
    match endpoint {
        Ok(config) => readiness(path, snapshot, config, failed),
        Err(_) => json!({ "state": "unconfigured", "indexed": 0, "eligible": 0 }),
    }
}

/// The excerpts one exchange's retrievals sent, read from the saved record when the person opens
/// a row of "Used N earlier messages" (RETRIEVE-32).
///
/// The exception to RETRIEVE-27: the words cross only for this request, for the one exchange it
/// names, and never on an event. Every word is made printable first.
pub fn excerpts(request: &Request) -> Result<Value, Failure> {
    let directory = std::path::PathBuf::from(request.string("directory")?);
    let id = session_name(request)?;
    let prompt = u32::try_from(request.number("prompt")?)
        .map_err(|_| Failure::bad_request("`prompt` is not an exchange"))?;
    let record = bravebot_session::sessions::load(&directory, &id)
        .ok_or_else(|| Failure::new(ErrorCode::NoSuchSession, format!("no session {id}")))?;
    let conversation = bravebot_agent::Conversation::restored(record.conversation);
    Ok(sent_for(&conversation, prompt))
}

/// What each retrieval put in front of exchange `prompt` sent, in the order they were put in.
///
/// Only that exchange's own messages: excerpts an earlier exchange's retrieval found, which a
/// refreshed request still carries whole, are that exchange's to show. A send a later cut in the
/// same exchange replaced is marked, since the requests after it carried a stub in its place.
fn sent_for(conversation: &bravebot_agent::Conversation, prompt: u32) -> Value {
    use bravebot_agent::conversation::Composed;
    let mine: Vec<_> = conversation
        .retrievals()
        .into_iter()
        .filter(|(at, _)| *at == prompt)
        .map(|(_, stored)| stored)
        .collect();
    let sends: Vec<Value> = mine
        .iter()
        .enumerate()
        .filter_map(|(at, stored)| {
            let Some(Composed::Retrieved {
                excerpts,
                model,
                refreshed,
                context,
            }) = &stored.composed
            else {
                return None;
            };
            let replaced = mine[at + 1..].iter().any(|later| {
                matches!(
                    later.composed,
                    Some(Composed::Retrieved {
                        refreshed: false,
                        ..
                    })
                )
            });
            Some(json!({
                "label": printable(&stored.provenance().label()),
                "refreshed": refreshed,
                "replacedInTurn": replaced,
                "model": printable(model),
                "excerpts": paired(&stored.message.content.text(), excerpts, context),
            }))
        })
        .collect();
    json!({ "sends": sends, "contextBytes": bravebot_session::retrieve::CONTEXT_BYTES })
}

/// Each excerpt the record lists beside the block that holds its words, matched by the place the
/// block names and in order, so two chunks of one message pair with their own blocks. An excerpt
/// with no block is left out rather than given another's words.
fn paired(text: &str, excerpts: &[(u32, u16)], context: &[u16]) -> Vec<Value> {
    let blocks = bravebot_session::retriever::blocks(text);
    let mut next = 0;
    let mut out = Vec::new();
    for (position, &(ordinal, chunk)) in excerpts.iter().enumerate() {
        let Some(found) = blocks[next..]
            .iter()
            .position(|block| block.ordinal == ordinal)
        else {
            continue;
        };
        let block = &blocks[next + found];
        next += found + 1;
        let is_context = u16::try_from(position).is_ok_and(|at| context.contains(&at));
        out.push(json!({
            "ordinal": ordinal,
            "chunk": chunk,
            "from": printable(&block.from),
            "context": is_context,
            "text": printable(&block.text),
        }));
    }
    out
}

fn printable(text: &str) -> String {
    bravebot_agent::confirm::printable(text)
}

fn readiness(
    path: &Path,
    snapshot: &bravebot_agent::conversation::Snapshot,
    config: &EmbeddingsConfig,
    failed: &dyn Fn(&str) -> Option<bravebot_aichat::embeddings::Failure>,
) -> Value {
    use bravebot_session::indexer::Standing;
    let model = bravebot_session::vectors::Model {
        endpoint: bravebot_aichat::embeddings::endpoint_identity(&config.url),
        id: config.model.clone(),
        dimensions: u32::try_from(config.dimensions).unwrap_or(u32::MAX),
    };
    let readiness = bravebot_session::indexer::readiness(path, snapshot, &model);
    let state = match readiness.state {
        Standing::Ready => "ready",
        Standing::Indexing => "indexing",
        Standing::Rebuilding => "rebuilding",
        Standing::Untrusted => "untrusted",
    };
    let mut status =
        json!({ "state": state, "indexed": readiness.indexed, "eligible": readiness.eligible });
    if let Standing::Indexing | Standing::Rebuilding = readiness.state
        && let Some(why) = failed(&model.endpoint)
    {
        status["failure"] = json!(bravebot_session::retriever::because(why).word());
    }
    status
}

/// The endpoint retrieval sends to, as the window and `doctor` name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub service: Service,
    pub url: String,
    pub host: String,
    pub model: String,
    pub dimensions: usize,
    /// Brave's development service rather than a production one.
    pub development: bool,
}

/// The endpoint retrieval sends to, or why the settings choose none.
pub fn described(selected: Option<&Path>) -> Result<Endpoint, Unresolved> {
    let home = bravebot_agent::home::directory();
    let chosen = chosen(home.as_deref(), selected, &Managed::load());
    chosen
        .endpoint
        .map(|config| endpoint_of(chosen.service, config))
}

/// An endpoint resolves only where a service is in force, so `service` is that one.
fn endpoint_of(service: Option<Service>, config: EmbeddingsConfig) -> Endpoint {
    Endpoint {
        service: service.unwrap_or(Service::Brave),
        host: host(&config.url).to_string(),
        development: config.url == bravebot_aichat::embeddings::DEV_EMBEDDINGS_URL,
        url: config.url,
        model: config.model,
        dimensions: config.dimensions,
    }
}

/// The endpoint as the window is told it, or `null` where the settings choose none.
fn endpoint(chosen: &Chosen) -> Value {
    let Ok(config) = &chosen.endpoint else {
        return Value::Null;
    };
    let Endpoint {
        service,
        url,
        host,
        model,
        dimensions,
        development,
    } = endpoint_of(chosen.service, config.clone());
    json!({
        "service": service.word(),
        "url": url,
        "host": host,
        "model": model,
        "dimensions": dimensions,
        "development": development,
    })
}

/// One query embedding of a fixed word at the endpoint the settings choose, and how long it took
/// to answer.
///
/// The text is a constant, so the call sends nothing of the person's.
pub fn test_call(selected: Option<&Path>) -> Result<std::time::Duration, String> {
    let client = EmbeddingsClient::new(resolved(selected).map_err(|why| why.to_string())?);
    let started = std::time::Instant::now();
    client
        .embed(Kind::Query, &["bravebot doctor".to_string()])
        .map_err(|error| error.to_string())?;
    Ok(started.elapsed())
}

/// What one `retrieval.set` asks for, and what the person was told and when.
#[derive(Clone, Copy)]
struct Change<'a> {
    enabled: bool,
    /// Where retrieval sends, where the settings choose an endpoint.
    url: Option<&'a str>,
    /// Why the settings choose none, where they do not.
    unresolved: Option<&'a Unresolved>,
    /// The endpoint the window says it showed the person with this request.
    shown: Option<&'a str>,
    /// The endpoint the person was shown before, as the store keeps it.
    saved: Option<&'a str>,
}

pub(crate) fn layered(home: Option<&Path>, selected: Option<&Path>, managed: &Managed) -> Settings {
    Settings::layered(home.map(Path::to_path_buf), None, selected).under(managed)
}

pub(crate) fn state_in(
    home: Option<&Path>,
    selected: Option<&Path>,
    managed: &Managed,
    disclosed: Option<&str>,
    chosen: &Chosen,
) -> Value {
    let url = chosen
        .endpoint
        .as_ref()
        .ok()
        .map(|config| config.url.as_str());
    let settings = layered(home, selected, managed);
    let file = home.map(bravebot_config::user_settings_file);
    json!({
        "enabled": settings.retrieval(),
        "path": file,
        "from": settings.retrieval_from(),
        "locked": locked(&settings, managed, file.as_deref(), selected),
        "disclosed": url.is_some() && disclosed == url,
        "endpoint": endpoint(chosen),
        "unconfigured": chosen.endpoint.as_ref().err().map(ToString::to_string),
        "embeddings": {
            "selected": chosen.selected.map(Service::word),
            "inForce": chosen.service.map(Service::word),
            "locked": crate::embeddings_choice::locked(&settings, managed, file.as_deref(), selected),
            "options": chosen.options,
        },
    })
}

/// What fixes the key so the person's own file cannot change it, where something does: the
/// managed file turning it off, or the file the process was started on stating it.
pub(crate) fn locked(
    settings: &Settings,
    managed: &Managed,
    own: Option<&Path>,
    selected: Option<&Path>,
) -> Option<Value> {
    if managed.retrieval_off() {
        return Some(json!({ "by": "managed", "path": managed.path() }));
    }
    let from = settings.retrieval_from()?;
    (selected == Some(from) && own != Some(from)).then(|| json!({ "by": "run", "path": from }))
}

fn change(
    home: &Path,
    selected: Option<&Path>,
    managed: &Managed,
    asked: Change<'_>,
    keep: impl FnOnce(&str) -> bool,
) -> Result<(), Failure> {
    let settings = layered(Some(home), selected, managed);
    let own = bravebot_config::user_settings_file(home);
    if let Some(lock) = locked(&settings, managed, Some(&own), selected) {
        let path = lock["path"].as_str().unwrap_or_default();
        let message = match lock["by"].as_str() {
            Some("managed") => {
                format!("retrieval is turned off by the administrator's file {path}")
            }
            _ => format!("retrieval is set by {path} for this run"),
        };
        return Err(Failure::new(ErrorCode::Config, message));
    }
    if let Some(why) = asked.unresolved.filter(|_| asked.enabled) {
        return Err(Failure::bad_request(format!(
            "retrieval was not turned on: {why}"
        )));
    }
    if let Some(url) = asked.url.filter(|_| asked.enabled)
        && asked.saved != Some(url)
    {
        if asked.shown != Some(url) {
            return Err(Failure::bad_request(format!(
                "retrieval was not turned on: the person has not been shown that it sends to {url}"
            )));
        }
        if !keep(url) {
            return Err(Failure::new(
                ErrorCode::Internal,
                "retrieval was not turned on: that the person was shown where it sends could not be kept",
            ));
        }
    }
    let mut file = crate::connectors::user_settings(home)?;
    if !file.set_retrieval(asked.enabled) {
        return Err(Failure::new(
            ErrorCode::Config,
            format!(
                "{} was left as it is: its `retrieval` is not a block",
                own.display()
            ),
        ));
    }
    crate::connectors::write_settings(&file)
}

/// The host of `url`, for a sentence that names where something is sent.
pub(crate) fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = bravebot_aichat::embeddings::DEV_EMBEDDINGS_URL;

    #[test]
    fn brave_embeddings_are_asked_for_by_a_one_alone() {
        assert!(flag_set(Some("1".into())));
        assert!(flag_set(Some(" 1 ".into())));
        for value in [
            None,
            Some("0".into()),
            Some("true".into()),
            Some(String::new()),
        ] {
            assert!(!flag_set(value));
        }
    }

    #[test]
    fn brave_is_offered_only_to_a_build_that_reaches_it_and_asked_for_it() {
        assert!(offers_brave(true, true));
        assert!(!offers_brave(true, false));
        assert!(!offers_brave(false, true));
        assert!(!offers_brave(false, false));
    }

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "bravebot-retrieval-switch-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("home")).expect("scratch home");
            Self(dir)
        }

        fn home(&self) -> std::path::PathBuf {
            self.0.join("home")
        }

        fn write(&self, name: &str, text: &str) -> std::path::PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, text).expect("scratch file");
            path
        }

        fn own(&self) -> Value {
            let text = std::fs::read_to_string(bravebot_config::user_settings_file(&self.home()))
                .expect("the home file");
            serde_json::from_str(&text).expect("json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn turn<'a>(enabled: bool, shown: Option<&'a str>, saved: Option<&'a str>) -> Change<'a> {
        Change {
            enabled,
            url: Some(URL),
            shown,
            saved,
            unresolved: None,
        }
    }

    /// Brave's development endpoint in force, as it is where nothing else is chosen.
    fn dev() -> Chosen {
        Chosen {
            service: Some(Service::Brave),
            endpoint: Ok(EmbeddingsConfig::dev()),
            selected: None,
            options: Value::Null,
        }
    }

    /// RETRIEVE-24: the switch writes `retrieval.enabled` into the person's own file and keeps the
    /// rest of it, turning it off writes `false`, and the state read back says so with that file
    /// as the one that decided it.
    #[test]
    fn the_switch_writes_the_persons_own_file_and_keeps_the_rest() {
        let scratch = Scratch::new("writes");
        let own = bravebot_config::user_settings_file(&scratch.home());
        std::fs::write(&own, r#"{"model": "kept"}"#).expect("home file");
        let none = Managed::default();
        change(
            &scratch.home(),
            None,
            &none,
            turn(true, None, Some(URL)),
            |_| true,
        )
        .expect("turned on");
        assert_eq!(
            scratch.own(),
            json!({"model": "kept", "retrieval": {"enabled": true}})
        );
        let state = state_in(Some(&scratch.home()), None, &none, Some(URL), &dev());
        assert_eq!(state["enabled"], true);
        assert_eq!(state["from"], json!(own));
        assert_eq!(state["locked"], Value::Null);
        assert_eq!(state["disclosed"], true);

        change(
            &scratch.home(),
            None,
            &none,
            turn(false, None, None),
            |_| true,
        )
        .expect("turned off");
        assert_eq!(scratch.own()["retrieval"], json!({"enabled": false}));
        assert_eq!(
            state_in(Some(&scratch.home()), None, &none, None, &dev())["enabled"],
            false
        );
    }

    /// RETRIEVE-24: an administrator's `false` and a `--settings` file stating the key each lock
    /// the switch, the state names which and the file, and a request to change it is refused
    /// with that reason and leaves the person's file as it was. A `--settings` file that says
    /// nothing about retrieval locks nothing.
    #[test]
    fn a_managed_pin_or_a_run_file_stating_the_key_refuses_the_switch() {
        let scratch = Scratch::new("locked");
        let managed_path = scratch.write("managed.json", r#"{"retrieval": {"enabled": false}}"#);
        let managed = Managed::at(&managed_path);
        let state = state_in(Some(&scratch.home()), None, &managed, None, &dev());
        assert_eq!(
            state["locked"],
            json!({"by": "managed", "path": managed_path})
        );
        let refused = change(
            &scratch.home(),
            None,
            &managed,
            turn(true, Some(URL), None),
            |_| panic!("kept a disclosure for a refused change"),
        )
        .expect_err("an administrator's pin was overridden");
        assert!(
            refused.message.contains("administrator"),
            "{}",
            refused.message
        );
        assert!(
            refused
                .message
                .contains(&managed_path.display().to_string())
        );

        let run = scratch.write("run.json", r#"{"retrieval": {"enabled": true}}"#);
        let none = Managed::default();
        let state = state_in(Some(&scratch.home()), Some(&run), &none, None, &dev());
        assert_eq!(state["enabled"], true);
        assert_eq!(state["locked"], json!({"by": "run", "path": run}));
        let refused = change(
            &scratch.home(),
            Some(&run),
            &none,
            turn(false, None, None),
            |_| true,
        )
        .expect_err("a run's file was overridden");
        assert!(
            refused.message.contains("for this run"),
            "{}",
            refused.message
        );
        assert!(
            !bravebot_config::user_settings_file(&scratch.home()).exists(),
            "a refused change wrote the person's file"
        );

        let quiet = scratch.write("quiet.json", r#"{"model": "m"}"#);
        let state = state_in(Some(&scratch.home()), Some(&quiet), &none, None, &dev());
        assert_eq!(state["locked"], Value::Null);
    }

    /// RETRIEVE-25: turning it on is refused unless the request names the endpoint retrieval sends
    /// to or the person was shown that endpoint before; a request naming it keeps it; one shown an
    /// older endpoint is asked again; and turning it off needs nothing.
    #[test]
    fn turning_retrieval_on_needs_the_endpoint_to_have_been_shown() {
        let scratch = Scratch::new("disclosed");
        let none = Managed::default();
        let refused = change(&scratch.home(), None, &none, turn(true, None, None), |_| {
            panic!("kept a disclosure nobody was shown")
        })
        .expect_err("turned on without the disclosure");
        assert!(refused.message.contains(URL), "{}", refused.message);
        assert!(!bravebot_config::user_settings_file(&scratch.home()).exists());

        let older = "https://example.invalid/v1/embeddings";
        assert!(
            change(
                &scratch.home(),
                None,
                &none,
                turn(true, Some(older), Some(older)),
                |_| true
            )
            .is_err(),
            "a disclosure of another endpoint was taken for this one"
        );

        let mut kept = None;
        change(
            &scratch.home(),
            None,
            &none,
            turn(true, Some(URL), None),
            |url| {
                kept = Some(url.to_string());
                true
            },
        )
        .expect("turned on after the disclosure");
        assert_eq!(kept.as_deref(), Some(URL));
        assert_eq!(scratch.own()["retrieval"]["enabled"], true);

        let refused = change(
            &scratch.home(),
            None,
            &none,
            turn(true, Some(URL), None),
            |_| false,
        )
        .expect_err("turned on with the disclosure unkept");
        assert!(refused.message.contains("could not be kept"));

        change(
            &scratch.home(),
            None,
            &none,
            turn(false, None, None),
            |_| panic!("asked for a disclosure to turn it off"),
        )
        .expect("turned off");
    }

    /// RETRIEVE-34, RETRIEVE-38: settings that choose no endpoint name none to the window, say why,
    /// count as shown nothing, and turning retrieval on with them is refused with that reason,
    /// asking for no disclosure and writing nothing.
    #[test]
    fn with_no_endpoint_chosen_nothing_is_named_and_retrieval_stays_off() {
        let scratch = Scratch::new("unconfigured");
        let none = Managed::default();
        let unchosen = Chosen {
            service: None,
            endpoint: Err(Unresolved::NothingSelected),
            selected: None,
            options: Value::Null,
        };
        let state = state_in(Some(&scratch.home()), None, &none, Some(URL), &unchosen);
        assert_eq!(state["endpoint"], Value::Null);
        assert_eq!(state["disclosed"], false);
        assert_eq!(state["unconfigured"], "no embeddings service is selected");
        assert_eq!(
            state_in(Some(&scratch.home()), None, &none, None, &dev())["unconfigured"],
            Value::Null
        );

        let refused = change(
            &scratch.home(),
            None,
            &none,
            Change {
                enabled: true,
                url: None,
                shown: None,
                saved: None,
                unresolved: Some(&Unresolved::NothingSelected),
            },
            |_| panic!("kept a disclosure of no endpoint"),
        )
        .expect_err("turned on with nothing to send to");
        assert!(
            refused
                .message
                .contains("no embeddings service is selected"),
            "{}",
            refused.message
        );
        assert!(!bravebot_config::user_settings_file(&scratch.home()).exists());
    }

    /// RETRIEVE-34: a custom endpoint selected in the person's own file resolves to its URL, model
    /// and length with the key stored beside the settings, and is named as the custom service.
    #[test]
    fn a_custom_endpoint_resolves_with_its_stored_key() {
        let scratch = Scratch::new("custom");
        std::fs::write(
            bravebot_config::user_settings_file(&scratch.home()),
            r#"{"retrieval": {"enabled": true, "embeddings": {"selected": "custom",
                "custom": {"url": "https://embed.example/v1/embeddings", "model": "m", "dimensions": 384}}}}"#,
        )
        .expect("home file");
        std::fs::write(
            bravebot_config::embeddings_key::file(&scratch.home()),
            bravebot_config::embeddings_key::to_text(&bravebot_config::Secret::new("the-key"))
                .expose(),
        )
        .expect("key file");

        let chosen = chosen(Some(&scratch.home()), None, &Managed::default());

        assert_eq!(chosen.service, Some(Service::Custom));
        let config = chosen.endpoint.as_ref().expect("resolved");
        assert_eq!(config.url, "https://embed.example/v1/embeddings");
        assert_eq!((config.model.as_str(), config.dimensions), ("m", 384));
        assert_eq!(
            config.token.as_ref().map(bravebot_config::Secret::expose),
            Some("the-key")
        );
        let named = endpoint(&chosen);
        assert_eq!(named["service"], "custom");
        assert_eq!(named["host"], "embed.example");
        assert_eq!(named["development"], false);
    }

    /// RETRIEVE-33: the file the process was started on chooses over the person's own, and a custom
    /// endpoint with no key stored sends none.
    #[test]
    fn the_run_file_chooses_and_a_custom_endpoint_without_a_key_sends_none() {
        let scratch = Scratch::new("run-chooses");
        let custom = |url: &str| {
            json!({"retrieval": {"enabled": true, "embeddings": {"selected": "custom",
                "custom": {"url": url, "model": "m", "dimensions": 8}}}})
            .to_string()
        };
        std::fs::write(
            bravebot_config::user_settings_file(&scratch.home()),
            custom("https://own.example/v1/embeddings"),
        )
        .expect("home file");
        let run = scratch.write("run.json", &custom("https://run.example/v1/embeddings"));
        let none = Managed::default();

        let own = chosen(Some(&scratch.home()), None, &none)
            .endpoint
            .expect("own");
        assert_eq!(own.url, "https://own.example/v1/embeddings");
        assert!(own.token.is_none());
        let named = chosen(Some(&scratch.home()), Some(&run), &none)
            .endpoint
            .expect("named");
        assert_eq!(named.url, "https://run.example/v1/embeddings");
    }

    /// RETRIEVE-34: a provider block named `openai` in the person's file is the OpenAI that is
    /// offered, and its own base URL is where the vectors are asked for.
    #[test]
    fn a_configured_openai_block_is_resolved_from_the_settings() {
        let scratch = Scratch::new("openai-block");
        std::fs::write(
            bravebot_config::user_settings_file(&scratch.home()),
            json!({
                "provider": {"openai": {"options": {"baseURL": "http://127.0.0.1:9/v1"},
                    "models": {"m": {}}}},
                "model": "openai/m",
                "retrieval": {"enabled": true, "embeddings": {"selected": "openai"}},
            })
            .to_string(),
        )
        .expect("home file");

        let chosen = chosen(Some(&scratch.home()), None, &Managed::default());

        assert_eq!(chosen.service, Some(Service::OpenAi));
        let config = chosen.endpoint.expect("resolved");
        assert_eq!(config.url, "http://127.0.0.1:9/v1/embeddings");
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.dimensions, 1536);
    }

    /// The endpoint is named by its host, the one part of the address a person can check.
    #[test]
    fn the_endpoint_is_named_by_its_host() {
        assert_eq!(host(URL), "ai-chat-backend-dev.bsg.brave.software");
        assert_eq!(host("http://localhost:8080?x"), "localhost:8080");
        assert_eq!(
            endpoint(&dev())["host"],
            "ai-chat-backend-dev.bsg.brave.software"
        );
        assert_eq!(endpoint(&dev())["service"], "brave");
        assert_eq!(endpoint(&dev())["development"], true);
    }

    /// RETRIEVE-5: the status reads vectors from the endpoint now chosen as ready, whatever query
    /// its URL carries, and vectors from another host serving the same model as rebuilding.
    #[test]
    fn vectors_from_another_endpoint_read_as_rebuilding() {
        use bravebot_aichat::embeddings::{Embedder, FakeEmbedder};
        use bravebot_aichat::protocol::Message;
        let scratch = Scratch::new("switched");
        let path = scratch.0.join("a1b2.vectors");
        let mut conversation = bravebot_agent::conversation::Conversation::new();
        conversation.push(Message::user("plan the trip"));
        conversation.push(Message::assistant("three days in Lisbon"));
        let snapshot = conversation.snapshot();
        let fake = FakeEmbedder::new(4);
        bravebot_session::indexer::catch_up(&path, &snapshot, &fake, 0);
        let config = |url: String| EmbeddingsConfig {
            url,
            model: fake.model().to_string(),
            dimensions: 4,
            ..EmbeddingsConfig::dev()
        };

        assert_eq!(
            readiness(
                &path,
                &snapshot,
                &config(format!("{}?key=k", fake.endpoint())),
                &|_| None
            )["state"],
            "ready"
        );
        assert_eq!(
            readiness(
                &path,
                &snapshot,
                &config("https://elsewhere.example/v1/embeddings".to_string()),
                &|_| None
            ),
            json!({ "state": "rebuilding", "indexed": 0, "eligible": 2 })
        );
    }

    /// RETRIEVE-29: the window is told how many messages the vectors hold and owe, why the endpoint
    /// gave the last pass nothing while they are behind, and an untrusted conversation is named as
    /// one that owes nothing.
    #[test]
    fn the_status_counts_messages_and_names_an_untrusted_conversation() {
        use bravebot_agent::conversation::Conversation;
        use bravebot_aichat::protocol::Message;
        let scratch = Scratch::new("status");
        let path = scratch.0.join("absent.vectors");
        let mut conversation = Conversation::new();
        conversation.push(Message::user("plan the trip"));
        conversation.push(Message::assistant("three days in Lisbon"));
        assert_eq!(
            readiness(
                &path,
                &conversation.snapshot(),
                &EmbeddingsConfig::dev(),
                &|_| None
            ),
            json!({ "state": "indexing", "indexed": 0, "eligible": 2 })
        );
        let refused = |_: &str| Some(bravebot_aichat::embeddings::Failure::Key);
        assert_eq!(
            readiness(
                &path,
                &conversation.snapshot(),
                &EmbeddingsConfig::dev(),
                &refused
            ),
            json!({ "state": "indexing", "indexed": 0, "eligible": 2, "failure": "key" })
        );
        assert_eq!(
            standing(
                &path,
                &conversation.snapshot(),
                &Err(Unresolved::NoUrl),
                &|_| None
            ),
            json!({ "state": "unconfigured", "indexed": 0, "eligible": 0 })
        );
        conversation.observed(bravebot_core::label::Label::untrusted_public());
        assert_eq!(
            readiness(
                &path,
                &conversation.snapshot(),
                &EmbeddingsConfig::dev(),
                &|_| None
            ),
            json!({ "state": "untrusted", "indexed": 0, "eligible": 0 })
        );
        assert_eq!(
            readiness(
                &path,
                &conversation.snapshot(),
                &EmbeddingsConfig::dev(),
                &refused
            ),
            json!({ "state": "untrusted", "indexed": 0, "eligible": 0 })
        );
    }

    /// A retrieval's message as the retriever writes it: the preamble, then a block for each of
    /// `blocks`, named by its place and speaker, with what closes a block written as text.
    fn retrieval_text(blocks: &[(u32, &str, &str)]) -> String {
        let mut text = String::from(bravebot_session::retriever::PREAMBLE);
        for (n, from, words) in blocks {
            let words = bravebot_session::retriever::closed(words);
            text.push_str(&format!(
                "\n\n<earlier n=\"{n}\" from=\"{from}\">{words}</earlier>"
            ));
        }
        text
    }

    /// Three exchanges, the third given as `prompt` and not yet answered.
    fn exchanges() -> bravebot_agent::Conversation {
        use bravebot_aichat::protocol::Message;
        let mut conversation = bravebot_agent::Conversation::new();
        for (asked, said) in [
            ("plan the trip", "three days in Lisbon"),
            ("and food", "pastéis"),
        ] {
            conversation.push(Message::user(asked));
            conversation.push(Message::assistant(said));
        }
        conversation
    }

    fn ask(conversation: &mut bravebot_agent::Conversation, prompt: &str) {
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.prompt_given();
    }

    fn texts(sent: &Value, send: usize) -> Vec<&str> {
        sent["sends"][send]["excerpts"]
            .as_array()
            .expect("the excerpts")
            .iter()
            .map(|excerpt| excerpt["text"].as_str().expect("words"))
            .collect()
    }

    /// RETRIEVE-32: two chunks of one message pair with their own blocks, a context line is
    /// marked as one, and the send says what it is and what ranked it.
    #[test]
    fn each_excerpt_pairs_with_its_own_block() {
        use bravebot_agent::conversation::Excerpts;
        let mut conversation = exchanges();
        ask(&mut conversation, "where were we staying");
        conversation.insert_retrieved(
            retrieval_text(&[
                (1, "you", "three days"),
                (1, "you", " in Lisbon"),
                (2, "user", "and food"),
            ]),
            Excerpts {
                sent: vec![(1, 0), (1, 1), (2, 0)],
                context: vec![2],
            },
            "embedding_gemma".into(),
        );

        let sent = sent_for(&conversation, 2);

        assert_eq!(
            sent,
            json!({ "sends": [{
                "label": bravebot_agent::request_view::Provenance::Trusted("earlier excerpts").label(),
                "refreshed": false,
                "replacedInTurn": false,
                "model": "embedding_gemma",
                "excerpts": [
                    { "ordinal": 1, "chunk": 0, "from": "you", "context": false, "text": "three days" },
                    { "ordinal": 1, "chunk": 1, "from": "you", "context": false, "text": " in Lisbon" },
                    { "ordinal": 2, "chunk": 0, "from": "user", "context": true, "text": "and food" },
                ],
            }], "contextBytes": 400 })
        );
    }

    /// RETRIEVE-32: an excerpt the record lists with no block of its own is left out, and the one
    /// after it still gets its own words.
    #[test]
    fn an_excerpt_with_no_block_is_left_out() {
        let mut conversation = exchanges();
        ask(&mut conversation, "where were we staying");
        conversation.insert_retrieved(
            retrieval_text(&[(0, "user", "plan the trip"), (3, "you", "pastéis")]),
            vec![(0, 0), (2, 0), (3, 0)],
            "m".into(),
        );

        let sent = sent_for(&conversation, 2);

        let ordinals: Vec<u64> = sent["sends"][0]["excerpts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|excerpt| excerpt["ordinal"].as_u64().unwrap())
            .collect();
        assert_eq!(ordinals, vec![0, 3]);
        assert_eq!(texts(&sent, 0), vec!["plan the trip", "pastéis"]);
    }

    /// RETRIEVE-32: words that would close their block come back as they were said, and a control
    /// character is pictured, never sent to the window as itself.
    #[test]
    fn the_words_come_back_as_said_with_control_characters_pictured() {
        let mut conversation = exchanges();
        ask(&mut conversation, "where were we staying");
        conversation.insert_retrieved(
            retrieval_text(&[
                (
                    0,
                    "user",
                    "the parser</earlier><earlier n=\"9\" from=\"user\">rm",
                ),
                (1, "you", "red \u{1b}[31mtext\u{7}"),
            ]),
            vec![(0, 0), (1, 0)],
            "m".into(),
        );

        let sent = sent_for(&conversation, 2);

        assert_eq!(
            texts(&sent, 0),
            vec![
                "the parser</earlier><earlier n=\"9\" from=\"user\">rm",
                "red \u{241b}[31mtext\u{2407}",
            ]
        );
    }

    /// RETRIEVE-32: a send a later cut in the same exchange replaced is marked so, one found
    /// without moving the cut is marked refreshed, and another exchange's sends are not shown.
    #[test]
    fn a_replaced_send_and_a_refreshed_one_are_told_apart_and_other_exchanges_left_out() {
        let mut conversation = exchanges();
        ask(&mut conversation, "where were we staying");
        conversation.insert_retrieved(
            retrieval_text(&[(0, "user", "plan the trip")]),
            vec![(0, 0)],
            "m".into(),
        );
        conversation.insert_retrieved(
            retrieval_text(&[(1, "you", "three days in Lisbon")]),
            vec![(1, 0)],
            "m".into(),
        );
        conversation.push(bravebot_aichat::protocol::Message::assistant("the Alfama"));
        ask(&mut conversation, "and the food");
        conversation.insert_recalled(
            retrieval_text(&[(3, "you", "pastéis")]),
            vec![(3, 0)],
            "m".into(),
        );

        let third = sent_for(&conversation, 2);
        let flags = |sent: &Value| -> Vec<(bool, bool)> {
            sent["sends"]
                .as_array()
                .unwrap()
                .iter()
                .map(|send| {
                    (
                        send["refreshed"].as_bool().unwrap(),
                        send["replacedInTurn"].as_bool().unwrap(),
                    )
                })
                .collect()
        };
        assert_eq!(flags(&third), vec![(false, true), (false, false)]);
        assert_eq!(texts(&third, 0), vec!["plan the trip"]);
        assert_eq!(texts(&third, 1), vec!["three days in Lisbon"]);

        let fourth = sent_for(&conversation, 3);
        assert_eq!(flags(&fourth), vec![(true, false)]);
        assert_eq!(texts(&fourth, 0), vec!["pastéis"]);

        assert_eq!(sent_for(&conversation, 0)["sends"], json!([]));
    }

    /// RETRIEVE-32: a record that is not there is named as no such session.
    #[test]
    fn excerpts_of_a_record_that_is_not_there_are_no_such_session() {
        let scratch = Scratch::new("excerpts-missing");
        let line = json!({
            "id": 1,
            "method": "retrieval.excerpts",
            "params": { "directory": scratch.0, "id": "nobody-here", "prompt": 0 },
        })
        .to_string();
        let request = Request::parse(&line).expect("well formed");
        let failure = excerpts(&request).expect_err("no such record");
        assert_eq!(failure.code, ErrorCode::NoSuchSession);

        let line = json!({
            "id": 1,
            "method": "retrieval.excerpts",
            "params": { "directory": scratch.0, "id": "nobody-here", "prompt": -1 },
        })
        .to_string();
        let request = Request::parse(&line).expect("well formed");
        assert_eq!(
            excerpts(&request).expect_err("not an exchange").code,
            ErrorCode::BadRequest
        );
    }

    /// RETRIEVE-32: an id that is not a session's name is refused before it is joined into a
    /// path, by the excerpts and by the status alike.
    #[test]
    fn an_id_that_is_not_a_session_name_is_refused() {
        let scratch = Scratch::new("excerpts-name");
        for id in ["../x", "a/b", "", "x.json"] {
            for method in ["retrieval.excerpts", "retrieval.status"] {
                let line = json!({
                    "id": 1,
                    "method": method,
                    "params": { "directory": scratch.0, "id": id, "prompt": 0 },
                })
                .to_string();
                let request = Request::parse(&line).expect("well formed");
                let failure = if method == "retrieval.excerpts" {
                    excerpts(&request)
                } else {
                    status(&request, None, None)
                }
                .expect_err("not a session name");
                assert_eq!(failure.code, ErrorCode::BadRequest, "{method} took {id:?}");
            }
        }
    }
}
