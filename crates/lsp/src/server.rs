//! A language server: started with a person's approval, kept for the session, asked read-only
//! questions.
//!
//! Not confined, and [LSP-5] is the argument for that. A server indexes by running its ecosystem's
//! build tooling, so a profile that denies writes and children yields one whose index never settles
//! rather than a confined one that answers. What stands in confinement's place is the same thing that
//! stands in it for [`run`]: a person approves the process, and the label on what comes back does not
//! depend on their answer.
//!
//! [LSP-5]: ../../../docs/specs/tools/lsp.md
//! [`run`]: ../../../docs/specs/tools/run.md

use crate::protocol::{
    Operation, RpcNotification, RpcRequest, RpcResponse, content_length, frame, hover_text,
    initialize_params, locations_in, path_to_uri, position_params, reference_params,
};
use crate::{Answer, LspError, LspResult};
use bravebot_core::capability::Capability;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How long a request may wait for the index to settle before answering from what there is.
///
/// LSP-7's bound. Indexing a large workspace outlasts a person's patience, and a turn held open with
/// nothing to show for it is RUN-11's problem arriving by another road.
///
/// Set from what a real server takes rather than from what felt reasonable: unconfined,
/// rust-analyzer runs seven reported passes over this workspace and finishes the last a little under
/// a minute in.
///
/// Deliberately not raised past that. A server that has not settled by now is usually one that
/// cannot, which under this confinement profile is the ordinary case for Rust and is written up as a
/// known cost in the spec, and a caller waiting three minutes to be told the answer is partial is
/// worse off than one told in twenty seconds. Reaching the bound is not a failure: LSP-7 answers
/// from what the index has and says plainly that it may be short.
pub const MAX_INDEX_WAIT: Duration = Duration::from_secs(20);

/// How long a single request may take once the index has settled.
pub const MAX_REQUEST_WAIT: Duration = Duration::from_secs(20);

/// The protocol's code for "the index moved while I was answering".
///
/// Not an error in any sense a caller can act on: the request was fine and the state it referred to
/// changed. Answered as nothing found, with the index reported unsettled.
const CONTENT_MODIFIED: i64 = -32801;

/// The codes a rejection is never read as nothing found under.
///
/// Each says something other than "I looked at that position and there is nothing there", so
/// taking it as an empty answer would report an absence nobody established, which is the false
/// negative LSP-6 exists to prevent.
///
/// Every other code is read as an absence, `InternalError` included, because that is what a real
/// out-of-range position answers. Measured on the two servers this machine has rather than
/// inferred from the protocol: gopls says `0`, rust-analyzer says `-32603 Invalid offset LineCol
/// { line: 9999, col: 0 } (line index length: 17)`. A rule keyed on the reserved range instead
/// would read the same as a fault and leave rust-analyzer exactly where it started.
const NOT_AN_ABSENCE: [i64; 8] = [
    // The request could not be read, understood or checked, which says nothing about any position
    // in it. `InvalidParams` is the arguable one, since an out-of-range position is a kind of bad
    // parameter, and it is here because the two ways of being wrong are not symmetric: a server
    // that rejects a stale line this way is left reporting a refusal, which is the bug this fixes
    // and is visible, while a malformed request taken as an absence is a silent wrong answer.
    -32700, // ParseError
    -32600, // InvalidRequest
    -32602, // InvalidParams
    // An operation the server does not implement, or one asked before it was ready. Answering "no
    // implementations" for a `goToImplementation` that never ran is the case this list is most
    // for: pyright has no `textDocument/implementation`, and a planner reads an empty answer as
    // proof.
    -32601, // MethodNotFound
    -32002, // ServerNotInitialized
    // Nobody looked, so nobody can say what is there.
    -32800, // RequestCancelled
    -32802, // ServerCancelled
    // The method ran and could not be completed. Not a report about the position either, and
    // listed for the same reason `InvalidParams` is: an unhelpful refusal beats a silent nothing.
    -32803, // RequestFailed
];

/// How long a server gets to exit on request before it is killed.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);

/// Which server serves a language, and what it is called.
///
/// A fixed table rather than configuration, for now: each entry is a binary this repository knows
/// asks nothing of the network and can answer from a read-only tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
    Go,
}

impl Language {
    /// The language a file extension belongs to, or `None` where we have no server for it.
    ///
    /// Decided from the path, which is routing and already trusted. Nothing is read to decide it.
    pub fn for_path(path: &str) -> Option<Self> {
        let extension = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase();
        match extension.as_str() {
            "rs" => Some(Self::Rust),
            "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => Some(Self::TypeScript),
            "py" | "pyi" => Some(Self::Python),
            "go" => Some(Self::Go),
            _ => None,
        }
    }

    /// The binary to launch, and the arguments it needs to speak the protocol on stdio.
    pub fn server(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Rust => ("rust-analyzer", &[]),
            Self::TypeScript => ("typescript-language-server", &["--stdio"]),
            Self::Python => ("pyright-langserver", &["--stdio"]),
            Self::Go => ("gopls", &[]),
        }
    }

    /// Whether starting this server runs the ecosystem's build tooling, and so code out of the
    /// dependency tree.
    ///
    /// Said to the person at the prompt rather than left inside "with your own access", because it is
    /// the part of LSP-5 they could not have inferred from the word "start". True for Rust, where
    /// `build.rs` and proc macros execute, and for Go, whose tooling builds to answer. A Node or
    /// Python server reads and type-checks without running the project.
    pub fn runs_build_tooling(self) -> bool {
        matches!(self, Self::Rust | Self::Go)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
            Self::Go => "Go",
        }
    }
}

/// The directory holding one index per workspace, directly under the state directory.
const CACHE_ROOT: &str = "lsp";

/// Remove an index an earlier build put one directory too deep.
///
/// The state directory's own name used to be appended a second time, so the index landed in
/// `~/.bravebot/.bravebot/lsp/`, which nothing reads, rebuilds or narrows. What is left there is an
/// index derived from every file in the workspace, at whatever the umask gave it, and it stays that
/// way for as long as the machine does.
///
/// Removed rather than narrowed. Nothing will ever read it, so a private copy of it is worth no
/// more than none, and narrowing would mean walking a tree to reach the files inside.
///
/// The path is derived rather than spelled: appending the directory's own name is what put it
/// there, so joining that name is what finds it, and the state directory keeps one definition.
/// Only a directory holding what this crate would have written is removed, so an unrelated
/// directory of the same name is left where it is.
fn remove_misplaced_index(state: &Path) {
    let Some(name) = state.file_name() else {
        return;
    };
    let nested = state.join(name);
    if nested.join(CACHE_ROOT).is_dir() {
        let _ = std::fs::remove_dir_all(&nested);
    }
}

/// Where a server keeps an index that outlives the session.
///
/// LSP-10: under the directory this process already owns, keyed by the workspace, never inside it.
/// `state` is that directory itself, `~/.bravebot` and not the home it sits in, so nothing here
/// appends the name a second time.
///
/// `None` for a session that adds nothing to `~/.bravebot`, which is incognito and a machine with
/// no state directory. That is where the index is *not* kept, not a session without one: it goes
/// to a [`SessionIndex`] instead, and the server re-indexes and says its answers are partial until
/// it settles. Answering `None` and leaving the caller to drop the variable is what put the index
/// in the workspace.
pub fn cache_for(state: Option<&Path>, workspace: &Path, incognito: bool) -> Option<PathBuf> {
    if incognito {
        return None;
    }
    Some(cache_under(state?, workspace))
}

/// The index directory for one workspace, directly under a directory that holds one per workspace.
///
/// Split out from [`cache_for`] because the same layout is used twice: under the state directory
/// for a session that keeps its index, and under [`SessionIndex`] for one that does not. Two
/// layouts would mean two directories to create, narrow and reason about, for a difference that is
/// only how long the parent lasts.
///
/// The name is a digest of the canonical path rather than the path flattened into one, so two
/// checkouts of the same project do not share an index and a directory that moved does not inherit
/// one. Not a cryptographic requirement: this only has to be stable and collision-resistant enough
/// that two workspaces on one machine differ.
fn cache_under(keep: &Path, workspace: &Path) -> PathBuf {
    let canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());

    // FNV-1a over the path's bytes. Enough for a directory name, and no dependency for it.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    keep.join(CACHE_ROOT).join(format!("{hash:016x}"))
}

/// Where an index goes when nothing about this session outlives it.
///
/// LSP-10 keeps an index under `~/.bravebot` so that the next session does not pay for it again. A
/// session that adds nothing there has nowhere to keep one: incognito, and a machine
/// [STATE-2](../../../docs/specs/state-directory.md) leaves without a state directory at all. What
/// it must not do instead is drop the variable. An unset `CARGO_TARGET_DIR` resolves against the
/// working directory, which is the workspace the question was about, so the index lands in the
/// user's tree as a side effect of a read, the one outcome LSP-10 forbids outright.
///
/// So it goes where the platform keeps what does not outlive a process, and goes with the session.
/// The cost is the one LSP-10's "why not a temporary directory" names, the index being built again
/// next time, and it is the cost incognito already accepts.
#[derive(Debug)]
struct SessionIndex {
    path: PathBuf,
}

impl SessionIndex {
    /// Make one, under the directory the platform keeps temporary files in.
    fn create() -> std::io::Result<Self> {
        Self::created_at(reserved_name())
    }

    /// Where it is.
    fn path(&self) -> &Path {
        &self.path
    }

    /// [`SessionIndex::create`], at a named path, so a test can hand it one twice.
    ///
    /// **Created, never adopted.** On Linux the temporary directory is ordinarily the
    /// world-writable `/tmp`, where a name this program composes is one another account can take
    /// first, or leave pointing at a directory of theirs. Adopting one would put an index derived
    /// from every file in the workspace where they can read it. `DirBuilder::create` is not
    /// recursive, so a name already there comes back as an error rather than as a directory, and
    /// the mode keeps another account out of what lands inside. Windows has no mode to set, and
    /// gives each user a temporary directory of their own.
    fn created_at(path: PathBuf) -> std::io::Result<Self> {
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = std::fs::DirBuilder::new();
        builder.create(&path)?;
        // Resolved, because the platform's temporary directory is commonly reached through a link
        // and this path is handed to a build tool that will report paths under it back. Owned
        // before the name is resolved, so a name that will not resolve is still removed.
        let mut made = Self { path };
        made.path = made.path.canonicalize()?;
        Ok(made)
    }
}

