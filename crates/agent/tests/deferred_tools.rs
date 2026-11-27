//! A turn's built-in tools offered in full from a fixed core list, and every other one offered by
//! name under `load_tool` until the planner loads it (TOOL-7), end to end against a mock chat
//! server. Each is asserted on the bodies that went out to the model.

#[allow(
    dead_code,
    reason = "turns.rs is shared by several test binaries, and this one uses only run"
)]
#[path = "../test-support/turns.rs"]
mod turns;

#[allow(
    dead_code,
    reason = "open_files.rs is shared by several test binaries, and this one uses only bind_mock_server"
)]
#[path = "../test-support/open_files.rs"]
mod open_files;

use bravebot_agent::confirm::{
    CallDecision, Confirmer, Decision, ExposureRequest, FetchRequest, HostRequest, ManifestRequest,
    McpCallRequest, MessageDecision, MessageRequest, MoveRequest, OutputRequest, PathRequest,
    RunDecision, RunRequest, ServerRequest, ToolListRequest, Unattended, VetRequest, VouchRequest,
    WriteDecision, WriteRequest,
};
use bravebot_agent::turn::Task;
use bravebot_agent::{Workspace, tools::CORE};
use bravebot_config::Config;
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::event::RecordingSink;
use bravebot_net::Egress;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bravebot-deferred-tools-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Read one HTTP request off a connection and hand back its body.
fn body_of(stream: &std::net::TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    let _ = reader.read_exact(&mut body);
    String::from_utf8_lossy(&body).to_string()
}

/// What a confined check's own request is recognised by.
const A_CHECK_ASKING: &str = "prompt-injection classifier";

/// A mock chat server: the turn's own rounds get `replies` in order, and every check is answered
/// that it found nothing. Every body is reported, checks included.
fn serve_chat(replies: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = open_files::bind_mock_server();
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut replies = replies.into_iter();
        let mut answered: Option<(String, String)> = None;
        while let Ok((mut stream, _)) = listener.accept() {
            let body = body_of(&stream);
            let _ = sender.send(body.clone());
            let reply = match &answered {
                Some((asked, reply)) if *asked == body => reply.clone(),
                _ if body.contains(A_CHECK_ASKING) => reply_with(
                    r#"{\"verdict\": \"safe\", \"reason\": \"nothing addressed to a reader\"}"#,
                ),
                _ => match replies.next() {
                    Some(reply) => reply,
                    None => {
                        let refusal = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(refusal.as_bytes());
                        continue;
                    }
                },
            };
            answered = Some((body, reply.clone()));
            let frames = as_sse(&reply);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{frames}",
                frames.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), receiver)
}

/// A whole chat response as the SSE stream that would have delivered it.
fn as_sse(reply: &str) -> String {
    let parsed: Value = serde_json::from_str(reply).expect("a valid reply");
    let mut frames = String::new();
    let mut frame = |value: Value| frames.push_str(&format!("data: {value}\n\n"));
    frame(json!({"model": "test-model", "choices": [{"delta": {"role": "assistant"}}]}));
    let message = parsed
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or(json!({}));
    if let Some(content) = message.get("content").and_then(Value::as_str) {
        frame(json!({"choices": [{"delta": {"content": content}}]}));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            frame(json!({"choices": [{"delta": {"tool_calls": [{
                "index": index,
                "id": call.get("id"),
                "function": {
                    "name": call.pointer("/function/name"),
                    "arguments": call.pointer("/function/arguments"),
                },
            }]}}]}));
        }
    }
    frame(json!({
        "choices": [{"finish_reason": "stop"}],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0},
    }));
    frames.push_str("data: [DONE]\n\n");
    frames
}

fn config_for(endpoint: &str) -> Config {
    Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.to_string()),
        _ => None,
    })
    .expect("config")
}

fn reply_with(content: &str) -> String {
    format!(
        r#"{{"model":"test-model","choices":[{{"message":{{"role":"assistant","content":"{content}"}}}}]}}"#
    )
}

fn tool_request(tool: &str, arguments: &Value) -> String {
    json!({
        "model": "test-model",
        "choices": [{"message": {"role": "assistant", "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": {"name": tool, "arguments": arguments.to_string()},
        }]}}],
    })
    .to_string()
}

/// Refuses every question as nobody there would, and counts each one it was put. Approves a run
/// where `approving_runs` says so, for a test that needs a program to start.
#[derive(Default)]
struct Counting {
    asked: usize,
    approving_runs: bool,
}

