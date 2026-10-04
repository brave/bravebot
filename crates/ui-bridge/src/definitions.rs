//! The definition a desktop bot is made with.
//!
//! The desktop writes nothing into `~/.bravebot` itself ([STATE-3]), so making a bot asks the
//! crate that reads the person's definitions to write one ([MEMORY-8]). What is written, which
//! names are taken and what is refused are all answered there; this only carries the request in
//! and the name it was given out.
//!
//! [STATE-3]: ../../../docs/specs/state-directory.md
//! [MEMORY-8]: ../../../docs/specs/definition-memory.md

use crate::protocol::{ErrorCode, Failure, Request};
use bravebot_agent::agents::{self, MakeRefused};
use serde_json::{Value, json};
use std::path::Path;

/// Write the definition for a bot being made, and answer with the name it was given.
///
/// `slug` and `purpose` are required and `model` is a string or absent. A refusal writes nothing,
/// and the desktop then makes no bot.
pub fn make(request: &Request) -> Result<Value, Failure> {
    let home = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "this machine names no state directory, so a bot has nowhere to keep its definition",
        )
    })?;
    make_in(&home, request)
}

fn make_in(home: &Path, request: &Request) -> Result<Value, Failure> {
    let slug = request.string("slug")?;
    let purpose = request.string("purpose")?;
    let model = request.optional_string("model");

    match agents::make_definition(home, &slug, &purpose, model.as_deref()) {
        Ok(made) => Ok(json!({
            "name": made.name,
            "file": made.file.display().to_string(),
        })),
        Err(MakeRefused::Name) => Err(Failure::bad_request(
            "the bot's name is not one a definition can be named",
        )),
        Err(MakeRefused::Model) => Err(Failure::bad_request(
            "the model must be one line with nothing around it",
        )),
        Err(MakeRefused::Purpose) => Err(Failure::bad_request(
            "the purpose needs at least one line that is not blank",
        )),
        Err(MakeRefused::Io(error)) => Err(Failure::new(
            ErrorCode::NoHome,
            format!("the bot's definition could not be written: {error}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(params: Value) -> Request {
        Request::parse(&json!({"id": 1, "method": "bot.define", "params": params}).to_string())
            .expect("a request")
    }

    fn home(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("bravebot-bridge-define-{name}-"))
            .tempdir()
            .expect("a scratch state directory")
    }

    /// MEMORY-8: making a bot answers with the name its definition was given and writes the file.
    #[test]
    fn a_bot_is_answered_with_the_name_its_definition_was_given() {
        let home = home("named");
        let answer = make_in(
            home.path(),
            &request(json!({"slug": "rev", "purpose": "\nReviews.\nMore.", "model": "m"})),
        )
        .expect("made");
        let again = make_in(
            home.path(),
            &request(json!({"slug": "rev", "purpose": "Other."})),
        )
        .expect("made");

        assert_eq!(answer["name"], json!("rev"));
        assert_eq!(again["name"], json!("rev-2"));
        let text = std::fs::read_to_string(home.path().join("agents/rev.md")).expect("written");
        assert!(text.contains("description: 'Reviews.'"), "{text}");
        assert!(text.contains("kind: worker"), "{text}");
    }

    /// MEMORY-8: a model of several lines, or a purpose with nothing in it, makes no bot and no
    /// file.
    #[test]
    fn a_refused_bot_writes_nothing() {
        let home = home("refused");
        for params in [
            json!({"slug": "a", "purpose": "P.", "model": "m\nkind: reader"}),
            json!({"slug": "a", "purpose": " \n "}),
            json!({"slug": "../a", "purpose": "P."}),
            json!({"slug": "a"}),
        ] {
            let refused = make_in(home.path(), &request(params.clone()));
            assert!(refused.is_err(), "{params} was made");
        }
        assert!(!home.path().join("agents").exists());
    }
}
