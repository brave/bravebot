//! An incognito session lists a project's standing MCP answers and cannot forget them: it writes
//! nothing, and forgetting is a write.
//!
//! A file of its own, because incognito mode is engaged for the whole process and cannot be left.

#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_config::mcp::{self as records, Projects, Standing};
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{ErrorCode, Request};
use serde_json::json;
use std::path::Path;

#[test]
fn an_incognito_session_lists_the_answers_and_refuses_to_forget_them() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!(
            "../../target/test-scratch/bridge-mcp-remembered-incognito-{}",
            std::process::id()
        ));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let state = home.join(".bravebot");
    let project = root.join("project");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let project = project.canonicalize().unwrap();
    let state = state.canonicalize().unwrap();
    // SAFETY: the only test in this binary, so nothing else reads or writes the environment.
    unsafe { std::env::set_var("HOME", &home) };
    unsafe { std::env::remove_var("BRAVEBOT_HOME") };
    bravebot_core::incognito::engage();

    let mut projects = Projects::default();
    assert!(projects.add(&project));
    std::fs::write(records::projects_file(&state), projects.to_text()).unwrap();
    let mut standing = Standing::default();
    assert!(standing.add("weather", "get_forecast", &project));
    std::fs::write(records::tools_file(&state), standing.to_text()).unwrap();
    let before = (
        std::fs::read_to_string(records::projects_file(&state)).unwrap(),
        std::fs::read_to_string(records::tools_file(&state)).unwrap(),
    );

    let mut bridge = Bridge::new(Box::new(|_| {}));
    let mut dispatch = |method: &str, params: serde_json::Value| {
        let line = json!({"id": 1, "method": method, "params": params}).to_string();
        bridge.dispatch(&Request::parse(&line).unwrap())
    };
    let made = dispatch("session.new", json!({"directory": project})).unwrap();
    let session = made["session"].as_str().unwrap().to_string();

    let listed = dispatch("permissions.list", json!({"session": session})).unwrap();
    let remembered = &listed["mcpRemembered"];
    assert_eq!(remembered["everyServer"], true, "{listed}");
    assert_eq!(
        remembered["tools"],
        json!([{"tool": "weather:get_forecast", "shownTool": "weather:get_forecast"}])
    );
    assert_eq!(remembered["forgettable"], false);

    let refused = dispatch(
        "permissions.revoke",
        json!({"session": session, "kind": "mcp"}),
    )
    .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(refused.message.contains("incognito"), "{}", refused.message);
    assert_eq!(
        (
            std::fs::read_to_string(records::projects_file(&state)).unwrap(),
            std::fs::read_to_string(records::tools_file(&state)).unwrap(),
        ),
        before
    );
    let _ = std::fs::remove_dir_all(&root);
}
