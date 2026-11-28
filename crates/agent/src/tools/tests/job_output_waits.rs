use super::*;

/// A wait is refused rather than clamped, which is where it parts company with a deadline. A
/// deadline cut short still ends the run it was given for and the answer says how long that
/// took. A wait cut short comes back with the silence of a shorter window, and a planner that
/// asked about ten minutes and was quietly given one reads that silence as ten minutes of it.
#[test]
fn a_job_output_wait_outside_the_bounds_is_refused_rather_than_shortened() {
    use std::time::Duration;

    let held = Deadlines::BUILT_IN;

    // Nothing asked for is no wait, which is what every call before this one did.
    assert_eq!(wait_from(&json!({}), held).unwrap(), None);
    assert_eq!(
        wait_from(&json!({"wait_seconds": null}), held).unwrap(),
        None
    );

    assert_eq!(
        wait_from(&json!({"wait_seconds": 30}), held).unwrap(),
        Some(Duration::from_secs(30))
    );
    // Both ends are inside, and written out as seconds rather than as the constants they come
    // from: the numbers here are the ones the description and the refusal quote to the planner,
    // so a test that reads them from the same constants would agree with itself while every
    // sentence the planner sees had gone wrong. The upper end is under the five minutes a
    // provider keeps a prefix when nobody chose a lifetime.
    assert_eq!(
        wait_from(&json!({"wait_seconds": 1}), held).unwrap(),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        wait_from(&json!({"wait_seconds": 270}), held).unwrap(),
        Some(Duration::from_secs(270))
    );

    for outside in [0, -1, 271, 300, 600, 601, 86_400] {
        let refusal = wait_from(&json!({"wait_seconds": outside}), held)
            .expect_err("a wait outside the bounds is refused");
        assert!(
            refusal.contains("between 1 and 270"),
            "the refusal does not say what is allowed: {refusal}"
        );
        assert!(
            refusal.contains("prompt cache"),
            "the refusal does not say why the bound is there: {refusal}"
        );
    }

    assert!(wait_from(&json!({"wait_seconds": "a while"}), held).is_err());
    assert!(wait_from(&json!({"wait_seconds": 12.5}), held).is_err());
    assert!(wait_from(&json!({"wait_seconds": true}), held).is_err());
    assert!(wait_from(&json!({"wait_seconds": [30]}), held).is_err());
}

