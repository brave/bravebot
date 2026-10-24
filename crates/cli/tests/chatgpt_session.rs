//! CHATGPT-10 through the built binary, against a stand-in for the serving host: the identifier
//! each request to the account carries as its session, thread and client request identifier, and
//! the effort level its body asks for.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on ([CHATGPT-2]). A request to the
//! account is a zstd frame, which the stand-in decodes for the level and reads nothing else of. It
//! answers by the order sessions first appear in, so the first identifier it sees is the run's own
//! and the next is a delegate's, which starts only after the run's first reply asked for it.
//!
//! These tests run on Linux and macOS, as `chatgpt_account.rs` does, since the home directory the
//! binary reads is set by `HOME`.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

#[path = "../../aichat/test-support/account_body.rs"]
mod account_body;

/// The variable the seam reads, taken from the crate so that this file does not compile where the
/// seam is off.
const STAND_IN: &str = bravebot_skus::chatgpt::STAND_IN;

const MODEL: &str = "chatgpt/gpt-placeholder";
const ACCESS: &str = "PLACEHOLDER-ACCESS-a1a1";
const REFRESH: &str = "PLACEHOLDER-REFRESH-r1r1";

/// A directory a run takes as its home, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".bravebot")).expect("create scratch");
        std::fs::create_dir_all(path.join("work")).expect("create the working directory");
        std::fs::create_dir_all(path.join("no-programs")).expect("create an empty PATH");
        let path = path.canonicalize().expect("canonical scratch");
        // Signed in, with a token that is good for an hour, so no run renews it.
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_secs()
            + 3600;
        std::fs::write(
            path.join(".bravebot/chatgpt-account.json"),
            serde_json::json!({
                "version": 1,
                "access_token": ACCESS,
                "refresh_token": REFRESH,
                "expires_at": expires,
                "client_id": "placeholder-client",
                "host_id": "urn:uuid:placeholder-host-0000",
            })
            .to_string(),
        )
        .expect("a stored sign-in");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// What the stand-in answers one request with.
#[derive(Clone, Copy)]
enum Reply {
    /// A text reply.
    Text(&'static str),
    /// A call of a tool, with its arguments as the JSON they are sent in.
    Call(&'static str, &'static str),
}

/// What the three headers of one request held, whether the request carried the token, and the
/// level its body asked for.
#[derive(Debug, Clone, PartialEq)]
struct Asked {
    session: Option<String>,
    thread: Option<String>,
    client_request: Option<String>,
    bearer: Option<String>,
    /// `reasoning.effort` of the body, or `None` where the body has no such field.
    level: Option<String>,
}

#[derive(Default)]
struct State {
    /// The replies for the first session that appears, then for the second, and so on. A session
    /// beyond the list is answered with text.
    scripts: Vec<Vec<Reply>>,
    /// Each distinct `session-id` in the order it first appeared, and how many requests it made.
    sessions: Vec<(String, usize)>,
    asked: Vec<Asked>,
}

/// A stand-in for the serving host at `base`.
struct StandIn {
    base: String,
    state: Arc<Mutex<State>>,
}

impl StandIn {
    fn new(scripts: Vec<Vec<Reply>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let state = Arc::new(Mutex::new(State {
            scripts,
            ..State::default()
        }));
        std::thread::spawn({
            let state = Arc::clone(&state);
            move || {
                while let Ok((stream, _)) = listener.accept() {
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || serve(stream, &state));
                }
            }
        });
        Self { base, state }
    }

    fn asked(&self) -> Vec<Asked> {
        self.state.lock().expect("the state").asked.clone()
    }
}

/// The events of `reply`, ending as the host ends a stream.
fn events(reply: Reply) -> String {
    let event = |value: serde_json::Value| format!("data: {value}\n\n");
    let finished = event(serde_json::json!({"type": "response.completed",
        "response": {"model": "gpt-placeholder", "output": []}}));
    match reply {
        Reply::Text(words) => format!(
            "{}{finished}",
            event(serde_json::json!({"type": "response.output_text.delta", "delta": words}))
        ),
        Reply::Call(name, arguments) => format!(
            "{}{}{finished}",
            event(
                serde_json::json!({"type": "response.output_item.added", "output_index": 0,
                "item": {"type": "function_call", "call_id": "call_1", "name": name,
                    "arguments": ""}})
            ),
            event(
                serde_json::json!({"type": "response.output_item.done", "output_index": 0,
                "item": {"type": "function_call", "call_id": "call_1", "name": name,
                    "arguments": arguments}})
            ),
        ),
    }
}

