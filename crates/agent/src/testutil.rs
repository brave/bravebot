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

/// What the stand-ins for the ChatGPT serving host share: a request read off a connection, the reply
/// written to it, and the events of a reply.
#[path = "../test-support/chatgpt_host.rs"]
mod chatgpt_host;

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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("addr").port()
    );
    let taken = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = std::sync::Arc::clone(&taken);
    std::thread::spawn(move || {
        for answer in answers {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let Some(request) = chatgpt_host::read_request(&stream) else {
                return;
            };
            if let Ok(mut kept) = kept.lock() {
                kept.push(request.headers);
            }
            match answer {
                Answer::Status(status) => chatgpt_host::respond(&stream, status, "text/plain", ""),
                Answer::Words(words) => chatgpt_host::respond(
                    &stream,
                    200,
                    "text/event-stream",
                    &chatgpt_host::text_events(words),
                ),
            }
        }
    });
    Host { endpoint, taken }
}

/// How a stand-in for the xAI serving host answers one request.
#[derive(Clone)]
pub(crate) enum XaiAnswer {
    /// A streamed chat completion holding these words.
    Streamed(&'static str),
    /// A whole chat completion holding these words.
    Whole(&'static str),
    /// A status and a body, which a test makes carry text a reply must never be quoted for.
    Status(u16, &'static str),
    /// A redirect to this address.
    Redirect(String),
    /// The request is read and nothing is answered for this long, which is the wait a person presses
    /// stop in before the model has said anything.
    Silent(std::time::Duration),
    /// A streamed reply that begins with these words and then says nothing for this long, which is a
    /// reply that stalls part way.
    Stalled(&'static str, std::time::Duration),
}

/// A request a [`XaiHost`] took.
pub(crate) struct XaiRequest {
    pub(crate) path: String,
    pub(crate) authorization: Option<String>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: serde_json::Value,
}

/// A server standing in for the xAI serving host: it answers each connection with the next of the
/// answers it was given, and keeps each request it took.
pub(crate) struct XaiHost {
    /// The base address to give [`crate::xai::stand_in::at`].
    pub(crate) endpoint: String,
    taken: std::sync::Arc<std::sync::Mutex<Vec<XaiRequest>>>,
}

impl XaiHost {
    /// Every request taken so far, in order.
    pub(crate) fn requests(&self) -> std::sync::MutexGuard<'_, Vec<XaiRequest>> {
        self.taken.lock().expect("the requests taken")
    }
}

/// Start an [`XaiHost`] that answers its first connections with `answers`, in order.
pub(crate) fn xai_host(answers: Vec<XaiAnswer>) -> XaiHost {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("addr").port()
    );
    let taken = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = std::sync::Arc::clone(&taken);
    std::thread::spawn(move || {
        for answer in answers {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let Some(request) = chatgpt_host::read_request(&stream) else {
                return;
            };
            if let Ok(mut kept) = kept.lock() {
                kept.push(XaiRequest {
                    path: request.path.clone(),
                    authorization: request.header("authorization").map(str::to_string),
                    headers: request.headers.clone(),
                    body: serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null),
                });
            }
            match answer {
                XaiAnswer::Streamed(words) => {
                    let frame = serde_json::json!({"model": "grok-test", "choices": [{
                        "index": 0, "delta": {"role": "assistant", "content": words},
                        "finish_reason": "stop"}],
                        "usage": {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9}});
                    chatgpt_host::respond(
                        &stream,
                        200,
                        "text/event-stream",
                        &format!("data: {frame}\n\ndata: [DONE]\n\n"),
                    );
                }
                XaiAnswer::Whole(words) => {
                    let body = serde_json::json!({"model": "grok-test", "choices": [{
                        "index": 0, "message": {"role": "assistant", "content": words},
                        "finish_reason": "stop"}],
                        "usage": {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9}});
                    chatgpt_host::respond(&stream, 200, "application/json", &body.to_string());
                }
                XaiAnswer::Status(status, body) => {
                    chatgpt_host::respond(&stream, status, "application/json", body);
                }
                XaiAnswer::Silent(wait) => std::thread::sleep(wait),
                XaiAnswer::Stalled(words, wait) => {
                    use std::io::Write;
                    let frame = serde_json::json!({"model": "grok-test", "choices": [{
                        "index": 0, "delta": {"role": "assistant", "content": words}}]});
                    let mut stream = &stream;
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 \r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\ndata: {frame}\n\n"
                        )
                        .as_bytes(),
                    );
                    let _ = stream.flush();
                    std::thread::sleep(wait);
                }
                XaiAnswer::Redirect(to) => {
                    use std::io::Write;
                    let mut stream = &stream;
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 307 \r\nlocation: {to}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                }
            }
        }
    });
    XaiHost { endpoint, taken }
}
