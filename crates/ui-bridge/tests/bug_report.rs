//! `bug-report.compose`: the report the desktop app shows and saves (DIAG-9).
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[allow(
    dead_code,
    reason = "the shared helpers include one for a project, which this needs none of"
)]
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::Request;
use serde_json::{Value, json};

fn compose(bridge: &mut Bridge, params: Value) -> Value {
    let line = json!({ "id": 1, "method": "bug-report.compose", "params": params }).to_string();
    bridge
        .dispatch(&Request::parse(&line).expect("well formed"))
        .expect("answered")
}

/// The report names both versions, the target, the settings file and the newest log by its path,
/// and holds neither a value from the settings file nor a line of the log.
#[test]
fn the_report_names_the_build_the_settings_file_and_the_log_and_holds_neither() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let home = bravebot_agent::home::directory().expect("a state directory");
    let logs = home.join(bravebot_diag::DIRECTORY);
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("20260101T000000Z-1.log"), "an older line\n").unwrap();
    let newest = logs.join("20260102T000000Z-2.log");
    std::fs::write(&newest, "a line the log keeps-5531\n").unwrap();
    let settings = home.join("chosen-settings.json");
    std::fs::write(&settings, r#"{"style": "a-style-of-its-own-8842"}"#).unwrap();

    let mut bridge = Bridge::new(Box::new(|_| {})).with_settings(Some(settings.clone()));
    let answer = compose(&mut bridge, json!({ "interface": "0.19.0" }));
    assert_eq!(answer["available"], true, "{answer}");
    assert_eq!(answer["log"], newest.display().to_string(), "{answer}");
    let text = answer["text"].as_str().expect("a text");
    for part in [
        "bravebot desktop\n".to_string(),
        "interface 0.19.0\n".to_string(),
        format!("agent {}\n", bravebot_ui_bridge::agent_build()),
        format!(
            "target {}-{}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        "== settings report ==\n".to_string(),
        settings.display().to_string(),
        format!("separately ==\n{}\n", newest.display()),
    ] {
        assert!(text.contains(&part), "missing {part:?} in {text}");
    }
    for kept in ["a-style-of-its-own-8842", "keeps-5531", "an older line"] {
        assert!(!text.contains(kept), "{kept:?} reached the report: {text}");
    }
}

/// An interface version that is not a version's shape is written as unknown, so text from the
/// window cannot reach the file through it, and no log is `none`.
#[test]
fn an_interface_version_of_another_shape_is_unknown_and_no_log_is_none() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let answer = compose(
        &mut bridge,
        json!({ "interface": "1.0\n== doctor ==\nforged" }),
    );
    let text = answer["text"].as_str().expect("a text");
    assert!(text.contains("interface unknown\n"), "{text}");
    assert!(!text.contains("forged"), "{text}");
    assert!(answer["log"].is_null(), "{answer}");
    assert!(text.ends_with("separately ==\nnone\n"), "{text}");
    let answer = compose(&mut bridge, json!({}));
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("interface unknown\n")
    );
}