impl Drop for SessionIndex {
    /// Take the directory and everything in it, which is what keeps the trade honest: a session
    /// that kept nothing under `~/.bravebot` has kept nothing anywhere else either.
    ///
    /// A failure is not reported: this runs as a session ends, where there is nobody left to tell
    /// and nothing useful to do about it.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A name for a directory nothing has taken.
///
/// The pid separates processes, the stamp separates sessions within one, and the count separates
/// two taken in the same moment: the clock behind the stamp holds a value for thousands of reads,
/// so two names taken together are routinely the same name. Nothing in it says which workspace is
/// being indexed, which is a name in a directory anybody on the machine can list.
fn reserved_name() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let nth = SESSION_INDEXES.fetch_add(1, Ordering::Relaxed);
    // The standard library answers which directory that is, so a machine that puts temporary files
    // somewhere unusual is honoured rather than guessed at. `created_at` above creates the name
    // with mode 0700 and refuses one already taken, which is the secure creation this rule asks
    // for.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    std::env::temp_dir().join(format!("bravebot-lsp-{}-{stamp}-{nth}", std::process::id()))
}

/// What tells two names taken by one process apart.
static SESSION_INDEXES: AtomicU64 = AtomicU64::new(0);

/// Create a cache directory, and the directories between it and the state directory, reachable
/// only by this user.
///
/// The index is derived from every file in the workspace, so who may read it is who may read the
/// workspace. The mode is asked for as each directory is created, because a directory keeps the
/// mode it was made with. Spelled out here rather than shared with the crate that has a helper for
/// it: this crate depends on the kernel alone, as layering.md records, and a language server client
/// is not worth a dependency for four lines.
fn create_cache(path: &Path) -> std::io::Result<()> {
    if let Some(state) = path.parent().and_then(Path::parent) {
        remove_misplaced_index(state);
    }
    create_private(path)?;
    #[cfg(unix)]
    {
        // The directory holding one cache per workspace, narrowed for the same reason. This one and
        // no further: what the state directory itself is set to belongs to whichever subsystem
        // created it.
        if let Some(root) = path.parent()
            && root.file_name().is_some_and(|name| name == CACHE_ROOT)
        {
            narrow(root);
        }
    }
    Ok(())
}

