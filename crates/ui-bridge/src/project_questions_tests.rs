//! The questions a desktop session puts once its trust question is answered (PERM-15): one about
//! the `allow` rules the checkout proposed that the grant record does not already hold, then one
//! about each directory its settings name (TRUST-9).
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
    assert_eq!(
        answered["kept"],
        Value::Null,
        "a no had something to keep: {answered}"
    );
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
    assert_eq!(
        answered["kept"],
        Value::Null,
        "an incognito yes was said to be written, or to have failed: {answered}"
    );
    assert!(allows(&bridge, &session, "example.test"));
    assert!(
        !grant_record(&project).exists(),
        "an incognito yes was kept"
    );

    let (_, again) = opened_and_trusted(&mut bridge, &project);
    assert_eq!(texts(&again["rulesQuestion"]["rules"]), [THE_RULE]);
}

/// PERM-15: a yes that cannot be written to the grant record still grants the rules for this
/// session, and the reply says it was not kept, so the window can say the next session will ask
/// again. The next session does.
#[test]
fn a_yes_the_record_cannot_take_holds_for_the_session_and_says_so() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("unwritable-grant", &[THE_RULE]);
    let record = grant_record(&project);
    // A file where the record's directory would be, which no write gets past whoever runs this.
    let blocked = record.parent().unwrap();
    std::fs::create_dir_all(blocked.parent().unwrap()).unwrap();
    std::fs::write(blocked, "").unwrap();
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let answered = reply(&mut bridge, &session, &settled["rulesQuestion"], true);
    std::fs::remove_file(blocked).unwrap();
    assert_eq!(answered["granted"], true, "{answered}");
    assert_eq!(answered["kept"], false, "{answered}");
    assert!(allows(&bridge, &session, "example.test"));
    assert!(bridge.open[&session].ready_for_work().is_ok());
    assert!(!record.exists(), "the record was written after all");

    let (_, again) = opened_and_trusted(&mut bridge, &project);
    assert_eq!(texts(&again["rulesQuestion"]["rules"]), [THE_RULE]);
}

/// PERM-15: a grant names the session it was given in, as the terminal's does, also for a
/// session that has no record yet: the name written is the one its record is written under.
#[test]
fn a_grant_names_the_session_before_it_has_a_record() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("grant-names-session", &[THE_RULE]);
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    assert!(bridge.open[&session].state.lock().unwrap().handle.is_none());
    reply(&mut bridge, &session, &settled["rulesQuestion"], true);

    let line: Value = serde_json::from_str(
        std::fs::read_to_string(grant_record(&project))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    let named = line["session"].as_str().unwrap().to_string();
    assert!(!named.is_empty(), "the grant named no session: {line}");
    let mut state = bridge.open[&session].state.lock().unwrap();
    assert!(state.handle.is_none(), "answering wrote a record");
    assert_eq!(state.handle_or_begin(&project).id(), named);
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
    // A window refused as stale reads which question is out from the permissions list.
    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(listed["rulesQuestion"], question, "{listed}");
    assert_eq!(listed["folderQuestion"], Value::Null, "{listed}");
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
    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(listed["rulesQuestion"], Value::Null, "{listed}");
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
    a_recorded_session(bridge, project, bravebot_agent::opening::Whence::Asked)
}

/// A session with a written record, in `project`, with the trust its map holds, its questions
/// settled as a session that came to its trust answer by `whence` settles them.
fn a_recorded_session(
    bridge: &mut Bridge,
    project: &Path,
    whence: bravebot_agent::opening::Whence,
) -> String {
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
    bridge.settle_questions(&parent, whence);
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
    let (project, beside, _) = naming(
        "not-putting",
        json!({"allow": [THE_RULE], "additionalDirectories": ["../not-putting-beside"]}),
    );
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
    assert!(settled.get("folderQuestion").is_none(), "{settled}");
    assert!(sent(&events, "folder.request").is_empty());
    assert_eq!(holds(&bridge, &session, &beside), (vec![], false));
    let refused = call(
        &mut bridge,
        "folder.reply",
        json!({"session": session, "request": 1, "open": true}),
    )
    .expect_err("an answer to a question never put");
    assert_eq!(refused.code, ErrorCode::NoSuchRequest);
}

