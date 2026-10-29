//! A streamed Bedrock request through `Backend` puts back what an earlier session kept about the
//! model and keeps what the process found (BACKEND-63).
//!
//! Bedrock's host is fixed and its recording of a request is private to its crate, so there is no
//! endpoint to stand in. The request here is cancelled before it starts: the client loads the
//! record, finds the stop and sends nothing, and the code around the call runs as it does for any
//! request. What the first request carries, with and without the ask, is
//! `a_model_an_earlier_session_found_refusing_the_ask_is_not_sent_it` in `bravebot_bedrock`; the
//! order of the calls around a request is `bedrock_refusals`' own tests.
//!
//! # Why a binary of its own
//!
//! The state directory is named by a variable of the process, so this file sets it once for every
//! test in it and no test elsewhere is affected.

use bravebot_agent::backend::Backend;
use bravebot_aichat::protocol::{ChatRequest, Message};
use bravebot_config::{Config, env_var};
use bravebot_core::cancel::Cancel;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use std::path::PathBuf;

const SEEDED: &str = "arn:aws:bedrock:us-west-2:111122223333:application-inference-profile/seeded";
const FOUND: &str = "arn:aws:bedrock:us-west-2:111122223333:application-inference-profile/found";

/// A scratch state directory that removes itself.
struct Scratch {
    home: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!(
            "bravebot-agent-bedrock-refusals-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create scratch");
        // SAFETY: this binary holds one test, and nothing here reads the environment concurrently.
        unsafe { std::env::set_var(bravebot_agent::home::STATE_VARIABLE, &home) };
        Self { home }
    }

    fn file(&self) -> PathBuf {
        self.home.join("bedrock-refusals.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn two_bedrock_models() -> Config {
    Config::from_lookup(|key| {
        match key {
            env_var::USE_BEDROCK => Some("1"),
            env_var::AWS_REGION => Some("us-west-2"),
            env_var::BEDROCK_OPUS_MODEL => Some(SEEDED),
            env_var::BEDROCK_SONNET_MODEL => Some(FOUND),
            env_var::AWS_PROFILE => Some("a-profile-no-machine-has"),
            env_var::SIGNING_KEY => Some("test-signing-key"),
            env_var::KEY_ID => Some("test-key-id"),
            env_var::ENDPOINT => Some("https://example.invalid"),
            _ => None,
        }
        .map(str::to_string)
    })
    .expect("a configuration with two Bedrock models")
}

/// Stream a request for `model` through `Backend` that has been stopped before it began, and say
/// whether it came back as the stop.
fn stream_a_stopped_request(config: &Config, model: &str) -> bool {
    let egress = bravebot_net::Egress::new();
    let stop = Cancel::new();
    stop.cancel();
    let mut backend = Backend::select(config, &egress, model).with_cancel(stop);
    assert!(
        matches!(backend, Backend::Bedrock { .. }),
        "{model} was not served by Bedrock"
    );

    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "test");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("a policy");

    let request = ChatRequest::new(model, vec![Message::user("hello")]);
    backend
        .complete_streaming(&mut policy, &request, |_| {})
        .expect_err("a stopped request answered")
        .is_cancelled()
}

/// BACKEND-63: the streamed Bedrock arm of `Backend` puts back what an earlier session kept before
/// the request and keeps what the process found after it, once.
///
/// The faults this rejects: the arm left unbracketed, which sends the ask a model is known to
/// refuse in every session and loses what a session found, and a finding written by every request.
#[test]
fn a_streamed_request_puts_back_what_was_kept_and_keeps_what_was_found() {
    let scratch = Scratch::new();
    let config = two_bedrock_models();

    // An earlier ordinary session found SEEDED refusing the ask.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs();
    std::fs::write(
        scratch.file(),
        format!(r#"{{"thinking_shown":{{"{SEEDED}":{stamp}}}}}"#),
    )
    .expect("seed a record");
    let seeded = std::fs::read(scratch.file()).expect("the record");
    assert!(!bravebot_bedrock::refusals(SEEDED).thinking_shown);
    assert!(!bravebot_bedrock::refusals(FOUND).thinking_shown);

    assert!(stream_a_stopped_request(&config, SEEDED), "not the stop");
    assert!(
        bravebot_bedrock::refusals(SEEDED).thinking_shown,
        "the client was not told what the file holds"
    );
    assert!(
        !bravebot_bedrock::refusals(FOUND).thinking_shown,
        "the client was told about a model the file does not name"
    );
    assert_eq!(
        std::fs::read(scratch.file()).expect("the record"),
        seeded,
        "a refusal the file already names was written again"
    );

    // This process finds FOUND refusing the ask, as the client records it when the request
    // without the ask answers, and the next request through Backend keeps it.
    bravebot_bedrock::known_to_refuse_showing_thinking(FOUND);
    assert!(stream_a_stopped_request(&config, FOUND), "not the stop");
    let kept = std::fs::read_to_string(scratch.file()).expect("the record");
    assert!(
        kept.contains(FOUND) && kept.contains(SEEDED),
        "the finding was not kept beside the earlier one: {kept}"
    );

    // Kept once: a record removed is not written again by the next request for the model.
    std::fs::remove_file(scratch.file()).expect("removed");
    assert!(stream_a_stopped_request(&config, FOUND), "not the stop");
    assert!(
        !scratch.file().exists(),
        "a finding was kept again by a later request"
    );
}