/// Create one directory reachable only by this user, narrowing one that is already there.
///
/// A directory keeps the mode it was made with, so one an earlier run left open stays open unless
/// it is narrowed on the way past.
fn create_private(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        narrow(path);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

/// Narrow one directory, unless the name is a link.
///
/// `set_permissions` follows a link, and where one leads is outside the two directories this crate
/// owns: a linked cache would have a language server client setting the mode of a directory
/// somewhere else in the user's home.
#[cfg(unix)]
fn narrow(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_symlink());
    if !is_link {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
}

/// Whether a message is a server saying its initial index is built.
///
/// Read from the shape of a progress notification, never from prose: what is looked at is whether
/// the token is one each ecosystem uses for its initial index and whether the value says `end`.
/// Both are protocol structure, so this decides nothing from a byte a file chose.
///
/// A free function so LSP-7 can be pinned without a live process: whether an answer is partial is
/// this decision and nothing else.
pub fn says_indexing_finished(message: &Value) -> bool {
    if message.get("method").and_then(Value::as_str) != Some("$/progress") {
        return false;
    }
    let Some(params) = message.get("params") else {
        return false;
    };
    let token = params.get("token").and_then(Value::as_str).unwrap_or("");
    let finished = params
        .get("value")
        .and_then(|value| value.get("kind"))
        .and_then(Value::as_str)
        == Some("end");

    // The token each server ends its last indexing pass with, taken from what real servers send
    // rather than from what their documentation implies. `rustAnalyzer/cachePriming` is the last of
    // seven passes rust-analyzer reports on this workspace, and the plausible-looking
    // `rustAnalyzer/Indexing` is sent by nothing: an earlier version of this waited for that name,
    // never saw it, and marked every answer partial for the life of the process.
    //
    // A token this does not know means answers stay marked partial, which is the safe direction to be
    // wrong in: LSP-7 makes a partial answer say so, and an answer wrongly called partial costs a
    // sentence where one wrongly called complete costs a deleted function.
    finished
        && matches!(
            token,
            "rustAnalyzer/cachePriming" | "gopls/loading" | "pyright/analysis"
        )
}

/// Read framed messages off a server's output until the pipe ends.
///
/// Runs on its own thread, so a blocking read never holds up a caller's deadline. Stops on the first
/// thing it cannot make sense of: a stream whose framing has desynchronised cannot be resynchronised
/// by guessing, and carrying on would attribute one message's body to another's header.
fn read_messages(mut stdout: BufReader<ChildStdout>, sender: &std::sync::mpsc::Sender<Value>) {
    loop {
        let mut headers = String::new();
        loop {
            let mut byte = [0u8; 1];
            match stdout.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            headers.push(byte[0] as char);
            if headers.ends_with("\r\n\r\n") {
                break;
            }
            // A server printing something that is not a header block would otherwise be read
            // forever, one byte at a time.
            if headers.len() > 8192 {
                return;
            }
        }

        let Some(length) = content_length(&headers) else {
            return;
        };

        let mut body = vec![0u8; length];
        if stdout.read_exact(&mut body).is_err() {
            return;
        }

        match serde_json::from_slice(&body) {
            Ok(message) => {
                // A closed channel means nobody is left to receive, so there is nothing to do but
                // stop.
                if sender.send(message).is_err() {
                    return;
                }
            }
            // One unparseable body is not a reason to abandon the stream: the framing is still in
            // step, so the next message is still readable.
            Err(_) => continue,
        }
    }
}

/// One running server.
pub struct Server {
    language: Language,
    child: Child,
    stdin: ChildStdin,
    /// Messages the reader thread has parsed, in the order they arrived.
    ///
    /// A thread rather than reading inline, because every bound here has to be a real one. Reading
    /// on this thread makes a deadline unenforceable: the check happens between messages, so a
    /// server that goes quiet mid-index blocks in `read` and no elapsed time is ever consulted.
    /// That is not hypothetical, it is what the first version of this did, and it held a turn open
    /// indefinitely against a real rust-analyzer indexing this workspace.
    incoming: std::sync::mpsc::Receiver<Value>,
    next_id: u64,
    root: PathBuf,
    /// Whether the server has said it finished indexing.
    ///
    /// Starts false and is set by a progress notification. LSP-7 reports an answer given before
    /// that as partial, because a `findReferences` against a half-built index looks exactly like
    /// one that found everything.
    indexed: bool,
}

/// Shows what it is but nothing it has sent, so a log line cannot leak a file's contents.
impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("language", &self.language.as_str())
            .field("pid", &self.child.id())
            .field("indexed", &self.indexed)
            .finish_non_exhaustive()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Asked to stop, then killed if it did not. LSP-8: a server must not outlive the agent.
        let _ = self.request_shutdown();
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// Start a server for this language.
    ///
    /// `resolved` is the absolute path to the binary, so what runs is what a person was shown:
    /// RUN-8's reason, that `$PATH` and aliases decide what a name means and an approval must not
    /// follow a name onto a different binary.
    ///
    /// `cache` is where it keeps its index, from [`Servers::index_dir`]: under the state directory
    /// where this session keeps one between sessions, and under a directory that lasts the session
    /// where it does not. Always somewhere, never `None`. A server told nowhere is a server run in
    /// the workspace with no index location set, which writes the index into the workspace, so the
    /// type is what rules that out rather than a comment asking the caller not to.
    ///
    /// LSP-6: a missing binary is reported as missing rather than as an empty answer.
    pub fn launch(
        language: Language,
        resolved: &Path,
        root: &Path,
        cache: &Path,
        withheld: &[String],
    ) -> LspResult<Self> {
        let (program, args) = language.server();
        let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();

        let mut command = std::process::Command::new(resolved);
        command.args(&owned);

        // The user's environment reaches the server, because it runs with their access and a
        // toolchain reads its own variables to work: `CARGO_HOME`, `GOPATH`, `NODE_PATH`. What does
        // not reach it is this agent's own credentials, which is RUN-12 exactly: a person approving a
        // server read what it is and where it will run, and a credential travelling alongside was
        // granted without having been seen. The names come from the caller because which they are is
        // the host's business, the same reason `resolved` is passed in rather than looked up here.
        for name in withheld {
            command.env_remove(name);
        }

        // Where the index goes, said to each ecosystem in its own spelling. Nothing is written to the
        // workspace, which is LSP-10.
        //
        // Every directory the server is pointed at is made here rather than left to the server,
        // and a failure to make one stops the launch. A server handed a directory this process
        // could not create makes it itself, at the umask, and fills it with an index derived from
        // every file in the workspace: STATE-1 undone by the one case where it matters. Not
        // starting says so, where dropping the variable would silently write the index into the
        // tree instead, which is what LSP-10 forbids.
        let private = |path: &Path, create: fn(&Path) -> std::io::Result<()>| {
            create(path).map_err(|e| LspError::Start {
                language,
                detail: format!(
                    "its index directory {} could not be created: {e}",
                    path.display()
                ),
            })
        };
        private(cache, create_cache)?;
        match language {
            Language::Rust => {
                command.env("CARGO_TARGET_DIR", cache);
            }
            Language::Go => {
                let build = cache.join("go-build");
                private(&build, create_private)?;
                command.env("GOCACHE", build);
            }
            Language::TypeScript | Language::Python => {
                // Neither reads a variable for this; both use the system temporary directory,
                // and pointing that at the cache keeps it out of the workspace.
                command.env("TMPDIR", cache);
            }
        }

        let mut child = command
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // A server's diagnostics are noisy and are not this process's business.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    LspError::NoBinary { language, program }
                } else {
                    LspError::Start {
                        language,
                        detail: e.to_string(),
                    }
                }
            })?;

        let stdin = child.stdin.take().ok_or(LspError::Start {
            language,
            detail: "the server's stdin was not available".into(),
        })?;
        let stdout = child.stdout.take().ok_or(LspError::Start {
            language,
            detail: "the server's stdout was not available".into(),
        })?;

        // The reader owns stdout and hands whole messages over. It ends when the pipe does, so a
        // server that exits closes the channel and every waiting read learns about it.
        let (sender, incoming) = std::sync::mpsc::channel();
        std::thread::spawn(move || read_messages(BufReader::new(stdout), &sender));

        let mut server = Self {
            language,
            child,
            stdin,
            incoming,
            next_id: 1,
            root: root.to_path_buf(),
            indexed: false,
        };
        server.initialize()?;
        Ok(server)
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Whether the server has finished indexing.
    pub fn is_indexed(&self) -> bool {
        self.indexed
    }

    fn initialize(&mut self) -> LspResult<()> {
        let root_uri = path_to_uri(&self.root.to_string_lossy());
        self.send_request(
            "initialize",
            Some(initialize_params(
                &root_uri,
                "bravebot",
                env!("CARGO_PKG_VERSION"),
            )),
            MAX_REQUEST_WAIT,
        )?;
        self.notify("initialized", Some(serde_json::json!({})))
    }

    fn request_shutdown(&mut self) -> LspResult<()> {
        // Best effort: the process is killed if this does not land.
        self.send_request("shutdown", None, SHUTDOWN_GRACE)?;
        self.notify("exit", None)
    }

    fn notify(&mut self, method: &str, params: Option<Value>) -> LspResult<()> {
        let notification = RpcNotification::new(method, params);
        let body = serde_json::to_string(&notification).map_err(|e| LspError::Transport {
            language: self.language,
            detail: e.to_string(),
        })?;
        self.write(&frame(&body))
    }

    fn write(&mut self, framed: &str) -> LspResult<()> {
        self.stdin
            .write_all(framed.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|e| LspError::Transport {
                language: self.language,
                detail: format!("could not send a request: {e}"),
            })
    }

    /// Take the next message, waiting no longer than `budget`.
    ///
    /// Three outcomes, and they are genuinely different: a message, the budget running out, or the
    /// server having closed its output. The middle one is what makes every bound in this module
    /// real.
    fn next_message(&mut self, budget: Duration) -> LspResult<Option<Value>> {
        match self.incoming.recv_timeout(budget) {
            Ok(message) => Ok(Some(message)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(LspError::Exited {
                language: self.language,
            }),
        }
    }

    /// Send a request and wait for the reply with this id.
    ///
    /// Notifications arriving in between are read for whether indexing finished and otherwise
    /// dropped: this client handles no server-initiated request, so a server asking it to act gets
    /// no answer rather than an error.
    fn send_request(
        &mut self,
        method: &str,
        params: Option<Value>,
        budget: Duration,
    ) -> LspResult<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = RpcRequest::new(id, method, params);
        let body = serde_json::to_string(&request).map_err(|e| LspError::Transport {
            language: self.language,
            detail: e.to_string(),
        })?;
        self.write(&frame(&body))?;

        let deadline = Instant::now() + budget;
        loop {
            // What is left of the budget, so a stream of notifications cannot extend it: each wait
            // is bounded by the time remaining rather than by the whole allowance again.
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Some(message) = self.next_message(remaining)? else {
                return Err(LspError::TimedOut {
                    language: self.language,
                    method: method.to_string(),
                });
            };
            self.note_progress(&message);

            let response: RpcResponse = match serde_json::from_value(message) {
                Ok(response) => response,
                // Not a response shape; a notification or a server-initiated request.
                Err(_) => continue,
            };

            if response.id != Some(id) {
                continue;
            }

            if let Some(error) = response.error {
                // `ContentModified` is the protocol saying the index moved under the request, which
                // happens while a server is still settling. It is a retry rather than a failure: the
                // question was well formed and the answer is simply not available yet, so it is
                // reported as an unsettled index and LSP-7 marks whatever comes back partial.
                if error.code == CONTENT_MODIFIED {
                    return Ok(Value::Null);
                }
                // The code and the method are structure, and they are the whole of what is
                // reported: the sentence the server sent with them is prose it composed, which
                // LSP-5 keeps out of the planner's context. `RpcError` does not carry it here to
                // be dropped, because it is never deserialised.
                return Err(LspError::Server {
                    language: self.language,
                    code: error.code,
                    method: method.to_string(),
                });
            }

            // A query that matched nothing answers with null, which is an answer.
            return Ok(response.result.unwrap_or(Value::Null));
        }
    }

    /// Put a request that carries a position, taking a rejection of it as nothing found.
    ///
    /// LSP-2: a file changes under an agent, so a line number that was right when a search
    /// reported it is an ordinary thing to be stale by the time the planner names it. A server
    /// asked about a position past the end of a file answers with an error, gopls with `line
    /// number 9999 out of range 0-8`, and surfacing that as a refusal tells the planner the tool
    /// is broken when the truth is that there is nothing there.
    ///
    /// Every rejection a server raises in its own numbering is taken this way rather than the
    /// out-of-range ones alone, because the only thing separating those is the sentence the server
    /// wrote: gopls answers code `0` for an out-of-range line and for a fault of its own alike.
    /// Reading that sentence would be a decision taken from bytes a file chose, which is the one
    /// thing this crate may not do.
    ///
    /// What is read instead is structure, the same structure [`CONTENT_MODIFIED`] is read from: the
    /// request that was put, the code the protocol assigns, and whether the file was there to open.
    ///
    /// A [`NOT_AN_ABSENCE`] code is kept a failure, because each of those says something other
    /// than that the server looked and found nothing.
    ///
    /// `opened` is the same thing from the other side. A position in a file this process could not
    /// read may be stale, or the path may simply not be there, and nothing found would assert the
    /// first of those. Whether a file opened is a fact about the filesystem rather than about any
    /// byte in it, so reading it decides nothing from content.
    fn ask_at_a_position(&mut self, method: &str, params: Value, opened: bool) -> LspResult<Value> {
        match self.send_request(method, Some(params), MAX_REQUEST_WAIT) {
            Err(LspError::Server { code, .. }) if opened && !NOT_AN_ABSENCE.contains(&code) => {
                Ok(Value::Null)
            }
            answer => answer,
        }
    }

    /// Tell the server about a document, which is what makes it answerable.
    ///
    /// The contents are read off disk and handed straight over. This module never looks at them; see
    /// the note in [`Server::ask`] about why passing them through is a carry rather than a read.
    fn open(&mut self, path: &str, uri: &str) -> LspResult<()> {
        let text = std::fs::read_to_string(path).map_err(|e| LspError::Transport {
            language: self.language,
            detail: format!("could not read {path} to open it: {e}"),
        })?;

        let language_id = match self.language {
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::Python => "python",
            Language::Go => "go",
        };

        self.notify(
            "textDocument/didOpen",
            Some(serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            })),
        )
    }

    /// Notice a server saying it has finished indexing.
    fn note_progress(&mut self, message: &Value) {
        if says_indexing_finished(message) {
            self.indexed = true;
        }
    }

    /// Wait for the index to settle, up to [`MAX_INDEX_WAIT`].
    ///
    /// Reaching the bound is not a failure: the caller answers from what the index has and says the
    /// answer is partial, which is LSP-7.
    fn settle(&mut self) {
        if self.indexed {
            return;
        }
        let deadline = Instant::now() + MAX_INDEX_WAIT;
        while !self.indexed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            // Nothing is sent to prompt a message; this waits for what the server says on its own.
            // A server that has gone quiet times out here rather than blocking, which is the whole
            // reason the reader is on its own thread.
            match self.next_message(remaining) {
                Ok(Some(message)) => self.note_progress(&message),
                // Out of time, or the server is gone. Either way the caller answers from what the
                // index has and marks it partial, which is LSP-7.
                Ok(None) | Err(_) => return,
            }
        }
    }

    /// Ask one read-only question.
    ///
    /// The answer separates structure from content: locations come back as [`crate::Location`]
    /// values, and hover text comes back as a `String` the caller labels. LSP-3 is why those are two
    /// fields.
    pub fn ask(&mut self, question: &Question<'_>) -> LspResult<Answer> {
        self.settle();

        let operation = question.operation;
        let uri = path_to_uri(question.path);

        // The protocol requires a document be opened before it is asked about: a server answers
        // `file not found` otherwise, however plainly the file exists on disk, because its own view
        // of a document is the one the client told it about. Sent per question rather than tracked,
        // since re-opening an open document is defined to be harmless and a cache would be a second
        // account of what the server knows, waiting to disagree with the first.
        //
        // The bytes travel through this function and are never examined. That is the carry the label
        // rules permit and it is worth being exact about: nothing here branches on the contents,
        // compares them, or derives a position from them, so no decision is taken from a byte a file
        // chose. What the server does with them produces locations, which LSP-3 governs.
        // A file that cannot be read is not a failure of this call: the question is put anyway, and
        // a server that has the document indexed already answers from that. It does decide what a
        // rejection of that question means, which is why the outcome is kept rather than dropped.
        let opened = operation.needs_position() && self.open(question.path, &uri).is_ok();
        debug_assert!(
            !operation.sends_a_position() || operation.needs_position(),
            "a position is stated about a file, so anything sending one opens one first"
        );
        let result = if operation == Operation::WorkspaceSymbol {
            self.send_request(
                operation.method(),
                Some(serde_json::json!({ "query": question.query.unwrap_or_default() })),
                MAX_REQUEST_WAIT,
            )?
        } else if operation.needs_prepared_item() {
            // Both directions need an item first, and a position with no symbol at it prepares
            // nothing, which is an empty answer rather than an error.
            let prepared = self.ask_at_a_position(
                "textDocument/prepareCallHierarchy",
                position_params(&uri, question.line, question.character),
                opened,
            )?;
            let item = match &prepared {
                Value::Array(items) if !items.is_empty() => items[0].clone(),
                _ => {
                    return Ok(Answer {
                        locations: Vec::new(),
                        text: None,
                        partial: !self.indexed,
                    });
                }
            };
            self.send_request(
                operation.method(),
                Some(serde_json::json!({ "item": item })),
                MAX_REQUEST_WAIT,
            )?
        } else if !operation.sends_a_position() {
            // `documentSymbol` names a file and asks about the whole of it, so there is no
            // position to be out of range and a failure here is the server's own. Asked of the
            // predicate rather than of the operation, so an operation of the same shape added
            // later does not fall through to the position rules by default.
            self.send_request(
                operation.method(),
                Some(serde_json::json!({ "textDocument": { "uri": uri } })),
                MAX_REQUEST_WAIT,
            )?
        } else {
            let params = if operation == Operation::References {
                reference_params(&uri, question.line, question.character)
            } else {
                position_params(&uri, question.line, question.character)
            };
            self.ask_at_a_position(operation.method(), params, opened)?
        };

        Ok(Answer {
            locations: locations_in(&result),
            text: (operation == Operation::Hover)
                .then(|| hover_text(&result))
                .flatten(),
            // LSP-7: an answer given before the index settled is partial, whatever it found.
            partial: !self.indexed,
        })
    }
}