/// Answer one request.
fn serve(stream: TcpStream, state: &Mutex<State>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    let mut asked = Asked {
        session: None,
        thread: None,
        client_request: None,
        bearer: None,
        level: None,
    };
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim().to_string());
        match name.as_str() {
            "content-length" => length = value.parse().unwrap_or(0),
            "session-id" => asked.session = Some(value),
            "thread-id" => asked.thread = Some(value),
            "x-client-request-id" => asked.client_request = Some(value),
            "authorization" => asked.bearer = value.strip_prefix("Bearer ").map(str::to_string),
            _ => {}
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);

    let (status, kind, reply) = if path == "/v1/responses" {
        asked.level = account_body::level_in(&body);
        let mut state = state.lock().expect("the state");
        let session = asked.session.clone().unwrap_or_default();
        let position = match state.sessions.iter().position(|(seen, _)| *seen == session) {
            Some(position) => position,
            None => {
                state.sessions.push((session, 0));
                state.sessions.len() - 1
            }
        };
        let nth = state.sessions[position].1;
        state.sessions[position].1 += 1;
        let reply = state
            .scripts
            .get(position)
            .and_then(|replies| replies.get(nth))
            .copied()
            .unwrap_or(Reply::Text("done"));
        state.asked.push(asked);
        (200, "text/event-stream", events(reply))
    } else {
        (404, "text/plain", String::new())
    };
    let mut stream = stream;
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status} \r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        )
        .as_bytes(),
    );
    let _ = stream.flush();
}

/// Run the binary in `home` against `stand_in` with `arguments`, its stdin empty.
fn bravebot(home: &Scratch, stand_in: &StandIn, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bravebot"))
        .env_clear()
        .env("HOME", &home.path)
        .env("PATH", home.path.join("no-programs"))
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env(STAND_IN, &stand_in.base)
        .current_dir(home.path.join("work"))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs")
}

/// What a run wrote to its two streams, as one text.
fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The identifier a request carried, which is the value of all three headers or no value at all.
fn identifier(asked: &Asked) -> String {
    let session = asked.session.clone().expect("a session-id header");
    assert_eq!(asked.thread, Some(session.clone()), "{asked:?}");
    assert_eq!(asked.client_request, Some(session.clone()), "{asked:?}");
    assert_eq!(asked.bearer.as_deref(), Some(ACCESS), "{asked:?}");
    session
}

