// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A line typed while the model is still thinking goes to it at once, and one typed while it is
//! writing prose waits for the prose (INPUT-10).
//!
//! The server holds its first reply open with the connection quiet, as a model does while it thinks
//! or writes a long call. The test types a line meanwhile and reads what the server is asked next.
//!
//! # Why a binary of its own
//!
//! The reply the server holds back is released by the test, and a held connection is something
//! the shared scripted server in `turn.rs` has no way to hold.

#[path = "../test-support/mock_model.rs"]
mod mock_model;

#[allow(
    dead_code,
    reason = "turns.rs is shared by several test binaries, and this one uses only run_cancellable"
)]
#[path = "../test-support/turns.rs"]
mod turns;

use bravebot_agent::Workspace;
use bravebot_agent::confirm::{Confirmer, Unattended};
use bravebot_agent::interjections::{Arrival, Interjections};
use bravebot_agent::report::Reporter;
use bravebot_agent::turn::Task;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::todo::Row;
use mock_model::{Scratch, as_sse, config_for, reply_with, trust_of};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// A person at the keyboard, as the turn sees one: lines they type wait in a buffer the turn takes
/// from, and the buffer says when one has arrived. Everything else is refused, as `Unattended`
/// refuses it.
struct Types {
    buffer: Interjections,
    refuses: Unattended,
}

impl Types {
    fn new(buffer: &Interjections) -> Self {
        Self {
            buffer: buffer.clone(),
            refuses: Unattended,
        }
    }
}

impl Confirmer for Types {
    fn confirm_write(
        &mut self,
        request: &bravebot_agent::WriteRequest,
    ) -> bravebot_agent::WriteDecision {
        self.refuses.confirm_write(request)
    }

    fn confirm_run(&mut self, request: &bravebot_agent::RunRequest) -> bravebot_agent::RunDecision {
        self.refuses.confirm_run(request)
    }

    fn confirm_read_output(
        &mut self,
        request: &bravebot_agent::confirm::OutputRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_read_output(request)
    }

    fn confirm_vetted_read(
        &mut self,
        request: &bravebot_agent::confirm::VetRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_vetted_read(request)
    }

    fn confirm_fetch(
        &mut self,
        request: &bravebot_agent::confirm::FetchRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_fetch(request)
    }

    fn confirm_server(
        &mut self,
        request: &bravebot_agent::confirm::ServerRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_server(request)
    }

    fn confirm_manifest(
        &mut self,
        request: &bravebot_agent::confirm::ManifestRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_manifest(request)
    }

    fn confirm_vouch(
        &mut self,
        request: &bravebot_agent::confirm::VouchRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_vouch(request)
    }

    fn confirm_exposing_read(
        &mut self,
        request: &bravebot_agent::confirm::ExposureRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_exposing_read(request)
    }

    fn confirm_tool_list(
        &mut self,
        request: &bravebot_agent::confirm::ToolListRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_tool_list(request)
    }

    fn confirm_mcp_call(
        &mut self,
        request: &bravebot_agent::confirm::McpCallRequest,
    ) -> bravebot_agent::confirm::CallDecision {
        self.refuses.confirm_mcp_call(request)
    }

    fn confirm_move(
        &mut self,
        request: &bravebot_agent::confirm::MoveRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_move(request)
    }

    fn confirm_path(
        &mut self,
        request: &bravebot_agent::confirm::PathRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_path(request)
    }

    fn confirm_host(
        &mut self,
        request: &bravebot_agent::confirm::HostRequest,
    ) -> bravebot_agent::Decision {
        self.refuses.confirm_host(request)
    }

    fn confirm_message(
        &mut self,
        request: &bravebot_agent::confirm::MessageRequest,
    ) -> bravebot_agent::confirm::MessageDecision {
        self.refuses.confirm_message(request)
    }

    fn ask_user(&mut self, asking: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        self.refuses.ask_user(asking)
    }

    fn interjection(&mut self) -> Option<String> {
        self.buffer.take()
    }

    fn arrival(&self) -> Arrival {
        self.buffer.arrival()
    }
}

