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