/// A session in lines in `home`, which reads its prompts from a terminal, given each of `prompts`
/// once the stand-in has answered the one before. Returns what the terminal showed.
fn in_lines(home: &Scratch, stand_in: &StandIn, prompts: &[&str]) -> String {
    let program = env!("CARGO_BIN_EXE_bravebot");
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args([
        "-q",
        "/dev/null",
        program,
        "--plain",
        "--dangerously-skip-permissions",
    ]);
    #[cfg(target_os = "linux")]
    command.args([
        "-qec",
        &format!("'{program}' --plain --dangerously-skip-permissions"),
        "/dev/null",
    ]);
    let mut child = command
        .env_clear()
        .env("HOME", &home.path)
        .env("PATH", home.path.join("no-programs"))
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env(STAND_IN, &stand_in.base)
        .current_dir(home.path.join("work"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("script runs the built binary in a terminal");
    let shown = Arc::new(Mutex::new(Vec::new()));
    let reading = std::thread::spawn({
        let shown = Arc::clone(&shown);
        let mut out = child.stdout.take().expect("the terminal");
        move || {
            let mut buffer = [0u8; 4096];
            while let Ok(read) = out.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                shown
                    .lock()
                    .expect("what was shown")
                    .extend_from_slice(&buffer[..read]);
            }
        }
    });
    let mut typed = child.stdin.take().expect("its stdin");
    for (at, prompt) in prompts.iter().enumerate() {
        writeln!(typed, "{prompt}").expect("a prompt");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while stand_in.asked().len() <= at && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    // End of input, which ends the session. A session that does not end is stopped and fails the
    // test.
    drop(typed);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let ended = loop {
        if let Some(status) = child.try_wait().expect("the terminal") {
            break Some(status);
        }
        if std::time::Instant::now() >= until {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = reading.join();
    let shown = String::from_utf8_lossy(&shown.lock().expect("what was shown")).to_string();
    let status =
        ended.unwrap_or_else(|| panic!("the session did not end at the end of its input: {shown}"));
    assert!(status.success(), "the session ended with {status}: {shown}");
    shown
}

/// A delegate is a session of its own. The run asks for one in its first reply, so the requests are
/// the run's three and the delegate's one, each carrying its session's identifier in all three
/// headers. The regressions it rejects are a delegate that carries the identifier of the run that
/// spawned it, a delegate that begins an identifier for each request, a header that holds an
/// identifier other than the other two, and one identifier for the whole process.
#[test]
fn a_run_carries_one_identifier_in_all_three_headers_and_its_delegate_carries_another() {
    let home = Scratch::new("cli-chatgpt-session-delegate");
    let stand_in = StandIn::new(vec![
        vec![
            Reply::Call(
                "spawn_agent",
                r#"{"kind":"reader","task":"say something short"}"#,
            ),
            Reply::Text("nothing to add while it works"),
            Reply::Text("relayed"),
        ],
        vec![Reply::Text("REPORTED BACK")],
    ]);

    let output = bravebot(
        &home,
        &stand_in,
        &["-p", "send a reader", "--model", MODEL, "--trust-workspace"],
    );

    assert!(output.status.success(), "{}", said(&output));
    let asked = stand_in.asked();
    let sessions = stand_in.state.lock().expect("the state").sessions.clone();
    assert_eq!(
        sessions.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
        [3, 1],
        "the run makes three requests and its delegate one: {asked:?}"
    );
    let identifiers: Vec<String> = asked.iter().map(identifier).collect();
    assert_ne!(
        sessions[0].0, sessions[1].0,
        "the delegate shares the run's session"
    );
    assert_eq!(
        identifiers
            .iter()
            .filter(|each| **each == sessions[0].0)
            .count(),
        3
    );
    assert_eq!(
        identifiers
            .iter()
            .filter(|each| **each == sessions[1].0)
            .count(),
        1
    );
}

/// A goal run asks the account for its rounds and for the check after each, and all of them are one
/// session's. The regression it rejects is a check made under an identifier of its own, which is
/// what a check built with a session of its own would carry.
#[test]
fn the_rounds_and_the_checks_of_a_goal_run_carry_one_identifier() {
    let home = Scratch::new("cli-chatgpt-session-goal");
    let stand_in = StandIn::new(vec![vec![
        Reply::Text("it is made"),
        Reply::Text("MET\nthe exchange says it is made"),
    ]]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "-p",
            "make it so",
            "--goal",
            "it is made",
            "--model",
            MODEL,
            "--trust-workspace",
        ],
    );

    assert!(output.status.success(), "{}", said(&output));
    let asked = stand_in.asked();
    assert_eq!(asked.len(), 2, "one round and one check: {asked:?}");
    assert_eq!(identifier(&asked[0]), identifier(&asked[1]));
}

/// A session in lines, which is also what a background session runs, carries one identifier through
/// every prompt it takes. The regression it rejects is a session that begins an identifier for each
/// prompt, which is what a task built for each prompt and told nothing of its session does.
#[test]
fn a_session_in_lines_carries_one_identifier_through_every_prompt() {
    let home = Scratch::new("cli-chatgpt-session-lines");
    // The model the person last picked, which a session in lines opens on.
    std::fs::write(home.path.join(".bravebot/model"), format!("{MODEL}\n")).expect("a saved pick");
    let stand_in = StandIn::new(vec![]);

    let terminal = in_lines(&home, &stand_in, &["first", "second"]);

    let asked = stand_in.asked();
    assert_eq!(asked.len(), 2, "{asked:?}\n{terminal}");
    assert_eq!(identifier(&asked[0]), identifier(&asked[1]));
}

/// A one-shot run asks the account for the level it was started with, in every round, and a check of
/// its goal, which asks for none, carries none. The regressions it rejects are a run that is sent no
/// level because its session began without one, and a check that is sent the run's level although
/// it asked for none, which is what a client does that puts the session's level on every request
/// and not only on one that asks for a level.
#[test]
fn a_one_shot_run_asks_for_the_level_it_was_started_with_and_a_check_of_its_goal_for_none() {
    let home = Scratch::new("cli-chatgpt-session-level");
    let stand_in = StandIn::new(vec![vec![
        Reply::Text("it is made"),
        Reply::Text("MET\nthe exchange says it is made"),
    ]]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "-p",
            "make it so",
            "--goal",
            "it is made",
            "--effort",
            "low",
            "--model",
            MODEL,
            "--trust-workspace",
        ],
    );

    assert!(output.status.success(), "{}", said(&output));
    let levels: Vec<Option<String>> = stand_in
        .asked()
        .into_iter()
        .map(|asked| asked.level)
        .collect();
    assert_eq!(levels, [Some("low".to_string()), None]);
}

/// A session in lines, which is also what a background session runs, asks the account for the level
/// the person last chose in every prompt it takes. The regression it rejects is a session begun with
/// no level, which would send the account nothing whatever was chosen.
#[test]
fn a_session_in_lines_asks_for_the_level_chosen_in_every_prompt() {
    let home = Scratch::new("cli-chatgpt-session-lines-level");
    // The model and the level the person last chose, which a session in lines opens on.
    std::fs::write(home.path.join(".bravebot/model"), format!("{MODEL}\n")).expect("a saved pick");
    std::fs::write(home.path.join(".bravebot/effort"), "medium\n").expect("a saved level");
    let stand_in = StandIn::new(vec![]);

    let terminal = in_lines(&home, &stand_in, &["first", "second"]);

    let asked = stand_in.asked();
    assert_eq!(asked.len(), 2, "{asked:?}\n{terminal}");
    assert_eq!(
        asked
            .iter()
            .map(|asked| asked.level.as_deref())
            .collect::<Vec<_>>(),
        [Some("medium"), Some("medium")]
    );
}