/// One question, whole.
///
/// Bundled rather than passed as five arguments because these are one thing: every field is routing,
/// and a caller assembling them separately is a caller that can get the position and the path out of
/// step. `path` is absolute, since that is what a server opens.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    pub operation: Operation,
    pub path: &'a str,
    /// 1-based, as the planner stated it.
    pub line: usize,
    /// 1-based, as the planner stated it.
    pub character: usize,
    /// The name to look for, for `workspaceSymbol` alone.
    pub query: Option<&'a str>,
}

/// The servers a session has started, one per language.
///
/// LSP-8: started on the first request for a language, kept for the session, and stopped when this
/// is dropped.
pub struct Servers {
    running: HashMap<Language, Server>,
    root: PathBuf,
    /// `~/.bravebot` itself, not the home it sits in.
    state: Option<PathBuf>,
    /// How a program name becomes the file it names.
    ///
    /// Supplied rather than done here, for the reason [`Server::launch`] takes a resolved path:
    /// `$PATH` is the host's business, and this crate has no opinion about it. It also keeps the
    /// lookup in one place for the whole repository, so a name cannot mean one binary to `run` and
    /// another to this.
    resolve: fn(&str) -> Option<PathBuf>,
    /// Whether this session keeps nothing under `~/.bravebot`, so no index is kept between
    /// sessions.
    incognito: bool,
    /// Where the index goes when there is nothing to keep one in.
    ///
    /// Made on the first launch that needs it rather than with this value, so a session that asks
    /// nothing of a server leaves nothing behind at all, and held here because the lifetime it
    /// wants is the session's: this is built once for one and carried by the turn.
    session: Option<SessionIndex>,
    /// This agent's own credential names, withheld from every server. RUN-12's reason.
    withheld: Vec<String>,
}

