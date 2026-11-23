//! What holds the crash report switch off where the switch cannot be driven through `bravebot-rpc`:
//! the managed file, which is read from a fixed path a test cannot redirect, and an incognito
//! session, which the executable never engages (CRASH-4, CRASH-5).
//!
//! Each case runs as a child of this test binary, since incognito and the home directory are the
//! process's. The child states the managed file through the bridge's `_with` functions, which the
//! product's `get` and `set` call with the file they load.

#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "the test starts its own executable as the session it examines"
)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_config::Managed;
use bravebot_ui_bridge::crash_reports::{get_with, set_with};
use bravebot_ui_bridge::protocol::Request;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

const KIND: &str = "BRAVEBOT_LOCKS_TEST_KIND";
const MANAGED: &str = "BRAVEBOT_LOCKS_TEST_MANAGED";
const CHILD: &str = "one_session_when_asked_to";

fn set_request(enabled: bool) -> Request {
    let line = json!({"id": 1, "method": "crash_reports.set", "params": {"enabled": enabled}});
    Request::parse(&line.to_string()).expect("well formed")
}

fn settings_file(state: &Path) -> PathBuf {
    state.join("settings.json")
}

/// The child. It does nothing unless a parent asked, so a plain run of this binary passes it.
#[test]
fn one_session_when_asked_to() {
    let Ok(kind) = std::env::var(KIND) else {
        return;
    };
    let state = PathBuf::from(std::env::var("BRAVEBOT_HOME").unwrap());
    let before = std::fs::read(settings_file(&state)).ok();
    let managed = match std::env::var(MANAGED) {
        Ok(text) => {
            let file = state.join("managed-for-the-test.json");
            std::fs::write(&file, text).unwrap();
            Managed::at(&file)
        }
        Err(_) => Managed::at(&state.join("no-managed-file.json")),
    };
    match kind.as_str() {
        "admin" => {
            // The person's own `true` is in the file and still does not outrank the managed `false`.
            assert_eq!(
                get_with(&managed),
                json!({"enabled": false, "changeable": false, "reason": {"code": "admin"}})
            );
            for enabled in [true, false] {
                let refused = set_with(&set_request(enabled), &managed).expect_err("a locked set");
                assert!(
                    refused.message.contains("managed"),
                    "{enabled}: {}",
                    refused.message
                );
            }
            assert_eq!(
                std::fs::read(settings_file(&state)).ok(),
                before,
                "a locked switch wrote the settings file"
            );
        }
        "managed_true_is_not_read" => {
            // A managed `true` can only say what saying nothing does: the person's own `false`
            // stands, and the switch can still be turned back on.
            assert_eq!(
                get_with(&managed),
                json!({"enabled": false, "changeable": true, "reason": null})
            );
            let kept: Value = set_with(&set_request(true), &managed).expect("changeable");
            assert_eq!(kept["enabled"], true, "{kept}");
            assert_eq!(kept["saved"], true, "{kept}");
        }
        "incognito" => {
            bravebot_core::incognito::engage();
            assert_eq!(
                get_with(&managed),
                json!({"enabled": false, "changeable": false, "reason": {"code": "incognito"}})
            );
            for enabled in [true, false] {
                let refused =
                    set_with(&set_request(enabled), &managed).expect_err("an incognito set");
                assert!(
                    refused.message.contains("incognito"),
                    "{enabled}: {}",
                    refused.message
                );
            }
            assert_eq!(
                std::fs::read(settings_file(&state)).ok(),
                before,
                "an incognito session wrote the settings file"
            );
            let listing = std::fs::read_dir(&state).unwrap().count();
            assert!(
                listing <= 1,
                "an incognito session left something in the state directory"
            );
        }
        other => panic!("unknown kind {other}"),
    }
}

/// Run the child as `kind` with the person's own file `settings` and the managed file `managed`.
/// Panics with the child's output when an assertion in the child failed. Returns the settings file
/// as the child left it.
fn launch(kind: &str, settings: Option<&str>, managed: Option<&str>) -> Option<String> {
    let scratch = tempfile::tempdir().unwrap();
    let state = scratch.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    if let Some(text) = settings {
        std::fs::write(settings_file(&state), text).unwrap();
    }
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", CHILD, "--nocapture"])
        .env_remove("BRAVEBOT_CRASH_REPORTS")
        .env_remove(MANAGED)
        .env("BRAVEBOT_HOME", &state)
        .env("HOME", scratch.path())
        .env(KIND, kind);
    if let Some(text) = managed {
        child.env(MANAGED, text);
    }
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "{kind}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(settings_file(&state)).ok()
}

/// CRASH-4: a managed `crashReports: false` turns reports off over the person's own `true`, the
/// window shows it as off for the administrator, and a change either way is refused and writes
/// nothing. The faults this rejects are a switch that is changeable under a pin (it would do
/// nothing when used), one that lets the person's `true` outrank the pin, and one that writes
/// before it refuses.
#[test]
fn a_managed_false_locks_the_switch_and_refuses_a_change() {
    let left = launch(
        "admin",
        Some(r#"{"crashReports": true}"#),
        Some(r#"{"crashReports": false}"#),
    );
    assert_eq!(left.as_deref(), Some(r#"{"crashReports": true}"#));
}

/// CRASH-4: a managed `true` is not read, so it is no stronger than saying nothing. The fault this
/// rejects is a managed `true` that overrides the person's own `false` or locks the switch.
#[test]
fn a_managed_true_does_not_override_the_persons_choice() {
    let left = launch(
        "managed_true_is_not_read",
        Some(r#"{"crashReports": false}"#),
        Some(r#"{"crashReports": true}"#),
    );
    assert_eq!(
        left.as_deref()
            .map(str::trim_end)
            .map(|text| { serde_json::from_str::<Value>(text).unwrap() }),
        Some(json!({"crashReports": true}))
    );
}

/// CRASH-5: an incognito session shows the switch as off for that reason, refuses a change and
/// writes nothing, whatever the person's file says.
#[test]
fn an_incognito_session_shows_the_switch_off_and_writes_nothing() {
    launch("incognito", None, None);
    let left = launch("incognito", Some(r#"{"crashReports": true}"#), None);
    assert_eq!(left.as_deref(), Some(r#"{"crashReports": true}"#));
}
