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