/// What the turn tells its interface, as events a test can wait on instead of sleeping.
///
/// The turn notes each chunk of the reply before it reports it, so once a test hears of a chunk
/// the turn has already decided whether a line typed now may cut the request.
struct Tells {
    told: mpsc::Sender<&'static str>,
    interjected: Vec<String>,
}

impl Tells {
    fn new() -> (Self, mpsc::Receiver<&'static str>) {
        let (told, heard) = mpsc::channel();
        (
            Self {
                told,
                interjected: Vec::new(),
            },
            heard,
        )
    }
}

impl Reporter for Tells {
    fn todos(&mut self, _rows: Vec<Row>) {}

    fn output_tokens(&mut self, _written: u64) {
        let _ = self.told.send("a chunk");
    }

    fn streaming(&mut self, text: String) {
        if !text.is_empty() {
            let _ = self.told.send("prose");
        }
    }

    fn composing(&mut self, call: Option<&'static str>) {
        if call.is_some() {
            let _ = self.told.send("a call");
        }
    }

    fn interjected(&mut self, said: String) {
        self.interjected.push(said);
    }
}

/// Block until the turn has told its interface of `kind`, or fail after ten seconds.
fn wait_for(heard: &mpsc::Receiver<&'static str>, kind: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match heard.recv_timeout(left) {
            Ok(told) if told == kind => return,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    panic!("the turn never told its interface of {kind}");
}

/// A model service whose first reply is held open after `opening` until `release` is set, and
/// whose later replies come from `rest` as soon as they are asked for.
///
/// Each connection is served on a thread of its own, so a request sent while the first is held is
/// answered. Reports every request body, in the order the requests arrived.
fn serve_a_held_first_reply(
    opening: String,
    finishing: String,
    rest: Vec<String>,
) -> (String, mpsc::Receiver<String>, Arc<AtomicBool>) {
    serve_holding(Arc::new(|_, nth| nth == 0), opening, finishing, rest)
}

/// Decides, from a request's body and how many requests came before it, whether its reply is held.
type Holds = Arc<dyn Fn(&str, usize) -> bool + Send + Sync>;

/// As [`serve_a_held_first_reply`], holding the replies `holds` picks. The replies that are not
/// held come from `rest` in the order they are asked for.
fn serve_holding(
    holds: Holds,
    opening: String,
    finishing: String,
    rest: Vec<String>,
) -> (String, mpsc::Receiver<String>, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (bodies, received) = mpsc::channel();
    let release = Arc::new(AtomicBool::new(false));
    let released = Arc::clone(&release);
    let counted = Arc::new(AtomicUsize::new(0));
    let rest = Arc::new(Mutex::new(rest.into_iter()));

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let (bodies, released, counted, rest) = (
                bodies.clone(),
                Arc::clone(&released),
                Arc::clone(&counted),
                Arc::clone(&rest),
            );
            let (opening, finishing) = (opening.clone(), finishing.clone());
            let holds = Arc::clone(&holds);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let mut content_length = 0usize;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
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
                let body = String::from_utf8_lossy(&body).to_string();
                let _ = bodies.send(body.clone());

                if holds(&body, counted.fetch_add(1, Ordering::SeqCst)) {
                    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(opening.as_bytes());
                    let _ = stream.flush();
                    let until = Instant::now() + Duration::from_secs(30);
                    while !released.load(Ordering::Acquire) && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    let _ = stream.write_all(finishing.as_bytes());
                    let _ = stream.flush();
                    return;
                }
                let next = rest.lock().expect("script").next();
                let frames = as_sse(&next.unwrap_or_else(|| reply_with("nothing more scripted")));
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{frames}",
                    frames.len()
                );
                let _ = stream.write_all(answer.as_bytes());
                let _ = stream.flush();
            });
        }
    });

    (format!("http://127.0.0.1:{port}"), received, release)
}

/// The frame that opens every reply and says nothing.
const OPENS: &str =
    "data: {\"model\":\"test-model\",\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n";

/// A frame carrying `words` of the reply proper.
fn says(words: &str) -> String {
    format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{words}\"}}}}]}}\n\n")
}