impl Confirmer for Counting {
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        self.asked += 1;
        Unattended.confirm_write(request)
    }
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        self.asked += 1;
        if self.approving_runs {
            return RunDecision::approve();
        }
        Unattended.confirm_run(request)
    }
    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_read_output(request)
    }
    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_vetted_read(request)
    }
    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_fetch(request)
    }
    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_server(request)
    }
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_manifest(request)
    }
    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_vouch(request)
    }
    fn confirm_exposing_read(&mut self, request: &ExposureRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_exposing_read(request)
    }
    fn confirm_tool_list(&mut self, request: &ToolListRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_tool_list(request)
    }
    fn confirm_mcp_call(&mut self, request: &McpCallRequest) -> CallDecision {
        self.asked += 1;
        Unattended.confirm_mcp_call(request)
    }
    fn confirm_move(&mut self, request: &MoveRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_move(request)
    }
    fn confirm_path(&mut self, request: &PathRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_path(request)
    }
    fn confirm_host(&mut self, request: &HostRequest) -> Decision {
        self.asked += 1;
        Unattended.confirm_host(request)
    }
    fn confirm_message(&mut self, request: &MessageRequest) -> MessageDecision {
        self.asked += 1;
        Unattended.confirm_message(request)
    }
    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        self.asked += 1;
        Unattended.ask_user(asking)
    }
    fn interjection(&mut self) -> Option<String> {
        None
    }
}

/// The bodies of one turn's own rounds, in order, without the checks between them.
fn run(name: &str, task: Task, replies: Vec<String>, confirmer: &mut Counting) -> Vec<Value> {
    run_configured(name, task, replies, confirmer, |config| config)
}

/// [`run`] with the configuration `configure` makes of the one the mock server needs.
fn run_configured(
    name: &str,
    task: Task,
    replies: Vec<String>,
    confirmer: &mut Counting,
    configure: impl FnOnce(Config) -> Config,
) -> Vec<Value> {
    let scratch = Scratch::new(name);
    let (endpoint, chat) = serve_chat(replies);
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    turns::run(
        &configure(config_for(&endpoint)),
        &Egress::new(),
        &workspace,
        &task,
        confirmer,
        &mut RecordingSink::new(),
    )
    .expect("the turn runs");
    chat.try_iter()
        .filter(|body| !body.contains(A_CHECK_ASKING))
        .map(|body| serde_json::from_str(&body).expect("a request"))
        .collect()
}

/// The functions a request offered, as sent.
fn functions(request: &Value) -> Vec<Value> {
    request["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["function"].clone())
        .collect()
}

fn names(request: &Value) -> Vec<String> {
    functions(request)
        .iter()
        .map(|function| function["name"].as_str().expect("a name").to_string())
        .collect()
}

fn function<'a>(functions: &'a [Value], name: &str) -> Option<&'a Value> {
    functions.iter().find(|function| function["name"] == name)
}

