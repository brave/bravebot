//! The effort level the window's model picker shows and records.
//!
//! The same record `/effort` keeps under `~/.bravebot` (SESSION-15), so a level picked in one front
//! end is the level in the other, ranked against the settings layers as BACKEND-43 ranks it.

use crate::protocol::{Failure, Request};
use bravebot_config::Settings;
use bravebot_session::store::{self, Effort};
use serde_json::{Value, json};

/// The level recorded, the level a turn would ask for, and whether a checkout's settings name one
/// that outranks any pick.
pub fn get(settings: &Settings) -> Value {
    let recorded = store::load_effort();
    let in_force = store::effort(recorded, settings);
    json!({
        "recorded": recorded.map(Effort::as_str),
        "inForce": in_force.map(Effort::as_str),
        "outranked": settings.effort_outranks_a_pick() && in_force.is_some(),
    })
}

/// Record the level named by `level`, or remove the record for `null`. A word that names no level is
/// refused and nothing is written.
pub fn set(request: &Request) -> Result<Value, Failure> {
    let level = match request.params.get("level") {
        None | Some(Value::Null) => None,
        Some(Value::String(word)) => Some(Effort::named(word).ok_or_else(|| {
            Failure::bad_request(format!(
                "`{}` is not an effort level; the levels are {}",
                word.trim(),
                Effort::ALL.map(Effort::as_str).join(", ")
            ))
        })?),
        Some(_) => return Err(Failure::bad_request("level must be a word or null")),
    };
    store::save_effort(level);
    Ok(json!({ "recorded": store::load_effort().map(Effort::as_str) }))
}
