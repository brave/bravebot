//! What the tests that run the built binary against the stand-in of
//! `crates/agent/test-support/chatgpt_stand_in.rs` share: a home to run in, the environment and the
//! run, and a terminal to run in.
//!
//! The including file declares the stand-in as `chatgpt_stand_in` beside this module.
#![allow(
    dead_code,
    reason = "each test file that includes this module uses some of it"
)]

use crate::chatgpt_stand_in::{CLIENT, STAND_IN, StandIn};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

#[path = "../../../skus/test-support/stored_sign_in.rs"]
mod stored_sign_in;

/// The host identifier of a stored sign-in.
pub const SIGN_IN_HOST: &str = "urn:uuid:placeholder-host-0000";

/// A directory a run takes as its home, removed when the test ends.
pub struct Scratch {
    pub path: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".bravebot")).expect("create scratch");
        std::fs::create_dir_all(path.join("work")).expect("create the working directory");
        std::fs::create_dir_all(path.join("no-programs")).expect("create an empty PATH");
        Self {
            path: path.canonicalize().expect("canonical scratch"),
        }
    }

    /// A home whose path is short enough for a socket inside it, which a background session listens
    /// on. `name` is told apart from another test's by the process.
    pub fn short(name: &str) -> Self {
        let path = PathBuf::from(format!("/tmp/bbcg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        for directory in [".bravebot", "work", "no-programs"] {
            std::fs::create_dir_all(path.join(directory)).expect("create the short home");
        }
        Self {
            path: path.canonicalize().expect("canonical short home"),
        }
    }

    pub fn account(&self) -> PathBuf {
        self.path.join(".bravebot/chatgpt-account.json")
    }

    /// Store a sign-in whose access token expires `expires_at` seconds after 1970-01-01.
    pub fn store_sign_in(&self, access_token: &str, refresh_token: &str, expires_at: u64) {
        std::fs::write(
            self.account(),
            stored_sign_in::stored_sign_in(
                access_token,
                refresh_token,
                expires_at,
                CLIENT,
                SIGN_IN_HOST,
            ),
        )
        .expect("a stored sign-in");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The environment every run gets: nothing inherited, the home, the stand-in, Brave's endpoint at a
/// port nothing listens on, and a `PATH` with nothing in it, so no browser is opened.
pub fn environment(command: &mut Command, home: &Scratch, stand_in: &StandIn) {
    command
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
        .current_dir(home.path.join("work"));
}

/// Run the binary with `arguments`, its stdin empty.
pub fn bravebot(home: &Scratch, stand_in: &StandIn, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, home, stand_in);
    command
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the built binary runs")
}

/// What a run wrote to its two streams, as one text.
pub fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The command that runs the built binary with `arguments` in a terminal, which a program that asks
/// whether it has one needs. It is `script`, whose arguments differ between the two systems these
/// tests run on. Windows has no `script`, and these tests do not run there. The arguments are
/// words that need no quoting.
pub fn in_a_terminal(arguments: &[&str]) -> Command {
    let program = env!("CARGO_BIN_EXE_bravebot");
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null", program]).args(arguments);
    #[cfg(target_os = "linux")]
    command.args([
        "-qec",
        &format!("'{program}' {}", arguments.join(" ")),
        "/dev/null",
    ]);
    command
}

/// What a terminal has shown, read as it arrives on a thread of its own.
pub struct Shown {
    bytes: Arc<Mutex<Vec<u8>>>,
    reading: std::thread::JoinHandle<()>,
}

impl Shown {
    pub fn read(mut source: impl Read + Send + 'static) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let reading = std::thread::spawn({
            let bytes = Arc::clone(&bytes);
            move || {
                let mut buffer = [0u8; 4096];
                while let Ok(read) = source.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    bytes
                        .lock()
                        .expect("what was shown")
                        .extend_from_slice(&buffer[..read]);
                }
            }
        });
        Self { bytes, reading }
    }

    /// What has been shown so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().expect("what was shown")).into_owned()
    }

    /// Everything shown, once the terminal has closed.
    pub fn finished(self) -> String {
        let Self { bytes, reading } = self;
        reading.join().expect("the terminal was read");
        String::from_utf8_lossy(&bytes.lock().expect("what was shown")).into_owned()
    }
}