/// The longest wait follows the cache lifetime the person chose, and nothing else. Unset and
/// `5m` are held under five minutes, since the provider drops the prefix then; `1h` keeps the
/// built-in ten minutes, which is under an hour.
/// Rejects: a bound that stays at 600 whatever the setting says (the unset and `5m` rows), and
/// one that ignores a chosen `1h` and holds every wait to 270 (the `1h` row).
#[test]
fn the_longest_wait_follows_the_cache_lifetime_chosen() {
    use bravebot_config::CacheTtl;
    use std::time::Duration;

    for (lifetime, longest) in [
        (None, 270),
        (Some(CacheTtl::FiveMinutes), 270),
        (Some(CacheTtl::OneHour), 600),
    ] {
        let held = Deadlines::BUILT_IN.waiting_under(lifetime);
        assert_eq!(
            held.wait_ceiling,
            Duration::from_secs(longest),
            "{lifetime:?}"
        );
        assert_eq!(
            wait_from(&json!({"wait_seconds": longest}), held).unwrap(),
            Some(Duration::from_secs(longest)),
            "{lifetime:?}"
        );
        assert!(
            wait_from(&json!({"wait_seconds": longest + 1}), held).is_err(),
            "{lifetime:?}"
        );
    }

    // Under five minutes where that is the lifetime, and under an hour where it is that.
    assert!(Deadlines::BUILT_IN.wait_ceiling < Duration::from_secs(300));
    assert!(
        Deadlines::BUILT_IN
            .waiting_under(Some(CacheTtl::OneHour))
            .wait_ceiling
            > Duration::from_secs(270)
    );
    assert!(
        Deadlines::BUILT_IN
            .waiting_under(Some(CacheTtl::OneHour))
            .wait_ceiling
            < Duration::from_secs(3600)
    );

    // Resolved from the settings the caller read, the way the run block is, and only the
    // lifetime moves the wait: a raised run ceiling leaves it where it was.
    let from_file = |text: &str| Deadlines::from_settings(&bravebot_config::Settings::parse(text));
    assert_eq!(from_file("{}"), Deadlines::BUILT_IN);
    assert_eq!(
        from_file(r#"{"promptCacheTtl": "1h"}"#).wait_ceiling,
        Duration::from_secs(600)
    );
    assert_eq!(
        from_file(r#"{"run": {"maxSeconds": 1800}}"#).wait_ceiling,
        Duration::from_secs(270)
    );
}

/// BACKEND-27: the refusal under `1h` states the bound in force, 600, and not the 270 of the
/// shorter lifetimes, and each refusal states its own figure.
///
/// Rejects: a refusal whose figure is written as 270 whatever the lifetime.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn the_wait_refusal_quotes_the_bound_of_the_lifetime_in_force() {
    use bravebot_config::CacheTtl;

    for (lifetime, longest, over) in [
        (None, "between 1 and 270", 271),
        (Some(CacheTtl::FiveMinutes), "between 1 and 270", 271),
        (Some(CacheTtl::OneHour), "between 1 and 600", 601),
    ] {
        let held = Deadlines::BUILT_IN.waiting_under(lifetime);
        let refusal = wait_from(&json!({"wait_seconds": over}), held)
            .expect_err("a wait over the bound is refused");
        assert!(
            refusal.contains(longest),
            "{lifetime:?}: the refusal does not quote the bound in force: {refusal}"
        );
    }
    let held = Deadlines::BUILT_IN.waiting_under(Some(CacheTtl::OneHour));
    assert!(
        !wait_from(&json!({"wait_seconds": 601}), held)
            .expect_err("refused")
            .contains("270"),
        "the refusal under 1h still quotes the 270 of the shorter lifetimes"
    );
}

/// The description quotes the bound in force and the cache cost of a longer wait, and says the same
/// of a foreground deadline, for each cache lifetime.
/// Rejects: a description that still says a long wait is free, one that quotes 270 under `1h`
/// (or 600 under `5m`), and one that leaves the cache cost out of `run`'s `deadline_seconds`.
#[test]
fn the_descriptions_quote_the_wait_in_force_and_its_cache_cost() {
    use bravebot_config::CacheTtl;

    for (lifetime, longest) in [
        (None, "From 1 to 270"),
        (Some(CacheTtl::FiveMinutes), "From 1 to 270"),
        (Some(CacheTtl::OneHour), "From 1 to 600"),
    ] {
        let offered = available(
            Scheduling::ArrangingALook,
            Arming::Allowed { free: 1 },
            Deadlines::BUILT_IN.waiting_under(lifetime),
            Running::Offered,
        );
        let job_output = offered
            .iter()
            .find(|t| t.function.name == "job_output")
            .expect("job_output is offered");
        let said = job_output.function.parameters.to_string();
        assert!(said.contains(longest), "{lifetime:?}: {said}");
        assert!(
            said.contains("prompt cache lifetime")
                && said.contains("write the whole conversation to the cache again"),
            "{lifetime:?}: wait_seconds does not give the cache cost: {said}"
        );
        assert!(
            job_output.function.description.contains("wait_seconds"),
            "{lifetime:?}: the job_output description does not name the wait"
        );
        let run = offered
            .iter()
            .find(|t| t.function.name == "run")
            .expect("run is offered");
        assert!(
            run.function
                .parameters
                .to_string()
                .contains("prompt cache lifetime"),
            "{lifetime:?}: deadline_seconds does not give the cache cost"
        );
        assert!(
            run.function
                .parameters
                .to_string()
                .contains("background: true, watched with job_output in shorter waits"),
            "{lifetime:?}: deadline_seconds does not point at background: true with shorter waits"
        );
    }
}

/// Without a way to wait, watching a job costs one whole turn per look: the planner calls, is
/// told nothing has happened, answers, and is asked the same question again. The argument is
/// what makes one call able to cover a window, so it has to be in the schema the planner reads
/// and the description has to say the wait ends when something arrives.
#[test]
fn job_output_offers_a_bounded_wait_rather_than_only_a_snapshot() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "job_output")
    .expect("job_output is offered");

    let wait = &tool.function.parameters["properties"]["wait_seconds"];
    assert_eq!(wait["type"], "integer", "wait_seconds is not offered");
    let said = wait["description"].as_str().expect("it is described");
    // Built from the figures in force rather than written out, so changing the bound fails
    // here instead of leaving the planner told a number that is no longer the one enforced.
    let bounds = format!(
        "From {} to {}",
        crate::exec::WAIT_FLOOR.as_secs(),
        Deadlines::BUILT_IN.wait_ceiling.as_secs()
    );
    for stated in [bounds.as_str(), "outside is refused"] {
        assert!(
            said.contains(stated),
            "wait_seconds no longer says '{stated}': {said}"
        );
    }
    let refusal =
        wait_from(&json!({"wait_seconds": 0}), Deadlines::BUILT_IN).expect_err("zero is refused");
    assert!(
        refusal.contains(&format!(
            "between {} and {}",
            crate::exec::WAIT_FLOOR.as_secs(),
            Deadlines::BUILT_IN.wait_ceiling.as_secs()
        )),
        "the refusal quotes bounds that are not the ones enforced: {refusal}"
    );
    let described = &tool.function.description;
    assert!(
        described.contains("wait_seconds waits for new output, the end or a typed line"),
        "job_output does not say the wait ends at new output or a typed line: {described}"
    );
    assert!(
        !said.contains("costs nothing"),
        "wait_seconds still says a long wait is free: {said}"
    );
    assert!(
        tool.function.description.contains("wait_seconds"),
        "job_output's description never names the argument: {}",
        tool.function.description
    );
}