/// A project whose checkout writes `permissions`, and two directories beside it.
fn naming(name: &str, permissions: Value) -> (PathBuf, PathBuf, PathBuf) {
    let project = test_profile::project(name);
    std::fs::create_dir_all(project.join(".bravebot")).unwrap();
    let beside = project.parent().unwrap().join(format!("{name}-beside"));
    let other = project.parent().unwrap().join(format!("{name}-other"));
    std::fs::create_dir_all(&beside).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        project.join(".bravebot/settings.json"),
        json!({ "permissions": permissions }).to_string(),
    )
    .unwrap();
    (
        project.canonicalize().unwrap(),
        beside.canonicalize().unwrap(),
        other.canonicalize().unwrap(),
    )
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

fn answer_folder(bridge: &mut Bridge, session: &str, question: &Value, open: bool) -> Value {
    call(
        bridge,
        "folder.reply",
        json!({"session": session, "request": question["request"], "open": open}),
    )
    .unwrap()
}

/// What the session holds open beside its project, and whether its map vouches for `directory`.
fn holds(bridge: &Bridge, session: &str, directory: &Path) -> (Vec<PathBuf>, bool) {
    let state = bridge.open[session].state.lock().unwrap();
    let vouched = state
        .answers
        .trust
        .is_trusted(&bravebot_agent::workspace::key_of(
            &directory.join("notes.md"),
        ));
    (state.directories.clone(), vouched)
}

/// TRUST-9: each directory a settings file names is its own question, put after the trust
/// question, one at a time: a yes to one opens that one and no other, and a no leaves it closed.
/// Nothing runs while one is out, and the one not opened is still reported as named.
#[test]
fn each_named_directory_is_asked_about_on_its_own() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, other) = naming(
        "each-named",
        json!({"additionalDirectories": ["../each-named-beside", "../each-named-other"]}),
    );
    let (mut bridge, events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    assert_eq!(settled["rulesQuestion"], Value::Null, "{settled}");
    assert_eq!(settled["foldersRefused"], json!([]), "{settled}");
    let first = settled["folderQuestion"].clone();
    assert_eq!(first["directory"], json!(shown(&beside)), "{first}");
    assert_eq!(first["named"], json!(["../each-named-beside"]), "{first}");
    assert_eq!(first["remaining"], 1, "{first}");
    assert_eq!(
        sent(&events, "folder.request"),
        std::slice::from_ref(&first)
    );
    let refused = bridge.open[&session].ready_for_work().unwrap_err();
    assert!(
        refused.message.contains("folder.reply"),
        "{}",
        refused.message
    );
    assert_eq!(holds(&bridge, &session, &beside), (vec![], false));

    let answered = answer_folder(&mut bridge, &session, &first, true);
    assert_eq!(answered["opened"], true, "{answered}");
    assert_eq!(answered["folder"], json!(shown(&beside)), "{answered}");
    assert_eq!(answered["folders"], json!([shown(&beside)]), "{answered}");
    let second = answered["folderQuestion"].clone();
    // Opened as a picked folder is: listed with them, and not opened a second time.
    let listed = call(
        &mut bridge,
        "session.folders.list",
        json!({"session": session}),
    )
    .unwrap();
    assert_eq!(listed["folders"], json!([shown(&beside)]), "{listed}");
    assert_eq!(second["directory"], json!(shown(&other)), "{second}");
    assert_eq!(second["remaining"], 0, "{second}");
    assert_ne!(second["request"], first["request"]);
    assert_eq!(sent(&events, "folder.request").last(), Some(&second));
    assert!(bridge.open[&session].ready_for_work().is_err());
    // The first answer cannot be given again.
    let repeated = call(
        &mut bridge,
        "folder.reply",
        json!({"session": session, "request": first["request"], "open": false}),
    )
    .expect_err("a repeated answer");
    assert_eq!(repeated.code, ErrorCode::NoSuchRequest);

    let answered = answer_folder(&mut bridge, &session, &second, false);
    assert_eq!(answered["opened"], false, "{answered}");
    assert_eq!(answered["folders"], json!([shown(&beside)]), "{answered}");
    assert_eq!(answered["folderQuestion"], Value::Null, "{answered}");
    assert!(bridge.open[&session].ready_for_work().is_ok());
    let again = call(
        &mut bridge,
        "session.folders.add",
        json!({"session": session, "path": beside}),
    )
    .expect_err("a folder a yes opened is open already");
    assert!(again.message.contains("already open"), "{}", again.message);
    assert_eq!(
        holds(&bridge, &session, &beside),
        (vec![beside.clone()], true)
    );
    assert_eq!(
        holds(&bridge, &session, &other),
        (vec![beside.clone()], false)
    );

    // The workspace a turn is given reaches the one accepted and not the one declined.
    let open = &bridge.open[&session];
    let settings = crate::settings::layers(Some(&project), None);
    let workspace = session_workspace(open, &settings).unwrap();
    assert_eq!(workspace.added_directories(), std::slice::from_ref(&beside));
    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(
        listed["settingsRules"]["directories"],
        json!(["../each-named-other"]),
        "{listed}"
    );
}

