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
use bravebot_agent::agents::{self, MakeRefused, RedefineRefused, Refused};
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

/// Rewrite the definition of a bot being edited, and answer with its name.
///
/// `name`, `purpose` are required and `model` is a string or absent, which removes the `model:`
/// line. Only the description, the model and the body change (MEMORY-9); a refusal leaves the file
/// as it was, and the desktop then saves no edit.
pub fn remake(request: &Request) -> Result<Value, Failure> {
    let home = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "this machine names no state directory, so a bot has nowhere to keep its definition",
        )
    })?;
    remake_in(&home, request)
}

fn remake_in(home: &Path, request: &Request) -> Result<Value, Failure> {
    let name = request.string("name")?;
    let purpose = request.string("purpose")?;
    let model = request.optional_string("model");

    match agents::redefine(home, &name, &purpose, model.as_deref()) {
        Ok(()) => Ok(json!({ "name": name })),
        Err(RedefineRefused::Name) => Err(Failure::bad_request(
            "the bot's definition is not named as a definition can be",
        )),
        Err(RedefineRefused::Missing) => Err(Failure::bad_request(format!(
            "the definition {name} is not there to edit"
        ))),
        Err(RedefineRefused::Refused(_)) => Err(Failure::bad_request(
            "the purpose needs a line that is not blank, and the model must be one line",
        )),
        Err(RedefineRefused::Io(error)) => Err(Failure::new(
            ErrorCode::NoHome,
            format!("the bot's definition could not be written: {error}"),
        )),
    }
}

/// Answer what a bot's definition says about the servers it may use.
///
/// `name` is required. The answer is `servers`, the names `mcpServers:` lists or null where there
/// is no such line, `tools`, whether a `tools:` line limits it to no servers unless listed,
/// `inline`, whether the line declares a server itself, and the definition's `kind`.
pub fn servers_read(request: &Request) -> Result<Value, Failure> {
    let home = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "this machine names no state directory, so a bot has nowhere to keep its definition",
        )
    })?;
    servers_read_in(&home, request)
}

fn servers_read_in(home: &Path, request: &Request) -> Result<Value, Failure> {
    let name = request.string("name")?;
    match agents::servers_of(home, &name) {
        Ok(choice) => Ok(json!({
            "servers": choice.servers,
            "tools": choice.tools,
            "inline": choice.inline,
            "kind": choice.kind,
        })),
        Err(refused) => Err(servers_refusal(&name, refused)),
    }
}

/// Rewrite the `mcpServers:` line of a bot's definition, and answer with its name.
///
/// `name` and `servers` are required. `servers` is an array of server names, which writes a line
/// naming those alone, or null, which removes the line so the bot holds every server. A refusal
/// leaves the file as it was.
pub fn servers_write(request: &Request) -> Result<Value, Failure> {
    let home = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "this machine names no state directory, so a bot has nowhere to keep its definition",
        )
    })?;
    servers_write_in(&home, request)
}

fn servers_write_in(home: &Path, request: &Request) -> Result<Value, Failure> {
    let name = request.string("name")?;
    let servers = match request.params.get("servers") {
        Some(Value::Null) => None,
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Failure::bad_request("every server must be named by a string"))?,
        ),
        _ => {
            return Err(Failure::bad_request(
                "servers must be a list of server names, or null for every server",
            ));
        }
    };
    match agents::set_servers(home, &name, servers.as_deref()) {
        Ok(()) => Ok(json!({ "name": name })),
        Err(refused) => Err(servers_refusal(&name, refused)),
    }
}