/// What the planner was sent as the answer to its last tool call.
fn answer(request: &Value) -> String {
    request["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .rev()
        .find(|message| message["role"] == "tool")
        .and_then(|message| message["content"].as_str())
        .expect("a tool message")
        .to_string()
}

/// A built-in a planner's turn offers and the core list does not hold.
const HELD: &str = "repo_map";

/// TOOL-7: a turn's first request offers in full the core tools and nothing else of the table, and
/// lists every other built-in it would offer under `load_tool`, with none of their descriptions. The
/// regressions it rejects are deferring nothing, deferring a core tool, and a held-back tool left
/// out of the loader's list, which could then never be loaded.
#[test]
fn a_turns_first_request_offers_the_core_list_in_full_and_the_rest_by_name() {
    let sent = run(
        "first-request",
        Task::new("say done"),
        vec![reply_with("done")],
        &mut Counting::default(),
    );
    let first = &sent[0];
    let offered = names(first);
    let mut expected: Vec<String> = CORE.iter().map(|name| name.to_string()).collect();
    expected.push("load_tool".to_string());
    let mut got = offered.clone();
    got.sort();
    expected.sort();
    assert_eq!(got, expected, "the first request offered {offered:?}");

    let all = functions(first);
    assert!(
        function(&all, "load_tool").is_some(),
        "the first request offered no loader: {offered:?}"
    );
    let listed = listed_for_loading(first);
    for held in [
        HELD,
        "lsp",
        "request_path",
        "download_url",
        "remember",
        "recall",
    ] {
        assert!(
            listed.contains(&held.to_string()),
            "{held} is not listed for loading: {listed:?}"
        );
    }
    assert!(
        !first.to_string().contains("Map the declarations"),
        "repo_map's description was sent before it was loaded"
    );
}

/// TOOL-7, TOOL-5: a load offers the tool in full from the next request to the end of the turn, and
/// its definition takes `why` and lists it as required. The regressions it rejects are a load that
/// lasts one request, one that offers the definition on the request that loaded it, and a loaded
/// definition that skipped the step that asks why.
#[test]
fn a_loaded_tool_is_offered_from_the_next_request_to_the_end_of_the_turn_and_asks_why() {
    let sent = run(
        "loaded",
        Task::new("map the code"),
        vec![
            tool_request(
                "load_tool",
                &json!({"name": HELD, "why": "to map the code"}),
            ),
            tool_request("list_files", &json!({"directory": ".", "why": "to look"})),
            reply_with("done"),
        ],
        &mut Counting::default(),
    );
    let [first, second, third, ..] = sent.as_slice() else {
        panic!("the turn made {} requests", sent.len());
    };
    assert!(
        !names(first).contains(&HELD.to_string()),
        "{:?}",
        names(first)
    );
    assert!(
        answer(second).contains("is loaded"),
        "the load was not confirmed: {}",
        answer(second)
    );
    for (which, request) in [("second", second), ("third", third)] {
        let all = functions(request);
        let loaded = function(&all, HELD).unwrap_or_else(|| {
            panic!(
                "the {which} request did not offer {HELD}: {:?}",
                names(request)
            )
        });
        assert_eq!(
            loaded["parameters"]["properties"]["why"]["type"], "string",
            "the loaded {HELD} does not take why"
        );
        assert!(
            loaded["parameters"]["required"]
                .as_array()
                .expect("required")
                .contains(&json!("why")),
            "the loaded {HELD} does not require why"
        );
    }
}

/// TOOL-7: a load appends the definition after every tool offered before it, and leaves those as
/// they were, so the cached prefix in front of it is the one the earlier request sent. The
/// regression it rejects is a load that rebuilds or re-sorts the list.
#[test]
fn a_load_leaves_the_tools_offered_before_it_unchanged() {
    let sent = run(
        "prefix",
        Task::new("map the code"),
        vec![
            tool_request(
                "load_tool",
                &json!({"name": HELD, "why": "to map the code"}),
            ),
            reply_with("done"),
        ],
        &mut Counting::default(),
    );
    let [first, second, ..] = sent.as_slice() else {
        panic!("the turn made {} requests", sent.len());
    };
    let before = functions(first);
    let after = functions(second);
    assert_eq!(after.len(), before.len() + 1, "{:?}", names(second));
    assert_eq!(
        &after[..before.len()],
        before.as_slice(),
        "the tools offered before the load changed"
    );
    assert_eq!(after[before.len()]["name"], HELD);
}

/// TOOL-7, LOAD-2: the name is matched exactly, so another case, one character out, a longer name,
/// a space and a prefix each load nothing, and the planner is told so. The regression it rejects
/// is a load that folds case, trims or takes the nearest or a prefix match.
#[test]
fn a_name_close_to_a_held_back_tool_loads_nothing() {
    for near in ["Repo_map", "repo_ma", "repo_map2", "repo_map ", "repo"] {
        let sent = run(
            "near",
            Task::new("map the code"),
            vec![
                tool_request(
                    "load_tool",
                    &json!({"name": near, "why": "to map the code"}),
                ),
                reply_with("done"),
            ],
            &mut Counting::default(),
        );
        let [_, second, ..] = sent.as_slice() else {
            panic!("{near:?}: the turn made {} requests", sent.len());
        };
        assert!(
            !names(second).contains(&HELD.to_string()),
            "{near:?} loaded {HELD}"
        );
        assert!(
            answer(second).contains("no tool named"),
            "{near:?} was not refused: {}",
            answer(second)
        );
    }
}

/// TOOL-3, TOOL-7: a call to a held-back tool that was not loaded is refused with the way to load
/// it, ahead of the check of its arguments, and puts no question to anybody. The same call in a
/// turn that offers every tool in full is put to the person, which shows the fixture would ask. The
/// regression it rejects is a dispatch that runs a built-in on its name alone, whatever the turn
/// offered.
#[test]
fn a_call_to_a_held_back_tool_is_refused_runs_nothing_and_asks_nobody() {
    let call = || {
        tool_request(
            "download_url",
            &json!({
                "url": "http://127.0.0.1:9/file.txt",
                "path": "file.txt",
                "undeclared": true,
                "why": "to fetch the file",
            }),
        )
    };
    let mut held = Counting::default();
    let sent = run(
        "unloaded-call",
        Task::new("fetch the file"),
        vec![call(), reply_with("done")],
        &mut held,
    );
    let told = answer(&sent[1]);
    assert!(
        told.contains("download_url is not loaded") && told.contains("load_tool"),
        "{told}"
    );
    assert_eq!(
        held.asked, 0,
        "a call to a held-back tool was put to the person"
    );

    let mut full = Counting::default();
    let sent = run(
        "unloaded-call-control",
        Task::new("fetch the file").offering_every_tool_in_full(),
        vec![
            tool_request(
                "download_url",
                &json!({
                    "url": "http://127.0.0.1:9/file.txt",
                    "path": "file.txt",
                    "why": "to fetch the file",
                }),
            ),
            reply_with("done"),
        ],
        &mut full,
    );
    assert!(
        !answer(&sent[1]).contains("not loaded"),
        "{}",
        answer(&sent[1])
    );
    assert!(
        full.asked > 0,
        "the same call in a turn offering it was not put to the person, so this says nothing"
    );
}

/// The built-in names `load_tool`'s description lists in `request`, read off the sentence that
/// lists them. Empty where the request offers no loader or the loader lists no built-in.
fn listed_for_loading(request: &Value) -> Vec<String> {
    let all = functions(request);
    let Some(loader) = function(&all, "load_tool") else {
        return Vec::new();
    };
    let description = loader["description"].as_str().expect("a description");
    let Some((_, after)) = description.split_once("Tools of this program: ") else {
        return Vec::new();
    };
    let (list, _) = after
        .split_once('.')
        .expect("the list ends with a full stop");
    list.split(", ").map(str::to_string).collect()
}

/// The built-ins a planner's turn offers only in some situations: a later look to arrange, a session
/// keeping watches, a front end keeping links and a search key, and the two it offers every planner.
const SITUATIONAL: [&str; 6] = [
    "watch_file",
    "schedule_next",
    "remember",
    "propose_rule",
    "set_link",
    "web_search",
];

/// TOOL-7: in a turn that would offer each of `watch_file`, `schedule_next`, `remember`,
/// `propose_rule`, `set_link` and `web_search`, none of them is in the first request, each is listed
/// under `load_tool`, and each loads by its name. The regression it rejects is a split applied to
/// the table alone, before the tools a turn adds for its situation are appended, which would send
/// each of these in full on every request.
#[test]
fn the_tools_a_turn_adds_for_its_situation_are_held_back_and_each_loads() {
    let keys = Scratch::new("situational-key");
    let key = keys.path.join("search-key.txt");
    std::fs::write(&key, "placeholder-key\n").expect("write the key file");
    let mut replies: Vec<String> = SITUATIONAL
        .iter()
        .map(|name| tool_request("load_tool", &json!({"name": name, "why": "to use it"})))
        .collect();
    replies.push(reply_with("done"));
    let task = Task::new("watch the notes and keep what matters")
        .looking_again(true)
        .arming(bravebot_agent::watch::Arming::Allowed { free: 8 })
        .keeping_links(true);
    let sent = run_configured(
        "situational",
        task,
        replies,
        &mut Counting::default(),
        |config| config.with_web_search_key_file(&key),
    );
    assert_eq!(
        sent.len(),
        SITUATIONAL.len() + 1,
        "the turn made {} requests",
        sent.len()
    );
    let first = &sent[0];
    let offered = names(first);
    let listed = listed_for_loading(first);
    for name in SITUATIONAL {
        assert!(
            !offered.contains(&name.to_string()),
            "{name} was offered in full before it was loaded: {offered:?}"
        );
        assert!(
            listed.contains(&name.to_string()),
            "{name} is not listed for loading: {listed:?}"
        );
    }
    let last = names(sent.last().expect("a request"));
    for name in SITUATIONAL {
        assert!(
            last.contains(&name.to_string()),
            "{name} was not offered after it was loaded: {last:?}"
        );
    }
}

/// TOOL-7: a tool the turn would not offer is neither listed under `load_tool` nor loaded by its
/// name. A one-shot turn with no later look, no watches, no links and no search key would offer none
/// of `watch_file`, `schedule_next`, `set_link` and `web_search`. The regression it rejects is a
/// loader that lists or loads from a fixed list of held-back names, which would hand a turn a tool
/// its situation withholds.
#[test]
fn a_tool_the_turn_would_not_offer_is_not_listed_and_does_not_load() {
    let withheld = ["watch_file", "schedule_next", "set_link", "web_search"];
    let mut replies: Vec<String> = withheld
        .iter()
        .map(|name| tool_request("load_tool", &json!({"name": name, "why": "to use it"})))
        .collect();
    replies.push(reply_with("done"));
    let sent = run(
        "withheld",
        Task::new("say done"),
        replies,
        &mut Counting::default(),
    );
    assert_eq!(
        sent.len(),
        withheld.len() + 1,
        "the turn made {} requests",
        sent.len()
    );
    let listed = listed_for_loading(&sent[0]);
    assert!(
        listed.contains(&HELD.to_string()),
        "the loader lists nothing, so this says nothing: {listed:?}"
    );
    for (at, name) in withheld.iter().enumerate() {
        assert!(
            !listed.contains(&name.to_string()),
            "{name} is listed for a turn that would not offer it: {listed:?}"
        );
        assert!(
            answer(&sent[at + 1]).contains("no tool named"),
            "loading {name} was not refused: {}",
            answer(&sent[at + 1])
        );
    }
    let last = names(sent.last().expect("a request"));
    for name in withheld {
        assert!(
            !last.contains(&name.to_string()),
            "{name} was loaded: {last:?}"
        );
    }
}

/// TOOL-7: `download_url` and `request_path` are not in a one-shot turn's first request,
/// each is listed under `load_tool`, each loads by its name, and the loaded `download_url` is
/// called and reaches its question to the person instead of the not-loaded refusal. The names are
/// written out here rather than read off `CORE`, so the regression it rejects is either tool added
/// to the core list, which sends its definition in full on every request.
#[test]
fn download_url_and_request_path_are_held_back_and_each_loads() {
    let mut asked = Counting::default();
    let sent = run(
        "download-and-path",
        Task::new("save the file"),
        vec![
            tool_request(
                "load_tool",
                &json!({"name": "download_url", "why": "to save a file"}),
            ),
            tool_request(
                "load_tool",
                &json!({"name": "request_path", "why": "to reach a path"}),
            ),
            tool_request(
                "download_url",
                &json!({
                    "url": "http://127.0.0.1:9/file.txt",
                    "path": "file.txt",
                    "why": "to save the file",
                }),
            ),
            reply_with("done"),
        ],
        &mut asked,
    );
    let [first, .., last] = sent.as_slice() else {
        panic!("the turn made {} requests", sent.len());
    };
    assert_eq!(sent.len(), 4, "the turn made {} requests", sent.len());
    let offered = names(first);
    let listed = listed_for_loading(first);
    for name in ["download_url", "request_path"] {
        assert!(
            !offered.contains(&name.to_string()),
            "{name} was offered in full before it was loaded: {offered:?}"
        );
        assert!(
            listed.contains(&name.to_string()),
            "{name} is not listed for loading: {listed:?}"
        );
        assert!(
            names(last).contains(&name.to_string()),
            "{name} was not offered after it was loaded: {:?}",
            names(last)
        );
    }
    assert!(
        !first.to_string().contains("Save an http or https URL"),
        "download_url's description was sent before it was loaded"
    );
    let told = answer(last);
    assert!(!told.contains("not loaded"), "{told}");
    assert!(
        asked.asked > 0,
        "the loaded download_url did not reach its question: {told}"
    );
}

/// A mock chat server for a turn and the delegates it starts, which run beside it. Each body is
/// answered by the first rule whose marker it holds and that has a reply left, and every check is
/// answered that it found nothing. A turn replays the arguments it called with, so its own marker
/// goes before the tasks it hands out. Every body is reported.
fn serve_by_marker(rules: Vec<(&'static str, Vec<String>)>) -> (String, mpsc::Receiver<String>) {
    let listener = open_files::bind_mock_server();
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = mpsc::channel();
    let mut rules: Vec<(&str, std::collections::VecDeque<String>)> = rules
        .into_iter()
        .map(|(marker, replies)| (marker, replies.into()))
        .collect();
    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let body = body_of(&stream);
            let _ = sender.send(body.clone());
            let reply = match body.contains(A_CHECK_ASKING) {
                true => Some(reply_with(
                    r#"{\"verdict\": \"safe\", \"reason\": \"nothing addressed to a reader\"}"#,
                )),
                false => rules
                    .iter_mut()
                    .find(|(marker, replies)| body.contains(marker) && !replies.is_empty())
                    .and_then(|(_, replies)| replies.pop_front()),
            };
            let Some(reply) = reply else {
                let refusal =
                    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(refusal.as_bytes());
                continue;
            };
            let frames = as_sse(&reply);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{frames}",
                frames.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), receiver)
}