/// The frames that end a reply.
const ENDS: &str =
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

/// The line the tests type.
const LINE: &str = "no, use the other one";

/// INPUT-10: a line typed while the reply is no more than thinking is in the next request at once.
///
/// The server holds its first reply open with nothing in it for thirty seconds. Without the cut the
/// line waits for that reply, and the second request does not go out until the server lets the
/// first go. Rejects a turn that waits the reply out: the second request arrives within a few
/// seconds, while the first is still held.
#[test]
fn a_line_typed_while_the_model_is_thinking_is_in_the_next_request_at_once() {
    let scratch = Scratch::new("cut-thinking");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let (endpoint, received, release) = serve_a_held_first_reply(
        OPENS.to_string(),
        ENDS.to_string(),
        vec![reply_with("switching to the other one")],
    );
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let buffer = Interjections::new();
    let mut confirmer = Types::new(&buffer);
    let (mut reporter, heard) = Tells::new();
    let mut sink = RecordingSink::new();

    let (second, outcome) = std::thread::scope(|scope| {
        let typed = buffer.clone();
        let typing = scope.spawn(move || {
            let first = received
                .recv_timeout(Duration::from_secs(20))
                .expect("the first request never arrived");
            // The reply is in flight and has said nothing: its opening chunk has been read.
            wait_for(&heard, "a chunk");
            let typed_at = Instant::now();
            assert!(typed.push(LINE.into()));
            let second = received.recv_timeout(Duration::from_secs(10));
            let waited = typed_at.elapsed();
            release.store(true, Ordering::Release);
            (first, second, waited)
        });
        let outcome = turns::run_cancellable(
            &config,
            &egress,
            &workspace,
            &Task::new("what is in a.txt?"),
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust_of(&workspace, false),
            &Cancel::new(),
        );
        (typing.join().expect("the typist"), outcome)
    });
    let (first, second, waited) = second;

    outcome.expect("the turn runs to its answer");
    let second = second.expect("no second request went out while the first was held");
    assert!(
        !first.contains(LINE),
        "the line was in the request that started the wait"
    );
    assert!(
        second.contains(LINE),
        "the line did not reach the model in the request that replaced the held one: {second}"
    );
    assert!(
        waited < Duration::from_secs(5),
        "the line waited {waited:?} for a reply that had said nothing"
    );
    assert_eq!(
        reporter.interjected,
        vec![LINE.to_string()],
        "the interface was not told the line had gone in"
    );
    assert!(
        buffer.drain().is_empty(),
        "the line was left in the buffer as well"
    );
}

/// INPUT-10: a line typed while a call is being written is in the next request at once, even
/// after a sentence of prose.
///
/// The server says a sentence, opens a call and then holds the reply. The sentence is over once
/// the call has begun, and the call is what the person is waiting on. Rejects a gate that stays
/// shut after the first words, which would make every round that opens with a sentence wait out
/// the whole call.
#[test]
fn a_line_typed_while_a_call_is_being_written_is_in_the_next_request_at_once() {
    let scratch = Scratch::new("cut-call");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let opens_a_call = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"list_files\",\"arguments\":\"\"}}]}}]}\n\n";
    let (endpoint, received, release) = serve_a_held_first_reply(
        format!("{OPENS}{}{opens_a_call}", says("I will look around.")),
        ENDS.to_string(),
        vec![reply_with("switching to the other one")],
    );
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let buffer = Interjections::new();
    let mut confirmer = Types::new(&buffer);
    let (mut reporter, heard) = Tells::new();
    let mut sink = RecordingSink::new();

    let (second, outcome) = std::thread::scope(|scope| {
        let typed = buffer.clone();
        let typing = scope.spawn(move || {
            received
                .recv_timeout(Duration::from_secs(20))
                .expect("the first request never arrived");
            // The sentence has been read, and the call it led to has been opened.
            wait_for(&heard, "prose");
            wait_for(&heard, "a call");
            let typed_at = Instant::now();
            assert!(typed.push(LINE.into()));
            let second = received.recv_timeout(Duration::from_secs(10));
            let waited = typed_at.elapsed();
            release.store(true, Ordering::Release);
            (second, waited)
        });
        let outcome = turns::run_cancellable(
            &config,
            &egress,
            &workspace,
            &Task::new("what is in a.txt?"),
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust_of(&workspace, false),
            &Cancel::new(),
        );
        (typing.join().expect("the typist"), outcome)
    });
    let (second, waited) = second;

    outcome.expect("the turn runs to its answer");
    let second = second.expect("no second request went out while the call was being written");
    assert!(
        second.contains(LINE),
        "the line was not in the request: {second}"
    );
    assert!(
        !second.contains("I will look around."),
        "what the model wrote before the cut reply was dropped is in the next request: {second}"
    );
    assert!(
        waited < Duration::from_secs(5),
        "the line waited {waited:?} for a call that was still being written"
    );
}

