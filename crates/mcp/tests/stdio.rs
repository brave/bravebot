//! stdio transport tests, driving a real subprocess.
//!
//! The fake server is a shell script speaking newline-delimited JSON-RPC. Using a real
//! process rather than an in-memory pipe is the point: it exercises the sandbox spawn
//! path, which is where a confinement failure would surface.

use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_mcp::{McpError, StdioServer};
use bravebot_sandbox::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use bravebot_sandbox::{ConfinedChild, Environment, Sandbox, Streams, Unavailable};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Serialises the tests in this binary, which all write a script and then execute it.
///
/// Linux refuses to execute a file any process still holds open for writing. These tests run on
/// threads of one process and each spawns a child, and a child forked while another thread is
/// part-way through writing its script inherits that write handle: the sibling's exec then fails
/// with `Text file busy`, on whichever test happened to lose the race. It failed in CI on a
/// commit that touched a prompt string, which is the tell.
///
/// One at a time closes the window, since no write is ever in flight while a fork happens.
static SPAWNING: Mutex<()> = Mutex::new(());

/// Hold this for the length of a test that spawns a server.
///
/// Poisoning is ignored on purpose: a test that panicked while holding it left nothing behind
/// that the next one cannot overwrite.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    SPAWNING.lock().unwrap_or_else(|held| held.into_inner())
}

/// Write a fake MCP server that replies to each request by id.
fn fake_server(name: &str, script: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bravebot-mcp-fake-{name}.sh"));
    let mut file = std::fs::File::create(&path).expect("create script");
    file.write_all(script.as_bytes()).expect("write script");
    // Closed before the mode is set, and well before anything tries to execute it.
    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
    }

    path
}

/// A server that handles initialize, tools/list, and tools/call.
const WORKING_SERVER: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id"
      ;;
    *'"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"echoes","inputSchema":{"type":"object"}}]}}\n' "$id"
      ;;
    *'"tools/call"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"tool output here"}]}}\n' "$id"
      ;;
    *'"notifications/initialized"'*)
      ;;
  esac
done
"#;

/// A server whose tool reports failure of its own, with something to say about it.
const FAILING_SERVER: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id"
      ;;
    *'"tools/call"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"no such record"}],"isError":true}}\n' "$id"
      ;;
    *'"notifications/initialized"'*)
      ;;
  esac
done
"#;

/// A server that reports one variable of its own environment, so a test can say whether
/// this process's environment reached it.
///
/// `CARGO_MANIFEST_DIR` is the variable asked for because cargo sets it in the environment
/// of a test process, and no shell invents one for itself: an empty answer is the
/// environment having been emptied rather than the variable never having existed.
const ENVIRONMENT_REPORTING_SERVER: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id"
      ;;
    *'"tools/call"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"[%s]"}]}}\n' "$id" "$CARGO_MANIFEST_DIR"
      ;;
    *'"notifications/initialized"'*)
      ;;
  esac
done
"#;

/// The variable the server above reports, and the one this process is asked for.
const REPORTED_VARIABLE: &str = "CARGO_MANIFEST_DIR";

fn routing() -> Routing {
    let mut r = Routing::new();
    r.insert_trusted("task", "use a tool");
    r
}

/// A policy permissive enough for a shell script to run, while still confining it.
///
/// Network is granted, which is not what a real processor policy would do. The Linux
/// backend refuses a policy requiring network denial because that is not implemented
/// there yet, and refusing is the correct behaviour, so a test that wants a successful
/// spawn on both platforms has to ask for the weaker policy the backend can honour.
///
/// The list names both platforms' directories and grants the ones this machine has: the
/// temporary directory is under `/private/var` on macOS and `/tmp` on Linux, and a path
/// that is not there is a grant a backend may refuse the whole policy over.
fn sandbox_policy() -> SandboxPolicy {
    [
        "/usr",
        "/bin",
        "/lib",
        "/lib64",
        "/private/var/folders",
        "/tmp",
        "/var",
    ]
    .into_iter()
    .filter(|path| Path::new(path).exists())
    .fold(
        SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses(),
        SandboxPolicy::allow_read,
    )
}

/// Skip where no real backend exists, since these tests need a spawn to succeed.
fn sandbox_or_skip() -> Option<Box<dyn Sandbox>> {
    match bravebot_sandbox::for_current_platform() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("SKIPPED (no confinement backend): {e}");
            None
        }
    }
}

