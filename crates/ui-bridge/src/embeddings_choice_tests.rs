use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

/// The window's choice as a build that was not asked for Brave's embeddings service makes it,
/// whatever this process's environment says.
fn chosen(
    home: Option<&Path>,
    selected: Option<&Path>,
    managed: &Managed,
) -> crate::retrieval::Chosen {
    crate::retrieval::chosen_with(home, selected, managed, false)
}

/// The option for the service named `word`.
fn of<'a>(options: &'a [Value], word: &str) -> &'a Value {
    options
        .iter()
        .find(|option| option["service"] == word)
        .unwrap_or_else(|| panic!("no option for {word}"))
}

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "bravebot-embeddings-choice-{name}-{}",
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

    fn own_file(&self) -> std::path::PathBuf {
        bravebot_config::user_settings_file(&self.home())
    }

    fn keep(&self, settings: &Value) {
        std::fs::write(self.own_file(), settings.to_string()).expect("home file");
    }

    fn own(&self) -> Value {
        let text = std::fs::read_to_string(self.own_file()).expect("the home file");
        serde_json::from_str(&text).expect("json")
    }

    fn options(&self) -> Vec<Value> {
        let chosen = chosen(Some(&self.home()), None, &Managed::default());
        chosen.options.as_array().expect("an array").clone()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    authorization: Option<String>,
    body: String,
}

/// A server on loopback answering each request by its path and body, and what it was sent; its
/// `http://127.0.0.1:<port>/v1` is the base URL.
fn serve(
    answer: impl Fn(&str, &str) -> (u16, String) + Send + 'static,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let base = format!("http://{}/v1", listener.local_addr().expect("an address"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut words = line.split_whitespace();
            let method = words.next().unwrap_or_default().to_string();
            let path = words.next().unwrap_or_default().to_string();
            let (mut length, mut authorization) = (0, None);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    match name.trim().to_ascii_lowercase().as_str() {
                        "content-length" => length = value.trim().parse().unwrap_or(0),
                        "authorization" => authorization = Some(value.trim().to_string()),
                        _ => {}
                    }
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let body = String::from_utf8_lossy(&body).into_owned();
            let (status, reply) = answer(&path, &body);
            log.lock().expect("log").push(Seen {
                method,
                path,
                authorization,
                body,
            });
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (base, seen)
}

fn vector(dimensions: usize) -> String {
    json!({"data": [{"index": 0, "embedding": vec![0.5; dimensions]}]}).to_string()
}

/// Settings offering `id` (`openai` or `openrouter`) at `base` with a key.
fn provider(id: &str, base: &str) -> Value {
    json!({
        "provider": {id: {"options": {"baseURL": base, "apiKey": "sk-test"}, "models": {"m": {}}}},
        "model": format!("{id}/m"),
    })
}

fn asked(params: Value) -> Result<Asked, Failure> {
    let line = json!({"id": 1, "method": "retrieval.embeddings.set", "params": params});
    Asked::read(&Request::parse(&line.to_string()).expect("a request"))
}

fn run(scratch: &Scratch, params: Value, saved: Option<&str>) -> Result<Done, Failure> {
    apply(
        &scratch.home(),
        None,
        &Managed::default(),
        &asked(params).expect("read"),
        saved,
        |_| panic!("kept a disclosure that was not asked for"),
    )
}

fn tested_of(done: Done) -> Value {
    match done {
        Done::Written { tested } => tested.unwrap_or(Value::Null),
        Done::NeedsDisclosure(url) => panic!("asked to disclose {url}"),
    }
}

/// Held by each test that lists models, since the listing kept in memory is one for the process.
static LISTING: Mutex<()> = Mutex::new(());

fn listing() -> std::sync::MutexGuard<'static, ()> {
    LISTING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

const URL_ELSEWHERE: &str = "https://elsewhere.example/v1/embeddings";

