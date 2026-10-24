//! Scratch directories, repositories and a stand-in serving host for tests.

#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// An absolute path under the workspace `target/test-scratch/`.
///
/// Tests built these under [`std::env::temp_dir`] before. That directory is shared
/// between users and between processes with different privileges, so a fixed name
/// under it collides whenever two checkouts run the tests at once, and it is the
/// insecure-temporary-file pattern the security scan flags. `target/` is
/// per-checkout and already ignored by git.
///
/// Nothing is created here: callers make and remove the directory as they already did.
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR is `<workspace>/crates/<crate>`, so two pops reach the root.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("target");
    path.push("test-scratch");
    path.push(name);
    path
}

/// Repositories written object by object, the fixture the integration tests of `read_git` use.
#[allow(
    dead_code,
    reason = "the integration tests that share this file use helpers the unit tests do not"
)]
#[path = "../tests/repository/mod.rs"]
pub(crate) mod repository;

/// What a stage puts at the name of a directory once it has moved the directory.
#[cfg(unix)]
pub(crate) type PutThere<'a> = Box<dyn Fn(&Path) + 'a>;

/// What a stage with a write row on the temporary directory can do to a directory's name: moves
/// the directory aside and puts something else at the name. Returns where the directory went, so
/// the test can remove it.
#[cfg(unix)]
pub(crate) fn moved_aside_and_replaced(path: &Path, put_there: impl FnOnce(&Path)) -> PathBuf {
    let aside = path.with_extension("aside");
    std::fs::rename(path, &aside).expect("the directory moved aside");
    put_there(path);
    aside
}

/// How a stand-in host answers one request.
#[derive(Clone, Copy)]
pub(crate) enum Answer {
    /// A streamed reply of the Responses protocol holding these words.
    Words(&'static str),
    /// A status and nothing else.
    Status(u16),
}

/// A server standing in for the ChatGPT account's serving host: it answers each connection with the
/// next of the answers it was given, and keeps the headers of each request it took.
pub(crate) struct Host {
    /// The base address to give [`crate::chatgpt::stand_in::at`].
    pub(crate) endpoint: String,
    taken: std::sync::Arc<std::sync::Mutex<Vec<Headers>>>,
}

/// The headers of one request, in the order they came.
type Headers = Vec<(String, String)>;

impl Host {
    /// The value of each header named `name`, one for each request taken so far, in order.
    pub(crate) fn header(&self, name: &str) -> Vec<Option<String>> {
        let taken = self.taken.lock().expect("the requests taken");
        taken
            .iter()
            .map(|headers| {
                headers
                    .iter()
                    .find(|(each, _)| each.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.clone())
            })
            .collect()
    }
}

/// Start a [`Host`] that answers its first connections with `answers`, in order.
pub(crate) fn serving_host(answers: Vec<Answer>) -> Host {
    use std::io::{BufRead, BufReader, Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("addr").port()
    );
    let taken = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = std::sync::Arc::clone(&taken);
    std::thread::spawn(move || {
        for answer in answers {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let Ok(clone) = stream.try_clone() else {
                return;
            };
            let mut reader = BufReader::new(clone);
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let mut headers = Vec::new();
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    headers.push((name.trim().to_string(), value.trim().to_string()));
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            if let Ok(mut kept) = kept.lock() {
                kept.push(headers);
            }
            let reply = match answer {
                Answer::Status(status) => format!(
                    "HTTP/1.1 {status} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                Answer::Words(words) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
                    words_as_a_stream(words)
                ),
            };
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    Host { endpoint, taken }
}

/// The events of a text reply, ending as the ChatGPT host ends one: the final response states the
/// figures and its own `output` list is empty.
fn words_as_a_stream(words: &str) -> String {
    let event = |kind: &str, fields: serde_json::Value| {
        let mut payload = serde_json::json!({ "type": kind });
        if let (Some(into), serde_json::Value::Object(from)) = (payload.as_object_mut(), fields) {
            into.extend(from);
        }
        format!("data: {payload}\n\n")
    };
    [
        event(
            "response.created",
            serde_json::json!({"response": {"status": "in_progress"}}),
        ),
        event(
            "response.output_item.added",
            serde_json::json!({"output_index": 0,
                "item": {"type": "message", "role": "assistant", "content": []}}),
        ),
        event(
            "response.output_text.delta",
            serde_json::json!({"delta": words}),
        ),
        event(
            "response.completed",
            serde_json::json!({"response": {"model": "served-model", "output": [], "usage": {
                "input_tokens": 23, "output_tokens": 5, "total_tokens": 28,
                "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 0}}}}),
        ),
    ]
    .concat()
}