fn servers_refusal(name: &str, refused: RedefineRefused) -> Failure {
    match refused {
        RedefineRefused::Name => {
            Failure::bad_request("the bot's definition is not named as a definition can be")
        }
        RedefineRefused::Missing => {
            Failure::bad_request(format!("the definition {name} is not there"))
        }
        RedefineRefused::Refused(Refused::ServerName) => Failure::bad_request(
            "a server name may hold only letters, digits, - and _, and must start with a letter or digit",
        ),
        RedefineRefused::Refused(Refused::InlineServers) => Failure::bad_request(format!(
            "the definition {name} declares a server inline, so its servers are edited in the file"
        )),
        RedefineRefused::Refused(_) => Failure::bad_request(format!(
            "the servers could not be written so that the definition {name} reads them back"
        )),
        RedefineRefused::Io(error) => Failure::new(
            ErrorCode::NoHome,
            format!("the bot's definition could not be written: {error}"),
        ),
    }
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

    /// MEMORY-9: a model of several lines, or a purpose with nothing in it, makes no edit and no
    /// file.
    #[test]
    fn an_edit_the_agent_refuses_or_cannot_find_is_a_bad_request() {
        let home = home("remake-refused");
        let made = request(json!({"slug": "ed", "purpose": "Original."}));
        make_in(home.path(), &made).expect("made");
        let path = home.path().join("agents/ed.md");
        let before = std::fs::read_to_string(&path).expect("written");

        for params in [
            json!({"name": "ed", "purpose": " \n "}),
            json!({"name": "ed", "purpose": "P.", "model": "m\nkind: reader"}),
            json!({"name": "absent", "purpose": "P."}),
            json!({"name": "../ed", "purpose": "P."}),
            json!({"name": "ed"}),
        ] {
            let refused = remake_in(home.path(), &request(params.clone()));
            assert!(refused.is_err(), "{params} rewrote the definition");
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(!home.path().join("agents/absent.md").exists());
    }

    /// MEMORY-9: an edit answers with the name it rewrote, and the file carries the new purpose.
    #[test]
    fn an_edit_rewrites_the_file_made_for_the_bot() {
        let home = home("remake-rewrites");
        make_in(
            home.path(),
            &request(json!({"slug": "ed", "purpose": "Original.", "model": "m"})),
        )
        .expect("made");

        let answer = remake_in(
            home.path(),
            &request(json!({"name": "ed", "purpose": "Changed.\nMore."})),
        )
        .expect("rewritten");

        assert_eq!(answer["name"], json!("ed"));
        let text = std::fs::read_to_string(home.path().join("agents/ed.md")).expect("written");
        assert!(text.contains("description: 'Changed.'"), "{text}");
        assert!(
            !text.contains("model:"),
            "a model no longer chosen stays: {text}"
        );
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

    /// The servers a bot may use are read back as they were written, and null clears them.
    #[test]
    fn the_servers_written_for_a_bot_are_read_back() {
        let home = home("servers-round-trip");
        make_in(
            home.path(),
            &request(json!({"slug": "ed", "purpose": "P."})),
        )
        .expect("made");

        let read = servers_read_in(home.path(), &request(json!({"name": "ed"}))).expect("read");
        assert_eq!(read["servers"], Value::Null);
        assert_eq!(read["tools"], json!(false));
        assert_eq!(read["inline"], json!(false));
        assert_eq!(read["kind"], json!("worker"));

        let answer = servers_write_in(
            home.path(),
            &request(json!({"name": "ed", "servers": ["github", "slack"]})),
        )
        .expect("written");
        assert_eq!(answer["name"], json!("ed"));
        let read = servers_read_in(home.path(), &request(json!({"name": "ed"}))).expect("read");
        assert_eq!(read["servers"], json!(["github", "slack"]));

        servers_write_in(home.path(), &request(json!({"name": "ed", "servers": []})))
            .expect("written");
        let read = servers_read_in(home.path(), &request(json!({"name": "ed"}))).expect("read");
        assert_eq!(read["servers"], json!([]));

        servers_write_in(
            home.path(),
            &request(json!({"name": "ed", "servers": null})),
        )
        .expect("written");
        let read = servers_read_in(home.path(), &request(json!({"name": "ed"}))).expect("read");
        assert_eq!(read["servers"], Value::Null);
    }

    /// A name that is no alias, a list that is not one of names, a missing servers key, an inline
    /// declaration and a definition that is not there each leave the file as it was.
    #[test]
    fn a_refused_server_write_leaves_the_file_untouched() {
        let home = home("servers-refused");
        make_in(
            home.path(),
            &request(json!({"slug": "ed", "purpose": "P."})),
        )
        .expect("made");
        let path = home.path().join("agents/ed.md");
        let before = std::fs::read_to_string(&path).expect("written");

        for params in [
            json!({"name": "ed", "servers": ["git hub"]}),
            json!({"name": "ed", "servers": ["github\nkind: reader"]}),
            json!({"name": "ed", "servers": [1]}),
            json!({"name": "ed", "servers": "github"}),
            json!({"name": "ed"}),
            json!({"name": "../ed", "servers": null}),
            json!({"name": "absent", "servers": ["github"]}),
        ] {
            let refused = servers_write_in(home.path(), &request(params.clone()));
            assert!(refused.is_err(), "{params} rewrote the definition");
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(!home.path().join("agents/absent.md").exists());

        let inline = before.replacen("\n---", "\nmcpServers:\n  - gh:\n      command: gh\n---", 1);
        std::fs::write(&path, &inline).unwrap();
        let read = servers_read_in(home.path(), &request(json!({"name": "ed"}))).expect("read");
        assert_eq!(read["inline"], json!(true));
        let refused = servers_write_in(
            home.path(),
            &request(json!({"name": "ed", "servers": null})),
        )
        .expect_err("an inline declaration was rewritten");
        assert!(refused.message.contains("inline"), "{}", refused.message);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), inline);
    }
}