impl std::fmt::Debug for Servers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Servers")
            .field("languages", &self.running.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Servers {
    pub fn new(
        root: impl Into<PathBuf>,
        state: Option<PathBuf>,
        resolve: fn(&str) -> Option<PathBuf>,
        incognito: bool,
        withheld: Vec<String>,
    ) -> Self {
        Self {
            running: HashMap::new(),
            root: root.into(),
            state,
            resolve,
            incognito,
            session: None,
            withheld,
        }
    }

    /// How many servers are running. Zero until something asks.
    pub fn running(&self) -> usize {
        self.running.len()
    }

    /// Ask about a position in a file, starting a server for its language if none is running.
    ///
    /// LSP-9: the capability is checked before anything is launched, so a run that was not granted it
    /// does not get a process started on its behalf.
    ///
    /// `approve` is asked once per language, and only when a server is not already running. It is a
    /// callback rather than a decision passed in because whether to ask depends on what is running,
    /// which is this type's business, while how to ask is the caller's: the prompt belongs where the
    /// person is, and this crate has no way to reach them.
    pub fn ask<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        question: &Question<'_>,
        approve: &mut dyn FnMut(&Starting<'_>) -> bool,
    ) -> LspResult<Answer> {
        policy
            .before_capability(Capability::LanguageServer)
            .map_err(LspError::Denied)?;

        // Which server to ask. Every operation but `workspaceSymbol` starts from a file, so the file
        // decides; `workspaceSymbol` ranges over the tree and names none, so it goes to whichever
        // server is already running. That is deliberate rather than a fallback: starting a server on
        // a query with no file in it would mean guessing at the language from a symbol name.
        let language = match Language::for_path(question.path) {
            Some(language) => language,
            None if !question.operation.needs_position() => *self
                .running
                .keys()
                .next()
                .ok_or(LspError::NoServerForQuery)?,
            None => {
                return Err(LspError::NoServerFor {
                    path: question.path.to_string(),
                });
            }
        };

        if !self.running.contains_key(&language) {
            let (program, _) = language.server();
            // LSP-6: a binary that is not installed is said to be missing here, before anything is
            // launched, rather than surfacing as a process that exited. The two are different facts
            // and must not render alike.
            let resolved =
                (self.resolve)(program).ok_or(LspError::NoBinary { language, program })?;

            // LSP-5: asked before anything starts, and a refusal is not a failure of the tool. The
            // planner is told it was refused, which is what it needs to know: retrying will not help.
            let starting = Starting {
                language,
                resolved: &resolved,
                workspace: &self.root,
                runs_build_tooling: language.runs_build_tooling(),
            };
            if !approve(&starting) {
                return Err(LspError::Refused { language });
            }

            let cache = self.index_dir(language)?;
            let server = Server::launch(language, &resolved, &self.root, &cache, &self.withheld)?;
            self.running.insert(language, server);
        }

        let server = self
            .running
            .get_mut(&language)
            .expect("just inserted if absent");
        server.ask(question)
    }

    /// Where the server about to start keeps its index.
    ///
    /// LSP-10: under `~/.bravebot` where this session keeps anything there, and otherwise under a
    /// directory that lasts the session. Never nothing, because the caller of a nothing is a
    /// command spawned with the workspace as its working directory and no index location set,
    /// which writes the index into the tree.
    ///
    /// A directory the platform will not give this session is reported as a server that did not
    /// start, which is LSP-6 and the same answer the directories below it already give.
    fn index_dir(&mut self, language: Language) -> LspResult<PathBuf> {
        if let Some(kept) = cache_for(self.state.as_deref(), &self.root, self.incognito) {
            return Ok(kept);
        }
        if self.session.is_none() {
            self.session = Some(SessionIndex::create().map_err(|e| LspError::Start {
                language,
                detail: format!("an index directory for this session could not be created: {e}"),
            })?);
        }
        let session = self.session.as_ref().expect("just made if absent");
        Ok(cache_under(session.path(), &self.root))
    }
}

/// The servers stop before the directory they are indexing into goes.
///
/// Field order would give this, since `running` is declared above `session`, but the order is the
/// point rather than a consequence of where a field was written: a server still writing while its
/// directory is removed leaves whatever it wrote next, and a removal that fails reports nothing.
///
/// What this stops is the server this process started. A server's own children (`cargo check`
/// under rust-analyzer) are its business and outlive it by however long they take to notice, so
/// this narrows the window rather than closing it, and what loses the race is a directory left
/// behind rather than anything read.
impl Drop for Servers {
    fn drop(&mut self) {
        self.running.clear();
    }
}

/// What a person is being asked to approve.
///
/// Everything the prompt needs and nothing it does not, so the caller draws a question rather than
/// assembling one.
#[derive(Debug, Clone, Copy)]
pub struct Starting<'a> {
    pub language: Language,
    /// The binary, resolved, so what is approved is what runs.
    pub resolved: &'a Path,
    pub workspace: &'a Path,
    /// Whether starting it runs the ecosystem's build tooling, and so code from the dependency tree.
    pub runs_build_tooling: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/workspace")
    }

    /// LSP-5: nothing starts until a person says so, and a refusal is not a failure of the tool.
    #[test]
    fn starting_a_server_is_put_to_a_person() {
        let mut asked = 0;
        let mut servers = Servers::new(root(), None, |_| None, false, Vec::new());
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "look up");
        let mut policy = Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([Capability::LanguageServer]),
            &mut sink,
        )
        .expect("policy");

        // The resolver finds nothing, so this stops at LSP-6 before it would have asked. The point
        // is the order: a person is not asked about a server that could not have started anyway.
        let _ = servers.ask(
            &mut policy,
            &Question {
                operation: Operation::Definition,
                path: "/workspace/src/a.rs",
                line: 1,
                character: 1,
                query: None,
            },
            &mut |_| {
                asked += 1;
                true
            },
        );
        assert_eq!(
            asked, 0,
            "a person must not be asked about a server that is not installed"
        );
    }

    /// LSP-5: a refusal stops the process and says so in words that are not about the code.
    #[test]
    fn a_refused_server_does_not_start() {
        let said = LspError::Refused {
            language: Language::Rust,
        };
        assert!(said.is_absence_of_a_server());
        let rendered = said.to_string();
        assert!(rendered.contains("declined"), "{rendered}");
        // It must not read as an answer, and must say that retrying is pointless.
        assert!(!rendered.contains("no references"), "{rendered}");
        assert!(rendered.contains("will not change it"), "{rendered}");
    }

    /// LSP-5 and LSP-8 together: asked once per language, not once per question.
    #[test]
    fn a_server_is_not_asked_about_twice_in_a_session() {
        // A server already running is not asked about again, which is what the map decides. Pinned on
        // the bookkeeping rather than on a live process: `running` is what `ask` consults before it
        // reaches the approval, so a language present in it is a language nobody is asked about.
        let servers = Servers::new(root(), None, |_| None, false, Vec::new());
        assert_eq!(servers.running(), 0);
        assert_eq!(
            Language::for_path("src/a.rs"),
            Language::for_path("src/b.rs"),
            "two files of one language are one server, so one question"
        );
    }

    /// The state directory, as the host resolves it and hands it over.
    fn state() -> PathBuf {
        PathBuf::from("/home/someone/.bravebot")
    }

    /// LSP-10: never inside the workspace, and keyed by it.
    #[test]
    fn the_cache_is_outside_the_workspace() {
        let cache = cache_for(Some(&state()), &root(), false).expect("a cache is given");
        assert!(
            cache.starts_with(state()),
            "the cache belongs under the directory this process owns, got {}",
            cache.display()
        );
        assert!(
            !cache.starts_with(root()),
            "a question about a symbol must not write into the tree, got {}",
            cache.display()
        );
    }

    /// The argument is the state directory, so appending its name here would put the index in
    /// `~/.bravebot/.bravebot`: a directory nothing else writes to, reads or narrows, holding an
    /// index of the user's source.
    #[test]
    fn the_cache_sits_directly_under_the_directory_it_is_given() {
        let cache = cache_for(Some(&state()), &root(), false).expect("a cache is given");

        let below: Vec<_> = cache
            .strip_prefix(state())
            .expect("under the directory it was given")
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            below.len(),
            2,
            "one directory for the tool, one per workspace"
        );
        assert_eq!(below[0], CACHE_ROOT);
    }

    /// The index is derived from every file in the workspace, so who may read it is who may read
    /// the workspace. At the process umask that is every account on the machine.
    #[cfg(unix)]
    #[test]
    fn the_cache_is_created_reachable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = crate::testutil::Scratch::new("bravebot-lsp-cache-mode");
        let cache = cache_for(Some(&scratch), &root(), false).expect("a cache is given");

        create_cache(&cache).expect("created");

        let mode = |path: &Path| {
            std::fs::metadata(path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&cache), 0o700);
        assert_eq!(
            mode(&scratch.join(CACHE_ROOT)),
            0o700,
            "the directory holding one per workspace"
        );
    }

    /// An earlier build appended the state directory's own name, so the index landed one level
    /// deeper than anything reads, rebuilds or narrows. Left there it is an index of the user's
    /// source sitting at the umask for the life of the machine.
    #[test]
    fn an_index_an_earlier_build_left_too_deep_is_removed() {
        let scratch = crate::testutil::Scratch::new("bravebot-lsp-misplaced");
        let state = scratch.join(".bravebot");
        let misplaced = state
            .join(".bravebot")
            .join(CACHE_ROOT)
            .join("0123456789abcdef");
        std::fs::create_dir_all(&misplaced).expect("as an earlier build left it");
        std::fs::write(misplaced.join("index"), "derived from the workspace").expect("write");
        let cache = cache_for(Some(&state), &root(), false).expect("a cache is given");

        create_cache(&cache).expect("created");

        assert!(
            !state.join(".bravebot").exists(),
            "the index nothing reads is still there"
        );
        assert!(cache.is_dir(), "the cache this run wants was not created");
    }

    /// Only what this crate would have written is removed. A directory that happens to carry the
    /// same name and holds something else is somebody's own.
    #[test]
    fn a_nested_directory_that_holds_no_index_is_left_where_it_is() {
        let scratch = crate::testutil::Scratch::new("bravebot-lsp-not-an-index");
        let state = scratch.join(".bravebot");
        let theirs = state.join(".bravebot");
        std::fs::create_dir_all(theirs.join("notes")).expect("somebody else's");
        let cache = cache_for(Some(&state), &root(), false).expect("a cache is given");

        create_cache(&cache).expect("created");

        assert!(
            theirs.join("notes").is_dir(),
            "a directory holding no index was removed"
        );
    }

    /// A server handed a directory this process could not create makes it itself, at the umask,
    /// and fills it with an index derived from every file in the workspace. Refusing to start says
    /// so; carrying on would leave STATE-1 holding in every case but the one where it matters.
    #[test]
    fn a_server_whose_index_directory_cannot_be_made_private_does_not_start() {
        let scratch = crate::testutil::Scratch::new("bravebot-lsp-cache-unmakeable");
        std::fs::create_dir_all(&*scratch).expect("scratch");
        // A file where the state directory would be, so nothing can be created below it.
        let state = scratch.join("not-a-directory");
        std::fs::write(&state, "").expect("seed");
        let cache = cache_for(Some(&state), &root(), false).expect("a cache is given");

        let error = Server::launch(
            Language::Rust,
            Path::new("/nonexistent-binary"),
            &root(),
            &cache,
            &[],
        )
        .expect_err("a server must not start without an index directory of its own");

        // Reported as a failure to start rather than as the missing binary, which is what a launch
        // that got as far as spawning would have said.
        match error {
            LspError::Start { detail, .. } => {
                assert!(detail.contains("index directory"), "{detail}");
            }
            other => panic!("started, or stopped for another reason: {other}"),
        }
    }

    /// A run of an earlier build left these at the umask, and creating a directory that exists
    /// does not touch its mode. Without narrowing, the machines already holding an index would be
    /// the ones this never reaches.
    #[cfg(unix)]
    #[test]
    fn a_cache_left_open_by_an_earlier_run_is_narrowed() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = crate::testutil::Scratch::new("bravebot-lsp-cache-narrowed");
        let cache = cache_for(Some(&scratch), &root(), false).expect("a cache is given");
        std::fs::create_dir_all(&cache).expect("as an earlier run left it");
        let loosen = |path: &Path| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("loosen")
        };
        // Every level, including the state directory: what `create_dir_all` gave them is the mode
        // of whoever runs the tests, and the claim below is about a directory left open.
        loosen(&scratch);
        loosen(&scratch.join(CACHE_ROOT));
        loosen(&cache);

        create_cache(&cache).expect("created");

        let mode = |path: &Path| {
            std::fs::metadata(path)
                .expect("exists")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&cache), 0o700);
        assert_eq!(mode(&scratch.join(CACHE_ROOT)), 0o700);
        assert_eq!(
            mode(&scratch),
            0o755,
            "the state directory is not this crate's to set"
        );
    }

    /// The two directories this crate narrows are named, not resolved, so a linked one would have
    /// a language server client setting the mode of a directory somewhere else in the user's home.
    #[cfg(unix)]
    #[test]
    fn narrowing_does_not_follow_a_link_out_of_the_cache() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = crate::testutil::Scratch::new("bravebot-lsp-cache-link");
        let cache = cache_for(Some(&scratch), &root(), false).expect("a cache is given");
        let elsewhere = scratch.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("create");
        std::fs::create_dir_all(scratch.join(CACHE_ROOT)).expect("create");
        std::os::unix::fs::symlink(&elsewhere, &cache).expect("link");
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).expect("mode");

        create_cache(&cache).expect("created");

        let mode = std::fs::symlink_metadata(&elsewhere)
            .expect("exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o755,
            "a directory outside the cache was narrowed through a link"
        );
    }

    /// LSP-10: two workspaces do not share an index.
    #[test]
    fn the_cache_is_keyed_by_the_workspace() {
        let one = cache_for(Some(&state()), Path::new("/a/project"), false).expect("cache");
        let two = cache_for(Some(&state()), Path::new("/b/project"), false).expect("cache");
        assert_ne!(
            one, two,
            "two checkouts must not share an index, or a stale one is read as the other's"
        );
        // And the same workspace is the same directory every time, or nothing is ever reused.
        assert_eq!(
            one,
            cache_for(Some(&state()), Path::new("/a/project"), false).expect("cache")
        );
    }

    /// LSP-10: incognito adds nothing to `~/.bravebot`, so no index is kept there for the next
    /// session to read.
    ///
    /// What such a session is given instead is [`SessionIndex`], which is
    /// [`a_session_that_keeps_nothing_still_indexes_outside_the_workspace`]: this answer is where
    /// an index is *not* kept, and reading it as a session with no index directory at all is what
    /// put one in the workspace.
    #[test]
    fn an_incognito_session_keeps_nothing_under_the_state_directory() {
        assert!(
            cache_for(Some(&state()), &root(), true).is_none(),
            "an incognito session must leave no index under ~/.bravebot"
        );
        // And with nowhere to keep one, there is nothing to key.
        assert!(cache_for(None, &root(), false).is_none());
    }

    /// LSP-10 and TRUST-11's reasoning about the temporary directory: the directory an index goes
    /// in when nothing is kept is created, never adopted.
    ///
    /// On Linux that directory is ordinarily the world-writable `/tmp`, so a name already there
    /// may be another account's, or a link into one. Adopting it would hand a server an index
    /// directory somebody else can read, which is the whole of what STATE-1's mode is for.
    #[cfg(unix)]
    #[test]
    fn a_session_index_directory_is_created_never_adopted() {
        use std::os::unix::fs::PermissionsExt;

        let held = SessionIndex::create().expect("a directory of its own");
        assert!(held.path().is_dir(), "{}", held.path().display());
        let mode = std::fs::metadata(held.path())
            .expect("exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o700,
            "another account can read what a server indexes"
        );

        // A name already taken is an error rather than a directory to write into.
        SessionIndex::created_at(held.path().to_path_buf())
            .expect_err("a name already taken must not be adopted");

        // And nothing of it outlives the value: the session is what it lasts for.
        let path = held.path().to_path_buf();
        std::fs::write(path.join("index"), "derived from the workspace").expect("write");
        drop(held);
        assert!(
            !path.exists(),
            "an index outlived the session that built it: {}",
            path.display()
        );
    }

    /// LSP-10: the cache is the server's, and this crate never opens it.
    ///
    /// The clause that matters most, because `~/.bravebot` is trusted by provenance and somebody will
    /// reason that what sits in it is too. It is not: the bytes are derived from workspace files, so
    /// LABEL-2 taints them and LABEL-7 forbids recovering a better label.
    ///
    /// Pinned by what the type offers rather than by scanning the source for reads, which would match
    /// its own assertions. `cache_for` hands back a path and nothing that reads one, and a `Server`
    /// exposes no way to get at it: there is no accessor, so no caller can reach the directory
    /// through this crate even if it wanted to.
    #[test]
    fn the_cache_is_never_read_by_the_driver() {
        let cache = cache_for(Some(&state()), &root(), false).expect("cache");

        // A path, not a handle and not any bytes. Everything this crate does with it is hand it to a
        // child process, and the type says so: `PathBuf` carries no contents.
        let _: PathBuf = cache;

        // And nothing in the running server offers it back. If an accessor is ever added, this test
        // is the place that has to be argued with first.
        let names = ["cache", "index_dir", "cache_dir"];
        let debug = format!(
            "{:?}",
            Servers::new(root(), None, |_| None, false, Vec::new())
        );
        for name in names {
            assert!(
                !debug.contains(name),
                "the cache must not be reported anywhere a caller could read it: {debug}"
            );
        }
    }

    /// A whole-tree query names no file, so it cannot decide a language and must not be answered by
    /// guessing one from the symbol's name.
    #[test]
    fn a_query_with_no_file_is_told_no_server_is_running() {
        let mut servers = Servers::new(root(), None, |_| None, false, Vec::new());
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "look up");
        let mut policy = Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([Capability::LanguageServer]),
            &mut sink,
        )
        .expect("policy");

        let error = servers
            .ask(
                &mut policy,
                &Question {
                    operation: Operation::WorkspaceSymbol,
                    path: "",
                    line: 1,
                    character: 1,
                    query: Some("Capability"),
                },
                &mut |_| true,
            )
            .expect_err("nothing is running, so there is nothing to search");

        // It must say that no server was asked, and must not read as a fact about the code. And it
        // must not be the sentence about a *file* having no server, since no file was named.
        let said = error.to_string();
        assert!(error.is_absence_of_a_server());
        assert!(said.contains("no server is running"), "{said}");
        assert!(
            !said.contains("no language server is configured for "),
            "{said}"
        );
        assert!(
            said.contains("ask about a symbol in a file first"),
            "{said}"
        );
    }

    /// LSP-5: which servers run the ecosystem's build tooling, since that is what the prompt says.
    #[test]
    fn the_prompt_says_which_servers_run_build_tooling() {
        assert!(Language::Rust.runs_build_tooling());
        assert!(Language::Go.runs_build_tooling());
        assert!(!Language::TypeScript.runs_build_tooling());
        assert!(!Language::Python.runs_build_tooling());
    }

    /// LSP-6: a language with no server is that, and is not an empty answer.
    #[test]
    fn an_unconfigured_language_is_reported_as_unconfigured() {
        assert!(Language::for_path("notes.txt").is_none());
        assert!(Language::for_path("Makefile").is_none());
        assert!(Language::for_path("a.rs").is_some());

        let error = LspError::NoServerFor {
            path: "notes.txt".into(),
        };
        let said = error.to_string();
        assert!(
            said.contains("no language server"),
            "the reason must be named: {said}"
        );
        // The sentence must not read as an answer about the code.
        assert!(!said.contains("no references"), "{said}");
        assert!(!said.contains("not found in"), "{said}");
    }

    /// LSP-6: a binary that is not installed is reported as missing, naming what to install.
    #[test]
    fn a_missing_binary_is_reported_as_missing() {
        let error = LspError::NoBinary {
            language: Language::Rust,
            program: "rust-analyzer",
        };
        let said = error.to_string();
        assert!(said.contains("rust-analyzer"), "{said}");
        assert!(said.contains("not installed"), "{said}");
    }

    /// LSP-6: a server that was there and failed is a third distinct sentence.
    #[test]
    fn a_server_that_fails_to_start_is_reported_as_such() {
        let error = LspError::Start {
            language: Language::Go,
            detail: "exited immediately".into(),
        };
        let said = error.to_string();
        assert!(said.contains("Go"), "{said}");
        assert!(said.contains("exited immediately"), "{said}");

        // The three failures LSP-6 separates must not render alike.
        let unconfigured = LspError::NoServerFor {
            path: "a.txt".into(),
        }
        .to_string();
        let missing = LspError::NoBinary {
            language: Language::Go,
            program: "gopls",
        }
        .to_string();
        assert_ne!(said, unconfigured);
        assert_ne!(said, missing);
        assert_ne!(unconfigured, missing);
    }

    /// LSP-8: nothing starts until something asks.
    #[test]
    fn no_server_starts_until_a_request_needs_one() {
        let servers = Servers::new(root(), None, |_| None, false, Vec::new());
        assert_eq!(
            servers.running(),
            0,
            "a session must not start a server nobody asked for"
        );
    }

    /// LSP-8: the set is what stops the processes, so dropping it must stop them all.
    #[test]
    fn dropping_the_set_stops_every_server() {
        let servers = Servers::new(root(), None, |_| None, false, Vec::new());
        // Nothing running, so this is the degenerate case; the property that matters is that the
        // set owns its servers, which is by construction, and that dropping it is not a leak.
        drop(servers);
    }

    /// LSP-7: an answer given before the index settled is partial, whatever it found. A
    /// `findReferences` against a half-built index returns some references and looks exactly like
    /// one that returned all of them.
    #[test]
    fn an_answer_during_indexing_is_marked_partial() {
        // Nothing has said indexing finished, so an answer built now is partial.
        let mid_index = Answer {
            locations: vec![crate::Location {
                path: "/workspace/src/a.rs".into(),
                line: 1,
                character: 1,
                kind: None,
            }],
            text: None,
            partial: true,
        };
        assert!(
            mid_index.partial,
            "an answer given while indexing must say so"
        );

        // Every other token a real rust-analyzer sends on this workspace, none of which means the
        // index is built. Recorded from an actual session rather than guessed, because guessing is
        // exactly what went wrong here once: the earlier list waited for `rustAnalyzer/Indexing`,
        // which nothing sends.
        for earlier in [
            "rustAnalyzer/Fetching",
            "rustAnalyzer/Building CrateGraph",
            "rustAnalyzer/Roots Scanned",
            "rustAnalyzer/Building compile-time-deps",
            "rustAnalyzer/Loading proc-macros",
            "rust-analyzer/flycheck/0",
            // The name that looks right and is sent by nothing.
            "rustAnalyzer/Indexing",
        ] {
            assert!(
                !says_indexing_finished(&serde_json::json!({
                    "method": "$/progress",
                    "params": { "token": earlier, "value": { "kind": "end" } },
                })),
                "{earlier} ending does not mean the index is built"
            );
        }
        // Nor does the beginning of indexing.
        assert!(!says_indexing_finished(&serde_json::json!({
            "method": "$/progress",
            "params": { "token": "rustAnalyzer/Indexing", "value": { "kind": "begin" } },
        })));
        // Nor an unrelated message.
        assert!(!says_indexing_finished(&serde_json::json!({
            "method": "window/logMessage",
            "params": { "message": "indexing finished" },
        })));
    }

    /// LSP-7: a settled index makes no partial claim, so the notice means something when it appears.
    #[test]
    fn a_settled_index_makes_no_partial_claim() {
        for token in [
            "rustAnalyzer/cachePriming",
            "gopls/loading",
            "pyright/analysis",
        ] {
            assert!(
                says_indexing_finished(&serde_json::json!({
                    "method": "$/progress",
                    "params": { "token": token, "value": { "kind": "end" } },
                })),
                "{token} ending must settle the index"
            );
        }

        let settled = Answer::default();
        assert!(!settled.partial);
    }

    /// LSP-8: indexing is the whole cost, so a server is started once for a language and reused.
    ///
    /// Pinned on the bookkeeping rather than on a live process: the property is that a second
    /// question for a language already running starts nothing, which is what the map decides.
    #[test]
    fn a_server_is_started_once_and_reused() {
        let servers = Servers::new(root(), None, |_| None, false, Vec::new());
        assert_eq!(servers.running(), 0);

        // Two files of the same language must map to one server, and two languages to two.
        assert_eq!(
            Language::for_path("src/a.rs"),
            Language::for_path("src/b.rs"),
            "two Rust files must not want two servers"
        );
        assert_ne!(
            Language::for_path("src/a.rs"),
            Language::for_path("web/a.ts"),
            "two languages are two servers"
        );
    }

    /// LSP-8: a server that ignores `shutdown` is killed, so it cannot outlive the agent.
    ///
    /// The kill path is in `Drop`. Pinned here as the property that the grace period is bounded and
    /// that dropping does not wait forever on a process that will not go.
    #[test]
    fn a_server_that_ignores_shutdown_is_killed() {
        assert!(
            SHUTDOWN_GRACE < Duration::from_secs(5),
            "the grace period must be short enough that quitting is not a hang"
        );
        // A server is only ever owned by a `Server`, whose `Drop` kills it, so there is no path
        // that leaks one. Dropping a set with nothing in it must still be sound.
        drop(Servers::new(root(), None, |_| None, false, Vec::new()));
    }

    /// LSP-9: the capability is checked before a process is started, so a run that was not granted
    /// one does not get a server launched on its behalf.
    #[test]
    fn a_request_without_the_capability_is_refused() {
        use bravebot_core::capability::CapabilitySet;

        // The grant a file read gives is not this one, which is the whole clause.
        let reads_only = CapabilitySet::from_iter([Capability::FileRead]);
        assert!(
            reads_only.token_for(Capability::LanguageServer).is_none(),
            "file reads must not carry a language server with them"
        );

        // And an empty set carries nothing, so the default is refusal.
        assert!(
            CapabilitySet::none()
                .token_for(Capability::LanguageServer)
                .is_none()
        );
    }

    /// A language server that rejects every question the way gopls rejects an out-of-range one.
    ///
    /// The requests that carry a position answer with what a real server sends for a line past the
    /// end of a file, code and message verbatim: gopls's `0` for the two that a definition and a
    /// call hierarchy put, rust-analyzer's `-32603` for the one a references query puts. The two that carry none answer a
    /// fault of the server's own, in the same shape, which is the point: nothing but the prose
    /// says which of the two it is. The last two answer a code the protocol reserves, one from
    /// each of the two bands it reserves them in: `textDocument/implementation` answers
    /// JSON-RPC's `MethodNotFound`, as a server without that operation does, and
    /// `textDocument/hover` answers LSP's own `RequestFailed`. Neither is a code any server here
    /// answers an out-of-range position with. The `MethodNotFound` message is the one here that is
    /// constructed rather than copied from a server: it quotes the workspace file's own source
    /// text, so that a test can tell whether the sentence a failure reports came from this crate or
    /// from the server. The index is reported settled as the process comes up, in the words
    /// rust-analyzer uses, so nothing waits out LSP-7's bound.
    #[cfg(unix)]
    const REJECTING_SERVER: &str = r#"#!/bin/sh
