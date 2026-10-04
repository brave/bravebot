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

    refusal(
        agents::make_definition(home, &slug, &purpose, model.as_deref()),
        "the bot's definition could not be written",
    )
}

/// Give a bot made before definitions one, and record its old memory as untrusted ([MEMORY-11]).
///
/// `directory` is the folder the bot was made for, which the desktop holds and the window cannot
/// name. The old memory is not opened; only its path goes into the record, and the answer carries
/// the definition's name and nothing of what the memory holds.
///
/// [MEMORY-11]: ../../../docs/specs/definition-memory.md
pub fn migrate(request: &Request) -> Result<Value, Failure> {
    let home = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "this machine names no state directory, so a bot has nowhere to keep its definition",
        )
    })?;
    migrate_in(&home, request)
}

fn migrate_in(home: &Path, request: &Request) -> Result<Value, Failure> {
    let slug = request.string("slug")?;
    let purpose = request.string("purpose")?;
    let directory = request.string("directory")?;
    let model = request.optional_string("model");

    refusal(
        agents::migrate_definition(
            home,
            &slug,
            &purpose,
            model.as_deref(),
            Path::new(&directory),
        ),
        "the bot's old memory could not be recorded or its definition written",
    )
}

/// The answer for a definition made, or the failure for the reason it was not.
fn refusal(made: Result<agents::Made, MakeRefused>, io: &str) -> Result<Value, Failure> {
    match made {
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
        Err(MakeRefused::Io(error)) => {
            Err(Failure::new(ErrorCode::NoHome, format!("{io}: {error}")))
        }
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

    /// MEMORY-11: migrating a bot answers with its definition's name, writes the file, and records
    /// the old memory at the folder it was made for, without opening it.
    #[test]
    fn a_bot_made_before_definitions_is_migrated_and_its_old_memory_recorded() {
        let home = home("migrated");
        let folder = home_folder();
        let answer = migrate_in(
            home.path(),
            &request(json!({
                "slug": "rev",
                "purpose": "Reviews.",
                "directory": folder.path().display().to_string(),
            })),
        )
        .expect("migrated");

        assert_eq!(answer["name"], json!("rev"));
        assert!(home.path().join("agents/rev.md").is_file());
        let workspace = bravebot_agent::workspace::Workspace::new(folder.path()).unwrap();
        let key = bravebot_agent::workspace::key_of(workspace.root());
        let record = bravebot_agent::memory::Record::new(home.path(), &key);
        assert_eq!(
            record.paths(),
            vec![format!("{key}/.bravebot-ui/bots/rev.md")]
        );
    }

    /// MEMORY-11: a request with no folder, or a name that is no slug, migrates nothing and
    /// records nothing.
    #[test]
    fn a_refused_migration_writes_nothing() {
        let home = home("migrate-refused");
        let folder = home_folder();
        let directory = folder.path().display().to_string();
        for params in [
            json!({"slug": "rev", "purpose": "P."}),
            json!({"slug": "../x", "purpose": "P.", "directory": directory}),
            json!({"slug": "rev", "purpose": " ", "directory": directory}),
        ] {
            assert!(
                migrate_in(home.path(), &request(params.clone())).is_err(),
                "{params} was migrated"
            );
        }
        assert!(!home.path().join("agents").exists());
        assert!(!home.path().join("untrusted").exists());
    }

    fn home_folder() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("bravebot-bridge-folder-")
            .tempdir()
            .expect("a scratch folder")
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