/// RETRIEVE-35: every service is offered in its order, the ones not set up say so, what was kept
/// for an unselected one is shown, and the custom key is told as stored and never shown.
#[test]
fn every_service_is_offered_with_what_was_kept_and_never_the_key() {
    let scratch = Scratch::new("options");
    scratch.keep(&json!({"retrieval": {"embeddings": {
        "custom": {"url": "not-a-url", "model": "m", "queryPrefix": "q: ", "passagePrefix": ""},
    }}}));
    std::fs::write(
        bravebot_config::embeddings_key::file(&scratch.home()),
        bravebot_config::embeddings_key::to_text(&Secret::new("the-secret-key")).expose(),
    )
    .expect("key file");

    let options = scratch.options();

    let services: Vec<&str> = options
        .iter()
        .map(|option| option["service"].as_str().expect("a word"))
        .collect();
    assert_eq!(services, ["openai", "openrouter", "custom"]);
    assert!(
        !Value::Array(options.clone())
            .to_string()
            .contains("the-secret-key")
    );
    let [openai, openrouter, custom] = options.as_slice() else {
        panic!("three options")
    };
    assert_eq!(openai["available"], false);
    assert_eq!(openai["reason"], "OpenAI is not set up");
    assert_eq!(openai["models"], json!(OPENAI_MODELS));
    assert_eq!(openai["model"], "text-embedding-3-small");
    assert_eq!(openai["dimensions"], 1536);
    assert_eq!(openrouter["available"], false);
    assert_eq!(openrouter["url"], Value::Null);
    assert_eq!(custom["available"], true);
    assert_eq!(custom["keyStored"], true);
    assert_eq!(custom["url"], "not-a-url");
    assert_eq!(custom["host"], Value::Null);
    assert_eq!(custom["model"], "m");
    assert_eq!(custom["queryPrefix"], "q: ");
    assert_eq!(custom["passagePrefix"], "");
    assert_eq!(
        custom["reason"],
        "the custom embeddings endpoint has no http(s) URL"
    );
    assert!(custom.get("models").is_none() && openai.get("keyStored").is_none());
}

/// RETRIEVE-35: a provider block set up offers that service at its host, and a model with no
/// length known says it needs a test call.
#[test]
fn a_set_up_provider_is_offered_at_its_host() {
    let scratch = Scratch::new("options-provider");
    let mut settings = provider("openrouter", "http://127.0.0.1:9/api/v1");
    settings["retrieval"] = json!({"embeddings": {"openrouter": {"model": "acme/embed"}}});
    scratch.keep(&settings);

    let options = scratch.options();

    let openrouter = of(&options, "openrouter");
    assert_eq!(openrouter["available"], true);
    assert_eq!(openrouter["url"], "http://127.0.0.1:9/api/v1/embeddings");
    assert_eq!(openrouter["host"], "127.0.0.1:9");
    assert_eq!(openrouter["model"], "acme/embed");
    assert_eq!(
        openrouter["reason"],
        Unresolved::Untested(Service::OpenRouter).to_string()
    );
}

/// RETRIEVE-36: selecting OpenRouter with a model writes both, sends one constant passage with
/// its key, and keeps the length the reply had; a model then changed is tested again and its
/// own length kept.
#[test]
fn selecting_a_service_tests_it_and_keeps_the_length_found() {
    let (base, seen) = serve(|_, body| (200, vector(if body.contains("other") { 7 } else { 5 })));
    let scratch = Scratch::new("select");
    scratch.keep(&provider("openrouter", &base));

    let done = run(
        &scratch,
        json!({"select": "openrouter", "values": {"model": " acme/embed "}}),
        None,
    )
    .expect("selected");

    let test = tested_of(done);
    assert_eq!(
        (test["ok"].clone(), test["dimensions"].clone()),
        (json!(true), json!(5))
    );
    assert!(test["millis"].is_u64());
    let embeddings = &scratch.own()["retrieval"]["embeddings"];
    assert_eq!(embeddings["selected"], "openrouter");
    assert_eq!(
        embeddings["openrouter"],
        json!({"model": "acme/embed", "dimensions": 5})
    );
    let sent = seen.lock().expect("log").clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].method.as_str(), sent[0].path.as_str()),
        ("POST", "/v1/embeddings")
    );
    assert_eq!(sent[0].authorization.as_deref(), Some("Bearer sk-test"));
    assert!(sent[0].body.contains(TEST_PASSAGE) && sent[0].body.contains("acme/embed"));
    let chosen = chosen(Some(&scratch.home()), None, &Managed::default());
    assert_eq!(chosen.endpoint.expect("resolved").dimensions, 5);

    let test = tested_of(
        run(
            &scratch,
            json!({"service": "openrouter", "values": {"model": "acme/other"}}),
            None,
        )
        .expect("changed"),
    );
    assert_eq!(test["dimensions"], 7);
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["openrouter"],
        json!({"model": "acme/other", "dimensions": 7})
    );
}

