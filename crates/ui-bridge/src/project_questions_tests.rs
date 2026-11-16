//! The questions a desktop session puts once its trust question is answered (PERM-15): one about
//! the `allow` rules the checkout proposed that the grant record does not already hold.
//!
//! Each test runs in a profile of its own, since a yes is written to the grant record under it.

use super::*;
use bravebot_core::permissions::Decision;

const THE_RULE: &str = "WebFetch(domain:example.test)";
const ANOTHER_RULE: &str = "WebFetch(domain:another.test)";

/// A bridge, and every event it sent.
fn hearing() -> (Bridge, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let held = Arc::clone(&events);
    let bridge = Bridge::new(Box::new(move |event| held.lock().unwrap().push(event)));
    (bridge, events)
}

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, Failure> {
    let request =
        Request::parse(&json!({"id": 1, "method": method, "params": params}).to_string()).unwrap();
    bridge.dispatch(&request)
}

/// The data of every event named `name`, in the order sent.
fn sent(events: &Arc<Mutex<Vec<Event>>>, name: &str) -> Vec<Value> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.name == name)
        .map(|event| event.data.clone())
        .collect()
}

/// A project whose checkout proposes `allow`.
fn proposing(name: &str, allow: &[&str]) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(project.join(".bravebot")).unwrap();
    the_checkout_proposes(&project, allow);
    project.canonicalize().unwrap()
}

fn the_checkout_proposes(project: &Path, allow: &[&str]) {
    std::fs::write(
        project.join(".bravebot/settings.json"),
        json!({"permissions": {"allow": allow}}).to_string(),
    )
    .unwrap();
}

/// Open a session in `project` and answer yes to its trust question where it is put. Returns the
/// handle and the response that settled the questions after it.
fn opened_and_trusted(bridge: &mut Bridge, project: &Path) -> (String, Value) {
    let opened = call(bridge, "session.new", json!({"directory": project})).unwrap();
    let session = opened["session"].as_str().unwrap().to_string();
    let settled = if opened["remembered"].is_null() {
        call(
            bridge,
            "trust.reply",
            json!({"session": session, "trusted": true}),
        )
        .unwrap()
    } else {
        opened
    };
    (session, settled)
}

/// The grant record for `project`, as the terminal reads it.
fn grant_record(project: &Path) -> PathBuf {
    bravebot_agent::granted::Store::new(&bravebot_agent::home::directory().unwrap(), project)
        .path()
        .to_path_buf()
}

fn texts(rules: &Value) -> Vec<String> {
    rules
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule["rule"].as_str().unwrap().to_string())
        .collect()
}

fn reply(bridge: &mut Bridge, session: &str, question: &Value, granted: bool) -> Value {
    call(
        bridge,
        "rules.reply",
        json!({"session": session, "request": question["request"], "granted": granted}),
    )
    .unwrap()
}

/// Whether the session's rules allow fetching from `host`.
fn allows(bridge: &Bridge, session: &str, host: &str) -> bool {
    let state = bridge.open[session].state.lock().unwrap();
    matches!(
        state.answers.rules.permissions.for_host(host),
        Decision::Ruled(bravebot_core::permissions::Ruling::Allow)
    )
}