/// A wait is a way to spend part of one turn watching, and a planner that took it for a way to
/// be told later would end its turn promising to report on a job nobody is watching. Both
/// places the tool is offered say the wait ends with the turn.
#[test]
fn job_output_says_its_wait_stays_inside_the_turn() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "job_output")
    .expect("job_output is offered");

    let said = &tool.function.description;
    for stated in [
        "A wait stays inside this turn",
        "whether it outlives the turn",
    ] {
        assert!(
            said.contains(stated),
            "job_output's description no longer says '{stated}': {said}"
        );
    }
}

/// `until_exit` turns a wait that output ends into one that only the exit, the bound, the token
/// or a stop ends. A flag taken for `false` where it was not understood hands back a wait that
/// output ends, one given with no `wait_seconds` has no bound to wait to, and one given with
/// `kill` holds the kill back by the whole bound, so a value the call cannot honour is refused
/// instead of read generously.
#[test]
fn until_exit_is_a_boolean_that_needs_a_wait_and_no_kill_beside_it() {
    use std::time::Duration;

    let thirty = Some(Duration::from_secs(30));

    // Left out, null and false are the wait every call made before the flag existed, with or
    // without a bound and with or without a kill.
    for arguments in [
        json!({}),
        json!({"until_exit": null}),
        json!({"until_exit": false}),
    ] {
        for kill in [false, true] {
            assert_eq!(until_exit_from(&arguments, thirty, kill), Ok(false));
            assert_eq!(until_exit_from(&arguments, None, kill), Ok(false));
        }
    }

    assert_eq!(
        until_exit_from(&json!({"until_exit": true}), thirty, false),
        Ok(true)
    );

    let refusal = until_exit_from(&json!({"until_exit": true}), None, false)
        .expect_err("a wait for the exit with no bound is refused");
    assert!(
        refusal.contains("'wait_seconds'"),
        "the refusal does not name what to add: {refusal}"
    );

    let refusal = until_exit_from(&json!({"until_exit": true}), thirty, true)
        .expect_err("a wait for the exit beside a kill is refused");
    assert!(
        refusal.contains("'kill'"),
        "the refusal does not name the argument it cannot be given with: {refusal}"
    );

    // Neither a string nor a number is a boolean, whatever the bound beside it.
    for not_a_boolean in [json!("true"), json!(1), json!(0), json!([true])] {
        let refusal = until_exit_from(&json!({"until_exit": not_a_boolean}), thirty, false)
            .expect_err("a value that is not a boolean is refused");
        assert!(
            refusal.contains("true or false"),
            "the refusal does not say what is allowed: {refusal}"
        );
    }
}