/// RETRIEVE-36: values kept for a service not selected are written without a test call, and a
/// changed URL forgets the length found for the old one while a changed prefix keeps it.
#[test]
fn values_for_an_unselected_service_are_kept_untested() {
    let scratch = Scratch::new("unselected");
    scratch.keep(
        &json!({"retrieval": {"embeddings": {"selected": "openrouter",
        "custom": {"url": "http://127.0.0.1:9/v1/embeddings", "model": "m", "dimensions": 384}}}}),
    );

    let done = run(
        &scratch,
        json!({"service": "custom", "values": {"passagePrefix": "p: "}}),
        None,
    )
    .expect("kept");
    assert_eq!(tested_of(done), Value::Null);
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["custom"]["dimensions"],
        384
    );

    let done = run(
        &scratch,
        json!({"service": "custom", "values": {"url": "http://127.0.0.1:9/other"}}),
        None,
    )
    .expect("kept");
    assert_eq!(tested_of(done), Value::Null);
    let custom = &scratch.own()["retrieval"]["embeddings"]["custom"];
    assert_eq!(custom["url"], "http://127.0.0.1:9/other");
    assert_eq!(custom["passagePrefix"], "p: ");
    assert_eq!(custom.get("dimensions"), None);
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["selected"],
        "openrouter"
    );
}

/// RETRIEVE-36: a refused key, an address not found, and a reply that is not vectors are each
/// told by kind, and none keeps a length.
#[test]
fn a_failed_test_call_is_told_by_kind() {
    let (base, _) = serve(|path, _| match path {
        "/v1/refused" => (401, r#"{"error": "no"}"#.to_string()),
        "/v1/missing" => (404, "{}".to_string()),
        "/v1/failing" => (500, "{}".to_string()),
        _ => (200, "not json".to_string()),
    });
    let closed = format!("{}/embeddings", closed_base());
    let scratch = Scratch::new("failures");
    for (url, kind) in [
        (format!("{base}/refused"), "key"),
        (format!("{base}/missing"), "not-found"),
        (format!("{base}/garbled"), "shape"),
        (format!("{base}/failing"), "other"),
        (closed, "network"),
    ] {
        let test = tested_of(
            run(
                &scratch,
                json!({"select": "custom", "values": {"url": url, "model": "m"}}),
                None,
            )
            .expect("written"),
        );
        assert_eq!(test["ok"], false, "{url}");
        assert_eq!(test["kind"], kind, "{url}: {test}");
        assert!(
            test["message"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        );
        assert_eq!(
            scratch.own()["retrieval"]["embeddings"]["custom"].get("dimensions"),
            None
        );
        let chosen = chosen(Some(&scratch.home()), None, &Managed::default());
        assert_eq!(
            chosen.endpoint.err(),
            Some(Unresolved::Untested(Service::Custom)),
            "{url}"
        );
        assert_eq!(
            of(chosen.options.as_array().expect("an array"), "custom")["reason"],
            Unresolved::Untested(Service::Custom).to_string()
        );
    }
}

/// A base URL on loopback where nothing listens.
fn closed_base() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = listener.local_addr().expect("an address");
    drop(listener);
    format!("http://{address}/v1")
}

/// Settings with OpenRouter set up at `base`, `model` kept for it with `dimensions`, selected,
/// and retrieval `enabled` or not.
fn openrouter_kept(base: &str, model: &str, dimensions: usize, enabled: bool) -> Value {
    let mut settings = provider("openrouter", base);
    settings["retrieval"] = json!({"enabled": enabled, "embeddings": {"selected": "openrouter",
        "openrouter": {"model": model, "dimensions": dimensions}}});
    settings
}

/// RETRIEVE-38: with retrieval on, a model changed for the selected service is tested before
/// anything is written: a failed test refuses and leaves the file as it was, and a passing one
/// writes the model with the length it found in one go.
#[test]
fn with_retrieval_on_an_untested_change_is_tested_before_it_is_written() {
    let (base, seen) = serve(|_, body| {
        if body.contains("acme/broken") {
            (500, "{}".to_string())
        } else {
            (200, vector(6))
        }
    });
    let scratch = Scratch::new("pretest");
    scratch.keep(&openrouter_kept(&base, "acme/first", 5, true));
    let url = format!("{base}/embeddings");
    let before = scratch.own();

    let refused = run(
        &scratch,
        json!({"service": "openrouter", "values": {"model": "acme/broken"}}),
        Some(&url),
    )
    .err()
    .expect("a change whose test failed was written");
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(
        refused.message.contains("test call failed"),
        "{}",
        refused.message
    );
    assert_eq!(scratch.own(), before);
    assert_eq!(seen.lock().expect("log").len(), 1);

    let done = run(
        &scratch,
        json!({"service": "openrouter", "values": {"model": "acme/second"}}),
        Some(&url),
    )
    .expect("written");
    assert_eq!(tested_of(done)["dimensions"], 6);
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["openrouter"],
        json!({"model": "acme/second", "dimensions": 6})
    );
    assert_eq!(seen.lock().expect("log").len(), 2, "tested twice over");
}