/// PERM-12: a checkout that proposes nothing is asked nothing, and the session works once trusted.
#[test]
fn nothing_proposed_is_nothing_asked() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = test_profile::project("nothing-proposed");
    std::fs::create_dir_all(&project).unwrap();
    let (mut bridge, events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    assert_eq!(settled["rulesQuestion"], Value::Null, "{settled}");
    assert!(sent(&events, "rules.request").is_empty());
    assert_eq!(settled["settingsRules"]["grantedRecord"], Value::Null);
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// PERM-15: the rules are put as one question once the trust question is answered, on the reply
/// and as an event, and turns are refused until it is answered. A yes grants every rule listed,
/// keeps the grant, and the next session here is not asked about the same rules again.
#[test]
fn a_yes_grants_every_rule_listed_and_is_not_asked_for_again() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("yes-grants", &[THE_RULE, ANOTHER_RULE]);
    let record = grant_record(&project);
    let (mut bridge, events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);

    let question = settled["rulesQuestion"].clone();
    assert_eq!(
        texts(&question["rules"]),
        [THE_RULE, ANOTHER_RULE],
        "{question}"
    );
    assert_eq!(question["grantedBefore"], json!([]));
    assert_eq!(question["keeping"], json!(record.display().to_string()));
    assert_eq!(
        sent(&events, "rules.request"),
        std::slice::from_ref(&question)
    );
    let refused = bridge.open[&session].ready_for_work().unwrap_err();
    assert!(
        refused.message.contains("rules.reply"),
        "{}",
        refused.message
    );
    assert!(!allows(&bridge, &session, "example.test"));

    let answered = reply(&mut bridge, &session, &question, true);
    assert_eq!(answered["granted"], true, "{answered}");
    assert_eq!(answered["kept"], true, "{answered}");
    let rules = &answered["settingsRules"];
    assert_eq!(rules["allow"], json!([THE_RULE, ANOTHER_RULE]), "{rules}");
    assert_eq!(rules["proposed"], json!([]), "{rules}");
    assert_eq!(texts(&rules["granted"]), [THE_RULE, ANOTHER_RULE]);
    assert_eq!(rules["granted"][0]["before"], false);
    assert_eq!(rules["grantedRecord"], json!(record.display().to_string()));
    assert!(record.is_file(), "the yes was not kept");
    assert!(allows(&bridge, &session, "example.test"));
    assert!(allows(&bridge, &session, "another.test"));
    assert!(bridge.open[&session].ready_for_work().is_ok());
    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(listed["settingsRules"], *rules);

    // The yes about the directory was kept as well, so the next session is settled as it opens.
    let (again, opened) = opened_and_trusted(&mut bridge, &project);
    assert!(!opened["remembered"].is_null(), "{opened}");
    assert_eq!(opened["rulesQuestion"], Value::Null, "{opened}");
    assert_eq!(sent(&events, "rules.request").len(), 1);
    assert_eq!(opened["settingsRules"]["granted"][0]["before"], true);
    assert!(allows(&bridge, &again, "example.test"));
    assert!(bridge.open[&again].ready_for_work().is_ok());
}

/// PERM-15: a rule the record holds is not asked about again, and one the checkout edited since is
/// a rule nobody granted, so it is.
#[test]
fn an_edited_rule_is_asked_about_and_an_unchanged_one_is_not() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("edited-rule", &[THE_RULE, ANOTHER_RULE]);
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    reply(&mut bridge, &session, &settled["rulesQuestion"], true);

    the_checkout_proposes(&project, &[THE_RULE, "WebFetch(domain:edited.test)"]);
    let (again, opened) = opened_and_trusted(&mut bridge, &project);
    let question = &opened["rulesQuestion"];
    assert_eq!(texts(&question["rules"]), ["WebFetch(domain:edited.test)"]);
    assert_eq!(texts(&question["grantedBefore"]), [THE_RULE]);
    assert!(allows(&bridge, &again, "example.test"));
    assert!(!allows(&bridge, &again, "edited.test"));
    assert!(!allows(&bridge, &again, "another.test"));
    assert!(bridge.open[&again].ready_for_work().is_err());
}

/// PERM-15: a no grants none of the rules and keeps nothing. The session still opens, and the
/// rules are still reported as proposed, so the person knows they are not in force.
#[test]
fn a_no_grants_none_and_the_session_still_opens() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("no-grants", &[THE_RULE]);
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let answered = reply(&mut bridge, &session, &settled["rulesQuestion"], false);
    assert_eq!(answered["granted"], false);
    assert_eq!(answered["kept"], false);
    assert_eq!(answered["settingsRules"]["allow"], json!([]));
    assert_eq!(answered["settingsRules"]["granted"], json!([]));
    assert_eq!(texts(&answered["settingsRules"]["proposed"]), [THE_RULE]);
    assert!(!grant_record(&project).exists(), "a no was kept");
    assert!(!allows(&bridge, &session, "example.test"));
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// INCOG-5: in an incognito session a yes grants the rules for that session alone. Nothing is
/// written, the question offers to keep nothing, and the next session is asked again.
#[test]
fn an_incognito_yes_grants_the_rules_for_the_session_alone() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    bravebot_core::incognito::engage();
    let project = proposing("incognito-grants", &[THE_RULE]);
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let question = settled["rulesQuestion"].clone();
    assert_eq!(question["keeping"], Value::Null, "{question}");
    let answered = reply(&mut bridge, &session, &question, true);
    assert_eq!(answered["kept"], false, "{answered}");
    assert!(allows(&bridge, &session, "example.test"));
    assert!(
        !grant_record(&project).exists(),
        "an incognito yes was kept"
    );

    let (_, again) = opened_and_trusted(&mut bridge, &project);
    assert_eq!(texts(&again["rulesQuestion"]["rules"]), [THE_RULE]);
}