/// TRUST-9: a yes to a directory removed since it was asked about opens nothing and vouches for
/// nothing. The reply says why, the directory is said among the ones not opened and is still
/// named in the rules, and the next directory is still asked about.
#[test]
fn a_yes_to_a_directory_removed_since_it_was_asked_about_opens_nothing() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, other) = naming(
        "removed-named",
        json!({"additionalDirectories": ["../removed-named-beside", "../removed-named-other"]}),
    );
    let (mut bridge, events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let first = settled["folderQuestion"].clone();
    assert_eq!(first["directory"], json!(shown(&beside)), "{first}");
    std::fs::remove_dir_all(&beside).unwrap();

    let answered = answer_folder(&mut bridge, &session, &first, true);
    assert_eq!(answered["opened"], false, "{answered}");
    assert!(
        answered["problem"]
            .as_str()
            .is_some_and(|said| !said.is_empty()),
        "{answered}"
    );
    assert_eq!(answered["folders"], json!([]), "{answered}");
    let second = answered["folderQuestion"].clone();
    assert_eq!(second["directory"], json!(shown(&other)), "{answered}");
    assert_eq!(sent(&events, "folder.request").last(), Some(&second));
    assert_eq!(holds(&bridge, &session, &beside), (vec![], false));

    answer_folder(&mut bridge, &session, &second, false);
    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    let refused = listed["foldersRefused"].as_array().unwrap();
    assert_eq!(refused.len(), 1, "{listed}");
    assert_eq!(refused[0]["directory"], json!(shown(&beside)), "{listed}");
    assert_eq!(
        listed["settingsRules"]["directories"],
        json!(["../removed-named-beside", "../removed-named-other"]),
        "{listed}"
    );
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// TRUST-9: a directory whose name was swapped for a link after it was asked about is not
/// opened, so a yes never vouches for a directory the person was not shown.
#[cfg(unix)]
#[test]
fn a_yes_to_a_directory_swapped_for_a_link_opens_nothing() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, other) = naming(
        "swapped-named",
        json!({"additionalDirectories": ["../swapped-named-beside"]}),
    );
    let (mut bridge, _) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let question = settled["folderQuestion"].clone();
    assert_eq!(question["directory"], json!(shown(&beside)), "{question}");
    std::fs::remove_dir_all(&beside).unwrap();
    std::os::unix::fs::symlink(&other, &beside).unwrap();

    let answered = answer_folder(&mut bridge, &session, &question, true);
    assert_eq!(answered["opened"], false, "{answered}");
    assert!(
        answered["problem"]
            .as_str()
            .is_some_and(|said| said.contains("asked about")),
        "{answered}"
    );
    assert_eq!(answered["folders"], json!([]), "{answered}");
    assert_eq!(holds(&bridge, &session, &other), (vec![], false));
    assert_eq!(holds(&bridge, &session, &beside), (vec![], false));
}

/// Bypass opens the named directories unasked, and one it cannot open, here one removed after it
/// was resolved, is said among the ones not opened rather than dropped, and nothing of it is opened
/// or vouched for.
#[test]
fn bypass_says_a_named_directory_it_could_not_open() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, other) = naming("bypass-refuses", json!({}));
    let (mut bridge, _) = hearing();
    let (session, _) = opened_and_trusted(&mut bridge, &project);
    std::fs::remove_dir_all(&beside).unwrap();
    let folders = vec![
        crate::project_questions::Named {
            directory: shown(&beside),
            names: vec!["../bypass-refuses-beside".to_string()],
        },
        crate::project_questions::Named {
            directory: shown(&other),
            names: vec!["../bypass-refuses-other".to_string()],
        },
    ];
    let settings = crate::settings::layers(Some(&project), None);
    let open = bridge.open.get_mut(&session).unwrap();
    assert!(open_unasked(open, &settings, folders));
    assert_eq!(open.folders, std::slice::from_ref(&other));
    assert_eq!(
        holds(&bridge, &session, &beside),
        (vec![other.clone()], false)
    );

    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    let refused = listed["foldersRefused"].as_array().unwrap();
    assert_eq!(refused.len(), 1, "{listed}");
    assert_eq!(refused[0]["directory"], json!(shown(&beside)), "{listed}");
    assert!(
        refused[0]["problem"]
            .as_str()
            .is_some_and(|said| !said.is_empty()),
        "{listed}"
    );
}

