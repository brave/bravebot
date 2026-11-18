//! PERM-14 for a desktop session: a file `--settings` named that lies in a directory the session
//! opened beside its project is read as a checkout's file where the platform cannot hold one file
//! back from a stage, and the session says so when it opens.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::Request;
use serde_json::{Value, json};

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Value {
    let line = json!({"id": 1, "method": method, "params": params}).to_string();
    bridge
        .dispatch(&Request::parse(&line).unwrap())
        .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
}

/// Opening a session with a directory named beside the project counts that directory as one a
/// stage may write, so a named file inside it is withheld where the sandbox cannot hold it back,
/// and the opening response carries the sentence a front end says. Where it can, the file grants
/// and nothing is said. A bridge that did not count the directories it opens would report nothing
/// on either kind of host.
#[test]
fn a_session_opened_with_a_directory_holding_the_named_file_reports_it_withheld() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = test_profile::project("named-withheld-project");
    let added = project.with_file_name("named-withheld-added");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&added).unwrap();
    let added = std::fs::canonicalize(added).unwrap();
    let named = added.join("grant.json");
    std::fs::write(&named, "{}\n").unwrap();

    let mut bridge = Bridge::new(Box::new(|_| {})).with_settings(Some(named));
    let made = call(
        &mut bridge,
        "session.new",
        json!({"directory": project, "directories": [added]}),
    );

    assert!(
        bravebot_config::added_writable_directories().contains(&added),
        "the opened directory was not counted as one a stage may write"
    );
    let said = &made["settingsRules"]["namedWithheld"];
    if bravebot_sandbox::holds_one_file_back() {
        assert!(said.is_null(), "{said}");
    } else {
        let said = said.as_str().expect("a sentence");
        assert!(said.contains("grant.json"), "{said}");
    }
}
