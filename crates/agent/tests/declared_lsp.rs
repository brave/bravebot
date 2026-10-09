//! Language servers a person declares, read from their own directory and from nowhere else.
//!
//! LSP-11: a declaration names a command, and a command is the thing a checkout must never be able
//! to choose. These drive `LanguageServers::ask` against a state directory the test controls and a
//! workspace that holds files named like declarations, with a fake server that counts how often it
//! starts.
//!
//! Unix only: the server is a shell script.
#![cfg(unix)]

use bravebot_agent::Workspace;
use bravebot_agent::confirm::*;
use bravebot_agent::lsp::LanguageServers;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_lsp::{LspError, Operation, Question};
use serde_json::json;
use std::path::{Path, PathBuf};

/// A scratch directory under `target/`, removed with the test.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        // CARGO_MANIFEST_DIR is `<workspace>/crates/agent`, so two pops reach the root.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.extend(["target", "test-scratch", name]);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Answers a question about a server with `approve`, and keeps what it was asked.
struct Asked {
    requests: Vec<ServerRequest>,
    approve: bool,
}

impl Confirmer for Asked {
    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        self.requests.push(request.clone());
        if self.approve {
            Decision::Approve
        } else {
            Decision::Reject
        }
    }
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        ApproveWrites.confirm_write(request)
    }
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        ApproveWrites.confirm_run(request)
    }
    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        ApproveWrites.confirm_read_output(request)
    }
    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        ApproveWrites.confirm_vetted_read(request)
    }
    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        ApproveWrites.confirm_fetch(request)
    }
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        ApproveWrites.confirm_manifest(request)
    }
    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        ApproveWrites.confirm_vouch(request)
    }
    fn confirm_tool_list(&mut self, request: &ToolListRequest) -> Decision {
        ApproveWrites.confirm_tool_list(request)
    }
    fn confirm_mcp_call(&mut self, _: &McpCallRequest) -> CallDecision {
        CallDecision::reject()
    }
    fn confirm_move(&mut self, _: &MoveRequest) -> Decision {
        Decision::Reject
    }
    fn confirm_exposing_read(&mut self, _: &ExposureRequest) -> Decision {
        Decision::Reject
    }
    fn ask_user(&mut self, _: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        Vec::new()
    }
    fn interjection(&mut self) -> Option<String> {
        None
    }
}

/// Appends a line to `$STARTS` as it comes up, and answers a definition with `$DEFINED_AT`.
const FAKE_SERVER: &str = r#"#!/bin/sh
echo "started $*" >> "$STARTS"
reply() {
  printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"
}
while IFS= read -r header; do
  case "$header" in
    Content-Length:*) length=$(printf '%s' "$header" | tr -cd '0-9') ;;
    *) continue ;;
  esac
  IFS= read -r blank
  body=$(dd bs=1 count="$length" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$body" in
    *'"initialize"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"capabilities\":{}}}"
      reply '{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/cachePriming","value":{"kind":"end"}}}'
      ;;
    *'"textDocument/definition"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{\"uri\":\"file://$DEFINED_AT\",\"range\":{\"start\":{\"line\":0,\"character\":10},\"end\":{\"line\":0,\"character\":14}}}]}"
      ;;
    *'"shutdown"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}"
      ;;
  esac
done
"#;

/// What a test stands up: a workspace holding `a.zz`, a server script, and an empty state directory.
struct Fixture {
    _scratch: Scratch,
    workspace: Workspace,
    state: PathBuf,
    program: PathBuf,
    starts: PathBuf,
    file: PathBuf,
}

fn fixture(name: &str) -> Fixture {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new(name);
    let root = scratch.path.join("work");
    std::fs::create_dir_all(&root).expect("create the workspace");
    let file = root.join("a.zz");
    std::fs::write(&file, "pub struct Held;\n").expect("write a.zz");
    let state = scratch.path.join("state");
    std::fs::create_dir_all(&state).expect("create the state directory");
    let program = scratch.path.join("zzls");
    std::fs::write(&program, FAKE_SERVER).expect("write the server");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let starts = scratch.path.join("starts");
    Fixture {
        workspace: Workspace::new(&root).expect("workspace"),
        _scratch: scratch,
        state,
        program,
        starts,
        file,
    }
}

impl Fixture {
    /// The declaration a person would write, naming this fixture's server by absolute path.
    fn declaration(&self, args: &[&str]) -> serde_json::Value {
        json!({ "servers": { "zzls": {
            "command": self.program,
            "args": args,
            "extensions": { "zz": "zed" },
            "env": { "STARTS": self.starts, "DEFINED_AT": self.file },
        } } })
    }

    fn declare(&self, declaration: &serde_json::Value) {
        std::fs::write(self.state.join("lsp.json"), declaration.to_string())
            .expect("write the declaration");
    }

    fn starts(&self) -> usize {
        std::fs::read_to_string(&self.starts).map_or(0, |recorded| recorded.lines().count())
    }

    fn servers(&self) -> LanguageServers {
        LanguageServers::new(
            self.workspace.root().to_path_buf(),
            Some(self.state.clone()),
        )
    }

    fn ask(
        &self,
        servers: &mut LanguageServers,
        asked: &mut Asked,
        path: &Path,
    ) -> Result<bravebot_lsp::Answer, LspError> {
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "look up");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::LanguageServer]),
            &mut sink,
        )
        .expect("policy");
        servers.ask(
            &mut policy,
            asked,
            &Question {
                operation: Operation::Definition,
                path: path.to_str().expect("a utf-8 path"),
                line: 1,
                character: 12,
                query: None,
            },
            &self.workspace,
        )
    }
}