/// RETRIEVE-38: with retrieval on, selecting a service with no length known whose test fails is
/// refused, writes nothing, and keeps no disclosure of where it sends.
#[test]
fn with_retrieval_on_an_untested_selection_that_fails_is_refused() {
    let (base, _) = serve(|_, _| (500, "{}".to_string()));
    let scratch = Scratch::new("pretest-select");
    let url = format!("{base}/embeddings");
    scratch.keep(&json!({"retrieval": {"enabled": true, "embeddings": {
        "custom": {"url": url, "model": "m"}}}}));
    let before = scratch.own();

    let refused = apply(
        &scratch.home(),
        None,
        &Managed::default(),
        &asked(json!({"select": "custom", "disclosed": url})).expect("read"),
        None,
        |_| panic!("kept a disclosure for a refused change"),
    )
    .err()
    .expect("selected an endpoint whose test failed");
    assert!(
        refused.message.contains("test call failed"),
        "{}",
        refused.message
    );
    assert_eq!(scratch.own(), before);
}

/// RETRIEVE-38: with retrieval on, a URL changed for the selected custom endpoint is a move to an
/// endpoint not shown: nothing is sent or written until it is.
#[test]
fn with_retrieval_on_a_changed_custom_url_must_be_shown_first() {
    let (old, _) = serve(|_, _| (200, vector(4)));
    let (new, seen) = serve(|_, _| (200, vector(4)));
    let scratch = Scratch::new("custom-url");
    let shown = format!("{old}/embeddings");
    scratch.keep(
        &json!({"retrieval": {"enabled": true, "embeddings": {"selected": "custom",
        "custom": {"url": shown, "model": "m", "dimensions": 4}}}}),
    );
    let before = scratch.own();
    let moved = format!("{new}/embeddings");

    match run(
        &scratch,
        json!({"service": "custom", "values": {"url": moved}}),
        Some(&shown),
    )
    .expect("answered")
    {
        Done::NeedsDisclosure(url) => assert_eq!(url, moved),
        Done::Written { .. } => panic!("moved without the disclosure"),
    }
    assert_eq!(scratch.own(), before);
    assert!(
        seen.lock().expect("log").is_empty(),
        "sent before it was shown"
    );
}

/// RETRIEVE-36: with retrieval off, a changed model whose test fails is kept without the length
/// found for the old one, and the option says it needs a test.
#[test]
fn a_changed_model_whose_test_fails_forgets_the_old_length() {
    let (base, _) = serve(|_, _| (401, "{}".to_string()));
    let scratch = Scratch::new("stale");
    scratch.keep(&openrouter_kept(&base, "acme/first", 5, false));

    let test = tested_of(
        run(
            &scratch,
            json!({"service": "openrouter", "values": {"model": "acme/second"}}),
            None,
        )
        .expect("written"),
    );

    assert_eq!(test["kind"], "key");
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["openrouter"],
        json!({"model": "acme/second"})
    );
    assert_eq!(
        of(&scratch.options(), "openrouter")["reason"],
        Unresolved::Untested(Service::OpenRouter).to_string()
    );
}

/// RETRIEVE-35: a model changed for a service not selected forgets its length, untested.
#[test]
fn a_changed_model_for_an_unselected_service_forgets_its_length() {
    let scratch = Scratch::new("unselected-model");
    scratch.keep(&json!({"retrieval": {"embeddings": {"selected": "custom",
        "openrouter": {"model": "acme/first", "dimensions": 5}}}}));

    let done = run(
        &scratch,
        json!({"service": "openrouter", "values": {"model": "acme/second"}}),
        None,
    )
    .expect("written");

    assert_eq!(tested_of(done), Value::Null);
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["openrouter"],
        json!({"model": "acme/second"})
    );
}

/// RETRIEVE-36: a request selecting one service and keeping values for another tests the one
/// selected and keeps the length found for it alone.
#[test]
fn the_length_found_is_kept_for_the_service_selected() {
    let (base, _) = serve(|_, _| (200, vector(3)));
    let scratch = Scratch::new("mixed");
    let mut settings = provider("openrouter", &base);
    settings["retrieval"] = json!({"embeddings": {
        "openrouter": {"model": "acme/first", "dimensions": 5},
        "custom": {"url": format!("{base}/embeddings"), "model": "m"}}});
    scratch.keep(&settings);

    let done = run(
        &scratch,
        json!({"select": "custom", "service": "openrouter", "values": {"model": "acme/second"}}),
        None,
    )
    .expect("written");

    assert_eq!(tested_of(done)["dimensions"], 3);
    let embeddings = &scratch.own()["retrieval"]["embeddings"];
    assert_eq!(embeddings["selected"], "custom");
    assert_eq!(embeddings["custom"]["dimensions"], 3);
    assert_eq!(embeddings["openrouter"], json!({"model": "acme/second"}));
}