/// INPUT-10: a reply that has begun to say something is not cut, however long it goes on.
///
/// The server writes a sentence and then holds the reply. A line typed then waits for the reply to
/// end: no second request goes out while it is held, and once it ends the line is still in the
/// buffer for the front end to make a turn of. Rejects a turn that cuts any reply for any line,
/// which would throw away an answer a person is reading.
#[test]
fn a_line_typed_while_the_model_is_writing_prose_waits_for_the_reply() {
    let scratch = Scratch::new("cut-prose");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let (endpoint, received, release) = serve_a_held_first_reply(
        format!("{OPENS}{}", says("Let me look at it")),
        format!("{}{ENDS}", says(" now.")),
        Vec::new(),
    );
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let buffer = Interjections::new();
    let mut confirmer = Types::new(&buffer);
    let (mut reporter, heard) = Tells::new();
    let mut sink = RecordingSink::new();

    let (a_second_request, outcome) = std::thread::scope(|scope| {
        let typed = buffer.clone();
        let typing = scope.spawn(move || {
            received
                .recv_timeout(Duration::from_secs(20))
                .expect("the first request never arrived");
            // The sentence has been read and noted as prose.
            wait_for(&heard, "prose");
            assert!(typed.push(LINE.into()));
            let a_second_request = received.recv_timeout(Duration::from_millis(1500));
            release.store(true, Ordering::Release);
            a_second_request
        });
        let outcome = turns::run_cancellable(
            &config,
            &egress,
            &workspace,
            &Task::new("what is in a.txt?"),
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust_of(&workspace, false),
            &Cancel::new(),
        );
        (typing.join().expect("the typist"), outcome)
    });

    outcome.expect("the turn runs to its answer");
    assert!(
        a_second_request.is_err(),
        "a reply that was writing prose was cut for a line: {a_second_request:?}"
    );
    assert!(
        reporter.interjected.is_empty(),
        "the turn took a line it had no round boundary to take"
    );
    assert_eq!(
        buffer.drain(),
        vec![LINE.to_string()],
        "the line was lost when the reply ended"
    );
}

/// INPUT-10: stopping the turn while a request is held ends the turn, as it always did. A request
/// that answers to a token of its own must still answer to the turn's.
#[test]
fn stopping_the_turn_still_ends_a_request_that_a_line_could_have_cut() {
    let scratch = Scratch::new("cut-stop");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let (endpoint, received, release) =
        serve_a_held_first_reply(OPENS.to_string(), ENDS.to_string(), Vec::new());
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let buffer = Interjections::new();
    let mut confirmer = Types::new(&buffer);
    let stop = Cancel::new();
    let (mut reporter, heard) = Tells::new();
    let mut sink = RecordingSink::new();

    let (waited, outcome) = std::thread::scope(|scope| {
        let stopping = stop.clone();
        let typing = scope.spawn(move || {
            received
                .recv_timeout(Duration::from_secs(20))
                .expect("the first request never arrived");
            wait_for(&heard, "a chunk");
            let stopped_at = Instant::now();
            stopping.cancel();
            // Released afterwards, so the turn can only have ended because it was stopped.
            std::thread::sleep(Duration::from_secs(3));
            release.store(true, Ordering::Release);
            stopped_at
        });
        let outcome = turns::run_cancellable(
            &config,
            &egress,
            &workspace,
            &Task::new("what is in a.txt?"),
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust_of(&workspace, false),
            &stop,
        );
        let ended = Instant::now();
        let stopped_at = typing.join().expect("the typist");
        (ended.saturating_duration_since(stopped_at), outcome)
    });

    assert!(
        matches!(
            outcome,
            Err(bravebot_agent::turn::TurnError::Cancelled { .. })
        ),
        "a stopped turn did not report that it was stopped"
    );
    assert!(
        waited < Duration::from_secs(3),
        "the turn took {waited:?} to end after it was stopped"
    );
}

