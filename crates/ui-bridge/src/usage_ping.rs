//! The daily usage ping's switch, as the desktop settings show it.
//!
//! The same file `/config` writes, resolved by the same rule, so the switch in one is the switch in
//! the other. A reason stronger than the switch is reported rather than overruled.

use crate::protocol::{Failure, Request};
use bravebot_config::usage_ping::UsagePing;
use bravebot_session::store;
use serde_json::{Value, json};
use std::path::Path;

/// Start the daily usage ping for the window, on a thread of its own.
///
/// Called when the bridge starts and again when the window has shown the notice, since the first
/// start of a new home is before the notice and sends nothing. The crate sends one a day between
/// them. The window picks a session's model later, so the ping names the configured default, and a
/// configuration that does not load sends nothing.
pub fn start(selected: Option<&Path>) {
    if let Ok(config) = crate::settings::config(None, selected) {
        bravebot_usage_ping::at_startup(
            bravebot_usage_ping::Surface::Desktop,
            bravebot_usage_ping::backend(&config, None),
            bravebot_usage_ping::premium(&config),
        );
    }
}

/// Whether the ping is on, whether the switch may change that, and what holds it when it may not.
pub fn get() -> Value {
    report(store::usage_ping())
}

/// Record the switch, or refuse where something stronger than it decides.
pub fn set(request: &Request) -> Result<Value, Failure> {
    let Some(enabled) = request.param("enabled").as_bool() else {
        return Err(Failure::bad_request("`enabled` must be true or false"));
    };
    let now = store::usage_ping();
    if !now.person_may_change() {
        return Err(Failure::bad_request(refusal(now)));
    }
    store::save_usage_ping(enabled);
    let after = store::usage_ping();
    let mut answer = report(after);
    answer["saved"] = json!(store::load_usage_ping() == Some(enabled));
    Ok(answer)
}

fn report(state: UsagePing) -> Value {
    json!({
        "enabled": state.is_on(),
        "changeable": state.person_may_change(),
        "reason": reason(state),
    })
}

fn reason(state: UsagePing) -> Value {
    match state {
        UsagePing::On | UsagePing::OffByUser => Value::Null,
        UsagePing::OffBySetting => json!({ "code": "setting" }),
        UsagePing::OffByEnvironment(variable) => {
            json!({ "code": "environment", "variable": variable })
        }
        UsagePing::OffByAdmin => json!({ "code": "admin" }),
        UsagePing::Incognito => json!({ "code": "incognito" }),
    }
}

fn refusal(state: UsagePing) -> String {
    match state {
        UsagePing::OffBySetting => {
            "the usage ping is turned off by `usagePing` in the settings file".to_string()
        }
        UsagePing::OffByEnvironment(variable) => {
            format!("the usage ping is turned off by `{variable}` in the environment")
        }
        UsagePing::OffByAdmin => "the usage ping is turned off by the managed settings".to_string(),
        UsagePing::Incognito => "an incognito session records no usage ping choice".to_string(),
        UsagePing::On | UsagePing::OffByUser => "the usage ping switch is changeable".to_string(),
    }
}