/// RETRIEVE-33: selecting another service leaves what was kept for the one left.
#[test]
fn switching_away_keeps_what_was_kept() {
    let (base, _) = serve(|_, _| (200, vector(3)));
    let scratch = Scratch::new("switch");
    scratch.keep(&provider("openrouter", &base));
    run(
        &scratch,
        json!({"select": "custom", "values": {"url": format!("{base}/embeddings"),
            "model": "m", "queryPrefix": "q: "}}),
        None,
    )
    .expect("custom");
    let custom = scratch.own()["retrieval"]["embeddings"]["custom"].clone();
    assert_eq!(custom["dimensions"], 3);

    run(&scratch, json!({"select": "openrouter"}), None).expect("openrouter");

    let embeddings = &scratch.own()["retrieval"]["embeddings"];
    assert_eq!(embeddings["selected"], "openrouter");
    assert_eq!(embeddings["custom"], custom);
}

/// RETRIEVE-36: an OpenAI reply of another length than the model publishes is a shape failure,
/// and the published length is not overwritten.
#[test]
fn a_reply_of_another_length_than_published_is_a_shape_failure() {
    let (base, _) = serve(|_, _| (200, vector(3)));
    let scratch = Scratch::new("published");
    scratch.keep(&provider("openai", &base));

    let test = tested_of(run(&scratch, json!({"select": "openai"}), None).expect("written"));

    assert_eq!(test["kind"], "shape");
    assert!(
        test["message"]
            .as_str()
            .is_some_and(|text| text.contains("1536")),
        "{test}"
    );
    assert_eq!(scratch.own()["retrieval"]["embeddings"].get("openai"), None);
}

/// RETRIEVE-38: with retrieval on, moving to an endpoint the person was not shown writes nothing
/// and names it; asked again naming it, the disclosure is kept and the move made; a disclosure
/// that cannot be kept leaves it as it was.
#[test]
fn with_retrieval_on_a_new_endpoint_must_be_shown_first() {
    let (base, _) = serve(|_, _| (200, vector(4)));
    let scratch = Scratch::new("disclose");
    scratch.keep(&json!({"retrieval": {"enabled": true}}));
    let url = format!("{base}/embeddings");
    let params = json!({"select": "custom", "values": {"url": url, "model": "m"}});
    let before = scratch.own();

    match run(&scratch, params.clone(), Some(URL_ELSEWHERE)).expect("answered") {
        Done::NeedsDisclosure(named) => assert_eq!(named, url),
        Done::Written { .. } => panic!("moved without the disclosure"),
    }
    assert_eq!(scratch.own(), before);

    let mut shown = params.clone();
    shown["disclosed"] = json!(url);
    let refused = apply(
        &scratch.home(),
        None,
        &Managed::default(),
        &asked(shown.clone()).expect("read"),
        None,
        |_| false,
    )
    .err()
    .expect("moved with the disclosure unkept");
    assert!(refused.message.contains("could not be kept"));
    assert_eq!(scratch.own(), before);

    let mut kept = None;
    let done = apply(
        &scratch.home(),
        None,
        &Managed::default(),
        &asked(shown).expect("read"),
        None,
        |at| {
            kept = Some(at.to_string());
            true
        },
    )
    .expect("moved");
    assert_eq!(tested_of(done)["dimensions"], 4);
    assert_eq!(kept.as_deref(), Some(url.as_str()));
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["selected"],
        "custom"
    );

    let done = run(
        &scratch,
        json!({"service": "custom", "values": {"queryPrefix": "q"}}),
        Some(&url),
    )
    .expect("an endpoint already shown needs nothing");
    assert_eq!(tested_of(done)["ok"], true);
}

