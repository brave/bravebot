//! A stdio server started inside an NVIDIA OpenShell sandbox, spoken to by the same
//! [`StdioServer`] a native backend's child is.
//!
//! These need a running OpenShell gateway and a container runtime, so they run only where
//! `BRAVEBOT_OPENSHELL_TESTS=1` is set and are skipped everywhere else, CI included.

#![cfg(unix)]

use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_mcp::StdioServer;
use bravebot_sandbox::openshell::OpenShell;
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::{Stream, Variables};

/// A server that echoes, fetches a URL to show what the network policy allows, and writes a long
/// reply to show what the streams carry.
const SERVER: &str = r#"
import sys, json, urllib.request
TOOLS = [
    {"name": "echo", "description": "echo text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}},
    {"name": "fetch", "description": "GET a URL", "inputSchema": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
    {"name": "big", "description": "a long reply", "inputSchema": {"type": "object", "properties": {"bytes": {"type": "integer"}}, "required": ["bytes"]}},
]
def reply(i, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": i, "result": result}) + "\n"); sys.stdout.flush()
for line in sys.stdin:
    msg = json.loads(line); method = msg.get("method"); i = msg.get("id")
    if method == "initialize":
        reply(i, {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "spike", "version": "0"}})
    elif method == "tools/list":
        reply(i, {"tools": TOOLS})
    elif method == "tools/call":
        name = msg["params"]["name"]; args = msg["params"].get("arguments", {})
        if name == "echo":
            reply(i, {"content": [{"type": "text", "text": args["text"]}]})
        elif name == "big":
            reply(i, {"content": [{"type": "text", "text": "x" * args["bytes"]}]})
        else:
            try:
                with urllib.request.urlopen(args["url"], timeout=8) as r:
                    reply(i, {"content": [{"type": "text", "text": f"reached {r.status}"}]})
            except Exception as e:
                reply(i, {"content": [{"type": "text", "text": f"refused {type(e).__name__}"}]})
"#;

fn enabled() -> bool {
    std::env::var("BRAVEBOT_OPENSHELL_TESTS").as_deref() == Ok("1")
}

fn routing() -> Routing {
    let mut r = Routing::new();
    r.insert_trusted("task", "use a tool");
    r
}

/// Call `tool` and read what it said, as a test may and a planner never does.
fn text_of(server: &mut StdioServer, tool: &str, arguments: serde_json::Value) -> String {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("spike"))]),
        &mut sink,
    )
    .expect("policy");
    let result = server
        .call_tool(&mut policy, tool, arguments)
        .expect("the call is answered");
    let proof = policy.authorise_display_release("the test reads what the tool said");
    result.text.declassify(&proof)
}

#[test]
fn a_server_in_an_openshell_sandbox_answers_over_stdio_and_reaches_only_its_host() {
    if !enabled() {
        return;
    }
    let name = format!("bb-ok-{}", std::process::id());
    let sandbox = OpenShell::new(&name, "python:3.13-slim", &["example.com".to_string()])
        .expect("openshell is installed");

    let mut server = StdioServer::launch(
        "spike",
        "python3",
        &["-u".to_string(), "-c".to_string(), SERVER.to_string()],
        Variables::new(),
        &sandbox,
        &SandboxPolicy::strict(),
        Stream::Inherited,
    )
    .expect("the server starts inside the sandbox");

    server.initialize("bravebot", "0.1.0").expect("handshake");
    let listing = server.list_tools().expect("tools listed");
    assert_eq!((listing.offered(), listing.refused()), (3, 0));

    assert_eq!(
        text_of(&mut server, "echo", serde_json::json!({"text": "hi"})),
        "hi"
    );
    assert_eq!(
        text_of(
            &mut server,
            "fetch",
            serde_json::json!({"url": "https://example.com/"})
        ),
        "reached 200",
        "the host the server was allowed"
    );
    assert!(
        text_of(
            &mut server,
            "fetch",
            serde_json::json!({"url": "https://registry.npmjs.org/"})
        )
        .starts_with("refused"),
        "a host the server was not allowed"
    );

    drop(server);
    drop(sandbox);
    let listed = bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("openshell"))
            .expect("openshell"),
        bravebot_sandbox::programs::Inherits::Only(&["HOME"]),
        bravebot_sandbox::programs::SearchPath::Reduced,
    )
    .args(["sandbox", "list", "--color", "never"])
    .output()
    .expect("list sandboxes");
    assert!(
        !String::from_utf8_lossy(&listed.stdout).contains(&name),
        "the sandbox was left behind"
    );
}

#[test]
fn a_program_missing_from_the_image_is_refused_and_the_sandbox_removed() {
    if !enabled() {
        return;
    }
    let name = format!("bb-miss-{}", std::process::id());
    let sandbox = OpenShell::new(&name, "python:3.13-slim", &["example.com".to_string()])
        .expect("openshell is installed");
    let refused = StdioServer::launch(
        "spike",
        "node",
        &[],
        Variables::new(),
        &sandbox,
        &SandboxPolicy::strict(),
        Stream::Inherited,
    );
    assert!(
        refused.is_err(),
        "a server whose program the image lacks was started"
    );
}

