use crate::protocol::{ErrorCode, Failure, Request};
use bravebot_agent::bots::{self, Refused};
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn list(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let listing = bots::list(&data_folder);
    Ok(json!({
        "bots": listing.rows.iter().map(bots::Row::to_value).collect::<Vec<_>>(),
        "dropped": listing.dropped.iter().map(|each| json!({
            "index": each.index,
            "slug": each.slug,
            "reason": each.reason.to_string(),
        })).collect::<Vec<_>>(),
        "unreadable": listing.unreadable.map(|problem| json!({
            "path": problem.path.display().to_string(),
            "reason": problem.reason,
        })),
        "oldRowsWaiting": listing.old_rows_waiting,
    }))
}

pub fn replace(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let Some(entries) = request.param("bots").as_array() else {
        return Err(Failure::bad_request("`bots` must be an array"));
    };
    match bots::replace(&data_folder, entries) {
        Ok(rows) => Ok(json!({ "bots": rows.iter().map(bots::Row::to_value).collect::<Vec<_>>() })),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn make(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let new = bots::NewBot {
        name: request.string("name")?,
        purpose: request.string("purpose")?,
        model: optional_text(request, "model")?,
        avatar: text_unless_null(request, "avatar")?,
    };
    match bots::make(&data_folder, new) {
        Ok(saved) => Ok(saved_answer(&saved)),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn edit(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    let changes = bots::Changes {
        name: text_unless_null(request, "name")?,
        purpose: text_unless_null(request, "purpose")?,
        model: match request.params.get("model") {
            None => None,
            Some(_) => Some(optional_text(request, "model")?),
        },
        avatar: text_unless_null(request, "avatar")?,
    };
    match bots::edit(&data_folder, &slug, changes) {
        Ok(saved) => Ok(saved_answer(&saved)),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn set_model(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    if request.params.get("model").is_none() {
        return Err(Failure::bad_request(
            "`model` must be a string, or null to clear it",
        ));
    }
    let model = optional_text(request, "model")?;
    match bots::set_model(&data_folder, &slug, model.as_deref()) {
        Ok(row) => Ok(json!({ "bot": row.to_value() })),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn ensure_home(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    match bots::ensure_home(&data_folder, &slug) {
        Ok(home_made) => Ok(json!({ "homeMade": home_made })),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn archive(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    match bots::archive(&data_folder, &slug) {
        Ok(row) => Ok(json!({ "bot": row.to_value() })),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn restore(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    match bots::restore(&data_folder, &slug) {
        Ok(row) => Ok(json!({ "bot": row.to_value() })),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn delete(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    match bots::delete(&data_folder, &slug) {
        Ok(()) => Ok(json!({ "slug": slug })),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn record_conversation(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    let id = request.string("id")?;
    let directory = request.string("directory")?;
    match bots::record_conversation(&data_folder, &slug, &id, &directory) {
        Ok(noted) => Ok(noted_answer(&noted)),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn record_archived(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    let Some(value) = request.params.get("archived") else {
        return Err(Failure::bad_request("`archived` is required"));
    };
    let Some(archived) = value.as_f64() else {
        return Ok(json!({ "bot": Value::Null, "wrote": false }));
    };
    match bots::record_archived(&data_folder, &slug, archived) {
        Ok(noted) => Ok(noted_answer(&noted)),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn release_session(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let slug = request.string("slug")?;
    match bots::release_session(&data_folder, &slug) {
        Ok(noted) => Ok(noted_answer(&noted)),
        Err(refused) => Err(failure(&refused)),
    }
}

pub fn forget_conversation(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let id = request.string("id")?;
    let directory = request.string("directory")?;
    match bots::forget_conversation(&data_folder, &id, &directory) {
        Ok(changed) => Ok(json!({ "changed": changed, "wrote": changed > 0 })),
        Err(refused) => Err(failure(&refused)),
    }
}

fn noted_answer(noted: &bots::Noted) -> Value {
    json!({
        "bot": noted.row.as_ref().map(bots::Row::to_value),
        "wrote": noted.wrote,
    })
}

fn saved_answer(saved: &bots::Saved) -> Value {
    json!({ "bot": saved.row.to_value(), "homeMade": saved.home_made })
}

fn optional_text(request: &Request, name: &str) -> Result<Option<String>, Failure> {
    match request.params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(Failure::bad_request(format!("`{name}` must be a string"))),
    }
}

fn text_unless_null(request: &Request, name: &str) -> Result<Option<String>, Failure> {
    match request.params.get(name) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(Failure::bad_request(format!("`{name}` must be a string"))),
    }
}

fn data_folder(request: &Request) -> Result<PathBuf, Failure> {
    let named = request.string("dataFolder")?;
    if !bots::is_absolute_path(&named) {
        return Err(Failure::bad_request(
            "`dataFolder` must be an absolute path",
        ));
    }
    Ok(PathBuf::from(named))
}

fn failure(refused: &Refused) -> Failure {
    let code = match refused {
        Refused::Ceiling
        | Refused::Entry { .. }
        | Refused::Name
        | Refused::Purpose
        | Refused::Model
        | Refused::Avatar
        | Refused::NoSuchBot(_)
        | Refused::DataFolder(_)
        | Refused::Definition(_)
        | Refused::Conversation => ErrorCode::BadRequest,
        Refused::Unreadable(_) | Refused::OldFile(_) | Refused::Locked(_) => {
            ErrorCode::StateUnusable
        }
        Refused::NoFolder
        | Refused::Incognito
        | Refused::Io(_)
        | Refused::History(_)
        | Refused::DefinitionIo(_) => ErrorCode::NoHome,
    };
    Failure::new(code, refused.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::bots::{Problem, Reason};
    use std::path::Path;

    fn problem() -> Problem {
        Problem {
            path: Path::new("/state/bots.json").to_path_buf(),
            reason: "it is not JSON".to_string(),
        }
    }

    #[test]
    fn each_refusal_of_a_make_or_an_edit_is_answered_with_its_own_message_and_code() {
        let cases = [
            (Refused::Name, "bad_request"),
            (Refused::Purpose, "bad_request"),
            (Refused::Model, "bad_request"),
            (Refused::Avatar, "bad_request"),
            (Refused::NoSuchBot("ghost".to_string()), "bad_request"),
            (
                Refused::DataFolder(Path::new("relative").to_path_buf()),
                "bad_request",
            ),
            (Refused::Definition("no purpose".to_string()), "bad_request"),
            (Refused::DefinitionIo(problem()), "no_home"),
        ];
        for (refused, code) in cases {
            let answered = failure(&refused);
            assert_eq!(answered.code.as_str(), code, "{refused:?}");
            assert_eq!(answered.message, refused.to_string(), "{refused:?}");
        }
        let io = failure(&Refused::DefinitionIo(problem()));
        assert!(io.message.contains("/state/bots.json"), "{}", io.message);
        assert!(
            failure(&Refused::NoSuchBot("ghost".into()))
                .message
                .contains("ghost")
        );
    }

    fn request(method: &str, params: Value) -> Request {
        Request::parse(&json!({"id": 1, "method": method, "params": params}).to_string())
            .expect("a request")
    }

    #[test]
    fn a_wrongly_typed_or_missing_field_is_a_bad_request_before_anything_is_written() {
        let folder = "/gui";
        let cases = [
            ("bot.make", json!({"dataFolder": folder, "purpose": "P."})),
            ("bot.make", json!({"dataFolder": folder, "name": "N"})),
            (
                "bot.make",
                json!({"dataFolder": folder, "name": 3, "purpose": "P."}),
            ),
            (
                "bot.make",
                json!({"dataFolder": folder, "name": "N", "purpose": "P.", "model": 4}),
            ),
            (
                "bot.make",
                json!({"dataFolder": folder, "name": "N", "purpose": "P.", "avatar": []}),
            ),
            ("bot.edit", json!({"dataFolder": folder})),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "name": 1}),
            ),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "purpose": {}}),
            ),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "model": false}),
            ),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "avatar": 2}),
            ),
            ("bot.set_model", json!({"dataFolder": folder, "slug": "a"})),
            (
                "bot.set_model",
                json!({"dataFolder": folder, "slug": "a", "model": 5}),
            ),
            ("bot.set_model", json!({"dataFolder": folder, "model": "m"})),
            ("bot.ensure_home", json!({"dataFolder": folder})),
            ("bot.ensure_home", json!({"slug": "a"})),
            ("bot.archive", json!({"dataFolder": folder})),
            ("bot.archive", json!({"slug": "a"})),
            ("bot.restore", json!({"dataFolder": folder})),
            ("bot.restore", json!({"slug": "a"})),
            ("bot.delete", json!({"dataFolder": folder})),
            ("bot.delete", json!({"slug": "a"})),
            ("bot.record_conversation", json!({"dataFolder": folder})),
            (
                "bot.record_conversation",
                json!({"slug": "a", "id": "s", "directory": "/d"}),
            ),
            (
                "bot.record_conversation",
                json!({"dataFolder": folder, "slug": "a", "id": "s"}),
            ),
            (
                "bot.record_conversation",
                json!({"dataFolder": folder, "slug": "a", "directory": "/d"}),
            ),
            (
                "bot.record_conversation",
                json!({"dataFolder": folder, "slug": 1, "id": "s", "directory": "/d"}),
            ),
            (
                "bot.record_conversation",
                json!({"dataFolder": folder, "slug": "a", "id": 1, "directory": "/d"}),
            ),
            (
                "bot.record_conversation",
                json!({"dataFolder": folder, "slug": "a", "id": "s", "directory": 1}),
            ),
            (
                "bot.record_archived",
                json!({"dataFolder": folder, "archived": 1}),
            ),
            ("bot.record_archived", json!({"slug": "a", "archived": 1})),
            (
                "bot.record_archived",
                json!({"dataFolder": folder, "slug": "a"}),
            ),
            (
                "bot.record_archived",
                json!({"dataFolder": folder, "slug": 1, "archived": 1}),
            ),
            ("bot.release_session", json!({"dataFolder": folder})),
            ("bot.release_session", json!({"slug": "a"})),
            (
                "bot.release_session",
                json!({"dataFolder": folder, "slug": 1}),
            ),
            (
                "bot.forget_conversation",
                json!({"dataFolder": folder, "id": "s"}),
            ),
            (
                "bot.forget_conversation",
                json!({"dataFolder": folder, "directory": "/d"}),
            ),
            (
                "bot.forget_conversation",
                json!({"id": "s", "directory": "/d"}),
            ),
            (
                "bot.forget_conversation",
                json!({"dataFolder": folder, "id": 1, "directory": "/d"}),
            ),
            (
                "bot.forget_conversation",
                json!({"dataFolder": folder, "id": "s", "directory": true}),
            ),
        ];
        for (method, params) in cases {
            let call = request(method, params.clone());
            let refused = match method {
                "bot.make" => make(&call),
                "bot.edit" => edit(&call),
                "bot.set_model" => set_model(&call),
                "bot.ensure_home" => ensure_home(&call),
                "bot.archive" => archive(&call),
                "bot.restore" => restore(&call),
                "bot.delete" => delete(&call),
                "bot.record_conversation" => record_conversation(&call),
                "bot.record_archived" => record_archived(&call),
                "bot.release_session" => release_session(&call),
                "bot.forget_conversation" => forget_conversation(&call),
                _ => panic!("untested method {method}"),
            }
            .expect_err("refused");
            assert_eq!(refused.code.as_str(), "bad_request", "{method} {params}");
        }
    }

    #[test]
    fn a_non_number_archive_length_is_a_success() {
        for archived in [json!("no"), json!(null), json!(true)] {
            let call = request(
                "bot.record_archived",
                json!({"dataFolder": "/gui", "slug": "a", "archived": archived}),
            );
            let answered = record_archived(&call).expect("success");
            assert_eq!(answered, json!({"bot": null, "wrote": false}), "{archived}");
        }
    }

    #[test]
    fn a_null_name_purpose_or_avatar_is_a_bad_request_and_a_null_model_is_none() {
        let folder = "/gui";
        let cases = [
            (
                "bot.make",
                json!({"dataFolder": folder, "name": null, "purpose": "P."}),
            ),
            (
                "bot.make",
                json!({"dataFolder": folder, "name": "N", "purpose": null}),
            ),
            (
                "bot.make",
                json!({"dataFolder": folder, "name": "N", "purpose": "P.", "avatar": null}),
            ),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "name": null}),
            ),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "purpose": null}),
            ),
            (
                "bot.edit",
                json!({"dataFolder": folder, "slug": "a", "avatar": null}),
            ),
        ];
        for (method, params) in cases {
            let call = request(method, params.clone());
            let refused = match method {
                "bot.make" => make(&call),
                _ => edit(&call),
            }
            .expect_err("refused");
            assert_eq!(refused.code.as_str(), "bad_request", "{method} {params}");
            assert!(
                refused.message.contains("must be a string"),
                "{method} {params}: {}",
                refused.message
            );
        }

        let model = request("bot.edit", json!({"slug": "a", "model": null}));
        assert_eq!(optional_text(&model, "model").unwrap(), None);
        let absent = request("bot.edit", json!({"slug": "a"}));
        assert_eq!(text_unless_null(&absent, "name").unwrap(), None);
    }

    #[test]
    fn each_refusal_is_answered_with_the_code_that_says_what_went_wrong() {
        let cases = [
            (Refused::Ceiling, "bad_request"),
            (
                Refused::Entry {
                    index: 0,
                    slug: None,
                    reason: Reason::BadSlug,
                },
                "bad_request",
            ),
            (Refused::NoFolder, "no_home"),
            (Refused::Incognito, "no_home"),
            (Refused::Io(problem()), "no_home"),
            (Refused::History(problem()), "no_home"),
            (Refused::Unreadable(problem()), "state_unusable"),
            (Refused::OldFile(problem()), "state_unusable"),
            (
                Refused::Locked(Path::new("/state/bots.json.lock").to_path_buf()),
                "state_unusable",
            ),
        ];
        for (refused, code) in cases {
            assert_eq!(failure(&refused).code.as_str(), code, "{refused:?}");
        }
    }
}