fn approving() -> Asked {
    Asked {
        requests: Vec::new(),
        approve: true,
    }
}

/// A declared extension starts the declared command after approval, and the person is shown the
/// command, its arguments, the name they gave it, and that what it runs is unknown.
#[test]
fn a_declared_extension_starts_the_declared_command_after_approval() {
    let fixture = fixture("agent-declared-lsp-starts");
    fixture.declare(&fixture.declaration(&["--stdio"]));
    let mut servers = fixture.servers();

    let mut declined = Asked {
        requests: Vec::new(),
        approve: false,
    };
    let refused = fixture.ask(&mut servers, &mut declined, &fixture.file);
    assert!(
        matches!(refused, Err(LspError::Refused { .. })),
        "{refused:?}"
    );
    assert_eq!(fixture.starts(), 0, "a declined server does not start");

    let mut asked = approving();
    let answer = fixture
        .ask(&mut servers, &mut asked, &fixture.file)
        .expect("the declared server answers");

    assert_eq!(asked.requests.len(), 1);
    let request = &asked.requests[0];
    assert!(request.declared);
    assert_eq!(request.language, "zzls");
    assert_eq!(request.program, fixture.program.display().to_string());
    assert_eq!(request.args, ["--stdio"]);
    assert_eq!(answer.locations.len(), 1);
    assert_eq!(fixture.starts(), 1);
    let recorded = std::fs::read_to_string(&fixture.starts).expect("recorded");
    assert!(recorded.contains("started --stdio"), "{recorded}");
}

/// An extension no declaration names is unconfigured, as it was before declarations existed.
#[test]
fn an_undeclared_extension_answers_unconfigured() {
    let fixture = fixture("agent-declared-lsp-undeclared");
    fixture.declare(&fixture.declaration(&[]));
    let other = fixture.workspace.root().join("notes.txt");
    std::fs::write(&other, "text\n").expect("write notes.txt");
    let mut servers = fixture.servers();
    let mut asked = approving();

    let error = fixture
        .ask(&mut servers, &mut asked, &other)
        .expect_err("nothing serves a .txt file");

    assert!(matches!(error, LspError::NoServerFor { .. }), "{error:?}");
    assert!(asked.requests.is_empty());
    assert_eq!(fixture.starts(), 0);
}