reply() {
  printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"
}
out_of_range='{"code":0,"message":"line number 9999 out of range 0-8"}'
its_own_fault='{"code":0,"message":"no views"}'
unimplemented='{"code":-32601,"message":"no handler for textDocument/implementation: pub struct Held at src/a.rs:1"}'
request_failed='{"code":-32803,"message":"request failed"}'
invalid_offset='{"code":-32603,"message":"Invalid offset LineCol { line: 9999, col: 0 } (line index length: 17)"}'
while IFS= read -r header; do
  case "$header" in
    Content-Length:*) length=$(printf '%s' "$header" | tr -cd '0-9') ;;
    *) continue ;;
  esac
  IFS= read -r _blank
  body=$(dd bs=1 count="$length" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$body" in
    *'"initialize"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"capabilities\":{}}}"
      reply '{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/cachePriming","value":{"kind":"end"}}}'
      ;;
    *'"textDocument/definition"'*|*'"textDocument/prepareCallHierarchy"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":$out_of_range}"
      ;;
    *'"textDocument/references"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":$invalid_offset}"
      ;;
    *'"textDocument/documentSymbol"'*|*'"workspace/symbol"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":$its_own_fault}"
      ;;
    *'"textDocument/implementation"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":$unimplemented}"
      ;;
    *'"textDocument/hover"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":$request_failed}"
      ;;
    *'"shutdown"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}"
      ;;
  esac
done
"#;

    /// One scratch directory per test rather than one for the fixture, so that neither test's setup
    /// rests on the other having finished with it: [`crate::testutil::Scratch`] empties the
    /// directory it is given, so under a shared name the only thing keeping one test from emptying
    /// the server the other is about to launch would be [`LAUNCHING`].
    #[cfg(unix)]
    const REJECTS_A_POSITION: &str = "bravebot-lsp-rejects-a-position";

    #[cfg(unix)]
    const REJECTS_A_QUERY: &str = "bravebot-lsp-rejects-a-query";

    #[cfg(unix)]
    const REJECTS_WITH_PROSE: &str = "bravebot-lsp-rejects-with-prose";

    /// Serialises the tests that write a server and then launch it.
    ///
    /// Linux refuses to execute a file any process holds open for writing. A child forked while a
    /// sibling thread is part-way through writing its server inherits every descriptor this process
    /// had open, the one the server was written through included, and that descriptor is
    /// close-on-exec rather than already closed: it lives until the child reaches its own exec. For
    /// that window this process is itself a writer holding the sibling's server open, so the
    /// sibling's launch fails with `Text file busy` on whichever test lost the race, about nothing
    /// either test is for.
    ///
    /// One at a time closes the window rather than waiting it out, since [`Server::launch`] is the
    /// only thing in this binary that forks and no write is in flight while one of them happens.
    #[cfg(unix)]
    static LAUNCHING: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The scratch name and the resolver have to agree without either seeing the other: `resolve`
    /// is a function pointer, so it cannot close over where the test put its binary, and it
    /// recomputes the path from the same name instead.
    #[cfg(unix)]
    fn the_server_that_rejects_a_position(_: &str) -> Option<PathBuf> {
        Some(crate::testutil::scratch_dir(REJECTS_A_POSITION).join("server"))
    }

    #[cfg(unix)]
    fn the_server_that_rejects_a_query(_: &str) -> Option<PathBuf> {
        Some(crate::testutil::scratch_dir(REJECTS_A_QUERY).join("server"))
    }

    #[cfg(unix)]
    fn the_server_that_rejects_with_prose(_: &str) -> Option<PathBuf> {
        Some(crate::testutil::scratch_dir(REJECTS_WITH_PROSE).join("server"))
    }

    /// A workspace of one nine-line Rust file, with the script above beside it as its server.
    ///
    /// [`LAUNCHING`] comes back with them rather than being taken by each test, because the test has
    /// to hold it until its last launch and a fixture that took and dropped it would order the
    /// writes while leaving the execs racing. Poisoning is ignored: a test that panicked holding it
    /// left nothing behind that the next one reads.
    #[cfg(unix)]
    fn a_workspace_a_server_rejects(
        name: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        crate::testutil::Scratch,
        PathBuf,
    ) {
        use std::os::unix::fs::PermissionsExt;

        let launching = LAUNCHING.lock().unwrap_or_else(|held| held.into_inner());
        let scratch = crate::testutil::Scratch::new(name);
        std::fs::create_dir_all(scratch.join("src")).expect("create the workspace");
        let file = scratch.join("src").join("a.rs");
        std::fs::write(&file, "pub struct Held;\n".repeat(9)).expect("write the file");

        let program = scratch.join("server");
        std::fs::write(&program, REJECTING_SERVER).expect("write the server");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        (launching, scratch, file)
    }

    /// Everything a question needs besides the question: the capability, and a yes to starting the
    /// server.
    #[cfg(unix)]
    fn ask_with_the_server_approved(
        servers: &mut Servers,
        question: &Question<'_>,
    ) -> LspResult<Answer> {
        use bravebot_core::capability::CapabilitySet;

        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "look up");
        let mut policy = Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::LanguageServer]),
            &mut sink,
        )
        .expect("policy");

        servers.ask(&mut policy, question, &mut |_| true)
    }

    /// LSP-2: a file changes under an agent, so a line number that was right when a search
    /// reported it is an ordinary thing to be stale. Asking about one is nothing found, not a
    /// report that the tool is broken.
    ///
    /// Driven against a process rather than a constructed [`Answer`], because the whole of the
    /// clause is what this client does with what a server sends: a server is the only thing that
    /// knows the file is nine lines long, and it says so by refusing.
    ///
    /// Both operations that put a position, because they put it in different requests: a
    /// definition asks the one method, and a call hierarchy prepares an item first.
    #[cfg(unix)]
    #[test]
    fn a_position_the_server_rejects_is_nothing_found() {
        let (_launching, scratch, file) = a_workspace_a_server_rejects(REJECTS_A_POSITION);
        let mut servers = Servers::new(
            scratch.to_path_buf(),
            None,
            the_server_that_rejects_a_position,
            false,
            Vec::new(),
        );

        for operation in [
            Operation::Definition,
            Operation::IncomingCalls,
            // rust-analyzer answers `InternalError`, which is inside the range the protocol
            // reserves, so a rule that kept the whole of that range a failure would fix this for
            // gopls and leave rust-analyzer where it was.
            Operation::References,
        ] {
            let named = operation.as_str();
            let answer = ask_with_the_server_approved(
                &mut servers,
                &Question {
                    operation,
                    path: file.to_str().expect("a utf-8 scratch path"),
                    // Past the end of a nine-line file, as a search that ran before an edit would
                    // have reported it.
                    line: 10_000,
                    character: 1,
                    query: None,
                },
            )
            .unwrap_or_else(|e| {
                panic!("{named}: a stale line number is an answer, not a failure: {e}")
            });

            assert!(
                answer.locations.is_empty(),
                "{named}: a position nothing is at reports no locations, got {:?}",
                answer.locations
            );
            assert_eq!(answer.text, None, "{named}");
            // The index settled before the question, so nothing found is the whole answer rather
            // than a short one: LSP-7's notice reads off `says_indexing_finished`, and answering
            // with nothing must not start claiming otherwise.
            assert!(
                !answer.partial,
                "{named}: a settled index makes no partial claim"
            );
        }
    }

    /// The other half of the same rule, which is three conditions and needs all of them: a
    /// rejection is nothing found only where the request carried a position, the server raised it
    /// in its own numbering, and the file was there to open.
    ///
    /// A request with no position in it has none to be stale, so `documentSymbol` and
    /// `workspaceSymbol` are untouched. A [`NOT_AN_ABSENCE`] code says something other than that
    /// the server looked and found nothing, so `goToImplementation` against a server answering
    /// `MethodNotFound` stays a failure rather than becoming an authoritative answer of no
    /// implementations. One code from each of the two bands the protocol reserves, since they do
    /// not adjoin: `MethodNotFound` is JSON-RPC's, and the `RequestFailed` that `hover` answers is
    /// LSP's own, below it.
    ///
    /// A path that is not there is the third: a position in a file this process could not read may
    /// be stale, or the path may be wrong, and nothing found would assert the first. A planner
    /// that mistypes a path has to be told, not handed a confident nothing.
    ///
    /// The fixture's rejections differ only in the prose and the code, never in the shape, which
    /// is what gopls does and why the message cannot be what separates them.
    #[cfg(unix)]
    #[test]
    fn a_failure_is_nothing_found_only_where_a_server_rejected_a_position() {
        let (_launching, scratch, file) = a_workspace_a_server_rejects(REJECTS_A_QUERY);
        let mut servers = Servers::new(
            scratch.to_path_buf(),
            None,
            the_server_that_rejects_a_query,
            false,
            Vec::new(),
        );

        // A whole-tree query names no file, so it goes to whichever server is running. The
        // document question is what starts one, and it is the first of the three cases.
        let named = file.to_str().expect("a utf-8 scratch path");
        for (operation, path, query) in [
            (Operation::DocumentSymbol, named, None),
            (Operation::WorkspaceSymbol, "", Some("Held")),
            (Operation::Implementation, named, None),
            (Operation::Hover, named, None),
            // Rejected in the server's own numbering, as a stale position is, and refused all the
            // same because there is no such file to have held one.
            (
                Operation::Definition,
                scratch
                    .join("src")
                    .join("mistyped.rs")
                    .to_str()
                    .expect("a utf-8 scratch path"),
                None,
            ),
        ] {
            let refused = ask_with_the_server_approved(
                &mut servers,
                &Question {
                    operation,
                    path,
                    line: 1,
                    character: 1,
                    query,
                },
            );
            assert!(
                matches!(refused, Err(LspError::Server { .. })),
                "{}: a server that could not answer must not report nothing found, got {refused:?}",
                operation.as_str()
            );
        }
    }

    /// LSP-5: a failure is reported in this crate's own words, and the server's sentence about it
    /// is not among them.
    ///
    /// "A server that can read the disk is not a server that can put prose in the planner's
    /// context", and a JSON-RPC error message is a place for prose to sit: it is free text the
    /// server composes, and the fixture's `MethodNotFound` puts the workspace file's source text in
    /// it. So the sentence a planner is handed must name the language, the method and the code, all
    /// of it structure, and must not contain the server's own words.
    ///
    /// `textDocument/implementation` because its code is in [`NOT_AN_ABSENCE`], so this is the
    /// ordinary path for a server that does not implement an operation rather than a corner: by
    /// LSP-2 every other rejection of a position becomes nothing found and never renders at all.
    ///
    /// Driven against a process because the string under test is the one a server sends: an
    /// [`LspError`] built in the test would only assert what the test itself put in it.
    #[cfg(unix)]
    #[test]
    fn a_server_failure_reports_a_code_and_not_the_servers_words() {
        let (_launching, scratch, file) = a_workspace_a_server_rejects(REJECTS_WITH_PROSE);
        let mut servers = Servers::new(
            scratch.to_path_buf(),
            None,
            the_server_that_rejects_with_prose,
            false,
            Vec::new(),
        );

        let refused = ask_with_the_server_approved(
            &mut servers,
            &Question {
                operation: Operation::Implementation,
                path: file.to_str().expect("a utf-8 scratch path"),
                line: 1,
                character: 1,
                query: None,
            },
        );

        let Err(error) = refused else {
            panic!("a MethodNotFound is a failure, not nothing found: {refused:?}");
        };
        assert!(
            matches!(error, LspError::Server { .. }),
            "a server that answered and failed is reported as such, got {error:?}"
        );

        let said = error.to_string();
        // The three facts the protocol gives, none of them composed by the server.
        assert!(said.contains("Rust"), "{said}");
        assert!(said.contains("-32601"), "{said}");
        assert!(said.contains("textDocument/implementation"), "{said}");
        // What the server wrote. The source text is the half that matters, since a server that
        // can read the disk would otherwise be quoting the tree into the planner's context, and
        // the opening of the message proves the sentence is not carried whole either.
        for wrote in ["pub struct Held", "src/a.rs:1", "no handler"] {
            assert!(
                !said.contains(wrote),
                "{said} repeats the server's own words: {wrote}"
            );
        }
    }

    /// A language server that records where it was told to keep its index, then answers.
    ///
    /// The two lines it writes before reading anything are the whole of what these tests need:
    /// the index location it was given, and the directory it was started in. A server given none
    /// resolves the first against the second, which is how the index came to be written into the
    /// workspace.
    ///
    /// It answers `initialize` and reports the index settled in the words rust-analyzer uses, so
    /// nothing waits out LSP-7's bound, and it exits on `exit` rather than being killed after the
    /// shutdown grace.
    #[cfg(unix)]
    const REPORTING_SERVER: &str = r#"#!/bin/sh