/// RETRIEVE-38: with retrieval on, a selection that cannot be used is refused and nothing is
/// written; with it off, the same selection is kept untested.
#[test]
fn with_retrieval_on_an_unusable_selection_is_refused() {
    let scratch = Scratch::new("unusable");
    scratch.keep(&json!({"retrieval": {"enabled": true}}));
    let before = scratch.own();

    let refused = run(&scratch, json!({"select": "custom"}), None)
        .err()
        .expect("selected an endpoint with no URL");
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(
        refused.message.contains("retrieval is on"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("no http(s) URL"),
        "{}",
        refused.message
    );
    assert_eq!(scratch.own(), before);

    scratch.keep(&json!({"retrieval": {"enabled": false}}));
    let done = run(&scratch, json!({"select": "custom"}), None).expect("kept while off");
    assert_eq!(tested_of(done), Value::Null);
    assert_eq!(
        scratch.own()["retrieval"]["embeddings"]["selected"],
        "custom"
    );
}

/// RETRIEVE-35: an administrator's `false` and a run file stating `retrieval.embeddings` lock the
/// choice and refuse a change naming the file; a run file stating neither locks nothing.
#[test]
fn a_managed_pin_or_a_run_file_choosing_locks_the_choice() {
    let scratch = Scratch::new("locked");
    let params = json!({"select": "custom"});
    let managed_path = scratch.write("managed.json", r#"{"retrieval": {"enabled": false}}"#);
    let managed = Managed::at(&managed_path);
    let refused = apply(
        &scratch.home(),
        None,
        &managed,
        &asked(params.clone()).expect("read"),
        None,
        |_| true,
    )
    .err()
    .expect("an administrator's pin was overridden");
    assert_eq!(refused.code, ErrorCode::Config);
    assert!(refused.message.contains("administrator"));
    assert!(
        refused
            .message
            .contains(&managed_path.display().to_string())
    );

    let run_path = scratch.write(
        "run.json",
        r#"{"retrieval": {"embeddings": {"selected": "brave"}}}"#,
    );
    let none = Managed::default();
    let settings = layered(Some(&scratch.home()), Some(&run_path), &none);
    assert_eq!(
        locked(&settings, &none, Some(&scratch.own_file()), Some(&run_path)),
        Some(json!({"by": "run", "path": run_path}))
    );
    let refused = apply(
        &scratch.home(),
        Some(&run_path),
        &none,
        &asked(params.clone()).expect("read"),
        None,
        |_| true,
    )
    .err()
    .expect("a run's file was overridden");
    assert!(
        refused.message.contains("for this run"),
        "{}",
        refused.message
    );
    assert!(!scratch.own_file().exists());

    let switch = scratch.write("switch.json", r#"{"retrieval": {"enabled": false}}"#);
    let refused = apply(
        &scratch.home(),
        Some(&switch),
        &none,
        &asked(params.clone()).expect("read"),
        None,
        |_| true,
    )
    .err()
    .expect("a run's switch was overridden");
    assert!(
        refused.message.contains("for this run"),
        "{}",
        refused.message
    );
    let state = |run: &Path, managed: &Managed| {
        state_in(
            Some(&scratch.home()),
            Some(run),
            managed,
            None,
            &chosen(Some(&scratch.home()), Some(run), managed),
        )["embeddings"]["locked"]
            .clone()
    };
    assert_eq!(state(&switch, &none), json!({"by": "run", "path": switch}));
    assert_eq!(
        state(&run_path, &none),
        json!({"by": "run", "path": run_path})
    );
    assert_eq!(
        state(&run_path, &managed),
        json!({"by": "managed", "path": managed_path})
    );

    let quiet = scratch.write("quiet.json", r#"{"model": "m"}"#);
    let settings = layered(Some(&scratch.home()), Some(&quiet), &none);
    assert_eq!(
        locked(&settings, &none, Some(&scratch.own_file()), Some(&quiet)),
        None
    );
    apply(
        &scratch.home(),
        Some(&quiet),
        &none,
        &asked(params).expect("read"),
        None,
        |_| true,
    )
    .expect("a run file choosing nothing locks nothing");
}

/// RETRIEVE-35: the custom key is kept readable by this user alone, told as stored, sent with the
/// test call, and removed by `null`.
#[test]
fn the_custom_key_is_kept_privately_and_removed_by_null() {
    let (base, seen) = serve(|_, _| (200, vector(2)));
    let scratch = Scratch::new("key");
    let url = format!("{base}/embeddings");
    run(
        &scratch,
        json!({"select": "custom", "values": {"url": url, "model": "m", "key": "the-key"}}),
        None,
    )
    .expect("kept");

    let path = bravebot_config::embeddings_key::file(&scratch.home());
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .expect("the key file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(
        bravebot_config::embeddings_key::read(&scratch.home())
            .expect("readable")
            .as_ref()
            .map(Secret::expose),
        Some("the-key")
    );
    assert!(
        !std::fs::read_to_string(scratch.own_file())
            .expect("home")
            .contains("the-key")
    );
    assert_eq!(
        seen.lock().expect("log")[0].authorization.as_deref(),
        Some("Bearer the-key")
    );
    assert_eq!(of(&scratch.options(), "custom")["keyStored"], true);

    run(
        &scratch,
        json!({"service": "custom", "values": {"key": null}}),
        None,
    )
    .expect("removed");
    assert!(!path.exists());
    assert_eq!(of(&scratch.options(), "custom")["keyStored"], false);
    run(
        &scratch,
        json!({"service": "custom", "values": {"key": ""}}),
        None,
    )
    .expect("removing no key is no failure");
}

/// RETRIEVE-35: a request that names nothing to change, values for no service, values a service
/// does not keep, or a field of the wrong kind is refused before anything is read or written.
#[test]
fn a_request_that_cannot_be_applied_is_refused() {
    for (params, says) in [
        (json!({}), "nothing to change"),
        (json!({"select": "cohere"}), "not an embeddings service"),
        (json!({"select": 3}), "must be a string"),
        (json!({"values": {"model": "m"}}), "need a `service`"),
        (
            json!({"service": "custom", "values": {"dimensions": 3}}),
            "values.dimensions",
        ),
        (
            json!({"service": "openrouter", "values": {"url": "https://x"}}),
            "only the custom",
        ),
        (
            json!({"service": "openai", "values": {"key": "k"}}),
            "only the custom",
        ),
        (
            json!({"service": "brave", "values": {"model": "m"}}),
            "Brave's model",
        ),
        (
            json!({"service": "custom", "values": {"model": 1}}),
            "string or null",
        ),
        (
            json!({"service": "custom", "values": []}),
            "must be an object",
        ),
    ] {
        let refused = asked(params.clone()).expect_err(&params.to_string());
        assert_eq!(refused.code, ErrorCode::BadRequest, "{params}");
        assert!(
            refused.message.contains(says),
            "{params}: {}",
            refused.message
        );
    }
    let read = asked(json!({"select": "openrouter",
        "values": {"model": " ", "passagePrefix": ""}}))
    .expect("read");
    assert_eq!(read.service, Some(Service::OpenRouter));
    assert_eq!(read.model, Field::Removed);
    assert_eq!(read.passage_prefix, Field::Set(String::new()));
}

/// RETRIEVE-37: OpenRouter's models are listed with its key and named, answered from memory for
/// a few minutes unless refreshed or asked with another key, and asked again once that has passed.
#[test]
fn the_models_are_listed_and_answered_from_memory() {
    let _listing = listing();
    let listing = json!({"data": [
        {"id": "acme/embed", "name": "Acme Embed", "context_length": 8192,
         "pricing": {"prompt": "0.00000002"}, "extra": true},
        {"id": "bare/model"},
    ]});
    let (base, seen) = serve(move |_, _| (200, listing.to_string()));
    let url = format!("{base}/embeddings");
    let key = Secret::new("sk-list");
    let now = Instant::now();

    let answer = listed(&url, Some(&key), false, now).expect("listed");

    assert_eq!(
        answer,
        json!({"models": [
            {"id": "acme/embed", "name": "Acme Embed", "contextLength": 8192,
             "pricing": {"prompt": "0.00000002"}},
            {"id": "bare/model", "name": "bare/model", "contextLength": null,
             "pricing": {"prompt": null}},
        ]})
    );
    let count = || seen.lock().expect("log").len();
    assert_eq!(count(), 1);
    let first = seen.lock().expect("log")[0].clone();
    assert_eq!(
        (first.method.as_str(), first.path.as_str()),
        ("GET", "/v1/embeddings/models")
    );
    assert_eq!(first.authorization.as_deref(), Some("Bearer sk-list"));

    assert_eq!(
        listed(&url, Some(&key), false, now).expect("remembered"),
        answer
    );
    assert_eq!(count(), 1);
    listed(&url, Some(&key), true, now).expect("refreshed");
    assert_eq!(count(), 2);
    let later = now + LISTING_KEPT;
    listed(&url, Some(&key), false, later).expect("asked again");
    assert_eq!(count(), 3);
    let changed = Secret::new("sk-changed");
    listed(&url, Some(&changed), false, later).expect("asked with the new key");
    assert_eq!(
        count(),
        4,
        "a listing asked with another key was answered from memory"
    );
    assert_eq!(
        seen.lock().expect("log")[3].authorization.as_deref(),
        Some("Bearer sk-changed")
    );
    listed(&url, Some(&changed), false, later).expect("remembered");
    listed(&url, None, false, later).expect("asked with no key");
    assert_eq!(count(), 5);

    let (refusing, _) = serve(|_, _| (401, "{}".to_string()));
    let refused = listed(&format!("{refusing}/embeddings"), Some(&key), false, now)
        .expect_err("listed with a refused key");
    assert_eq!(refused.code, ErrorCode::Config);
    assert_eq!(refused.message, "OpenRouter refused its key (HTTP 401)");
    let (failing, _) = serve(|_, _| (500, "{}".to_string()));
    let failed = listed(&format!("{failing}/embeddings"), None, false, now).expect_err("listed");
    assert!(failed.message.contains("HTTP 500"), "{}", failed.message);
    let (garbled, _) = serve(|_, _| (200, "[]".to_string()));
    let unusable = listed(&format!("{garbled}/embeddings"), None, false, now).expect_err("listed");
    assert!(
        unusable.message.contains("not usable"),
        "{}",
        unusable.message
    );
}

/// RETRIEVE-37: where OpenRouter is not set up the listing says so and asks nothing; set up, it is
/// asked at its own base URL.
#[test]
fn the_models_of_an_openrouter_not_set_up_are_not_asked_for() {
    let _listing = listing();
    let scratch = Scratch::new("models-unset");
    let none = Managed::default();
    let refused = models_in(Some(&scratch.home()), None, &none, false).expect_err("listed");
    assert_eq!(refused.code, ErrorCode::Config);
    assert_eq!(refused.message, "OpenRouter is not set up");

    let (base, seen) = serve(|_, _| (200, r#"{"data": []}"#.to_string()));
    scratch.keep(&provider("openrouter", &base));
    assert_eq!(
        models_in(Some(&scratch.home()), None, &none, true).expect("listed"),
        json!({"models": []})
    );
    let sent = seen.lock().expect("log").clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].path, "/v1/embeddings/models");
    assert_eq!(sent[0].authorization.as_deref(), Some("Bearer sk-test"));
}

/// RETRIEVE-35: the window's state names the service selected and the one in force, what locks
/// the choice, and every option; a request answers that state with its test or the endpoint
/// still to be shown.
#[test]
fn the_window_state_carries_the_choice_and_the_answer_its_outcome() {
    let scratch = Scratch::new("state");
    scratch.keep(&json!({"retrieval": {"embeddings": {"selected": "custom",
        "custom": {"url": "https://embed.example/v1/embeddings", "model": "m", "dimensions": 8}}}}));
    let none = Managed::default();
    let state = state_in(
        Some(&scratch.home()),
        None,
        &none,
        None,
        &chosen(Some(&scratch.home()), None, &none),
    );
    let embeddings = &state["embeddings"];
    assert_eq!(embeddings["selected"], "custom");
    assert_eq!(embeddings["inForce"], "custom");
    assert_eq!(embeddings["locked"], Value::Null);
    assert_eq!(embeddings["options"].as_array().map(Vec::len), Some(3));
    assert_eq!(
        of(
            embeddings["options"].as_array().expect("an array"),
            "custom"
        )["reason"],
        Value::Null
    );
    assert_eq!(state["endpoint"]["host"], "embed.example");

    let shown = answer(
        &json!({"enabled": true}),
        &Done::NeedsDisclosure(URL_ELSEWHERE.into()),
    );
    assert_eq!(
        shown,
        json!({"state": {"enabled": true}, "needsDisclosure": URL_ELSEWHERE})
    );
    let untested = answer(&json!({}), &Done::Written { tested: None });
    assert_eq!(untested, json!({"state": {}, "test": null}));
}

fn services(chosen: &crate::retrieval::Chosen) -> Vec<String> {
    chosen
        .options
        .as_array()
        .expect("an array")
        .iter()
        .map(|option| option["service"].as_str().expect("a word").to_owned())
        .collect()
}

#[test]
fn without_the_flag_nothing_selected_is_nothing_in_force_and_brave_is_not_listed() {
    let scratch = Scratch::new("unflagged-nothing");
    let chosen =
        crate::retrieval::chosen_with(Some(&scratch.home()), None, &Managed::default(), false);
    assert_eq!(chosen.service, None);
    assert_eq!(
        chosen.endpoint.as_ref().err(),
        Some(&Unresolved::NothingSelected)
    );
    assert_eq!(services(&chosen), ["openai", "openrouter", "custom"]);
}

#[test]
fn without_the_flag_a_selection_of_brave_is_a_service_not_offered() {
    let scratch = Scratch::new("unflagged-brave");
    scratch.keep(&json!({"retrieval": {"embeddings": {"selected": "brave"}}}));
    let chosen =
        crate::retrieval::chosen_with(Some(&scratch.home()), None, &Managed::default(), false);
    assert_eq!(chosen.selected, Some(Service::Brave));
    assert_eq!(
        chosen.endpoint.as_ref().err(),
        Some(&Unresolved::NotOffered(Service::Brave))
    );
    assert!(!services(&chosen).iter().any(|word| word == "brave"));
}

#[test]
fn with_the_flag_brave_is_listed_and_in_force_where_the_build_reaches_brave() {
    let scratch = Scratch::new("flagged");
    let managed = Managed::default();
    let reaches = bravebot_config::Config::from_env_and_settings(&Settings::default(), &managed)
        .is_ok_and(|config| config.premium_endpoint.is_some());
    let chosen = crate::retrieval::chosen_with(Some(&scratch.home()), None, &managed, true);
    let listed = services(&chosen).iter().any(|word| word == "brave");
    assert_eq!(listed, reaches);
    assert_eq!(chosen.service, reaches.then_some(Service::Brave));
}
