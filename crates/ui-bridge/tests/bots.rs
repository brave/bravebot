#[allow(
    dead_code,
    reason = "this test uses the isolation and not the project helper"
)]
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::allowlist::{DENIED, OFFERED};
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{ErrorCode, Failure, Request};
use serde_json::{Value, json};
use std::path::PathBuf;

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, Failure> {
    let line = json!({ "id": 1, "method": method, "params": params }).to_string();
    bridge.dispatch(&Request::parse(&line).expect("well formed"))
}

fn bridge() -> Bridge {
    Bridge::new(Box::new(|_| {}))
}

fn data_folder() -> PathBuf {
    let folder = bravebot_agent::home::profile()
        .expect("the profile of the isolated test")
        .join("gui");
    std::fs::create_dir_all(&folder).expect("a data folder");
    folder
}

fn bots_file() -> PathBuf {
    bravebot_agent::home::directory()
        .expect("the state directory of the isolated test")
        .join("bots.json")
}

fn bot(slug: &str) -> Value {
    json!({
        "slug": slug, "name": slug, "purpose": "Does a thing.", "avatar": slug,
        "model": null, "home": format!("/gui/bot-homes/{slug}"), "definition": slug,
        "session": null, "conversations": [], "archived": 0, "retired": 0,
        "created": 5, "updated": 6,
    })
}

#[test]
fn the_methods_are_denied_to_the_host_and_offered_to_no_client() {
    for method in ["bot.list", "bot.replace"] {
        assert!(
            DENIED.contains(&method),
            "{method} is not denied to the host"
        );
        assert!(
            OFFERED.iter().all(|operation| operation.method != method),
            "{method} is offered to a client of the host"
        );
    }
}

#[test]
fn a_request_without_a_data_folder_or_a_list_is_a_bad_request() {
    let mut bridge = bridge();
    for (method, params) in [
        ("bot.list", json!({})),
        ("bot.list", json!({"dataFolder": "gui"})),
        ("bot.replace", json!({"bots": []})),
        ("bot.replace", json!({"dataFolder": "/gui"})),
        ("bot.replace", json!({"dataFolder": "/gui", "bots": {}})),
    ] {
        let refused = call(&mut bridge, method, params.clone()).expect_err("refused");
        assert_eq!(refused.code, ErrorCode::BadRequest, "{method} {params}");
    }
}