/// A person at the keyboard whose line the turn can never take: the buffer says one is waiting,
/// and asking for it finds nothing, as when a delegate's question holds the lock.
struct SeesALineItCannotTake {
    types: Types,
}

impl SeesALineItCannotTake {
    fn new(buffer: &Interjections) -> Self {
        Self {
            types: Types::new(buffer),
        }
    }
}

macro_rules! refuse_as_types {
    ($($name:ident($request:ty) -> $answer:ty;)*) => {
        $(fn $name(&mut self, request: &$request) -> $answer {
            self.types.$name(request)
        })*
    };
}

impl Confirmer for SeesALineItCannotTake {
    refuse_as_types! {
        confirm_write(bravebot_agent::WriteRequest) -> bravebot_agent::WriteDecision;
        confirm_run(bravebot_agent::RunRequest) -> bravebot_agent::RunDecision;
        confirm_read_output(bravebot_agent::confirm::OutputRequest) -> bravebot_agent::Decision;
        confirm_vetted_read(bravebot_agent::confirm::VetRequest) -> bravebot_agent::Decision;
        confirm_fetch(bravebot_agent::confirm::FetchRequest) -> bravebot_agent::Decision;
        confirm_server(bravebot_agent::confirm::ServerRequest) -> bravebot_agent::Decision;
        confirm_manifest(bravebot_agent::confirm::ManifestRequest) -> bravebot_agent::Decision;
        confirm_vouch(bravebot_agent::confirm::VouchRequest) -> bravebot_agent::Decision;
        confirm_exposing_read(bravebot_agent::confirm::ExposureRequest) -> bravebot_agent::Decision;
        confirm_tool_list(bravebot_agent::confirm::ToolListRequest) -> bravebot_agent::Decision;
        confirm_mcp_call(bravebot_agent::confirm::McpCallRequest) -> bravebot_agent::confirm::CallDecision;
        confirm_move(bravebot_agent::confirm::MoveRequest) -> bravebot_agent::Decision;
        confirm_path(bravebot_agent::confirm::PathRequest) -> bravebot_agent::Decision;
        confirm_host(bravebot_agent::confirm::HostRequest) -> bravebot_agent::Decision;
        confirm_message(bravebot_agent::confirm::MessageRequest) -> bravebot_agent::confirm::MessageDecision;
    }

    fn ask_user(&mut self, asking: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        self.types.ask_user(asking)
    }

    fn interjection(&mut self) -> Option<String> {
        None
    }

    fn arrival(&self) -> Arrival {
        self.types.arrival()
    }
}