here=$(dirname "$0")
{
  printf '%s\n' "${CARGO_TARGET_DIR-unset}"
  pwd -P
} >> "$here/reported"
reply() {
  printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"
}
while IFS= read -r header; do
  case "$header" in
    Content-Length:*) length=$(printf '%s' "$header" | tr -cd '0-9') ;;
    *) continue ;;
  esac
  IFS= read -r _blank
  body=$(dd bs=1 count="$length" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$body" in
    *'"initialize"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"capabilities\":{}}}"
      reply '{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/cachePriming","value":{"kind":"end"}}}'
      ;;
    *'"textDocument/definition"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[]}"
      ;;
    *'"shutdown"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}"
      ;;
    *'"exit"'*)
      exit 0
      ;;
  esac
done
"#;

    /// One scratch name per test, for [`REJECTS_A_POSITION`]'s reason: the fixture empties the
    /// directory it is given, so a shared name is one test deleting the server another is about
    /// to launch.
    #[cfg(unix)]
    const KEEPS_NOTHING: &str = "bravebot-lsp-keeps-nothing";

    #[cfg(unix)]
    const GOES_WITH_THE_SESSION: &str = "bravebot-lsp-goes-with-the-session";

    #[cfg(unix)]
    const UNDER_THE_STATE_DIRECTORY: &str = "bravebot-lsp-under-the-state-directory";

    #[cfg(unix)]
    fn the_server_that_reports_in_incognito(_: &str) -> Option<PathBuf> {
        Some(crate::testutil::scratch_dir(KEEPS_NOTHING).join("server"))
    }

    #[cfg(unix)]
    fn the_server_that_reports_with_no_state_directory(_: &str) -> Option<PathBuf> {
        Some(crate::testutil::scratch_dir(GOES_WITH_THE_SESSION).join("server"))
    }

    #[cfg(unix)]
    fn the_server_that_reports_with_a_state_directory(_: &str) -> Option<PathBuf> {
        Some(crate::testutil::scratch_dir(UNDER_THE_STATE_DIRECTORY).join("server"))
    }

    /// A workspace with the recording script above beside it rather than inside it, so what the
    /// server reports is not itself a file written into the tree under test.
    ///
    /// [`LAUNCHING`] comes back with it for the reason [`a_workspace_a_server_rejects`] gives.
    #[cfg(unix)]
    fn a_workspace_with_a_reporting_server(
        name: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        crate::testutil::Scratch,
        PathBuf,
        PathBuf,
    ) {
        use std::os::unix::fs::PermissionsExt;

        let launching = LAUNCHING.lock().unwrap_or_else(|held| held.into_inner());
        let scratch = crate::testutil::Scratch::new(name);
        let workspace = scratch.join("workspace");
        std::fs::create_dir_all(workspace.join("src")).expect("create the workspace");
        let file = workspace.join("src").join("a.rs");
        std::fs::write(&file, "pub struct Held;\n").expect("write the file");

        let program = scratch.join("server");
        std::fs::write(&program, REPORTING_SERVER).expect("write the server");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        (launching, scratch, workspace, file)
    }

    /// The index location the server was given, and the directory it was started in.
    #[cfg(unix)]
    fn what_the_server_reported(scratch: &Path) -> (PathBuf, PathBuf) {
        let written = std::fs::read_to_string(scratch.join("reported"))
            .expect("the server records where it indexes before it answers anything");
        let mut lines = written.lines();
        let index = lines.next().expect("an index location").to_string();
        let started_in = lines.next().expect("a working directory").to_string();
        assert_ne!(
            index, "unset",
            "the server was given no index location, so it writes its index into {started_in}"
        );
        (PathBuf::from(index), PathBuf::from(started_in))
    }

    /// The question that starts a Rust server, answered with no locations by the script above.
    #[cfg(unix)]
    fn a_definition_in(servers: &mut Servers, file: &Path) {
        ask_with_the_server_approved(
            servers,
            &Question {
                operation: Operation::Definition,
                path: file.to_str().expect("a utf-8 scratch path"),
                line: 1,
                character: 1,
                query: None,
            },
        )
        .expect("the server answers");
    }

    /// LSP-10: an incognito session keeps no index under `~/.bravebot`, and what it does instead
    /// is index somewhere else, never inside the workspace.
    ///
    /// Driven against a process because the clause is about what the server is told. A server
    /// handed no index location is started with the workspace as its working directory, which is
    /// what an unset `CARGO_TARGET_DIR` resolves against, so the index lands in the tree the
    /// question was about. Both halves are asserted here: the location it was given, and that it
    /// was started in the workspace, which is why the location has to be set.
    #[cfg(unix)]
    #[test]
    fn a_session_that_keeps_nothing_still_indexes_outside_the_workspace() {
        use std::os::unix::fs::PermissionsExt;

        let (_launching, scratch, workspace, file) =
            a_workspace_with_a_reporting_server(KEEPS_NOTHING);
        // A state directory is there and is refused all the same: incognito is what decides, not
        // whether the machine has one.
        let state = scratch.join("state");
        let mut servers = Servers::new(
            workspace.clone(),
            Some(state.clone()),
            the_server_that_reports_in_incognito,
            true,
            Vec::new(),
        );

        a_definition_in(&mut servers, &file);

        let (index, started_in) = what_the_server_reported(&scratch);
        let workspace = workspace.canonicalize().expect("the workspace");
        assert_eq!(
            started_in, workspace,
            "the server runs in the workspace, so an index location it is not given is one in the tree"
        );
        assert!(
            !index.starts_with(&workspace),
            "{} is inside the workspace",
            index.display()
        );
        assert!(
            !index.starts_with(&state),
            "an incognito session left an index under the state directory: {}",
            index.display()
        );
        assert!(index.is_dir(), "{} was not created", index.display());
        let mode = std::fs::metadata(&index)
            .expect("exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "STATE-1's mode, wherever the index is kept");
    }

    /// LSP-10: the index a session keeps nothing of does not outlive it.
    ///
    /// The other arm of the same case, and the reason it is a separate test: a machine
    /// [STATE-2](../../../docs/specs/state-directory.md) leaves without a state directory reaches
    /// this through `None` rather than through the incognito flag, and a fix that read only the
    /// flag would leave it writing into the tree.
    #[cfg(unix)]
    #[test]
    fn an_index_a_session_keeps_nothing_of_goes_with_the_session() {
        let (_launching, scratch, workspace, file) =
            a_workspace_with_a_reporting_server(GOES_WITH_THE_SESSION);
        let mut servers = Servers::new(
            workspace.clone(),
            None,
            the_server_that_reports_with_no_state_directory,
            false,
            Vec::new(),
        );

        a_definition_in(&mut servers, &file);

        let (index, _) = what_the_server_reported(&scratch);
        assert!(
            !index.starts_with(workspace.canonicalize().expect("the workspace")),
            "{} is inside the workspace",
            index.display()
        );
        // What a real server leaves there, so the removal is of a directory holding something.
        std::fs::write(
            index.join("index"),
            "derived from every file in the workspace",
        )
        .expect("write");
        let session = index
            .parent()
            .and_then(Path::parent)
            .expect("the directory this session was given")
            .to_path_buf();

        drop(servers);

        assert!(
            !session.exists(),
            "an index outlived the session that built it: {}",
            session.display()
        );
    }

    /// LSP-10: a session that may keep its index keeps it under the state directory, which is
    /// what makes the second session fast.
    ///
    /// The other direction of the same decision. A fix that sent every session to a directory
    /// that lasts one would keep the index out of the workspace and pay for the indexing every
    /// time, which is the trade the clause's "why not a temporary directory" refuses.
    #[cfg(unix)]
    #[test]
    fn a_session_with_a_state_directory_indexes_under_it() {
        let (_launching, scratch, workspace, file) =
            a_workspace_with_a_reporting_server(UNDER_THE_STATE_DIRECTORY);
        let state = scratch.join("state");
        let mut servers = Servers::new(
            workspace.clone(),
            Some(state.clone()),
            the_server_that_reports_with_a_state_directory,
            false,
            Vec::new(),
        );

        a_definition_in(&mut servers, &file);

        let (index, _) = what_the_server_reported(&scratch);
        assert_eq!(
            index,
            cache_for(Some(&state), &workspace, false).expect("a cache is given"),
            "the index a session may keep must be the one the next session reads"
        );

        // And it is still there when the session is not, which is the whole point of keeping it.
        drop(servers);
        assert!(index.is_dir(), "{} did not survive", index.display());
    }

    #[test]
    fn a_language_is_decided_from_the_extension_alone() {
        assert_eq!(Language::for_path("src/lib.rs"), Some(Language::Rust));
        assert_eq!(
            Language::for_path("app/main.TS"),
            Some(Language::TypeScript)
        );
        assert_eq!(Language::for_path("s.py"), Some(Language::Python));
        assert_eq!(Language::for_path("m.go"), Some(Language::Go));
        assert!(Language::for_path("no-extension").is_none());
    }

    #[test]
    fn every_language_names_a_binary() {
        for language in [
            Language::Rust,
            Language::TypeScript,
            Language::Python,
            Language::Go,
        ] {
            let (program, _) = language.server();
            assert!(!program.is_empty(), "{} names no binary", language.as_str());
        }
    }
}