/// The connection a session holds lasts: many calls in a row, a pause, and a reply near the line
/// bound (MCP-13) all pass through `openshell sandbox exec` as they do through a native child's
/// pipes.
#[test]
fn a_long_lived_connection_carries_many_calls_a_pause_and_a_long_reply() {
    if !enabled() {
        return;
    }
    let name = format!("bb-long-{}", std::process::id());
    let sandbox = OpenShell::new(&name, "python:3.13-slim", &[]).expect("openshell is installed");
    let mut server = StdioServer::launch(
        "spike",
        "python3",
        &["-u".to_string(), "-c".to_string(), SERVER.to_string()],
        Variables::new(),
        &sandbox,
        &SandboxPolicy::strict(),
        Stream::Inherited,
    )
    .expect("the server starts inside the sandbox");
    server.initialize("bravebot", "0.1.0").expect("handshake");

    for n in 0..200 {
        let said = text_of(
            &mut server,
            "echo",
            serde_json::json!({"text": format!("call {n}")}),
        );
        assert_eq!(
            said,
            format!("call {n}"),
            "call {n} was answered out of order"
        );
    }
    std::thread::sleep(std::time::Duration::from_secs(5));
    assert_eq!(
        text_of(
            &mut server,
            "echo",
            serde_json::json!({"text": "after a pause"})
        ),
        "after a pause"
    );
    let long = 12 * 1024 * 1024;
    assert_eq!(
        text_of(&mut server, "big", serde_json::json!({"bytes": long})).len(),
        long,
        "a reply near the line bound was cut"
    );
    assert_eq!(
        text_of(
            &mut server,
            "echo",
            serde_json::json!({"text": "still here"})
        ),
        "still here",
        "the connection did not survive the long reply"
    );
}

/// A recorded sandbox whose process has ended is deleted; a recorded one whose process runs is kept,
/// and so is one in no record here, as another machine's on a shared gateway would be.
#[test]
fn a_recorded_sandbox_of_an_ended_process_is_removed_and_no_other() {
    if !enabled() {
        return;
    }
    let openshell = || {
        bravebot_sandbox::programs::start(
            &bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("openshell"))
                .expect("openshell"),
            bravebot_sandbox::programs::Inherits::Only(&["HOME"]),
            bravebot_sandbox::programs::SearchPath::Reduced,
        )
    };
    let mut ended = bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("true")).expect("true"),
        bravebot_sandbox::programs::Inherits::Only(&[]),
        bravebot_sandbox::programs::SearchPath::Reduced,
    )
    .spawn()
    .expect("a child");
    let gone = ended.id();
    ended.wait().expect("waited");

    let path = std::env::temp_dir().join(format!(
        "bravebot-openshell-ledger-live-{}",
        std::process::id()
    ));
    let ledger = bravebot_sandbox::openshell::Ledger::at(path.clone());
    let left = bravebot_sandbox::openshell::next_name();
    let kept = bravebot_sandbox::openshell::next_name();
    let other = bravebot_sandbox::openshell::next_name();
    // Recorded by this process, then given the id of a process that has ended, as a session
    // killed by a signal would have left it.
    ledger.record(&left).expect("recorded");
    let mine = format!("{left} {} ", std::process::id());
    let record = std::fs::read_to_string(&path).expect("the record");
    assert!(record.contains(&mine), "{record}");
    std::fs::write(&path, record.replace(&mine, &format!("{left} {gone} "))).expect("rewritten");
    ledger.record(&kept).expect("recorded");
    for name in [&left, &kept, &other] {
        let created = openshell()
            .args([
                "sandbox",
                "create",
                "--name",
                name,
                "--from",
                "python:3.13-slim",
            ])
            .args([
                "--detach",
                "--no-tty",
                "--no-auto-providers",
                "--color",
                "never",
            ])
            .output()
            .expect("create");
        assert!(created.status.success(), "{name} was not created");
    }

    let removed = ledger.remove_left_behind().expect("swept");
    let listed = openshell()
        .args(["sandbox", "list", "--color", "never"])
        .output()
        .expect("list");
    let listed = String::from_utf8_lossy(&listed.stdout).into_owned();
    for name in [&kept, &other] {
        let _ = openshell()
            .args(["sandbox", "delete", name, "--color", "never"])
            .output();
    }
    let record = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    assert_eq!(removed, std::slice::from_ref(&left));
    assert!(
        listed.contains(&kept),
        "a running process's sandbox was removed: {listed}"
    );
    assert!(
        listed.contains(&other),
        "a sandbox in no record was removed: {listed}"
    );
    assert!(
        !record.contains(&left),
        "the removed sandbox stayed in the record"
    );
}