/// The directories are asked about after the rules, whatever the rules were answered.
#[test]
fn the_named_directories_are_asked_about_after_the_rules() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, _) = naming(
        "after-the-rules",
        json!({"allow": [THE_RULE], "additionalDirectories": ["../after-the-rules-beside"]}),
    );
    let (mut bridge, events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    assert_eq!(settled["folderQuestion"], Value::Null, "{settled}");
    assert!(sent(&events, "folder.request").is_empty());
    let answered = reply(&mut bridge, &session, &settled["rulesQuestion"], false);
    let folder = &answered["folderQuestion"];
    assert_eq!(folder["directory"], json!(shown(&beside)), "{answered}");
    assert_eq!(
        sent(&events, "folder.request"),
        std::slice::from_ref(folder)
    );
    assert!(bridge.open[&session].ready_for_work().is_err());
    answer_folder(&mut bridge, &session, folder, true);
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// A name that cannot be opened at all, one that does not exist or one inside the project, is
/// reported with why and not asked about. A path two names reach is asked about once.
#[test]
fn a_name_that_cannot_be_opened_is_reported_and_not_asked_about() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, _) = naming(
        "cannot-open",
        json!({"additionalDirectories": [
            "../cannot-open-nowhere", ".bravebot", "../cannot-open-beside",
            "../cannot-open/../cannot-open-beside"
        ]}),
    );
    let (mut bridge, events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let refused = settled["foldersRefused"].as_array().unwrap();
    let names: Vec<_> = refused.iter().map(|entry| &entry["directory"]).collect();
    assert_eq!(names, ["../cannot-open-nowhere", ".bravebot"], "{settled}");
    assert!(
        refused.iter().all(|entry| entry["problem"]
            .as_str()
            .is_some_and(|said| !said.is_empty())),
        "{settled}"
    );
    let question = &settled["folderQuestion"];
    assert_eq!(question["directory"], json!(shown(&beside)), "{settled}");
    assert_eq!(
        question["named"],
        json!([
            "../cannot-open-beside",
            "../cannot-open/../cannot-open-beside"
        ])
    );
    assert_eq!(question["remaining"], 0);
    assert_eq!(sent(&events, "folder.request").len(), 1);
    let answered = answer_folder(&mut bridge, &session, question, true);
    assert_eq!(answered["folders"], json!([shown(&beside)]), "{answered}");
    let listed = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(
        listed["settingsRules"]["directories"],
        json!(["../cannot-open-nowhere", ".bravebot"]),
        "{listed}"
    );
}

/// A session resumed from a record that carries its map asks nothing, about the rules or the
/// directories: the ones it has open are the ones its own record reopened.
#[test]
fn a_resumed_session_asks_nothing() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, _) = naming(
        "resumed-asks",
        json!({"allow": [THE_RULE], "additionalDirectories": ["../resumed-asks-beside"]}),
    );
    let (mut bridge, events) = hearing();
    let session = a_recorded_session(
        &mut bridge,
        &project,
        bravebot_agent::opening::Whence::Resumed,
    );
    assert!(sent(&events, "rules.request").is_empty());
    assert!(sent(&events, "folder.request").is_empty());
    assert!(bridge.open[&session].ready_for_work().is_ok());
    assert!(!allows(&bridge, &session, "example.test"));
    assert_eq!(holds(&bridge, &session, &beside), (vec![], false));
}

/// Bypass answers the directories as it answers everything: each is opened with nothing put.
#[test]
fn bypass_opens_the_named_directories_unasked() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, other) = naming(
        "bypass-opens",
        json!({"additionalDirectories": ["../bypass-opens-beside", "../bypass-opens-other"]}),
    );
    let (mut bridge, events) = hearing();
    let opened = call(&mut bridge, "session.new", json!({"directory": project})).unwrap();
    let session = opened["session"].as_str().unwrap().to_string();
    bridge.open[&session]
        .permission_mode
        .set(PermissionMode::Bypass);
    let settled = call(
        &mut bridge,
        "trust.reply",
        json!({"session": session, "trusted": true}),
    )
    .unwrap();
    assert_eq!(settled["folderQuestion"], Value::Null, "{settled}");
    assert!(sent(&events, "folder.request").is_empty());
    // The response says what the mode opened, so a window can browse them without asking.
    assert_eq!(
        settled["folders"],
        json!([shown(&beside), shown(&other)]),
        "{settled}"
    );
    assert_eq!(
        holds(&bridge, &session, &beside),
        (vec![beside.clone(), other.clone()], true)
    );
    assert!(bridge.open[&session].state.lock().unwrap().answered_by_mode);
    assert_eq!(
        settled["settingsRules"]["directories"],
        json!([]),
        "{settled}"
    );
    assert!(bridge.open[&session].ready_for_work().is_ok());
}