/// RPCVIEW-6: a reply answers the question stored under its id and nothing else. An id that is
/// not the one out, another session's, and a second answer to one question are refused, and none
/// of them changes what the session holds.
#[test]
fn a_repeat_or_stale_reply_is_refused() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("stale-reply", &[THE_RULE]);
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let question = settled["rulesQuestion"].clone();
    let id = question["request"].as_u64().unwrap();

    let refused = |outcome: Result<Value, Failure>| {
        let failure = outcome.expect_err("refused");
        assert_eq!(
            failure.code,
            ErrorCode::NoSuchRequest,
            "{}",
            failure.message
        );
    };
    refused(call(
        &mut bridge,
        "rules.reply",
        json!({"session": session, "request": id + 1, "granted": true}),
    ));
    assert!(!allows(&bridge, &session, "example.test"));
    let missing = call(
        &mut bridge,
        "rules.reply",
        json!({"session": session, "request": id}),
    )
    .expect_err("an answer that is not a boolean is no answer");
    assert_eq!(missing.code, ErrorCode::BadRequest);
    // Another session's question is not this one's, though both are about the same rules.
    let (other, _) = {
        let opened = call(&mut bridge, "session.new", json!({"directory": project})).unwrap();
        let other = opened["session"].as_str().unwrap().to_string();
        (other, opened)
    };
    refused(call(
        &mut bridge,
        "rules.reply",
        json!({"session": other, "request": id, "granted": true}),
    ));

    reply(&mut bridge, &session, &question, true);
    refused(call(
        &mut bridge,
        "rules.reply",
        json!({"session": session, "request": id, "granted": false}),
    ));
    assert!(
        allows(&bridge, &session, "example.test"),
        "a repeat undid the yes"
    );
}

/// PERM-15: while the question is out, nothing that works in the session starts: no turn, no run
/// and no watch. There is no default answer and no dismissal.
#[test]
fn work_is_refused_until_the_rules_are_answered() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("refused-until", &[THE_RULE]);
    std::fs::write(project.join("notes.md"), "notes").unwrap();
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let refused = bridge.open[&session].ready_for_work().unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(
        refused.message.contains("rules.reply"),
        "{}",
        refused.message
    );
    let watch = call(
        &mut bridge,
        "watches.add",
        json!({"session": session, "path": "notes.md"}),
    )
    .expect_err("a watch was armed with the question out");
    assert_eq!(watch.code, ErrorCode::BadRequest);

    reply(&mut bridge, &session, &settled["rulesQuestion"], false);
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// A session with a written record, in `project`, with the trust its map holds and the question
/// about the rules out, as one opened before the person answered.
fn a_parent_asking(bridge: &mut Bridge, project: &Path) -> String {
    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(bravebot_aichat::protocol::Message::user("work"));
    let record: Record = serde_json::from_value(json!({
        "id": "asking-parent", "directory": project, "title": "work",
        "started": 1, "updated": 1,
        "conversation": conversation.snapshot()
    }))
    .unwrap();
    let mut trust = TrustStore::new(project);
    trust.trust(&bravebot_agent::workspace::key_of(project));
    let state = State::resumed(project, &record, trust).expect("the record is free");
    let parent = bridge.mint(Open::opened(
        project.to_path_buf(),
        state,
        true,
        None,
        false,
    ));
    bridge.open.get_mut(&parent).unwrap().questions =
        Questions::Unsettled(Box::new(crate::settings::layers(Some(project), None)));
    bridge.settle_questions(&parent, bravebot_agent::opening::Whence::Asked);
    parent
}

