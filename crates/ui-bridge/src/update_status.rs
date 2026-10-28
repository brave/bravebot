//! `update.status`: whether a newer release than this one is recorded, as a yes or no and never the
//! version the record names, and when that was last heard (UPDATE-5, UPDATE-12).

use serde_json::{Value, json};

/// The answer from the update crate's status and this build's version.
///
/// `running` is sent only where it is a version's shape, the rule the bug report's header holds the
/// interface version to, and as null otherwise, so the window draws what it is handed and checks
/// nothing of its own. `newer` is null where nothing is recorded and where what is recorded is
/// stale, which the update crate decides. `checked` is when an ask last learned a version, as a
/// UTC timestamp built here from the record's number, and null where the record gives none.
pub fn answer(status: bravebot_update::Status, running: &str) -> Value {
    json!({
        "check": if status.check { "on" } else { "off" },
        "running": crate::bug_report::is_version(running).then_some(running),
        "newer": status.newer,
        "checked": status.checked.map(timestamp),
    })
}

/// `2026-10-28T09:30:00Z` from seconds since the epoch, so the window's date formatting reads it as
/// it reads every other.
fn timestamp(seconds: u64) -> String {
    let seconds = i64::try_from(seconds).unwrap_or(i64::MAX);
    let (year, month, day) = bravebot_agent::calendar::civil_from_days(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(newer: Option<bool>) -> bravebot_update::Status {
        bravebot_update::Status {
            check: true,
            newer,
            checked: None,
        }
    }

    /// The running version is sent as it is where it is a version's shape.
    #[test]
    fn a_running_version_of_a_versions_shape_is_sent() {
        assert_eq!(
            answer(on(Some(true)), "0.19.0"),
            json!({ "check": "on", "running": "0.19.0", "newer": true, "checked": null })
        );
        assert_eq!(
            answer(on(None), env!("CARGO_PKG_VERSION"))["running"],
            env!("CARGO_PKG_VERSION")
        );
    }

    /// A running version of another shape is null, so no line break or sentence reaches the banner
    /// through it.
    #[test]
    fn a_running_version_of_another_shape_is_null() {
        for odd in ["", "0.19.0\nanything", "a b", "../x"] {
            assert!(answer(on(Some(true)), odd)["running"].is_null(), "{odd:?}");
        }
    }

    /// The time an ask last learned a version is sent as a UTC timestamp, fresh or stale, and as
    /// null where the record gives none (UPDATE-12).
    #[test]
    fn the_last_check_is_sent_as_a_utc_timestamp() {
        let status = |newer, checked| bravebot_update::Status {
            check: true,
            newer,
            checked,
        };
        assert_eq!(
            answer(status(Some(false), Some(1_761_643_800)), "0.19.0")["checked"],
            "2025-10-28T09:30:00Z"
        );
        let stale = answer(status(None, Some(0)), "0.19.0");
        assert_eq!(stale["checked"], "1970-01-01T00:00:00Z");
        assert!(stale["newer"].is_null());
        assert!(answer(status(None, None), "0.19.0")["checked"].is_null());
    }
}