/// A fork takes over a directory question its parent had not answered, under an id of its own,
/// and what the child opens the parent does not.
#[test]
fn a_fork_takes_over_the_directory_question() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (project, beside, _) = naming(
        "fork-folder",
        json!({"additionalDirectories": ["../fork-folder-beside"]}),
    );
    let (mut bridge, events) = hearing();
    let parent = a_parent_asking(&mut bridge, &project);
    let asked = sent(&events, "folder.request");
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
    let question = forked["folderQuestion"].clone();
    assert_eq!(question["directory"], json!(shown(&beside)), "{forked}");
    assert_ne!(question["request"], parents["request"]);
    let stale = call(
        &mut bridge,
        "folder.reply",
        json!({"session": child, "request": parents["request"], "open": true}),
    )
    .expect_err("the parent's id answered the child's question");
    assert_eq!(stale.code, ErrorCode::NoSuchRequest);
    answer_folder(&mut bridge, &child, &question, true);
    assert_eq!(
        holds(&bridge, &child, &beside),
        (vec![beside.clone()], true)
    );
    assert_eq!(holds(&bridge, &parent, &beside), (vec![], false));
    assert!(bridge.open[&parent].ready_for_work().is_err());
}

/// CLI-30, PERM-15: `permissions.check` reports a checkout's `allow` rule the person has not
/// granted as not in force, and once they grant it the rule decides the call as a grant.
#[test]
fn a_checkouts_allow_rule_is_not_in_force_until_granted() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("check-not-in-force", &[THE_RULE]);
    let file = project.join(".bravebot/settings.json");
    let (mut bridge, _events) = hearing();
    let (session, settled) = opened_and_trusted(&mut bridge, &project);
    let check = |bridge: &mut Bridge| {
        call(
            bridge,
            "permissions.check",
            json!({"session": session, "family": "WebFetch", "words": ["https://example.test/a"]}),
        )
        .unwrap()
    };

    let waiting = check(&mut bridge);
    assert_eq!(waiting["decision"], "none", "{waiting}");
    assert_eq!(waiting["granted"], false, "{waiting}");
    assert_eq!(waiting["notInForce"]["rule"], json!(THE_RULE), "{waiting}");
    assert_eq!(
        waiting["notInForce"]["file"],
        json!(file.display().to_string()),
        "{waiting}"
    );
    assert_eq!(
        waiting["notInForce"]["shownFile"],
        json!(bravebot_agent::confirm::shown_path(&file)),
        "{waiting}"
    );

    reply(&mut bridge, &session, &settled["rulesQuestion"], true);
    let granted = check(&mut bridge);
    assert_eq!(granted["decision"], "allow", "{granted}");
    assert_eq!(granted["rule"], json!(THE_RULE), "{granted}");
    assert_eq!(granted["granted"], true, "{granted}");
    assert_eq!(
        granted["file"],
        json!(file.display().to_string()),
        "{granted}"
    );
    assert_eq!(granted["notInForce"], Value::Null, "{granted}");
}

/// `permissions.check` is refused while a turn runs, since the turn holds the session's rules.
#[test]
fn permissions_check_is_refused_during_a_turn_and_for_an_unknown_session() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = proposing("check-during-a-turn", &[]);
    let (mut bridge, _events) = hearing();
    let (session, _settled) = opened_and_trusted(&mut bridge, &project);
    let params = json!({"session": session, "family": "Bash", "words": ["ls"]});
    assert!(call(&mut bridge, "permissions.check", params.clone()).is_ok());
    let started = crate::running::Running::start();
    bridge.open.get_mut(&session).unwrap().running = Some(started.running);
    let refused = call(&mut bridge, "permissions.check", params).unwrap_err();
    assert_eq!(refused.code, ErrorCode::TurnInFlight, "{}", refused.message);

    let refused = call(
        &mut bridge,
        "permissions.check",
        json!({"session": "no-such", "family": "Bash", "words": ["ls"]}),
    )
    .unwrap_err();
    assert_eq!(
        refused.code,
        ErrorCode::NoSuchSession,
        "{}",
        refused.message
    );
}