#[test]
fn a_confined_server_completes_the_handshake_and_lists_tools() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("handshake", WORKING_SERVER);

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches under confinement");

    server.initialize("bravebot", "0.1.0").expect("handshake");

    let tools = server.list_tools().expect("tools listed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert!(tools[0].input_schema.is_some());

    let _ = std::fs::remove_file(&script);
}

/// A tool result is untrusted content, whatever the server says it is.
#[test]
fn a_tool_result_is_labelled_untrusted() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("call", WORKING_SERVER);

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("fake"))]),
        &mut sink,
    )
    .expect("policy");

    let result = server
        .call_tool(&mut policy, "echo", serde_json::json!({"text": "hi"}))
        .expect("tool call succeeds");

    assert_eq!(result.label(), Label::untrusted_public());
    assert!(policy.finish());

    let _ = std::fs::remove_file(&script);
}

/// Without the capability the tool must not be callable.
#[test]
fn a_tool_call_without_the_capability_is_refused() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("no-capability", WORKING_SERVER);

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut sink,
    )
    .expect("policy");

    let error = server
        .call_tool(&mut policy, "echo", serde_json::json!({}))
        .expect_err("must be refused");
    assert!(error.to_string().contains("mcp_call"), "got: {error}");

    let _ = std::fs::remove_file(&script);
}

/// SERVERS-9: a grant names the server it is about, so calling a second server is refused
/// while the first is callable. The fault this rejects is the gate reading the protocol out
/// of the capability and stopping there, which is what one alias-free `McpCall` left it
/// doing: a run that had been granted calls to `weather` could call `payments` too.
#[test]
fn a_grant_for_one_server_does_not_reach_another() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("other-server", WORKING_SERVER);

    let mut payments = StdioServer::launch(
        "payments",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    payments.initialize("bravebot", "0.1.0").expect("handshake");

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("weather"))]),
        &mut sink,
    )
    .expect("policy");

    let error = payments
        .call_tool(&mut policy, "echo", serde_json::json!({"text": "hi"}))
        .expect_err("a grant for weather must not reach payments");
    // The server that was asked for, not the one that was granted: a refusal naming
    // `mcp_call:weather` would mean the gate checked the grant it held rather than the
    // call in front of it.
    assert!(
        error.to_string().contains("mcp_call:payments"),
        "got: {error}"
    );

    let _ = std::fs::remove_file(&script);
}

/// SERVERS-9: a grant withdrawn stops answering at the next call, not at the next session.
/// The fault this rejects is a set fixed when the run began, which is what leaves a
/// withdrawal waiting for a restart while the server stays callable meanwhile.
#[test]
fn a_grant_withdrawn_stops_the_next_call() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("withdrawn", WORKING_SERVER);

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("fake"))]),
        &mut sink,
    )
    .expect("policy");

    server
        .call_tool(&mut policy, "echo", serde_json::json!({"text": "hi"}))
        .expect("granted, so the first call goes through");

    assert!(policy.revoke_mcp_call(&ServerAlias::new("fake")));

    // The same policy, so the same session: nothing was restarted between the two calls.
    let error = server
        .call_tool(&mut policy, "echo", serde_json::json!({"text": "hi"}))
        .expect_err("the grant was withdrawn before this call");
    assert!(error.to_string().contains("mcp_call:fake"), "got: {error}");

    let _ = std::fs::remove_file(&script);
}

/// The rule that matters most for stdio: no confinement means no server.
#[test]
fn a_server_is_not_launched_without_confinement() {
    let _spawning = one_at_a_time();
    let script = fake_server("unconfined", WORKING_SERVER);

    let error = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        &Unavailable,
        &SandboxPolicy::strict(),
    )
    .expect_err("must refuse to launch");

    assert!(matches!(error, McpError::Confinement(_)));
    assert!(
        error.to_string().contains("refusing to launch"),
        "got: {error}"
    );

    let _ = std::fs::remove_file(&script);
}

/// A backend that confines nothing because the program never started.
///
/// Written here rather than reached through a real backend, because no real backend
/// produces this on every platform: macOS wraps the program in `sandbox-exec`, which starts
/// whether or not the program it was given does.
struct ProgramWouldNotStart;

