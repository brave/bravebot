//! Which catalog the window's chrome is read in, and the choice `/language` keeps.
//!
//! The window chooses as the terminal does (LOCALE-8), from the same file, so a choice made in one
//! is the choice in the other. This process's own notes stay in the reference (LOCALE-7).

use crate::protocol::{Failure, Request};
use bravebot_i18n::{AUTO, LOCALES, Said};
use bravebot_session::store;
use serde_json::{Value, json};

/// More than any system lists; a longer list is read no further.
const MAX_SYSTEM_LANGUAGES: usize = 32;

/// The catalog in use, the one saved, every one there is, and whether `BRAVEBOT_LOCALE` outranks
/// them all. `system` is the operating system's preferred languages, which the desktop's main
/// process sends and which answer only where nothing else names a language.
pub fn get(request: &Request) -> Value {
    let picked = store::load_language();
    let written = bravebot_config::Settings::written_language();
    let system: Vec<String> = request
        .param("system")
        .as_array()
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .take(MAX_SYSTEM_LANGUAGES)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let in_use = bravebot_i18n::choose_for_window(
        Said {
            picked: picked.as_deref(),
            written: written.as_deref(),
        },
        &system,
    );
    let saved = picked
        .as_deref()
        .and_then(bravebot_i18n::find)
        .map(|locale| locale.tag());
    json!({
        "inUse": in_use.tag(),
        "saved": saved,
        "available": available(),
        "overridden": bravebot_i18n::overridden(),
    })
}

/// Save a catalog's tag, or forget the choice for `auto`. A word no catalog answers is refused.
pub fn set(request: &Request) -> Result<Value, Failure> {
    let word = request.string("tag")?;
    let tag = bravebot_i18n::pick(&word)
        .map_err(|_| {
            Failure::bad_request(format!(
                "no catalog answers `{}`; there are {}",
                word.trim(),
                bravebot_i18n::tags().join(", ")
            ))
        })?
        .map(|locale| locale.tag());
    Ok(json!({
        "saved": store::save_language(tag),
        "tag": tag.unwrap_or(AUTO),
    }))
}

fn available() -> Vec<Value> {
    LOCALES
        .iter()
        .map(|locale| json!({ "tag": locale.tag(), "name": locale.name() }))
        .collect()
}