/// INPUT-10: a cut that finds no line to take is not repeated for the rest of the turn.
///
/// The buffer says a line is waiting and asking for it finds none, so the cut that answers the
/// flag achieves nothing. Both replies are held, so a turn that cut again would send a third
/// request. Rejects a turn that keeps cutting: requests go out one after another for as long as
/// the flag stays set.
#[test]
fn a_cut_that_finds_no_line_is_not_repeated() {
    let scratch = Scratch::new("cut-once");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let (endpoint, received, release) = serve_holding(
        Arc::new(|_, nth| nth < 2),
        OPENS.to_string(),
        format!("{}{ENDS}", says("done.")),
        Vec::new(),
    );
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let buffer = Interjections::new();
    let mut confirmer = SeesALineItCannotTake::new(&buffer);
    let (mut reporter, heard) = Tells::new();
    let mut sink = RecordingSink::new();
    let stop = Cancel::new();
    let over = Arc::new(AtomicBool::new(false));

    let (requests, outcome) = std::thread::scope(|scope| {
        let typed = buffer.clone();
        let stopping = stop.clone();
        let finished = Arc::clone(&over);
        let typing = scope.spawn(move || {
            received
                .recv_timeout(Duration::from_secs(20))
                .expect("the first request never arrived");
            wait_for(&heard, "a chunk");
            assert!(typed.push(LINE.into()));
            let mut requests = 1;
            // The cut sends one more; a third would be a cut that was repeated. The turn is
            // stopped then, so a turn that keeps cutting fails on the count and does not go on
            // sending requests until the test is abandoned.
            while requests < 5 && received.recv_timeout(Duration::from_millis(1500)).is_ok() {
                requests += 1;
                if requests > 2 {
                    stopping.cancel();
                }
            }
            release.store(true, Ordering::Release);
            // A turn that cuts again before any request is sent is invisible to the count above,
            // and would never come back. It is stopped when it has had time to finish.
            let until = Instant::now() + Duration::from_secs(10);
            while !finished.load(Ordering::Acquire) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            stopping.cancel();
            requests
        });
        let outcome = turns::run_cancellable(
            &config,
            &egress,
            &workspace,
            &Task::new("what is in a.txt?"),
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust_of(&workspace, false),
            &stop,
        );
        over.store(true, Ordering::Release);
        (typing.join().expect("the typist"), outcome)
    });

    assert_eq!(
        requests, 2,
        "a cut that found no line was repeated, or never made"
    );
    outcome.expect("the turn runs to its answer");
}

/// INPUT-10: a delegate's request is not cut for a line waiting in the person's buffer.
///
/// The parent hands a task to a reader, and the reader's reply is held with nothing in it while a
/// line waits. The line is the parent's to take, so a second request from the delegate would be a
/// delegate asking for a line it may never have.
#[test]
fn a_delegates_request_is_not_cut_for_a_line_waiting() {
    let scratch = Scratch::new("cut-delegate");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    // The parent's requests hold its own task, which a delegate's do not.
    let (endpoint, received, release) = serve_holding(
        Arc::new(|body, _| !body.contains("DELEGATE-THE-WORK")),
        OPENS.to_string(),
        format!("{}{ENDS}", says("read it.")),
        vec![
            mock_model::tool_request("spawn_agent", r#"{"kind":"reader","task":"DO-THE-WORK"}"#),
            reply_with("waiting"),
            reply_with("done"),
        ],
    );
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let buffer = Interjections::new();
    let mut confirmer = SeesALineItCannotTake::new(&buffer);
    let (mut reporter, heard) = Tells::new();
    let mut sink = RecordingSink::new();

    let (delegate_requests, outcome) = std::thread::scope(|scope| {
        let typed = buffer.clone();
        let typing = scope.spawn(move || {
            let mut delegate_requests = 0;
            // The first request the delegate's own prompt marks is the delegate's.
            while let Ok(body) = received.recv_timeout(Duration::from_secs(20)) {
                if body.contains("DELEGATE-THE-WORK") {
                    continue;
                }
                delegate_requests += 1;
                break;
            }
            assert_eq!(delegate_requests, 1, "the delegate never asked");
            wait_for(&heard, "a chunk");
            assert!(typed.push(LINE.into()));
            while let Ok(body) = received.recv_timeout(Duration::from_millis(1500)) {
                if !body.contains("DELEGATE-THE-WORK") {
                    delegate_requests += 1;
                }
            }
            release.store(true, Ordering::Release);
            delegate_requests
        });
        let outcome = turns::run_cancellable(
            &config,
            &egress,
            &workspace,
            &Task::new("DELEGATE-THE-WORK"),
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust_of(&workspace, false),
            &Cancel::new(),
        );
        (typing.join().expect("the typist"), outcome)
    });

    outcome.expect("the turn runs to its answer");
    assert_eq!(
        delegate_requests, 1,
        "a delegate's request was cut for a line that was the turn's to take"
    );
}