impl Sandbox for ProgramWouldNotStart {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["none"],
            network_denial_enforced: true,
            grants_paths_that_do_not_exist: false,
        }
    }

    fn spawn(
        &self,
        _program: &str,
        _args: &[String],
        _policy: &SandboxPolicy,
        _streams: Streams,
        _environment: Environment,
    ) -> Result<ConfinedChild, bravebot_sandbox::SandboxError> {
        Err(bravebot_sandbox::SandboxError::SpawnFailed(
            std::io::Error::from(std::io::ErrorKind::NotFound),
        ))
    }
}

/// A server nobody can start is a person's own configuration to correct, and it is reported
/// as that rather than as confinement that could not be established. Told apart because the
/// two ask for different things: a confinement failure is a machine that cannot run a server
/// safely, and this is a path to fix in a settings file.
#[test]
fn a_server_that_could_not_be_started_is_not_reported_as_a_confinement_failure() {
    let error = StdioServer::launch(
        "fake",
        "/bravebot-no-such-server/never-installed",
        &[],
        &ProgramWouldNotStart,
        &SandboxPolicy::strict(),
    )
    .expect_err("a program that will not start does not launch");

    assert!(
        matches!(error, McpError::Transport(_)),
        "a program that would not start was reported as a confinement failure: {error}"
    );
}

/// A tool that reports failure of its own is a failure, and what it says about that failure is
/// content from outside like anything else it sent.
#[test]
fn a_tool_level_error_is_reported_as_a_failure() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("tool-error", FAILING_SERVER);

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("fake"))]),
        &mut sink,
    )
    .expect("policy");

    let error = server
        .call_tool(&mut policy, "echo", serde_json::json!({}))
        .expect_err("a tool-level error must be a failure");

    let McpError::ToolFailed { tool, detail } = error else {
        panic!("got: {error}");
    };
    assert_eq!(tool, "echo");
    assert_eq!(detail.label(), Label::untrusted_public());

    let proof = policy.authorise_display_release("test inspects the failure");
    assert_eq!(detail.declassify(&proof), "no such record");
    assert!(policy.finish());

    let _ = std::fs::remove_file(&script);
}

/// A JSON-RPC error must surface as an error rather than being read as a result.
#[test]
fn a_server_error_is_reported() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server(
        "error",
        r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"capabilities":{}}}\n' "$id"
      ;;
    *'"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"not implemented"}}\n' "$id"
      ;;
  esac
done
"#,
    );

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    let error = server.list_tools().expect_err("must report the error");
    assert!(
        matches!(error, McpError::Server { code: -32601, .. }),
        "got: {error}"
    );

    let _ = std::fs::remove_file(&script);
}

/// A server that dies must produce an error, not a hang.
#[test]
fn a_server_that_exits_early_is_an_error() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    let script = fake_server("exits", "#!/bin/sh\nexit 0\n");

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("launch succeeds even though the server exits");

    let error = server
        .initialize("bravebot", "0.1.0")
        .expect_err("a dead server cannot handshake");
    assert!(matches!(error, McpError::Transport(_)), "got: {error}");

    let _ = std::fs::remove_file(&script);
}

/// A server is code from outside, and a variable this process holds is not something a
/// policy over paths can withhold from it: an API key lives in the environment rather
/// than on disk, so a server that inherits it has been handed it whatever the profile
/// names. The launch asks for an empty environment rather than leaving it to whichever
/// backend happens to be in use, so both platforms hand a server the same nothing.
#[test]
fn a_server_does_not_receive_this_processes_environment() {
    let _spawning = one_at_a_time();
    let Some(sandbox) = sandbox_or_skip() else {
        return;
    };
    assert!(
        std::env::var_os(REPORTED_VARIABLE).is_some(),
        "this test needs a variable the parent holds, and cargo sets {REPORTED_VARIABLE} \
         for a test process"
    );
    let script = fake_server("environment", ENVIRONMENT_REPORTING_SERVER);

    let mut server = StdioServer::launch(
        "fake",
        script.to_str().expect("path"),
        &[],
        sandbox.as_ref(),
        &sandbox_policy(),
    )
    .expect("server launches");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("fake"))]),
        &mut sink,
    )
    .expect("policy");

    let reported = server
        .call_tool(&mut policy, "echo", serde_json::json!({}))
        .expect("tool call succeeds");

    let proof = policy.authorise_display_release("test reads what the server was given");
    assert_eq!(
        reported.declassify(&proof),
        "[]",
        "the server was handed a variable this process holds"
    );

    let _ = std::fs::remove_file(&script);
}
