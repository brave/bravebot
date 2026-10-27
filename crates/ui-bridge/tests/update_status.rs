//! `update.status`: what the desktop app is told of a newer release (UPDATE-12).
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
use std::path::PathBuf;

fn status(bridge: &mut Bridge) -> Value {
    let line = json!({ "id": 1, "method": "update.status", "params": {} }).to_string();
    bridge
        .dispatch(&Request::parse(&line).expect("well formed"))
        .expect("answered")
}

/// The isolated profile's state directory, made, with `record` as its update record.
fn recorded(record: &str) -> PathBuf {
    let home = bravebot_agent::home::directory().expect("a state directory");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("update-check"), record).unwrap();
    home
}

/// A newer release recorded by the terminal's ask of the release listing is a yes, and the answer
/// carries this build's version and never the one recorded.
#[test]
fn a_newer_release_recorded_is_a_yes_without_its_version() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    recorded("1700000000\treleases\t97.5.3\n");
    let answer = status(&mut Bridge::new(Box::new(|_| {})));
    assert_eq!(answer["check"], "on", "{answer}");
    assert_eq!(answer["newer"], true, "{answer}");
    assert_eq!(answer["running"], env!("CARGO_PKG_VERSION"), "{answer}");
    let text = answer.to_string();
    assert!(
        !text.contains("97"),
        "the recorded version reached the window: {text}"
    );
}

/// A record of a version this build is not behind is a no, and one of another registry alone is
/// no record for the app, which asks the release listing.
#[test]
fn a_release_this_build_is_not_behind_is_a_no_and_npm_s_record_is_none() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    recorded("1700000000\treleases\t0.0.1\n");
    assert_eq!(status(&mut Bridge::new(Box::new(|_| {})))["newer"], false);
    recorded("1700000000\tnpm\t97.5.3\n");
    assert!(status(&mut Bridge::new(Box::new(|_| {})))["newer"].is_null());
}

/// With nothing recorded the answer is neither yes nor no, so the window says it has not checked.
#[test]
fn nothing_recorded_is_no_answer() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let answer = status(&mut Bridge::new(Box::new(|_| {})));
    assert_eq!(answer["check"], "on", "{answer}");
    assert!(answer["newer"].is_null(), "{answer}");
}

/// `updateCheck: false` in the settings the window was started with turns the check off, and a
/// record already on disk is then not spoken of.
#[test]
fn the_check_turned_off_in_settings_says_off_and_nothing_of_the_record() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let home = recorded("1700000000\treleases\t97.5.3\n");
    let settings = home.join("chosen-settings.json");
    std::fs::write(&settings, r#"{"updateCheck": false}"#).unwrap();
    let answer = status(&mut Bridge::new(Box::new(|_| {})).with_settings(Some(settings.clone())));
    assert_eq!(answer["check"], "off", "{answer}");
    assert!(answer["newer"].is_null(), "{answer}");
}