/// A fork inherits what its parent holds, and takes over a question the parent had not answered,
/// under an id of its own: the child is refused turns until it answers, an answer to the parent's
/// id is not the child's, and the child's answer leaves the parent's question out.
#[test]
fn a_fork_takes_over_the_question_its_parent_had_not_answered() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("fork-takes-over", &[THE_RULE]);
    let (mut bridge, events) = hearing();
    let parent = a_parent_asking(&mut bridge, &project);
    let asked = sent(&events, "rules.request");
    let [parents] = asked.as_slice() else {
        panic!("{asked:?}");
    };

    let forked = call(
        &mut bridge,
        "session.fork",
        json!({"session": parent, "prompt": 0, "text": "work"}),
    )
    .unwrap();
    let child = forked["session"].as_str().unwrap().to_string();
    let question = forked["rulesQuestion"].clone();
    assert_eq!(texts(&question["rules"]), [THE_RULE], "{forked}");
    assert_ne!(question["request"], parents["request"]);
    assert_eq!(sent(&events, "rules.request").last(), Some(&question));
    assert!(bridge.open[&child].ready_for_work().is_err());

    let stale = call(
        &mut bridge,
        "rules.reply",
        json!({"session": child, "request": parents["request"], "granted": true}),
    )
    .expect_err("the parent's id answered the child's question");
    assert_eq!(stale.code, ErrorCode::NoSuchRequest);

    reply(&mut bridge, &child, &question, true);
    assert!(allows(&bridge, &child, "example.test"));
    assert!(bridge.open[&child].ready_for_work().is_ok());
    assert!(bridge.open[&parent].ready_for_work().is_err());
    assert!(!allows(&bridge, &parent, "example.test"));

    // A fork of a parent that answered inherits the answer and asks nothing.
    reply(&mut bridge, &parent, parents, false);
    let forked = call(
        &mut bridge,
        "session.fork",
        json!({"session": parent, "prompt": 0, "text": "work"}),
    )
    .unwrap();
    assert_eq!(forked["rulesQuestion"], Value::Null, "{forked}");
    let grandchild = forked["session"].as_str().unwrap();
    assert!(bridge.open[grandchild].ready_for_work().is_ok());
}

/// The mode that answers every question answers this one (`mode_answers`): the rules hold for the
/// session with nothing put, nothing is written to the record, and the record says the mode
/// answered (MODE-4).
#[test]
fn bypass_answers_the_rules_for_the_session_alone() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("bypass-answers", &[THE_RULE]);
    let (mut bridge, events) = hearing();
    let opened = call(&mut bridge, "session.new", json!({"directory": project})).unwrap();
    let session = opened["session"].as_str().unwrap().to_string();
    // No window may choose bypass (MODE-4); a session in it is one a host started that way.
    bridge.open[&session]
        .permission_mode
        .set(PermissionMode::Bypass);
    let settled = call(
        &mut bridge,
        "trust.reply",
        json!({"session": session, "trusted": true}),
    )
    .unwrap();
    assert_eq!(settled["rulesQuestion"], Value::Null, "{settled}");
    assert!(sent(&events, "rules.request").is_empty());
    assert!(allows(&bridge, &session, "example.test"));
    assert!(
        !grant_record(&project).exists(),
        "what bypass granted was kept"
    );
    assert!(bridge.open[&session].state.lock().unwrap().answered_by_mode);
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// A bridge built for a client that cannot answer the questions puts none: the rules stay proposed
/// and reported, nothing is added to its responses, and the session works once trusted.
#[test]
fn a_bridge_not_putting_the_questions_grants_nothing_and_asks_nothing() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("not-putting", &[THE_RULE]);
    let (bridge, events) = hearing();
    let mut bridge = bridge.not_putting_project_questions();
    let opened = call(&mut bridge, "session.new", json!({"directory": project})).unwrap();
    let session = opened["session"].as_str().unwrap().to_string();
    let settled = call(
        &mut bridge,
        "trust.reply",
        json!({"session": session, "trusted": true}),
    )
    .unwrap();
    assert!(settled.get("rulesQuestion").is_none(), "{settled}");
    assert!(opened.get("rulesQuestion").is_none(), "{opened}");
    assert!(sent(&events, "rules.request").is_empty());
    assert_eq!(texts(&opened["settingsRules"]["proposed"]), [THE_RULE]);
    assert!(!allows(&bridge, &session, "example.test"));
    assert!(bridge.open[&session].ready_for_work().is_ok());
    let refused = call(
        &mut bridge,
        "rules.reply",
        json!({"session": session, "request": 1, "granted": true}),
    )
    .expect_err("an answer to a question never put");
    assert_eq!(refused.code, ErrorCode::NoSuchRequest);
}