#[test]
fn a_bot_replaced_through_the_bridge_is_in_the_bots_file_and_listed_after_a_restart() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let old = br#"{"layout": null, "bots": []}"#.to_vec();
    std::fs::write(data.join("bravebot-ui.json"), &old).unwrap();

    let mut first = bridge();
    let empty = call(&mut first, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(empty["bots"], json!([]));
    assert_eq!(empty["dropped"], json!([]));
    assert_eq!(empty["unreadable"], Value::Null);
    let written = call(
        &mut first,
        "bot.replace",
        json!({"dataFolder": data, "bots": [bot("zed"), bot("amy")]}),
    )
    .unwrap();
    assert_eq!(written["bots"][0]["slug"], json!("amy"));

    let mut restarted = bridge();
    let listed = call(&mut restarted, "bot.list", json!({"dataFolder": data})).unwrap();
    let slugs: Vec<_> = listed["bots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["amy", "zed"]);
    let on_disk: Value = serde_json::from_slice(&std::fs::read(bots_file()).unwrap()).unwrap();
    assert_eq!(on_disk["bots"].as_array().unwrap().len(), 2);
    assert_eq!(std::fs::read(data.join("bravebot-ui.json")).unwrap(), old);
}

#[test]
fn what_a_read_dropped_and_a_file_it_could_not_use_are_answered_beside_the_rows() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let state = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let mut entries: Vec<Value> = (0..100).map(|n| bot(&format!("b{n:03}"))).collect();
    entries.insert(3, bot("Bad_Slug"));
    entries.push(bot("late"));
    std::fs::write(bots_file(), json!({"bots": entries}).to_string()).unwrap();

    let mut bridge = bridge();
    let listed = call(&mut bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(listed["bots"].as_array().unwrap().len(), 100);
    let dropped: Vec<_> = listed["dropped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|each| {
            (
                each["slug"].as_str().unwrap(),
                each["reason"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(dropped.len(), 2);
    assert_eq!(dropped[0].0, "Bad_Slug");
    assert!(dropped[0].1.contains("slug"), "{dropped:?}");
    assert_eq!(dropped[1].0, "late");
    assert!(dropped[1].1.contains("100th"), "{dropped:?}");

    std::fs::write(bots_file(), "{ not json").unwrap();
    let broken = call(&mut bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(broken["bots"], json!([]));
    assert_eq!(
        broken["unreadable"]["path"],
        json!(bots_file().display().to_string())
    );
    let refused = call(
        &mut bridge,
        "bot.replace",
        json!({"dataFolder": data, "bots": [bot("a")]}),
    )
    .expect_err("refused over a file that did not parse");
    assert!(
        refused.message.contains(&bots_file().display().to_string()),
        "{}",
        refused.message
    );
    assert_eq!(std::fs::read_to_string(bots_file()).unwrap(), "{ not json");
}

#[test]
fn a_read_says_whether_rows_in_the_old_file_still_wait_to_be_moved() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let state = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let mut bridge = bridge();

    std::fs::write(
        data.join("bravebot-ui.json"),
        json!({"bots": [bot("old")]}).to_string(),
    )
    .unwrap();
    let moved = call(&mut bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(moved["bots"][0]["slug"], json!("old"));
    assert_eq!(moved["oldRowsWaiting"].as_bool(), Some(false));

    std::fs::remove_file(bots_file()).unwrap();
    std::fs::write(data.join("bravebot-ui.json"), "{\"bots\": [").unwrap();
    let stuck = call(&mut bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(stuck["bots"], json!([]));
    assert_eq!(stuck["oldRowsWaiting"].as_bool(), Some(true));
}

#[test]
fn a_state_file_that_cannot_be_used_is_answered_as_unusable_and_not_as_no_home() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let state = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let mut bridge = bridge();
    let replace = |bridge: &mut Bridge| {
        call(
            bridge,
            "bot.replace",
            json!({"dataFolder": data, "bots": [bot("ok")]}),
        )
        .expect_err("refused")
    };

    std::fs::write(bots_file(), "{ not json").unwrap();
    assert_eq!(replace(&mut bridge).code.as_str(), "state_unusable");
    std::fs::remove_file(bots_file()).unwrap();

    std::fs::write(data.join("bravebot-ui.json"), "{\"bots\": 3}").unwrap();
    assert_eq!(replace(&mut bridge).code.as_str(), "state_unusable");
    std::fs::remove_file(data.join("bravebot-ui.json")).unwrap();

    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state.join("bots.json.lock"))
        .unwrap();
    held.lock().unwrap();
    let refused = replace(&mut bridge);
    assert_eq!(refused.code.as_str(), "state_unusable");
    assert!(
        refused.message.contains("bots.json.lock"),
        "{}",
        refused.message
    );
}

#[test]
fn a_refused_replacement_writes_nothing_and_says_why() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let mut bridge = bridge();

    let over: Vec<Value> = (0..101).map(|n| bot(&format!("b{n:03}"))).collect();
    let ceiling = call(
        &mut bridge,
        "bot.replace",
        json!({"dataFolder": data, "bots": over}),
    )
    .expect_err("refused");
    assert_eq!(ceiling.code, ErrorCode::BadRequest);
    let dropped = call(
        &mut bridge,
        "bot.replace",
        json!({"dataFolder": data, "bots": [bot("ok"), bot("Bad_Slug")]}),
    )
    .expect_err("refused");
    assert_eq!(dropped.code, ErrorCode::BadRequest);
    assert!(dropped.message.contains("Bad_Slug"), "{}", dropped.message);
    assert!(!bots_file().exists());

    std::fs::write(data.join("bravebot-ui.json"), "{\"bots\": 3}").unwrap();
    let old = call(
        &mut bridge,
        "bot.replace",
        json!({"dataFolder": data, "bots": [bot("ok")]}),
    )
    .expect_err("refused");
    assert!(
        old.message
            .contains(&data.join("bravebot-ui.json").display().to_string()),
        "{}",
        old.message
    );
    assert!(!bots_file().exists());
}

#[test]
fn the_methods_that_make_and_change_a_bot_are_denied_to_the_host_and_offered_to_no_client() {
    for method in ["bot.make", "bot.edit", "bot.set_model", "bot.ensure_home"] {
        assert!(
            DENIED.contains(&method),
            "{method} is not denied to the host"
        );
        assert!(
            OFFERED.iter().all(|operation| operation.method != method),
            "{method} is offered to a client of the host"
        );
    }
}

fn listed_slugs(bridge: &mut Bridge, data: &PathBuf) -> Vec<String> {
    let listed = call(bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    listed["bots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["slug"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_bot_made_edited_and_given_a_model_through_the_bridge_is_listed_after_a_restart() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let state = bravebot_agent::home::directory().unwrap();
    let mut first = bridge();

    let made = call(
        &mut first,
        "bot.make",
        json!({"dataFolder": data, "name": "Code Reviewer", "purpose": "Reviews code.\nCarefully.", "model": null}),
    )
    .unwrap();
    assert_eq!(made["bot"]["slug"], json!("code-reviewer"));
    assert_eq!(made["bot"]["avatar"], json!("code-reviewer"));
    assert_eq!(made["bot"]["definition"], json!("code-reviewer"));
    assert_eq!(made["bot"]["model"], Value::Null);
    assert_eq!(made["homeMade"], json!(true));
    let home = data.join("bot-homes/code-reviewer");
    assert_eq!(made["bot"]["home"], json!(home.to_str().unwrap()));
    assert!(home.is_dir());
    let definition = state.join("agents/code-reviewer.md");
    assert!(
        std::fs::read_to_string(&definition)
            .unwrap()
            .contains("Reviews code.\nCarefully.")
    );

    let edited = call(
        &mut first,
        "bot.edit",
        json!({"dataFolder": data, "slug": "code-reviewer", "name": "Reviewer", "purpose": "Reviews prose.", "avatar": "seed"}),
    )
    .unwrap();
    assert_eq!(edited["bot"]["name"], json!("Reviewer"));
    assert_eq!(edited["bot"]["slug"], json!("code-reviewer"));
    assert_eq!(edited["bot"]["avatar"], json!("seed"));
    assert_eq!(edited["homeMade"], json!(true));
    assert!(
        std::fs::read_to_string(&definition)
            .unwrap()
            .contains("Reviews prose.")
    );

    let set = call(
        &mut first,
        "bot.set_model",
        json!({"dataFolder": data, "slug": "code-reviewer", "model": "provider/model-a"}),
    )
    .unwrap();
    assert_eq!(set["bot"]["model"], json!("provider/model-a"));
    assert!(
        std::fs::read_to_string(&definition)
            .unwrap()
            .contains("model: 'provider/model-a'")
    );
    let untouched = call(
        &mut first,
        "bot.edit",
        json!({"dataFolder": data, "slug": "code-reviewer", "name": "Reviewer Two"}),
    )
    .unwrap();
    assert_eq!(untouched["bot"]["model"], json!("provider/model-a"));
    let cleared = call(
        &mut first,
        "bot.edit",
        json!({"dataFolder": data, "slug": "code-reviewer", "model": null}),
    )
    .unwrap();
    assert_eq!(cleared["bot"]["model"], Value::Null);
    let cleared_by_method = call(
        &mut first,
        "bot.set_model",
        json!({"dataFolder": data, "slug": "code-reviewer", "model": null}),
    )
    .unwrap();
    assert_eq!(cleared_by_method["bot"]["model"], Value::Null);

    std::fs::remove_dir_all(&home).unwrap();
    let ensured = call(
        &mut first,
        "bot.ensure_home",
        json!({"dataFolder": data, "slug": "code-reviewer"}),
    )
    .unwrap();
    assert_eq!(ensured, json!({"homeMade": true}));
    assert!(home.is_dir());

    let mut restarted = bridge();
    assert_eq!(listed_slugs(&mut restarted, &data), ["code-reviewer"]);
    let listed = call(&mut restarted, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(listed["bots"][0]["name"], json!("Reviewer Two"));
    assert_eq!(listed["bots"][0]["purpose"], json!("Reviews prose."));
    assert_eq!(listed["bots"][0]["model"], Value::Null);
}

#[test]
fn a_refused_make_edit_or_home_is_answered_with_the_reason_and_the_code_for_it() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let mut bridge = bridge();
    let refuse = |bridge: &mut Bridge, method: &str, params: Value| {
        call(bridge, method, params).expect_err("refused")
    };

    let blank = refuse(
        &mut bridge,
        "bot.make",
        json!({"dataFolder": data, "name": " ", "purpose": "P."}),
    );
    assert_eq!(blank.code, ErrorCode::BadRequest);
    assert!(blank.message.contains("name"), "{}", blank.message);
    let model = refuse(
        &mut bridge,
        "bot.make",
        json!({"dataFolder": data, "name": "N", "purpose": "P.", "model": "inherit"}),
    );
    assert_eq!(model.code, ErrorCode::BadRequest);
    assert!(model.message.contains("model"), "{}", model.message);
    let no_definition = refuse(
        &mut bridge,
        "bot.make",
        json!({"dataFolder": data, "name": "N", "purpose": "\u{85}"}),
    );
    assert_eq!(no_definition.code, ErrorCode::BadRequest);
    assert!(
        no_definition.message.contains("definition"),
        "{}",
        no_definition.message
    );
    assert!(!bots_file().exists());

    let unknown = refuse(
        &mut bridge,
        "bot.edit",
        json!({"dataFolder": data, "slug": "ghost", "name": "G"}),
    );
    assert_eq!(unknown.code, ErrorCode::BadRequest);
    assert!(unknown.message.contains("ghost"), "{}", unknown.message);
    for (method, params) in [
        (
            "bot.set_model",
            json!({"dataFolder": data, "slug": "ghost", "model": "m"}),
        ),
        (
            "bot.ensure_home",
            json!({"dataFolder": data, "slug": "ghost"}),
        ),
    ] {
        let refused = refuse(&mut bridge, method, params);
        assert_eq!(refused.code, ErrorCode::BadRequest, "{method}");
        assert!(refused.message.contains("ghost"), "{}", refused.message);
    }

    let ceiling: Vec<Value> = (0..100).map(|n| bot(&format!("b{n:03}"))).collect();
    call(
        &mut bridge,
        "bot.replace",
        json!({"dataFolder": data, "bots": ceiling}),
    )
    .unwrap();
    let before = std::fs::read_to_string(bots_file()).unwrap();
    let full = refuse(
        &mut bridge,
        "bot.make",
        json!({"dataFolder": data, "name": "One Too Many", "purpose": "P."}),
    );
    assert_eq!(full.code, ErrorCode::BadRequest);
    assert!(full.message.contains("100"), "{}", full.message);
    assert_eq!(std::fs::read_to_string(bots_file()).unwrap(), before);

    std::fs::write(bots_file(), "{ not json").unwrap();
    let unreadable = refuse(
        &mut bridge,
        "bot.make",
        json!({"dataFolder": data, "name": "N", "purpose": "P."}),
    );
    assert_eq!(unreadable.code.as_str(), "state_unusable");
    assert!(
        unreadable
            .message
            .contains(&bots_file().display().to_string()),
        "{}",
        unreadable.message
    );
    assert_eq!(std::fs::read_to_string(bots_file()).unwrap(), "{ not json");
}

#[test]
fn a_set_model_request_without_a_model_is_refused_and_leaves_the_bots_model_alone() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let data = data_folder();
    let mut bridge = bridge();
    call(
        &mut bridge,
        "bot.make",
        json!({"dataFolder": data, "name": "Modeled", "purpose": "P.", "model": "provider/model-a"}),
    )
    .unwrap();
    let before = call(&mut bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(before["bots"][0]["model"], json!("provider/model-a"));

    let refused = call(
        &mut bridge,
        "bot.set_model",
        json!({"dataFolder": data, "slug": "modeled"}),
    )
    .expect_err("refused");
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(refused.message.contains("model"), "{}", refused.message);

    let after = call(&mut bridge, "bot.list", json!({"dataFolder": data})).unwrap();
    assert_eq!(after["bots"][0]["model"], json!("provider/model-a"));
    assert_eq!(after["bots"][0]["updated"], before["bots"][0]["updated"]);

    let cleared = call(
        &mut bridge,
        "bot.set_model",
        json!({"dataFolder": data, "slug": "modeled", "model": null}),
    )
    .unwrap();
    assert_eq!(cleared["bot"]["model"], Value::Null);
}