/// Runs a turn that starts a delegate, over a workspace the person vouched for, and hands back the
/// requests the delegate sent: those holding `delegate` and not `parent`.
fn a_delegates_requests(
    name: &str,
    task: Task,
    parent: &'static str,
    delegate: &'static str,
    rules: Vec<(&'static str, Vec<String>)>,
) -> Vec<Value> {
    let scratch = Scratch::new(name);
    let (endpoint, chat) = serve_by_marker(rules);
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut trust = bravebot_core::trust::TrustStore::new(workspace.root());
    trust.trust(".");
    turns::run_with_trust(
        &config_for(&endpoint),
        &Egress::new(),
        &workspace,
        &task,
        &mut Counting::default(),
        &mut RecordingSink::new(),
        trust,
    )
    .expect("the turn runs");
    chat.try_iter()
        .filter(|body| {
            !body.contains(A_CHECK_ASKING) && body.contains(delegate) && !body.contains(parent)
        })
        .map(|body| serde_json::from_str(&body).expect("a request"))
        .collect()
}

/// TOOL-7: a delegate's turn is split as the planner's is. A worker's first request offers the core
/// tools its kind holds and `load_tool`, with `lsp` listed and not offered, and a load of `lsp`
/// offers it from the delegate's next request. The regression it rejects is a split applied to the
/// planner's turn alone, which sends a delegate every tool its kind holds in full.
#[test]
fn a_delegate_is_offered_the_core_list_and_loads_the_rest_by_name() {
    let sent = a_delegates_requests(
        "delegate-split",
        Task::new("HAVE-A-WORKER-LOAD"),
        "HAVE-A-WORKER-LOAD",
        "LOAD-LSP-AND-STOP",
        vec![
            (
                "HAVE-A-WORKER-LOAD",
                vec![
                    tool_request(
                        "spawn_agent",
                        &json!({"kind": "worker", "task": "LOAD-LSP-AND-STOP", "why": "to load"}),
                    ),
                    reply_with("waiting"),
                    reply_with("done"),
                ],
            ),
            (
                "LOAD-LSP-AND-STOP",
                vec![
                    tool_request("load_tool", &json!({"name": "lsp", "why": "to ask"})),
                    reply_with("loaded"),
                ],
            ),
        ],
    );
    let [first, second, ..] = sent.as_slice() else {
        panic!("the delegate made {} requests", sent.len());
    };
    let offered = names(first);
    assert!(
        offered
            .iter()
            .all(|name| name == "load_tool" || CORE.contains(&name.as_str())),
        "the delegate was offered a held-back tool in full: {offered:?}"
    );
    assert!(
        offered.contains(&"write_file".to_string()),
        "the delegate was not offered its kind's tools: {offered:?}"
    );
    assert!(
        listed_for_loading(first).contains(&"lsp".to_string()),
        "lsp is not listed for the delegate to load: {offered:?}"
    );
    assert!(
        names(second).contains(&"lsp".to_string()),
        "the delegate's load did not offer lsp: {:?}",
        names(second)
    );
}

/// TOOL-7: a delegate whose definition's `tools:` line names a held-back tool is offered it in full
/// from its first request, and where the line names nothing else held back there is no loader. The
/// regression it rejects is a split that reads the planner's `tools:` line and not a delegate's,
/// which holds back a tool a person named for that delegate.
#[test]
fn a_delegate_definition_naming_a_held_back_tool_is_offered_it_in_full() {
    let home = Scratch::new("delegate-named-home");
    std::fs::create_dir_all(home.path.join("agents")).expect("create the definitions directory");
    std::fs::write(
        home.path.join("agents").join("symbol-asker.md"),
        "---\nname: symbol-asker\ndescription: Asks about symbols.\nkind: worker\ntools: read_file, repo_map\n---\n\nASK\n",
    )
    .expect("write the definition");
    let sent = a_delegates_requests(
        "delegate-named",
        Task::new("HAVE-A-DEFINED-DELEGATE").with_home(Some(home.path.clone())),
        "HAVE-A-DEFINED-DELEGATE",
        "ASK-ABOUT-A-SYMBOL",
        vec![
            (
                "HAVE-A-DEFINED-DELEGATE",
                vec![
                    tool_request(
                        "spawn_agent",
                        &json!({"kind": "symbol-asker", "task": "ASK-ABOUT-A-SYMBOL", "why": "to ask"}),
                    ),
                    reply_with("waiting"),
                    reply_with("done"),
                ],
            ),
            ("ASK-ABOUT-A-SYMBOL", vec![reply_with("nothing to ask")]),
        ],
    );
    let first = sent.first().expect("the delegate asked for nothing");
    let offered = names(first);
    assert!(
        offered.contains(&"repo_map".to_string()),
        "a tool the definition named was held back: {offered:?}"
    );
    assert!(
        !offered.contains(&"load_tool".to_string()),
        "a delegate holding nothing back was offered a loader: {offered:?}"
    );
}

/// RUN-18: a turn's first request sends a request to watch something to the skill `watching`, and
/// the same request lists that skill, so the route is one the planner can take. The regression it
/// rejects is guidance moved to a skill the turn does not list.
#[test]
fn the_first_request_routes_a_watch_request_to_a_skill_it_lists() {
    let sent = run(
        "watching-route",
        Task::new("tell me when notes.txt changes"),
        vec![reply_with("done")],
        &mut Counting::default(),
    );
    let all = functions(&sent[0]);
    let described = function(&all, "run").expect("run is offered")["description"]
        .as_str()
        .expect("a description")
        .to_string();
    assert!(
        described.contains("load the built-in skill watching"),
        "{described}"
    );
    assert!(
        sent[0].to_string().contains("- watching: "),
        "the first request does not list the skill run sends a watch request to"
    );
}

/// TOOL-6, TOOL-7: a result names a held-back tool through `load_tool` only until the planner loads
/// it. In a trusted workspace whose programs are confined, the line after a failed run sends the
/// planner to `load_tool` for `request_path` while it is held back, and names `request_path` as a
/// tool to call once it is loaded. The regression it rejects is the turn's record of what its list
/// holds taken once as the turn starts and never moved by a load, which tells the planner to load a
/// tool it already holds.
#[test]
fn a_loaded_tool_is_named_directly_in_a_later_result() {
    if bravebot_sandbox::base::Prelude::current().is_none()
        || !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    const LOAD_SENTENCE: &str = "call `load_tool` with the name request_path first";
    // Said only where the list holds `request_path` (CREDENTIAL_LOCATIONS_SENTENCE).
    const LISTED_SENTENCE: &str = "Any other path a command needs is asked for with the \
                                   `request_path` tool";
    let failing = || {
        tool_request(
            "run",
            &json!({"command": "sh -c 'exit 1'", "why": "to fail"}),
        )
    };
    let failed_run = |name: &str, replies: Vec<String>| {
        let scratch = Scratch::new(name);
        let (endpoint, chat) = serve_chat(replies);
        let workspace = Workspace::new(&scratch.path).expect("workspace");
        let mut trust = bravebot_core::trust::TrustStore::new(workspace.root());
        trust.trust(".");
        let task = Task::new("run it")
            .with_profile(Some(
                bravebot_agent::home::profile().unwrap_or_else(|| scratch.path.join("profile")),
            ))
            .with_confined_runs(true)
            .with_sandbox_mode(bravebot_sandbox::SandboxMode::WorkspaceWrite);
        turns::run_with_trust(
            &config_for(&endpoint),
            &Egress::new(),
            &workspace,
            &task,
            &mut Counting {
                approving_runs: true,
                ..Counting::default()
            },
            &mut RecordingSink::new(),
            trust,
        )
        .expect("the turn runs");
        let sent: Vec<Value> = chat
            .try_iter()
            .filter(|body| !body.contains(A_CHECK_ASKING))
            .map(|body| serde_json::from_str(&body).expect("a request"))
            .collect();
        answer(&sent[sent.len() - 1])
    };

    let held = failed_run("named-held", vec![failing(), reply_with("done")]);
    assert!(held.contains("Confinement:"), "{held}");
    assert!(
        held.contains(LOAD_SENTENCE) && !held.contains(LISTED_SENTENCE),
        "a held-back request_path was not named through load_tool: {held}"
    );

    let loaded = failed_run(
        "named-loaded",
        vec![
            tool_request(
                "load_tool",
                &json!({"name": "request_path", "why": "to reach a path"}),
            ),
            failing(),
            reply_with("done"),
        ],
    );
    assert!(loaded.contains("Confinement:"), "{loaded}");
    assert!(
        loaded.contains(LISTED_SENTENCE) && !loaded.contains(LOAD_SENTENCE),
        "a loaded request_path was still named through load_tool: {loaded}"
    );
}

/// RUN-18: a skill of the person's own named `watching` takes the built-in's place in the
/// catalogue, so `run`'s description does not send a watch request to the built-in skill and says
/// the techniques itself. The control is the same turn with no home, whose prompt lists the
/// built-in's description and whose `run` names the skill. The regression it rejects is a route
/// decided by whether any skill has that name, which sends the planner to text the person wrote
/// for something else.
#[test]
fn a_skill_of_the_persons_own_named_watching_does_not_take_the_route() {
    const BUILT_IN_DESCRIPTION: &str = "How to answer a request about change";
    const ROUTE: &str = "load the built-in skill watching";
    let run_description = |request: &Value| {
        let all = functions(request);
        function(&all, "run").expect("run is offered")["description"]
            .as_str()
            .expect("a description")
            .to_string()
    };
    let control = run(
        "watching-built-in",
        Task::new("tell me when notes.txt changes"),
        vec![reply_with("done")],
        &mut Counting::default(),
    );
    assert!(
        control[0].to_string().contains(BUILT_IN_DESCRIPTION),
        "the control does not list the built-in skill"
    );
    assert!(run_description(&control[0]).contains(ROUTE));

    let home = Scratch::new("watching-shadowed-home");
    let at = home.path.join("skills").join("watching");
    std::fs::create_dir_all(&at).expect("create the skill directory");
    std::fs::write(
        at.join("SKILL.md"),
        "---\nname: watching\ndescription: Birds seen from the window.\n---\n\nA list of birds.\n",
    )
    .expect("write the skill");
    let sent = run(
        "watching-shadowed",
        Task::new("tell me when notes.txt changes").with_home(Some(home.path.clone())),
        vec![reply_with("done")],
        &mut Counting::default(),
    );
    assert!(
        !sent[0].to_string().contains(BUILT_IN_DESCRIPTION),
        "the person's skill did not take the built-in's place, so this says nothing"
    );
    let described = run_description(&sent[0]);
    assert!(
        !described.contains(ROUTE),
        "run sends a watch request to a skill the person wrote: {described}"
    );
    assert!(
        described.contains("decide first what is being watched"),
        "{described}"
    );
}

/// LSP-9, TOOL-7: a delegate of a kind that holds the language server is offered `lsp` by name
/// under `load_tool` and loads it; a `reader` is neither offered nor listed it, and its load is
/// refused. Run with the turn's own split, so the regression it rejects is a list for loading
/// built from the table rather than from what the delegate's kind holds, which would let a reader
/// load a server.
#[test]
fn a_checker_may_load_lsp_and_a_reader_may_not() {
    let delegate_of = |kind: &'static str, marker: &'static str, parent: &'static str| {
        a_delegates_requests(
            &format!("lsp-{kind}"),
            Task::new(parent),
            parent,
            marker,
            vec![
                (
                    parent,
                    vec![
                        tool_request(
                            "spawn_agent",
                            &json!({"kind": kind, "task": marker, "why": "to ask"}),
                        ),
                        reply_with("waiting"),
                        reply_with("done"),
                    ],
                ),
                (
                    marker,
                    vec![
                        tool_request("load_tool", &json!({"name": "lsp", "why": "to ask"})),
                        reply_with("stopped"),
                    ],
                ),
            ],
        )
    };
    let checker = delegate_of("checker", "CHECKER-LOADS-LSP", "HAVE-A-CHECKER-LOAD");
    let [first, second, ..] = checker.as_slice() else {
        panic!("the checker made {} requests", checker.len());
    };
    assert!(
        !names(first).contains(&"lsp".to_string()),
        "lsp was offered in full: {:?}",
        names(first)
    );
    assert!(
        listed_for_loading(first).contains(&"lsp".to_string()),
        "a checker was not offered lsp by name: {:?}",
        listed_for_loading(first)
    );
    assert!(
        names(second).contains(&"lsp".to_string()),
        "the checker's load did not offer lsp: {:?}",
        names(second)
    );

    let reader = delegate_of("reader", "READER-LOADS-LSP", "HAVE-A-READER-LOAD");
    let [first, second, ..] = reader.as_slice() else {
        panic!("the reader made {} requests", reader.len());
    };
    assert!(
        !listed_for_loading(first).contains(&"lsp".to_string()),
        "a reader was offered lsp by name: {:?}",
        listed_for_loading(first)
    );
    assert!(
        !names(second).contains(&"lsp".to_string()),
        "a reader loaded lsp: {:?}",
        names(second)
    );
}

