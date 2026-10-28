//! `update.status`: what the desktop app is told of a newer release (UPDATE-12).
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::Request;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A record of `source` naming `latest`, learned `ago` seconds before now: the line every version
/// reads, and the line saying when it was learned.
fn learned(source: &str, latest: &str, ago: u64) -> String {
    let at = now() - ago;
    format!("{at}\t{source}\t{latest}\n{at}\t{source}-learned\t{latest}\n")
}

/// A newer release recorded by the terminal's ask of the release listing is a yes, and the answer
/// carries this build's version and when the record was learned, and never the version recorded.
#[test]
fn a_newer_release_recorded_is_a_yes_without_its_version() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    recorded(&learned("releases", "97.5.3", 60));
    let answer = status(&mut Bridge::new(Box::new(|_| {})));
    assert_eq!(answer["check"], "on", "{answer}");
    assert_eq!(answer["newer"], true, "{answer}");
    assert_eq!(answer["running"], env!("CARGO_PKG_VERSION"), "{answer}");
    let checked = answer["checked"].as_str().expect("a time");
    assert!(checked.len() == 20 && checked.ends_with('Z'), "{answer}");
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
    recorded(&learned("releases", "0.0.1", 60));
    assert_eq!(status(&mut Bridge::new(Box::new(|_| {})))["newer"], false);
    recorded(&learned("npm", "97.5.3", 60));
    let answer = status(&mut Bridge::new(Box::new(|_| {})));
    assert!(answer["newer"].is_null(), "{answer}");
    assert!(answer["checked"].is_null(), "{answer}");
}

/// A record learned more than a week ago is no comparison, and carries the time it was learned so
/// the window can say when it last heard.
#[test]
fn a_record_older_than_a_week_is_no_comparison_and_keeps_its_date() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    recorded(&learned(
        "releases",
        "97.5.3",
        bravebot_update::STALE_AFTER + 60,
    ));
    let answer = status(&mut Bridge::new(Box::new(|_| {})));
    assert_eq!(answer["check"], "on", "{answer}");
    assert!(answer["newer"].is_null(), "{answer}");
    assert!(answer["checked"].is_string(), "{answer}");
}

/// A record that does not say when its version was learned, as one an earlier version wrote or
/// rewrote does not, is no answer.
#[test]
fn a_record_without_the_time_it_was_learned_is_no_answer() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    recorded(&format!("{}\treleases\t97.5.3\n", now()));
    let answer = status(&mut Bridge::new(Box::new(|_| {})));
    assert!(answer["newer"].is_null(), "{answer}");
    assert!(answer["checked"].is_null(), "{answer}");
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
    assert!(answer["checked"].is_null(), "{answer}");
}

/// `updateCheck: false` in the settings the window was started with turns the check off, and a
/// record already on disk is then not spoken of.
#[test]
fn the_check_turned_off_in_settings_says_off_and_nothing_of_the_record() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let home = recorded(&learned("releases", "97.5.3", 60));
    let settings = home.join("chosen-settings.json");
    std::fs::write(&settings, r#"{"updateCheck": false}"#).unwrap();
    let answer = status(&mut Bridge::new(Box::new(|_| {})).with_settings(Some(settings.clone())));
    assert_eq!(answer["check"], "off", "{answer}");
    assert!(answer["newer"].is_null(), "{answer}");
    assert!(answer["checked"].is_null(), "{answer}");
}

/// The check is the person's setting. A project's settings file in the directory the window was
/// started from does not turn it off, for `update.status` or for the startup ask, which read one
/// answer; the person's own file does, for both.
#[test]
fn the_check_is_read_from_the_persons_settings_and_not_a_projects() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let home = recorded(&learned("releases", "97.5.3", 60));
    let project = test_profile::project("started-here");
    std::fs::create_dir_all(project.join(".bravebot")).unwrap();
    std::fs::write(
        project.join(".bravebot/settings.json"),
        r#"{"updateCheck": false}"#,
    )
    .unwrap();
    // This test runs alone in its own process, so the directory it moves to is no other test's.
    std::env::set_current_dir(&project).unwrap();
    let here = std::env::current_dir().unwrap();
    assert_eq!(
        bravebot_ui_bridge::settings::layers(Some(&here), None).update_check(),
        Some(false),
        "the project's file is one the project's layers read"
    );
    assert_eq!(bravebot_ui_bridge::settings::update_check(None), None);
    assert_eq!(status(&mut Bridge::new(Box::new(|_| {})))["check"], "on");

    std::fs::write(home.join("settings.json"), r#"{"updateCheck": false}"#).unwrap();
    assert_eq!(
        bravebot_ui_bridge::settings::update_check(None),
        Some(false)
    );
    assert_eq!(status(&mut Bridge::new(Box::new(|_| {})))["check"], "off");
}