/// A map that says nothing reads as the whole tree, and a planner told nothing concludes that a
/// symbol is absent when the map was cut. Each way a map falls short has its own sentence, and a
/// map that fell short in none says nothing after its body.
#[test]
fn a_map_says_each_way_it_fell_short_and_a_whole_one_says_nothing() {
    use crate::workspace::RepoMap;
    let whole = RepoMap {
        body: "a.rs:\n  fn one()\n".to_string(),
        found: 3,
        shown: 3,
        files: 1,
        unvouched: 0,
        skipped: 0,
        truncated: false,
        unreadable: false,
    };
    assert_eq!(
        map_as_told(&whole),
        whole.body,
        "a whole map carries a note"
    );

    let fell_short = [
        (
            "fewer declarations shown than found",
            RepoMap {
                shown: 1,
                ..whole.clone()
            },
            "this map shows 1 of 3 declarations, most referenced first",
        ),
        (
            "the walk stopped at a cap",
            RepoMap {
                truncated: true,
                ..whole.clone()
            },
            "the walk stopped at a cap on files or bytes, so this map is incomplete",
        ),
        (
            "a directory could not be opened",
            RepoMap {
                unreadable: true,
                ..whole.clone()
            },
            "part of this tree could not be read and is not in this map",
        ),
        (
            "files nobody vouched for",
            RepoMap {
                unvouched: 2,
                ..whole.clone()
            },
            "2 source files were left out because nobody vouched for them",
        ),
        (
            "one file nobody vouched for",
            RepoMap {
                unvouched: 1,
                ..whole.clone()
            },
            "1 source file was left out because nobody vouched for it",
        ),
        (
            "files left out as unreadable or too large",
            RepoMap {
                skipped: 2,
                ..whole.clone()
            },
            "2 source files were left out as unreadable, too large or not safe to show",
        ),
    ];
    let sentences: Vec<&str> = fell_short.iter().map(|(_, _, said)| *said).collect();
    for (how, map, said) in &fell_short {
        let told = map_as_told(map);
        assert!(
            told.starts_with(&whole.body),
            "{how}: the body is not first: {told}"
        );
        assert!(
            told.ends_with(')') && told.contains(&format!("\n\n({said}")),
            "{how}: the map does not say so: {told}"
        );
        for other in sentences.iter().filter(|other| *other != said) {
            assert!(
                !told.contains(other),
                "{how}: the map also says '{other}': {told}"
            );
        }
    }
}

/// The flag is how a planner gets one call to cover a job that prints often, so it has to be in
/// the schema the planner reads, typed as a boolean, and the descriptions have to say what it
/// changes: output stops ending the wait, and a bound is still needed.
#[test]
fn job_output_offers_a_wait_that_output_does_not_end() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "job_output")
    .expect("job_output is offered");

    let until_exit = &tool.function.parameters["properties"]["until_exit"];
    assert_eq!(until_exit["type"], "boolean", "until_exit is not offered");
    let said = until_exit["description"].as_str().expect("it is described");
    for stated in ["Needs wait_seconds", "not with kill"] {
        assert!(
            said.contains(stated),
            "until_exit no longer says '{stated}': {said}"
        );
    }
    // Not required, or every look at a job would have to choose.
    let required = tool.function.parameters["required"]
        .as_array()
        .expect("job_output names what it requires");
    assert!(
        !required.iter().any(|name| name == "until_exit"),
        "until_exit is required of every look: {required:?}"
    );
    // The planner reads the tool's description before it reads a parameter's, and the case the
    // flag is for is one it has to recognise there: output ends wait_seconds and does not end
    // until_exit, which waits for the end.
    let described = &tool.function.description;
    assert!(
        described.contains("wait_seconds waits for new output")
            && described.contains("until_exit waits for the end"),
        "job_output does not say output ends one wait and only the end ends the other: \
         {described}"
    );
    let wait = tool.function.parameters["properties"]["wait_seconds"]["description"]
        .as_str()
        .expect("wait_seconds is described");
    assert!(
        wait.contains("outside is refused"),
        "wait_seconds does not give its bounds: {wait}"
    );
}
