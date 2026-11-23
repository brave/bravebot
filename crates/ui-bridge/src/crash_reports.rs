//! The crash report switch and the one-time notice, as the desktop window shows them.
//!
//! The same file `bravebot` reads at launch, resolved by the same rule
//! ([`bravebot_config::crash_reports::resolve`], through `bravebot_backtrace::current`), so the
//! switch in the window is the switch every executable obeys. A reason stronger than the switch is
//! reported rather than overruled. The one-time notice is [`crate::crash_notice`].

use crate::protocol::{Failure, Request};
use bravebot_backtrace::NotSaved;
use bravebot_config::crash_reports::CrashReports;
use bravebot_config::{Managed, Settings};
use serde_json::{Value, json};

/// Whether reports are on, whether the switch may change that, and what holds it when it may not.
pub fn get() -> Value {
    get_with(&Managed::load())
}

/// As [`get`], with the managed file stated, so a test can say what it holds. The window's answer
/// is [`get`]'s.
#[doc(hidden)]
pub fn get_with(managed: &Managed) -> Value {
    report(bravebot_backtrace::current_with(managed))
}

/// Write the switch into the person's own settings file, or refuse where something stronger than
/// it decides.
pub fn set(request: &Request) -> Result<Value, Failure> {
    set_with(request, &Managed::load())
}

/// As [`set`], with the managed file stated, so a test can say what it holds. The window's answer
/// is [`set`]'s.
#[doc(hidden)]
pub fn set_with(request: &Request, managed: &Managed) -> Result<Value, Failure> {
    let Some(enabled) = request.param("enabled").as_bool() else {
        return Err(Failure::bad_request("`enabled` must be true or false"));
    };
    let now = bravebot_backtrace::current_with(managed);
    if !now.person_may_change() {
        return Err(Failure::bad_request(refusal(now)));
    }
    let written = bravebot_backtrace::save(enabled);
    let mut answer = report(bravebot_backtrace::current_with(managed));
    answer["saved"] = json!(written.is_ok() && Settings::written_crash_reports() == Some(enabled));
    if let Err(why) = written {
        answer["unsaved"] = json!(match why {
            NotSaved::NoDirectory => "no_directory",
            NotSaved::Unwritable => "unwritable",
        });
    }
    Ok(answer)
}

fn report(state: CrashReports) -> Value {
    json!({
        "enabled": state.is_on(),
        "changeable": state.person_may_change(),
        "reason": reason(state),
    })
}

fn reason(state: CrashReports) -> Value {
    match state {
        CrashReports::On | CrashReports::OffByPerson => Value::Null,
        CrashReports::OffByEnvironment(variable) => {
            json!({ "code": "environment", "variable": variable })
        }
        CrashReports::OffByAdmin => json!({ "code": "admin" }),
        CrashReports::Incognito => json!({ "code": "incognito" }),
    }
}

fn refusal(state: CrashReports) -> String {
    match state {
        CrashReports::OffByEnvironment(variable) => {
            format!("crash reports are turned off by `{variable}` in the environment")
        }
        CrashReports::OffByAdmin => {
            "crash reports are turned off by the managed settings".to_string()
        }
        CrashReports::Incognito => {
            "an incognito session has no state directory to keep a choice in".to_string()
        }
        CrashReports::On | CrashReports::OffByPerson => {
            "the crash reports switch is changeable".to_string()
        }
    }
}