/// A declaration a checkout carries is a command a tool call could have written, so no file in the
/// workspace declares anything: not under any of the names other agents read, and not in the file
/// this one reads from the state directory.
#[test]
fn a_declaration_found_in_the_workspace_is_ignored() {
    let fixture = fixture("agent-declared-lsp-workspace");
    let declaration = fixture.declaration(&["--stdio"]);
    let root = fixture.workspace.root().to_path_buf();
    std::fs::create_dir_all(root.join(".github")).expect("create .github");
    std::fs::create_dir_all(root.join(".bravebot")).expect("create .bravebot");
    for name in [
        "lsp.json",
        ".lsp.json",
        ".github/lsp.json",
        ".bravebot/lsp.json",
    ] {
        std::fs::write(root.join(name), declaration.to_string()).expect("write a declaration");
    }
    let mut servers = fixture.servers();
    let mut asked = approving();

    let error = fixture
        .ask(&mut servers, &mut asked, &fixture.file)
        .expect_err("a workspace declares no server");

    assert!(matches!(error, LspError::NoServerFor { .. }), "{error:?}");
    assert!(asked.requests.is_empty(), "nobody is asked about one");
    assert_eq!(fixture.starts(), 0, "nothing runs");
}

/// An approval binds to the declaration it was given for: asked once while it stands, and asked
/// again when an argument changes.
#[test]
fn a_changed_declaration_is_asked_about_again() {
    let fixture = fixture("agent-declared-lsp-changed");
    fixture.declare(&fixture.declaration(&["--stdio"]));
    let mut servers = fixture.servers();
    let mut asked = approving();

    for _ in 0..2 {
        fixture
            .ask(&mut servers, &mut asked, &fixture.file)
            .expect("answers");
    }
    assert_eq!(
        asked.requests.len(),
        1,
        "an unchanged declaration is asked once"
    );
    assert_eq!(fixture.starts(), 1);

    fixture.declare(&fixture.declaration(&["--stdio", "--log=verbose"]));
    fixture
        .ask(&mut servers, &mut asked, &fixture.file)
        .expect("answers");

    assert_eq!(
        asked.requests.len(),
        2,
        "an edited declaration is a new question"
    );
    assert_eq!(asked.requests[1].args, ["--stdio", "--log=verbose"]);
    assert_eq!(fixture.starts(), 2);
}

/// A file caught half written is not a person taking the declaration out: the server they approved
/// keeps running and is not asked about again once the file is whole.
#[test]
fn an_unreadable_declaration_file_leaves_the_approved_server_running() {
    let fixture = fixture("agent-declared-lsp-unreadable");
    fixture.declare(&fixture.declaration(&["--stdio"]));
    let mut servers = fixture.servers();
    let mut asked = approving();
    fixture
        .ask(&mut servers, &mut asked, &fixture.file)
        .expect("answers");

    std::fs::write(fixture.state.join("lsp.json"), "{ \"servers\": { \"zz").expect("truncate");
    fixture
        .ask(&mut servers, &mut asked, &fixture.file)
        .expect("a half-written file does not stop the server");
    fixture.declare(&fixture.declaration(&["--stdio"]));
    fixture
        .ask(&mut servers, &mut asked, &fixture.file)
        .expect("answers");

    assert_eq!(asked.requests.len(), 1, "nobody is asked again");
    assert_eq!(fixture.starts(), 1, "the server was not restarted");
}

/// Two argument lists that start different commands do not read alike in the prompt.
#[test]
fn a_prompt_shows_an_argument_with_a_space_as_one_argument() {
    let request = |args: &[&str]| ServerRequest {
        language: "zzls".into(),
        program: "/usr/bin/zzls".into(),
        args: args.iter().map(|a| (*a).to_string()).collect(),
        workspace: "/work".into(),
        runs_build_tooling: false,
        declared: true,
    };
    let joined = request(&["--cmd", "a b"]).arguments_line();
    let split = request(&["--cmd", "a", "b"]).arguments_line();

    assert_eq!(joined, "--cmd \"a b\"");
    assert_eq!(split, "--cmd a b");
    assert_eq!(request(&["--log=a/b.c"]).arguments_line(), "--log=a/b.c");
    assert_eq!(request(&["x\ny"]).arguments_line(), "\"x\\ny\"");
}