/// The most bytes the tool list of a one-shot turn's first request may take, written compactly as
/// the request sends it. A little above what this test measures, so a description that grows back
/// fails here.
const FIRST_REQUEST_TOOL_BYTES: usize = 46_000;

/// TOOL-7: a tick of a self-paced loop is asked by its preamble and the `loop` skill to call
/// `schedule_next` on every tick, so its first request offers it in full and lists it nowhere for
/// loading. A turn that arranges a later look and is not a tick still holds it back, and a one-shot
/// turn neither offers nor lists it. The regressions it rejects are a tick sent to a tool it holds
/// back, which costs a refused call on every tick, and the tool offered in full in every turn.
#[test]
fn a_self_paced_tick_is_offered_schedule_next_in_full_and_other_turns_are_not() {
    let tick = run(
        "pacing-tick",
        Task::new("watch the build").ticking(Some(bravebot_agent::turn::Tick {
            number: 1,
            self_paced: true,
            unpaceable: false,
        })),
        vec![reply_with("nothing yet")],
        &mut Counting::default(),
    );
    let all = functions(&tick[0]);
    let schedule = function(&all, "schedule_next").unwrap_or_else(|| {
        panic!(
            "a self-paced tick was not offered schedule_next in full: {:?}",
            names(&tick[0])
        )
    });
    assert!(
        schedule["parameters"]["properties"]["delay_seconds"].is_object(),
        "{schedule}"
    );
    assert!(
        !listed_for_loading(&tick[0]).contains(&"schedule_next".to_string()),
        "a tick offered schedule_next in full also lists it for loading"
    );

    let arranging = run(
        "arranging-turn",
        Task::new("watch the build").looking_again(true),
        vec![reply_with("done")],
        &mut Counting::default(),
    );
    assert!(
        !names(&arranging[0]).contains(&"schedule_next".to_string()),
        "a turn that is not a tick was offered schedule_next in full: {:?}",
        names(&arranging[0])
    );
    assert!(
        listed_for_loading(&arranging[0]).contains(&"schedule_next".to_string()),
        "a turn that may arrange a look does not list schedule_next for loading"
    );

    let one_shot = run(
        "one-shot-turn",
        Task::new("watch the build"),
        vec![reply_with("done")],
        &mut Counting::default(),
    );
    assert!(
        !names(&one_shot[0]).contains(&"schedule_next".to_string())
            && !listed_for_loading(&one_shot[0]).contains(&"schedule_next".to_string()),
        "a one-shot turn was offered or listed schedule_next: {:?}",
        names(&one_shot[0])
    );
}

/// TOOL-7: the tool definitions of a one-shot turn's first request stay under a budget. The turn has
/// no home, no skills, no server and no setting of its own, which is what `--safe` leaves a one-shot
/// run. The regressions it rejects are a description that grows its paragraphs back and a held-back
/// tool offered in full again.
#[test]
fn a_one_shot_turns_first_request_offers_tools_within_a_budget() {
    let sent = run(
        "tool-budget",
        Task::new("Reply with exactly the word: ok"),
        vec![reply_with("ok")],
        &mut Counting::default(),
    );
    let tools = serde_json::to_string(&sent[0]["tools"]).expect("the tools serialise");
    assert!(
        tools.len() <= FIRST_REQUEST_TOOL_BYTES,
        "the first request's tools take {} bytes, over the budget of {FIRST_REQUEST_TOOL_BYTES}",
        tools.len()
    );
}
