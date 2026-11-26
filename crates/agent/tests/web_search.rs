//! `web_search` driven through whole turns against a stand-in for the Brave Search API
//! (`docs/specs/tools/web-search.md`).
//!
//! The key in these tests is a placeholder written to a scratch file. Nothing here reads a real
//! key.

#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/mock_model.rs"]
mod mock_model;

use bravebot_agent::confirm::*;
use bravebot_agent::turn::{self, Task};
use bravebot_agent::{Conversation, Workspace};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::programs::TrustedPrograms;
use mock_model::{Scratch, config_for, reply_with, serve_by_marker, serve_sequence, tool_request};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const KEY: &str = "placeholder-search-key-7c1e";
const SENTINEL: &str = "SENTINEL-FROM-THE-SEARCH-INDEX";

/// Records every search it is asked about and answers with `decision`; everything else is refused.
struct Asked {
    searches: Vec<FetchRequest>,
    decision: Decision,
}

impl Asked {
    fn answering(decision: Decision) -> Self {
        Self {
            searches: Vec::new(),
            decision,
        }
    }
}

impl Confirmer for Asked {
    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        self.searches.push(request.clone());
        self.decision
    }
    fn confirm_write(&mut self, _: &WriteRequest) -> WriteDecision {
        WriteDecision::reject()
    }
    fn confirm_run(&mut self, _: &RunRequest) -> RunDecision {
        RunDecision::reject()
    }
    fn confirm_read_output(&mut self, _: &OutputRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_vetted_read(&mut self, _: &VetRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_server(&mut self, _: &ServerRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_manifest(&mut self, _: &ManifestRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_vouch(&mut self, _: &VouchRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_exposing_read(&mut self, _: &ExposureRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_tool_list(&mut self, _: &ToolListRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_mcp_call(&mut self, _: &McpCallRequest) -> CallDecision {
        CallDecision::reject()
    }
    fn confirm_path(&mut self, _: &PathRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_host(&mut self, _: &HostRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_message(&mut self, _: &MessageRequest) -> MessageDecision {
        MessageDecision::Decline
    }
    fn confirm_move(&mut self, _: &MoveRequest) -> Decision {
        Decision::Reject
    }
    fn ask_user(&mut self, _: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        Vec::new()
    }
    fn interjection(&mut self) -> Option<String> {
        None
    }
}

/// A stand-in search service: answers each request with the next reply and reports the head of
/// every request it was sent. Keeps listening past its script, so a retry meets an answer.
fn serve_the_index(replies: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut replies = replies.into_iter();
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut head = String::new();
            let _ = reader.read_line(&mut head);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                head.push_str(&header);
            }
            let _ = sender.send(head);
            let reply = replies
                .next()
                .unwrap_or_else(|| answer(500, "text/plain", "out of script"));
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), receiver)
}

fn answer(status: u16, kind: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn json_results(body: &str) -> String {
    answer(200, "application/json", body)
}

/// A key file holding the placeholder, in a directory of its own.
fn a_key_file(name: &str) -> (Scratch, std::path::PathBuf) {
    let scratch = Scratch::new(name);
    let path = scratch.path.join("search-key.txt");
    std::fs::write(&path, format!("{KEY}\n")).unwrap();
    (scratch, path)
}

fn searching(endpoint: &str, key_file: &std::path::Path, index: &str) -> Config {
    config_for(endpoint)
        .with_web_search_key_file(key_file)
        .with_web_search_base(index)
}

fn rules(list: &[&str], which: &str) -> bravebot_core::permissions::Permissions {
    let owned: Vec<String> = list.iter().map(|s| s.to_string()).collect();
    let none: Vec<String> = Vec::new();
    let (deny, ask, allow) = match which {
        "deny" => (&owned, &none, &none),
        "ask" => (&none, &owned, &none),
        _ => (&none, &none, &owned),
    };
    let (permissions, rejected) = bravebot_core::permissions::Permissions::parse(
        deny,
        ask,
        allow,
        &bravebot_core::permissions::Anchors::none(),
    );
    assert!(rejected.is_empty(), "a rule in this test did not parse");
    permissions
}

/// One turn; returns every request the planner received.
fn a_turn(
    config: &Config,
    task: Task,
    confirmer: &mut Asked,
    received: &mpsc::Receiver<String>,
) -> Vec<String> {
    a_turn_with(config, task, confirmer, received)
}

/// [`a_turn`] for any confirmer, so a mode can sit in front of the one that records.
fn a_turn_with<C: Confirmer + Send>(
    config: &Config,
    task: Task,
    confirmer: &mut C,
    received: &mpsc::Receiver<String>,
) -> Vec<String> {
    a_turn_reporting(
        config,
        task,
        confirmer,
        &mut bravebot_agent::report::RecordingReporter::default(),
        received,
    )
}

/// [`a_turn_with`] for any reporter, so a test can act at the moment a call is announced.
fn a_turn_reporting<C: Confirmer + Send, R: bravebot_agent::report::Reporter + Send>(
    config: &Config,
    task: Task,
    confirmer: &mut C,
    reporter: &mut R,
    received: &mpsc::Receiver<String>,
) -> Vec<String> {
    // One directory for each turn: tests run in parallel, and a shared name has one test remove the
    // directory another is using.
    static TURNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let scratch = Scratch::new(&format!(
        "web-search-workspace-{}",
        TURNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut trust = bravebot_core::trust::TrustStore::new("/work");
    trust.trust(".");
    turn::resume(
        config,
        &bravebot_net::Egress::new(),
        &workspace,
        &task,
        &mut Conversation::new(),
        confirmer,
        reporter,
        &mut RecordingSink::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    received.try_iter().collect()
}

/// What the planner was told by the tool result of its last request.
fn told(requests: &[String]) -> String {
    requests
        .iter()
        .filter_map(|body| {
            let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
            parsed["messages"]
                .as_array()?
                .iter()
                .rev()
                .find(|message| message["role"] == "tool")
                .and_then(|message| message["content"].as_str().map(str::to_string))
        })
        .next_back()
        .unwrap_or_default()
}

const CALL: &str = r#"{"service":"web","q":"rust async","count":3}"#;

/// R1: with no key file configured the tool is not on the list and the name is unknown; with one it
/// is on the list. The regression it rejects: the tool offered whatever the configuration, which
/// sends a planner after a tool that can only fail.
#[test]
fn the_tool_is_offered_only_where_a_key_file_is_configured() {
    let (_keep, key) = a_key_file("search-offered");
    let (index, _heads) = serve_the_index(vec![]);

    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let none = config_for(&endpoint);
    let bodies = a_turn(
        &none,
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );
    assert!(
        !bodies[0].contains("\"web_search\""),
        "the tool was offered with no key file"
    );
    assert!(told(&bodies).contains("no such tool"), "{}", told(&bodies));

    let (endpoint, received) = serve_sequence(vec![reply_with("done")]);
    let with = searching(&endpoint, &key, &index);
    let bodies = a_turn(
        &with,
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );
    assert!(
        bodies[0].contains("\"web_search\""),
        "the tool was not offered"
    );

    // Named in settings and absent on disk is no key.
    let (endpoint, received) = serve_sequence(vec![reply_with("done")]);
    let missing = config_for(&endpoint)
        .with_web_search_key_file(key.with_file_name("not-there.txt"))
        .with_web_search_base(&index);
    let bodies = a_turn(
        &missing,
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );
    assert!(
        !bodies[0].contains("\"web_search\""),
        "a missing file offered the tool"
    );
}

/// R3, R5, R7: the planner gets a reference and not the body, the key is on the wire in one header
/// and nowhere in what the planner is sent, and the person is shown the query and the host. The
/// regressions it rejects: results read into the result, the key put in a URL or in text, and a
/// prompt that does not carry what is sent.
#[test]
fn a_search_returns_a_reference_sends_the_key_in_one_header_and_asks_about_the_query() {
    let (_keep, key) = a_key_file("search-reference");
    let (index, heads) = serve_the_index(vec![json_results(&format!(
        r#"{{"results":["{SENTINEL}"]}}"#
    ))]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let config = searching(&endpoint, &key, &index);
    let mut asked = Asked::answering(Decision::Approve);

    let bodies = a_turn(&config, Task::new("search"), &mut asked, &received);

    let head = heads
        .recv_timeout(Duration::from_secs(5))
        .expect("the request went out");
    assert!(
        head.starts_with("GET /v1/web/search?q=rust%20async&count=3 "),
        "{head}"
    );
    assert_eq!(
        head.to_lowercase().matches(&KEY.to_lowercase()).count(),
        1,
        "the key was not sent exactly once: {head}"
    );
    assert!(
        head.to_lowercase()
            .contains(&format!("x-subscription-token: {}", KEY.to_lowercase())),
        "the key is not in its header: {head}"
    );
    assert!(
        !head.lines().next().unwrap().contains(KEY),
        "the key is in the URL"
    );
    let said = told(&bodies);
    assert!(
        said.contains("ref:1"),
        "no reference was handed back: {said}"
    );
    for body in &bodies {
        assert!(
            !body.contains(SENTINEL),
            "the index's words reached the planner"
        );
        assert!(!body.contains(KEY), "the key reached the planner");
    }
    assert_eq!(
        asked.searches.len(),
        1,
        "the search was not put to the person"
    );
    assert!(asked.searches[0].url.contains("q=rust%20async"));
    assert_eq!(asked.searches[0].host, "127.0.0.1");
}

/// R3, WEBSEARCH-9 (FETCH-9): a search that asks to read, with permissions bypassed and no screening
/// asked for, is handed the results in the same result by `vet_content`'s own release, and the same
/// search without `read` stays behind its reference. The regressions it rejects: the flag ignored
/// (the results stay behind the reference) and the flag read as releasing whether or not it was
/// asked for (the control leaks).
#[test]
fn a_search_that_asks_to_read_is_released_as_a_fetch_is_and_one_that_does_not_is_not() {
    let (_keep, key) = a_key_file("search-read");
    for (arguments, released) in [
        (r#"{"service":"web","q":"rust async","read":true}"#, true),
        (r#"{"service":"web","q":"rust async"}"#, false),
    ] {
        let (index, _heads) = serve_the_index(vec![json_results(&format!(
            r#"{{"results":["{SENTINEL}"]}}"#
        ))]);
        let (endpoint, received) = serve_sequence(vec![
            tool_request("web_search", arguments),
            reply_with("done"),
        ]);
        let mode = bravebot_agent::PermissionMode::Bypass;
        let mut inner = Asked::answering(Decision::Approve);
        let mut confirmer = bravebot_agent::Confining::new(&mut inner, mode, false);
        let bodies = a_turn_with(
            &searching(&endpoint, &key, &index),
            Task::new("search")
                .with_permission_mode(mode)
                .with_auto_vetting(false),
            &mut confirmer,
            &received,
        );
        let last = bodies.last().expect("a request after the search");
        assert_eq!(
            last.contains(SENTINEL),
            released,
            "{arguments}: the results were {} the planner",
            if released { "kept from" } else { "given to" }
        );
        assert!(!last.contains(KEY), "the key reached the planner");
    }
}

/// VET-6: a search that asks to read results larger than the task's output cap keeps them behind
/// the reference, and the result says what the cap is, how large the results are and what
/// `vet_content` would cost, without the curl route that is advice about a page.
///
/// The regressions it rejects: results past the cap put into the result, a bare "too long" with no
/// figure, and the fetch's `run` and curl route given to a search.
#[test]
fn a_search_that_asks_to_read_past_the_output_cap_is_told_the_cap_the_size_and_the_cost() {
    let (_keep, key) = a_key_file("search-read-cap");
    let (index, _heads) = serve_the_index(vec![json_results(&format!(
        r#"{{"results":["{SENTINEL} {}"]}}"#,
        "x".repeat(300)
    ))]);
    let (endpoint, received) = serve_sequence(vec![
        tool_request(
            "web_search",
            r#"{"service":"web","q":"rust async","read":true}"#,
        ),
        reply_with("done"),
    ]);
    let mode = bravebot_agent::PermissionMode::Bypass;
    let mut inner = Asked::answering(Decision::Approve);
    let mut confirmer = bravebot_agent::Confining::new(&mut inner, mode, false);
    let bodies = a_turn_with(
        &searching(&endpoint, &key, &index),
        Task::new("search")
            .with_permission_mode(mode)
            .with_auto_vetting(false)
            .with_output_cap(Some(64)),
        &mut confirmer,
        &received,
    );
    let said = told(&bodies);
    assert!(
        !said.contains(SENTINEL),
        "results past the cap were released: {said}"
    );
    assert!(
        said.contains("It is longer than a result may hold (64 bytes)")
            && said.contains("This one is ")
            && said.contains("vet_content costs about")
            && said.contains("tokens, which stay in every later request"),
        "the cap, the size and the cost were not stated: {said}"
    );
    assert!(
        !said.contains("To read part of the page instead"),
        "a search was given the route for a page: {said}"
    );
}

/// WEBSEARCH-9 (FETCH-9): a `read` that is not a boolean is refused, the refusal says what it must
/// be, nobody is asked and no request reaches the index. The regression it rejects: a string read as
/// false, which searches under a call the planner did not validly make.
#[test]
fn a_search_whose_read_is_not_a_boolean_is_refused_before_a_question_or_a_request() {
    let (_keep, key) = a_key_file("search-read-not-boolean");
    for read in [r#""yes""#, "1", "[true]"] {
        let (index, heads) = serve_the_index(vec![json_results(&format!(
            r#"{{"results":["{SENTINEL}"]}}"#
        ))]);
        let arguments = format!(r#"{{"service":"web","q":"rust async","read":{read}}}"#);
        let (endpoint, received) = serve_sequence(vec![
            tool_request("web_search", &arguments),
            reply_with("done"),
        ]);
        let mut asked = Asked::answering(Decision::Approve);
        let bodies = a_turn(
            &searching(&endpoint, &key, &index),
            Task::new("search"),
            &mut asked,
            &received,
        );
        assert!(
            told(&bodies).contains("'read' must be true or false"),
            "read {read}: {}",
            told(&bodies)
        );
        assert!(
            asked.searches.is_empty(),
            "read {read}: a refused call was put to the person"
        );
        assert!(
            heads.recv_timeout(Duration::from_millis(300)).is_err(),
            "read {read}: a search was sent under a call that was refused"
        );
    }
}

/// WEBSEARCH-2, MODE-4: bypassing answers the question about a search without putting it to the
/// confirmer behind the mode, and auto does not consult its classifier for one, so the question
/// goes to that confirmer with no classifier word. The regressions it rejects: bypassing that
/// still asks, and a search put to the classifier, whose word could then answer for a person.
#[test]
fn bypass_answers_a_search_and_auto_leaves_it_to_the_person_without_a_classifier_word() {
    let (_keep, key) = a_key_file("search-modes");
    for (mode, sent, asked_of_the_person) in [
        (bravebot_agent::PermissionMode::Bypass, true, 0),
        (bravebot_agent::PermissionMode::Auto, false, 1),
    ] {
        let (index, heads) = serve_the_index(vec![json_results("{}")]);
        let (endpoint, received) =
            serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
        let mut inner = Asked::answering(Decision::Reject);
        {
            let mut confirmer = bravebot_agent::Confining::new(&mut inner, mode, false);
            a_turn_with(
                &searching(&endpoint, &key, &index),
                Task::new("search")
                    .with_permission_mode(mode)
                    .with_auto_vetting(false),
                &mut confirmer,
                &received,
            );
        }
        assert_eq!(
            heads.recv_timeout(Duration::from_millis(500)).is_ok(),
            sent,
            "{mode:?}"
        );
        assert_eq!(inner.searches.len(), asked_of_the_person, "{mode:?}");
        for request in &inner.searches {
            assert_eq!(
                request.classified,
                bravebot_agent::confirm::Classified::NotAsked,
                "{mode:?}: the classifier was consulted about a search"
            );
        }
    }
}

/// WEBSEARCH-8: a delegate is not offered `web_search`, and one it names anyway is answered as an
/// unknown name, with no question to the person and no request sent, although every kind holds the
/// capability. The regression it rejects: the capability held by a kind read as a licence to search,
/// which would put a delegate's query to a person who never saw the sub-task.
#[test]
fn a_delegate_naming_web_search_is_answered_as_an_unknown_name_and_sends_nothing() {
    let (_keep, key) = a_key_file("search-delegate");
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) = serve_by_marker(vec![
        (
            "DELEGATE-THE-SEARCH",
            vec![
                tool_request("spawn_agent", r#"{"kind":"reader","task":"DO-THE-SEARCH"}"#),
                reply_with("waiting"),
                reply_with("done"),
            ],
        ),
        (
            "DO-THE-SEARCH",
            vec![
                tool_request("web_search", CALL),
                reply_with("I could not search"),
            ],
        ),
    ]);
    let mut asked = Asked::answering(Decision::Approve);
    let bodies = a_turn(
        &searching(&endpoint, &key, &index),
        Task::new("DELEGATE-THE-SEARCH"),
        &mut asked,
        &received,
    );
    assert!(
        asked.searches.is_empty(),
        "a delegate's search was put to the person"
    );
    assert!(
        heads.recv_timeout(Duration::from_millis(300)).is_err(),
        "a delegate's search was sent"
    );
    let delegates: Vec<&String> = bodies
        .iter()
        .filter(|body| !body.contains("DELEGATE-THE-SEARCH"))
        .collect();
    let offered = delegates.first().expect("the delegate asked for nothing");
    assert!(
        !offered.contains("\"web_search\""),
        "a delegate was offered web_search"
    );
    assert!(
        delegates.iter().any(|body| body.contains("no such tool")),
        "a delegate's call to web_search was not refused as an unknown name"
    );
}

/// R5: a person who says no sends nothing, and the planner is told in the driver's words. The
/// regression it rejects: the request sent before the answer was read.
#[test]
fn a_search_the_person_declines_is_not_sent() {
    let (_keep, key) = a_key_file("search-declined");
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let mut asked = Asked::answering(Decision::Reject);

    let bodies = a_turn(
        &searching(&endpoint, &key, &index),
        Task::new("search"),
        &mut asked,
        &received,
    );

    assert!(
        heads.recv_timeout(Duration::from_millis(300)).is_err(),
        "a declined search was sent"
    );
    assert!(
        told(&bodies).contains("did not approve this search"),
        "{}",
        told(&bodies)
    );
}

/// R5: an `allow` rule for the family answers the prompt, and a `deny` rule refuses without one.
/// The regressions it rejects: a rule for `WebFetch` answering for searches, and a deny rule that
/// still asks.
#[test]
fn a_websearch_rule_answers_the_prompt_and_a_webfetch_rule_does_not() {
    let (_keep, key) = a_key_file("search-rules");
    for (list, which, sent, asks) in [
        (["WebSearch"], "allow", true, 0),
        (["WebFetch"], "allow", false, 1),
        (["WebSearch"], "deny", false, 0),
    ] {
        let (index, heads) = serve_the_index(vec![json_results("{}")]);
        let (endpoint, received) =
            serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
        let mut asked = Asked::answering(Decision::Reject);
        let task = Task::new("search").with_permissions(rules(&list, which));
        a_turn(
            &searching(&endpoint, &key, &index),
            task,
            &mut asked,
            &received,
        );
        assert_eq!(
            heads.recv_timeout(Duration::from_millis(500)).is_ok(),
            sent,
            "{list:?} {which}"
        );
        assert_eq!(asked.searches.len(), asks, "{list:?} {which}");
    }
}

/// R2: an argument off the documented range is refused naming the argument, before anybody is
/// asked and before anything is sent. The regression it rejects: validation after the prompt, or
/// not at all.
#[test]
fn an_argument_off_the_documented_range_is_refused_before_a_question_or_a_request() {
    let (_keep, key) = a_key_file("search-invalid");
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) = serve_sequence(vec![
        tool_request("web_search", r#"{"service":"web","q":"x","count":99}"#),
        reply_with("done"),
    ]);
    let mut asked = Asked::answering(Decision::Approve);
    let bodies = a_turn(
        &searching(&endpoint, &key, &index),
        Task::new("search"),
        &mut asked,
        &received,
    );
    assert!(told(&bodies).contains("'count'"), "{}", told(&bodies));
    assert!(
        asked.searches.is_empty(),
        "a refused call was put to the person"
    );
    assert!(heads.recv_timeout(Duration::from_millis(300)).is_err());
}

/// R8: each failure is reported in the driver's words and nothing of the vendor's error body
/// reaches the planner. The status number is told as a number, in one sentence that is the same for
/// every status. The regressions it rejects: the body, or its `detail`, formatted into the refusal,
/// and a sentence chosen from the status.
#[test]
fn a_failure_is_worded_by_the_driver_and_carries_none_of_the_vendors_body() {
    let (_keep, key) = a_key_file("search-failures");
    let mut sentences = Vec::new();
    for status in [401u16, 403, 422, 429] {
        let vendor = format!(
            r#"{{"error":{{"id":"x","status":{status},"detail":"{SENTINEL}","code":"C"}}}}"#
        );
        let (index, _heads) = serve_the_index(vec![answer(status, "application/json", &vendor)]);
        let (endpoint, received) =
            serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
        let bodies = a_turn(
            &searching(&endpoint, &key, &index),
            Task::new("search"),
            &mut Asked::answering(Decision::Approve),
            &received,
        );
        let said = told(&bodies);
        assert!(
            said.contains(&format!("answered with HTTP {status}.")),
            "{status}: {said}"
        );
        for body in &bodies {
            assert!(
                !body.contains(SENTINEL),
                "{status}: the vendor's words reached the planner"
            );
        }
        let (before, after) = said.split_once(" HTTP ").expect("the number is told");
        let (number, rest) = after.split_once('.').expect("the number ends a clause");
        assert_eq!(number, status.to_string());
        sentences.push(format!("{before}{rest}"));
    }
    assert!(
        sentences.windows(2).all(|pair| pair[0] == pair[1]),
        "the sentence differs by status: {sentences:?}"
    );
}

/// R8: a service that cannot be reached is reported as that.
#[test]
fn a_service_that_cannot_be_reached_is_reported_without_a_transport_message() {
    let (_keep, key) = a_key_file("search-unreachable");
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port())
    };
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let bodies = a_turn(
        &searching(&endpoint, &key, &closed),
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );
    assert!(
        told(&bodies).contains("could not be reached"),
        "{}",
        told(&bodies)
    );
}

/// R7: a redirect off the search host is refused with no question, the other host is sent nothing,
/// and the key goes there least of all. The regression it rejects: a hop followed, or followed with
/// the key header.
#[test]
fn a_redirect_off_the_search_host_is_refused_and_sent_neither_a_request_nor_the_key() {
    let (_keep, key) = a_key_file("search-redirect");
    let elsewhere = TcpListener::bind("127.0.0.1:0").unwrap();
    let elsewhere_port = elsewhere.local_addr().unwrap().port();
    let (hit, hits) = mpsc::channel();
    thread::spawn(move || {
        while let Ok((mut stream, _)) = elsewhere.accept() {
            let mut text = String::new();
            let _ = BufReader::new(stream.try_clone().unwrap()).read_line(&mut text);
            let _ = hit.send(text);
            let _ = stream.write_all(answer(200, "text/plain", "elsewhere").as_bytes());
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
        }
    });
    let location = format!("http://localhost:{elsewhere_port}/landed");
    let (index, _heads) = serve_the_index(vec![format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let mut asked = Asked::answering(Decision::Approve);

    let bodies = a_turn(
        &searching(&endpoint, &key, &index),
        Task::new("search"),
        &mut asked,
        &received,
    );

    assert!(
        hits.recv_timeout(Duration::from_millis(400)).is_err(),
        "the redirect was followed"
    );
    assert_eq!(asked.searches.len(), 1, "a hop was put to the person");
    let said = told(&bodies);
    assert!(said.contains("error: the search failed"), "{said}");
    assert!(
        !said.contains("localhost"),
        "the host a server chose is in the result: {said}"
    );
}

/// R7: a body past the cap is reported as incomplete in the driver's words.
#[test]
fn a_body_past_the_cap_is_reported_as_incomplete() {
    let (_keep, key) = a_key_file("search-capped");
    let big = "x".repeat(bravebot_net::MAX_RESPONSE_BYTES + 1024);
    let (index, _heads) = serve_the_index(vec![json_results(&big)]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let bodies = a_turn(
        &searching(&endpoint, &key, &index),
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );
    assert!(
        told(&bodies).contains("stopped at a cap"),
        "{}",
        told(&bodies)
    );
}

/// R6: a key file that cannot be used as a key is said to be so before anybody is asked, without
/// the path or what the file held.
#[test]
fn an_unusable_key_file_is_reported_before_a_question_without_its_path_or_content() {
    let scratch = Scratch::new("search-bad-key");
    let path = scratch.path.join("bad-key.txt");
    std::fs::write(&path, "two words").unwrap();
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let mut asked = Asked::answering(Decision::Approve);
    let bodies = a_turn(
        &searching(&endpoint, &path, &index),
        Task::new("search"),
        &mut asked,
        &received,
    );
    let said = told(&bodies);
    assert!(said.contains("webSearch.keyFile"), "{said}");
    assert!(
        !said.contains("bad-key.txt") && !said.contains("two words"),
        "{said}"
    );
    assert!(asked.searches.is_empty());
    assert!(heads.recv_timeout(Duration::from_millis(300)).is_err());
}

/// Serialises the tests below, which set variables of this process, and restores what was there.
static VARIABLES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Holds a variable of the process for a test and puts it back as it was. Every test that uses one
/// names a variable of its own, so two of them running side by side read no one else's value.
struct Exported {
    name: &'static str,
    before: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

fn export(name: &'static str, value: &str) -> Exported {
    let lock = VARIABLES.lock().unwrap_or_else(|e| e.into_inner());
    let before = std::env::var_os(name);
    // SAFETY: held under the lock above, with a name no other test uses, and restored on drop.
    unsafe { std::env::set_var(name, value) };
    Exported {
        name,
        before,
        _lock: lock,
    }
}

impl Drop for Exported {
    fn drop(&mut self) {
        // SAFETY: still under the lock this guard holds.
        match &self.before {
            Some(value) => unsafe { std::env::set_var(self.name, value) },
            None => unsafe { std::env::remove_var(self.name) },
        }
    }
}

/// A configuration whose key comes from the variables `names`, and from `key_file` where one is
/// given, with the stand-in index as the host.
fn searching_with(
    endpoint: &str,
    names: &[&str],
    key_file: Option<&std::path::Path>,
    index: &str,
) -> Config {
    let config = config_for(endpoint)
        .with_web_search_env(names)
        .with_web_search_base(index);
    match key_file {
        Some(path) => config.with_web_search_key_file(path),
        None => config,
    }
}

/// The head of the one request the stand-in index was sent, in lower case.
fn the_request_sent(heads: &mpsc::Receiver<String>) -> String {
    heads
        .recv_timeout(Duration::from_secs(5))
        .expect("the request went out")
        .to_lowercase()
}

/// The planner bodies hold neither the key nor a variable's name.
fn assert_nothing_of_the_variable_reached_the_planner(
    bodies: &[String],
    name: &str,
    values: &[&str],
) {
    for body in bodies {
        assert!(
            !body.contains(name),
            "a variable's name reached the planner"
        );
        for value in values {
            assert!(!body.contains(value), "a key reached the planner");
        }
    }
}

/// S2, S3: a variable that holds text offers the tool with no key file at all, and the key reaches
/// the request in the one header and nowhere else: not the URL, not the planner. The faults this
/// rejects are offering the tool by the file alone, which leaves the setting doing nothing by
/// itself, and putting the key anywhere but the header.
#[test]
fn a_variable_alone_offers_the_tool_and_the_key_reaches_only_the_header() {
    const NAME: &str = "BRAVEBOT_TEST_SEARCH_KEY_ALONE";
    let _held = export(NAME, KEY);
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let mut asked = Asked::answering(Decision::Approve);

    let bodies = a_turn(
        &searching_with(&endpoint, &[NAME], None, &index),
        Task::new("search"),
        &mut asked,
        &received,
    );

    assert!(
        bodies[0].contains("\"web_search\""),
        "a variable that holds text did not offer the tool"
    );
    let head = the_request_sent(&heads);
    assert_eq!(head.matches(&KEY.to_lowercase()).count(), 1, "{head}");
    assert!(
        head.contains(&format!("x-subscription-token: {}", KEY.to_lowercase())),
        "{head}"
    );
    assert!(!head.lines().next().unwrap().contains(&KEY.to_lowercase()));
    assert_nothing_of_the_variable_reached_the_planner(&bodies, NAME, &[KEY]);
}

/// S3: variables that hold nothing, and a variable that is not set, offer no tool, and the name is
/// answered as an unknown tool. The fault this rejects is offering the tool because a variable is
/// named, which sends a planner after a tool that can only fail.
#[test]
fn variables_that_hold_nothing_do_not_offer_the_tool() {
    const UNSET: &str = "BRAVEBOT_TEST_SEARCH_KEY_UNSET";
    const BLANK: &str = "BRAVEBOT_TEST_SEARCH_KEY_BLANK";
    let _held = export(BLANK, "   ");
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);

    let bodies = a_turn(
        &searching_with(&endpoint, &[UNSET, BLANK], None, &index),
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );

    assert!(
        !bodies[0].contains("\"web_search\""),
        "variables that hold nothing offered the tool"
    );
    assert!(told(&bodies).contains("no such tool"), "{}", told(&bodies));
    assert!(heads.recv_timeout(Duration::from_millis(300)).is_err());
}

/// S2: a variable that holds text beats a key file, and the file's key is sent nowhere. The fault
/// this rejects is reading the file first, which leaves a key exported for one session outranked
/// by a file left in the settings.
#[test]
fn a_variable_beats_the_key_file() {
    const NAME: &str = "BRAVEBOT_TEST_SEARCH_KEY_BEATS_FILE";
    const FROM_THE_FILE: &str = "placeholder-search-key-from-the-file";
    let _held = export(NAME, KEY);
    let scratch = Scratch::new("search-variable-beats-file");
    let file = scratch.path.join("key.txt");
    std::fs::write(&file, FROM_THE_FILE).unwrap();
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);

    let bodies = a_turn(
        &searching_with(&endpoint, &[NAME], Some(&file), &index),
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );

    let head = the_request_sent(&heads);
    assert!(
        head.contains(&format!("x-subscription-token: {}", KEY.to_lowercase())),
        "the variable's key was not sent: {head}"
    );
    assert!(
        !head.contains(FROM_THE_FILE),
        "the file's key was sent although a variable held one: {head}"
    );
    assert_nothing_of_the_variable_reached_the_planner(&bodies, NAME, &[KEY, FROM_THE_FILE]);
}

/// S2: with the variable unset the key file answers, so the setting adds a source and takes none
/// away. The fault this rejects is a search that needs a variable once one is named.
#[test]
fn the_key_file_answers_where_no_named_variable_holds_text() {
    const NAME: &str = "BRAVEBOT_TEST_SEARCH_KEY_FALLBACK";
    let (_keep, file) = a_key_file("search-file-fallback");
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let lock = VARIABLES.lock().unwrap_or_else(|e| e.into_inner());

    a_turn(
        &searching_with(&endpoint, &[NAME], Some(&file), &index),
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );
    drop(lock);

    let head = the_request_sent(&heads);
    assert!(
        head.contains(&format!("x-subscription-token: {}", KEY.to_lowercase())),
        "{head}"
    );
}

/// S2: the first variable of the list that holds text is the key, in the order written, and an
/// unset one and an empty one before it are skipped. The faults this rejects are taking the last
/// variable and stopping at an empty one.
#[test]
fn the_first_named_variable_that_holds_text_is_the_key() {
    const EMPTY: &str = "BRAVEBOT_TEST_SEARCH_KEY_ORDER_EMPTY";
    const FIRST: &str = "BRAVEBOT_TEST_SEARCH_KEY_ORDER_FIRST";
    const SECOND: &str = "BRAVEBOT_TEST_SEARCH_KEY_ORDER_SECOND";
    const OTHER: &str = "placeholder-search-key-from-the-second";
    let _empty = export(EMPTY, "");
    let _first = export_without_waiting(FIRST, KEY);
    let _second = export_without_waiting(SECOND, OTHER);
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);

    a_turn(
        &searching_with(&endpoint, &[EMPTY, FIRST, SECOND], None, &index),
        Task::new("search"),
        &mut Asked::answering(Decision::Approve),
        &received,
    );

    let head = the_request_sent(&heads);
    assert!(
        head.contains(&format!("x-subscription-token: {}", KEY.to_lowercase())),
        "{head}"
    );
    assert!(!head.contains(OTHER), "a later variable was sent: {head}");
}

/// Sets a variable of the process while the caller already holds the lock, for a test that sets
/// several.
fn export_without_waiting(name: &'static str, value: &str) -> RestoredVariable {
    let before = std::env::var_os(name);
    // SAFETY: the caller holds the lock of the first variable it set, and the name is this test's.
    unsafe { std::env::set_var(name, value) };
    RestoredVariable { name, before }
}

/// Puts a variable back as it was.
struct RestoredVariable {
    name: &'static str,
    before: Option<std::ffi::OsString>,
}

impl Drop for RestoredVariable {
    fn drop(&mut self) {
        // SAFETY: the lock of the test that set it is still held while this drops.
        match &self.before {
            Some(value) => unsafe { std::env::set_var(self.name, value) },
            None => unsafe { std::env::remove_var(self.name) },
        }
    }
}

/// S2: a variable that holds text that is not a key is refused in the driver's words, which name
/// the settings and neither the variable nor its text, and the key file is not used in its place.
/// Nobody is asked and nothing is sent. The faults this rejects are falling through to the file,
/// which hides a variable exported wrongly, and a refusal that quotes the variable or its name.
#[test]
fn a_variable_holding_no_usable_key_is_refused_without_its_name_or_its_text() {
    const NAME: &str = "BRAVEBOT_TEST_SEARCH_KEY_UNUSABLE";
    const TEXT: &str = "two words in the variable";
    let _held = export(NAME, TEXT);
    let (_keep, file) = a_key_file("search-unusable-variable");
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let mut asked = Asked::answering(Decision::Approve);

    let bodies = a_turn(
        &searching_with(&endpoint, &[NAME], Some(&file), &index),
        Task::new("search"),
        &mut asked,
        &received,
    );

    let said = told(&bodies);
    assert!(said.contains("webSearch.env"), "{said}");
    assert!(said.contains("webSearch.keyFile"), "{said}");
    assert!(
        !said.contains(NAME) && !said.contains("two words"),
        "the refusal holds the variable's name or its text: {said}"
    );
    assert!(asked.searches.is_empty());
    assert!(
        heads.recv_timeout(Duration::from_millis(300)).is_err(),
        "a search was sent although the variable held no usable key"
    );
}

/// What the person does in the middle of a turn: the variable the tool was offered for is removed
/// as the call is announced, which is after the list was settled and before the search reads it.
struct RemovesTheVariable(&'static str);

impl bravebot_agent::report::Reporter for RemovesTheVariable {
    fn todos(&mut self, _rows: Vec<bravebot_core::todo::Row>) {}

    fn tool_started(&mut self, _activity: bravebot_agent::report::Activity) {
        // SAFETY: the test holds the lock for this variable while the turn runs.
        unsafe { std::env::remove_var(self.0) };
    }
}

/// S3, S8: the tool list is settled at the start of the turn, so a variable removed after that
/// leaves the tool offered, and the call is reported as a key that cannot be used, in words that
/// hold neither the variable's name nor its text. The planner is not told the tool is unknown, and
/// nothing is asked or sent. The faults this rejects are asking the environment again at dispatch,
/// which answers an unknown tool for a tool that was on the list, and reading the key from a copy
/// kept at the start of the turn, which sends a key the person has since removed.
#[test]
fn a_variable_removed_after_the_turn_started_is_reported_as_an_unusable_key() {
    const NAME: &str = "BRAVEBOT_TEST_SEARCH_KEY_REMOVED";
    let _held = export(NAME, KEY);
    let (index, heads) = serve_the_index(vec![json_results("{}")]);
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let mut asked = Asked::answering(Decision::Approve);

    let bodies = a_turn_reporting(
        &searching_with(&endpoint, &[NAME], None, &index),
        Task::new("search"),
        &mut asked,
        &mut RemovesTheVariable(NAME),
        &received,
    );

    assert!(
        bodies[0].contains("\"web_search\""),
        "the tool was not offered at the start of the turn"
    );
    let said = told(&bodies);
    assert!(
        said.contains("cannot be used") && said.contains("webSearch.env"),
        "{said}"
    );
    assert!(!said.contains("no such tool"), "{said}");
    assert_nothing_of_the_variable_reached_the_planner(&bodies, NAME, &[KEY]);
    assert!(asked.searches.is_empty());
    assert!(
        heads.recv_timeout(Duration::from_millis(300)).is_err(),
        "a search was sent with a key the person had removed"
    );
}
