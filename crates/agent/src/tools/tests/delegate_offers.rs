use super::*;

/// DELEGATE-33. The sentence is built from the capabilities a delegate was started without.
/// It is absent where there are none, names each one in the driver's words, and says "Each"
/// for a fan-out. Rejects a sentence added to every start message, one that names what the
/// definition asked for instead of what was removed, and one that is the same for one
/// delegate and many.
#[test]
fn the_start_message_names_what_a_delegate_was_started_without() {
    use bravebot_core::capability::Capability;

    assert_eq!(started_without_sentence(&[], 1), None);
    assert_eq!(
        started_without_sentence(&[Capability::FileWrite], 1).as_deref(),
        Some(
            "It was started without writing files, because the run that started it does \
             not hold it."
        )
    );
    let many = started_without_sentence(&[Capability::FileWrite, Capability::ShellExec], 3)
        .expect("two were taken away");
    assert!(many.starts_with("Each was started without writing files, running programs"));
    assert!(many.ends_with("does not hold them."));
}

/// Every kind above the bottom of the tree is offered the way to delegate, with the kinds this
/// session resolved that its own capabilities reach, and none at the bottom is. This is the
/// offer; the kernel refusing a call from the bottom anyway is `policy::tests`' half.
#[test]
fn a_delegate_is_offered_a_way_to_delegate_only_above_the_bottom_of_the_tree() {
    use bravebot_core::delegate::{Definition, Definitions, Kind};

    let mut delegates = Definitions::default();
    delegates.insert(Definition::from_file(
        "rule-reviewer",
        "Checks a diff against the rule.",
        Kind::Reader,
        None,
        "",
        ".bravebot/agents/rule-reviewer.md",
    ));
    for name in Kind::NAMES {
        let kind = Kind::from_name(name).expect("enumerated");
        let above = for_delegate(
            &kind.capabilities(),
            None,
            Some(&delegates),
            Deadlines::BUILT_IN,
        );
        let spawn = above
            .iter()
            .find(|t| t.function.name == "spawn_agent")
            .unwrap_or_else(|| panic!("a {name} above the bottom was offered no way to delegate"));
        let reached = match kind {
            Kind::Reader => json!(["reader", "rule-reviewer"]),
            Kind::Checker => json!(["reader", "checker", "rule-reviewer"]),
            Kind::Worker => json!(["reader", "checker", "worker", "rule-reviewer"]),
        };
        assert_eq!(
            spawn.function.parameters["properties"]["kind"]["enum"], reached,
            "a {name} was offered kinds other than the ones this session resolved that it reaches"
        );

        let bottom: Vec<String> =
            for_delegate(&kind.capabilities(), None, None, Deadlines::BUILT_IN)
                .iter()
                .map(|t| t.function.name.clone())
                .collect();
        assert!(
            !bottom.iter().any(|t| t == "spawn_agent"),
            "a {name} at the bottom of the tree was offered a way to delegate"
        );
    }
}

/// Delegates read this description as well as the turn does, and a model told how many
/// delegates it has left spends them. The depth and the ceiling are the kernel's to keep, so
/// the description says they exist and not what they are.
#[test]
fn the_way_to_delegate_is_described_without_the_numbers_that_bound_it() {
    let offered = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = offered
        .iter()
        .find(|t| t.function.name == "spawn_agent")
        .expect("the turn is offered a way to delegate");
    let description = &spawn.function.description;
    assert!(
        !description.chars().any(|c| c.is_ascii_digit()),
        "the description told the model how many it may start: {description}"
    );
}

/// The prompt this tool draws belongs to the person who set the sub-task going, about content
/// they never asked to see, in the middle of work they are not reading. No kind is offered it,
/// whatever it holds: the capability a delegate has for reading files would otherwise hand it
/// over through the catch-all.
#[test]
fn a_delegate_is_never_offered_a_way_to_promote_a_slot() {
    for name in bravebot_core::delegate::Kind::NAMES {
        let kind = bravebot_core::delegate::Kind::from_name(name).expect("enumerated");
        let offered: Vec<String> = for_delegate(
            &kind.capabilities(),
            None,
            Some(&bravebot_core::delegate::Definitions::default()),
            Deadlines::BUILT_IN,
        )
        .iter()
        .map(|t| t.function.name.clone())
        .collect();
        assert!(
            !offered.iter().any(|t| t == "vet_content"),
            "a {name} was offered a way to put quarantined bytes into its own context"
        );
    }
}

/// A tool a run's gates would refuse on every call is a tool the model has to be told to
/// ignore, so the set follows the capabilities rather than being listed per kind.
#[test]
fn a_delegate_is_offered_only_the_tools_its_capabilities_reach() {
    use bravebot_core::delegate::Kind;

    let names = |kind: Kind| -> Vec<String> {
        for_delegate(&kind.capabilities(), None, None, Deadlines::BUILT_IN)
            .iter()
            .map(|t| t.function.name.clone())
            .collect()
    };

    let reader = names(Kind::Reader);
    assert!(reader.contains(&"read_file".to_string()));
    assert!(reader.contains(&"spawn_processor".to_string()));
    assert!(!reader.contains(&"run".to_string()));
    assert!(!reader.contains(&"read_output".to_string()));
    assert!(!reader.contains(&"write_file".to_string()));
    assert!(!reader.contains(&"edit_file".to_string()));

    let checker = names(Kind::Checker);
    assert!(checker.contains(&"run".to_string()));
    assert!(checker.contains(&"read_output".to_string()));
    assert!(!checker.contains(&"write_file".to_string()));
    assert!(!checker.contains(&"edit_file".to_string()));

    let worker = names(Kind::Worker);
    assert!(worker.contains(&"write_file".to_string()));
    assert!(worker.contains(&"edit_file".to_string()));
    assert!(worker.contains(&"run".to_string()));
}

/// Neither tool has an audience from inside a delegate. The question would put a task the
/// person never set to them, and the list would replace the one they are watching with the
/// steps of a sub-task they did not ask about.
#[test]
fn a_delegate_is_offered_no_task_list_and_no_way_to_ask() {
    for name in bravebot_core::delegate::Kind::NAMES {
        let kind = bravebot_core::delegate::Kind::from_name(name).expect("enumerated");
        let offered: Vec<String> = for_delegate(
            &kind.capabilities(),
            None,
            Some(&bravebot_core::delegate::Definitions::default()),
            Deadlines::BUILT_IN,
        )
        .iter()
        .map(|t| t.function.name.clone())
        .collect();
        assert!(
            !offered.iter().any(|t| t == "ask_user"),
            "a {name} was offered a question to put to somebody"
        );
        assert!(
            !offered.iter().any(|t| t == "todo_write"),
            "a {name} was offered the task list a person is watching"
        );
        assert!(
            !offered.iter().any(|t| t == "schedule_next"),
            "a {name} was offered a way to pace a loop it is not a tick of"
        );
        assert!(
            !offered.iter().any(|t| t == "request_path"),
            "a {name} was offered reach to a path nobody set up for it"
        );
    }
}

/// A kind holds the network capability so the driver can make its model call, and that is
/// the whole of what it buys: a delegate pointing a request at a host of its own would be
/// egress nobody approved for this sub-task, and the person asked about the host would be
/// arbitrating a task they never set. The one tool whose capability is held and whose name is
/// still withheld, so it is worth a test of its own.
#[test]
fn no_kind_is_offered_a_tool_that_reaches_the_network() {
    for name in bravebot_core::delegate::Kind::NAMES {
        let kind = bravebot_core::delegate::Kind::from_name(name).expect("enumerated");
        let capabilities = kind.capabilities();
        assert!(
            capabilities.contains(&bravebot_core::capability::Capability::WebFetch),
            "a {name} could not have made its own requests"
        );
        let offered: Vec<String> = for_delegate(
            &capabilities,
            None,
            Some(&bravebot_core::delegate::Definitions::default()),
            Deadlines::BUILT_IN,
        )
        .iter()
        .map(|t| t.function.name.clone())
        .collect();
        for reaching in ["fetch_url", "download_url", "web_search"] {
            assert!(
                !offered.iter().any(|t| t == reaching),
                "a {name} was offered {reaching}, a way to reach a host of its own"
            );
        }
    }
}

/// A definition confines a delegate to the tools it named, and only ever downwards: a name
/// it did not write is a tool the delegate does not get, and a name its kind does not reach
/// was dropped before this ever saw it.
#[test]
fn a_definition_confines_a_delegate_to_the_tools_it_named() {
    use bravebot_core::delegate::Kind;

    let named = ["read_file".to_string()];
    let offered: Vec<String> = for_delegate(
        &Kind::Worker.capabilities(),
        Some(&named),
        None,
        Deadlines::BUILT_IN,
    )
    .iter()
    .map(|t| t.function.name.clone())
    .collect();

    assert_eq!(offered, ["read_file"]);

    let all: Vec<String> = for_delegate(
        &Kind::Worker.capabilities(),
        None,
        None,
        Deadlines::BUILT_IN,
    )
    .iter()
    .map(|t| t.function.name.clone())
    .collect();
    assert!(
        all.len() > offered.len(),
        "confining a worker to one tool offered it no fewer than naming none did"
    );
}

/// Every kind that reads files is offered `read_git`, and only the two that may run a program
/// are offered `run`, so a reader reads a description that cannot send it there: it would spend
/// a round on a name that is not on its list and then have a refusal to explain to whoever asked
/// about the history.
#[test]
fn read_git_names_run_only_where_the_delegate_holds_one() {
    use bravebot_core::delegate::Kind;

    let described = |kind: Kind| {
        let offered = for_delegate(&kind.capabilities(), None, None, Deadlines::BUILT_IN);
        let runs = offered.iter().any(|tool| tool.function.name == "run");
        let said = offered
            .into_iter()
            .find(|tool| tool.function.name == "read_git")
            .expect("a kind that reads files is offered read_git")
            .function
            .description;
        (runs, said)
    };

    let (runs, reader) = described(Kind::Reader);
    assert!(!runs, "a reader was offered a way to run a program");
    assert!(
        !reader.contains("run"),
        "a reader's read_git named a tool it is not offered: {reader}"
    );
    assert!(
        reader.contains("no tool on your list reads that history instead"),
        "a reader was not told what it cannot read: {reader}"
    );

    let (runs, checker) = described(Kind::Checker);
    assert!(runs, "a checker was not offered a way to run a program");
    assert!(
        checker.contains("elsewhere it says so and you use run"),
        "a checker was not sent to the tool it holds: {checker}"
    );
    assert!(
        checker.contains("For --follow, blame or anything else use run."),
        "a checker was not sent to the tool it holds: {checker}"
    );
}

/// GIT-9. A status that ran out of time is told what can narrow it, which is a path and the
/// `search.maxSeconds` setting, and `git status` through run where the turn holds one. It is
/// not told about a range or a count, which a status does not take, and a log still is.
#[test]
fn a_status_out_of_time_names_what_narrows_a_status_and_a_log_keeps_its_advice() {
    use crate::git::Query;

    let with_run = read_git_time_note(Query::Status, None, true);
    assert!(with_run.contains("this status is partial"), "{with_run}");
    assert!(with_run.contains("search.maxSeconds"), "{with_run}");
    assert!(with_run.contains("use run with git status"), "{with_run}");
    assert!(!with_run.contains("range"), "{with_run}");
    assert!(!with_run.contains("count"), "{with_run}");

    let without_run = read_git_time_note(Query::Status, None, false);
    assert!(without_run.contains("search.maxSeconds"), "{without_run}");
    assert!(!without_run.contains("run"), "{without_run}");

    let log = read_git_time_note(Query::Log, None, true);
    assert!(
        log.contains("narrow it with a path, a range or a smaller count"),
        "{log}"
    );
    assert!(!log.contains("search.maxSeconds"), "{log}");
}

/// GIT-9. The sentence on an answer that stopped short names only arguments its question takes:
/// a cut show of a file names the offset that prints the next of its lines, and a show of a
/// commit or a directory is told nothing about a range or a count, which a show does not take,
/// whether it stopped at the cap or at the deadline. A show of a file cut only by a line past
/// 2,000 characters says the line was shortened and names `run` only where the turn holds it,
/// since `offset` and `limit` cannot print the rest of one line; one that also stopped with lines
/// left says both.
#[test]
fn a_stopped_answer_names_only_the_arguments_its_question_takes() {
    use crate::git::{Answer, Query, Showed};

    let file = Answer {
        cut: true,
        next_line: Some(2001),
        showed: Some(Showed::File),
        ..Answer::default()
    };
    let said = read_git_stopped_note(Query::Show, &file, true);
    assert!(said.contains("ask again with offset 2001"), "{said}");
    assert!(!said.contains("shortened"), "{said}");

    let long_line = Answer {
        cut: true,
        shortened: true,
        showed: Some(Showed::File),
        ..Answer::default()
    };
    let with_run = read_git_stopped_note(Query::Show, &long_line, true);
    assert!(
        with_run.contains("a line longer than 2,000 characters was shortened")
            && with_run.contains("run with git show prints it whole"),
        "{with_run}"
    );
    assert!(!with_run.contains("offset"), "{with_run}");
    let without_run = read_git_stopped_note(Query::Show, &long_line, false);
    assert!(
        without_run.contains("a line longer than 2,000 characters was shortened"),
        "{without_run}"
    );
    assert!(!without_run.contains("run"), "{without_run}");
    assert!(!without_run.contains("offset"), "{without_run}");

    let both = read_git_stopped_note(
        Query::Show,
        &Answer {
            next_line: Some(7),
            ..long_line.clone()
        },
        true,
    );
    assert!(
        both.contains("ask again with offset 7")
            && both.contains("a line longer than 2,000 characters was shortened"),
        "{both}"
    );

    for showed in [Showed::File, Showed::Tree, Showed::Commit] {
        let capped = Answer {
            cut: true,
            showed: Some(showed),
            ..Answer::default()
        };
        let late = Answer {
            timed_out: true,
            showed: Some(showed),
            ..Answer::default()
        };
        for said in [
            read_git_stopped_note(Query::Show, &capped, true),
            read_git_stopped_note(Query::Show, &late, true),
        ] {
            assert!(!said.is_empty(), "{showed:?}");
            assert!(!said.contains("range"), "{showed:?}: {said}");
            assert!(!said.contains("count"), "{showed:?}: {said}");
        }
    }
    let file_late = read_git_stopped_note(
        Query::Show,
        &Answer {
            timed_out: true,
            showed: Some(Showed::File),
            ..Answer::default()
        },
        true,
    );
    assert!(file_late.contains("offset and limit"), "{file_late}");
    let commit_capped = read_git_stopped_note(
        Query::Show,
        &Answer {
            cut: true,
            showed: Some(Showed::Commit),
            ..Answer::default()
        },
        true,
    );
    assert!(
        commit_capped.contains("narrow it with a path"),
        "{commit_capped}"
    );

    for (query, refused) in [
        (Query::Diff, "count"),
        (Query::Tags, "path"),
        (Query::Search, "range"),
    ] {
        let capped = Answer {
            cut: true,
            ..Answer::default()
        };
        let said = read_git_stopped_note(query, &capped, true);
        assert!(!said.contains(refused), "{query:?}: {said}");
    }
    assert!(read_git_stopped_note(Query::Show, &Answer::default(), true).is_empty());
}

/// GIT-8. read_git declines a `.git` that is a file, and the description says so in a turn
/// offered `run` and in one that is not, so a planner working in a linked worktree does not
/// learn it from a refused call.
#[test]
fn read_git_says_a_dot_git_that_is_a_file_is_declined() {
    use bravebot_core::delegate::Kind;

    for kind in [Kind::Reader, Kind::Checker] {
        let said = for_delegate(&kind.capabilities(), None, None, Deadlines::BUILT_IN)
            .into_iter()
            .find(|tool| tool.function.name == "read_git")
            .expect("a kind that reads files is offered read_git")
            .function
            .description;
        assert!(
            said.contains(
                "it declines the .git file a linked worktree or a submodule has, except at \
                 the root of a checkout you were started in"
            ),
            "the description did not say a .git file is declined: {said}"
        );
    }
}

/// The description and the refusal are read in one context, so they have to agree. A turn told
/// in the description that nothing on its list reads the history, and then told by the refusal
/// to read it with git, has been given both answers at once.
#[test]
fn a_declined_repository_names_run_only_where_the_turn_holds_one() {
    let failed = crate::workspace::WorkspaceError::Git {
        path: "project".to_string(),
        declined: crate::git::Declined::Untrusted,
    };

    let offered = git_failure(failed.describe("project"), &failed, Running::Offered);
    assert!(
        offered.contains("Use run to read it with git instead."),
        "a turn holding run was not told to use it: {offered}"
    );

    let withheld = git_failure(failed.describe("project"), &failed, Running::Withheld);
    assert!(
        withheld.contains("read_git does not open it"),
        "the refusal stopped saying what happened: {withheld}"
    );
    assert!(
        !withheld.contains("run"),
        "a turn holding no run was told to use it: {withheld}"
    );
}

/// The names a planner may write are the names the kernel will accept, and the reason to pick
/// one is the definition's own sentence. A schema listing the three compiled-in kinds in a
/// session that resolved a fourth would leave the only name worth writing unwritable.
#[test]
fn the_kinds_the_planner_is_offered_are_the_ones_this_session_resolved() {
    use bravebot_core::delegate::{Definition, Definitions, Kind};

    let mut delegates = Definitions::default();
    delegates.insert(Definition::from_file(
        "rule-reviewer",
        "Checks a diff against the rule. Use before asking for a review.",
        Kind::Reader,
        None,
        "",
        ".bravebot/agents/rule-reviewer.md",
    ));

    let tools = for_planner(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        &delegates,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = tools
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("a planner is offered a way to delegate");
    let kind = &spawn.function.parameters["properties"]["kind"];

    assert_eq!(
        kind["enum"],
        json!(["reader", "checker", "worker", "rule-reviewer"])
    );
    let described = kind["description"].as_str().expect("a description");
    assert!(
        described.contains(
            "\n- rule-reviewer: Checks a diff against the rule. Use before asking for a \
             review."
        ),
        "the planner was given no reason to pick the definition: {described}"
    );
}

/// AGENT-6. A list of names is handed on as one, and anything else is refused with what the
/// field takes, so a malformed list never reaches the kernel as "keep every server".
#[test]
fn a_server_list_that_is_not_a_list_of_names_is_refused() {
    assert!(matches!(servers_kept(&json!({"task": "t"})), Ok(None)));
    assert!(matches!(
        servers_kept(&json!({"mcp_servers": null})),
        Ok(None)
    ));
    let kept = servers_kept(&json!({"mcp_servers": ["docs", "weather"]}))
        .expect("a list of names")
        .expect("named");
    assert_eq!(
        kept.label(),
        bravebot_core::label::Label::untrusted_public(),
        "labelled other than as a planner's argument"
    );

    for malformed in [json!("docs"), json!(["docs", 3]), json!({"docs": true})] {
        let refused = servers_kept(&json!({ "mcp_servers": malformed }))
            .err()
            .unwrap_or_else(|| panic!("{malformed} was taken for a list of names"));
        assert!(refused.contains("must be an array"), "{refused}");
    }
}

/// DELEGATE-17 and AGENT-3. The result of a call says when the planner is asked again from the
/// call's own count, so a planner that asked to be woken after all of a batch is not told it
/// will be asked as each report arrives.
#[test]
fn the_start_message_says_when_the_planner_is_asked_again_from_the_calls_count() {
    let first = "the turn asks you again when the first report is in, while the others keep \
                 working, and again as each of the rest reports.";
    assert_eq!(when_asked_again(3, 1, true), first);
    assert_eq!(
        when_asked_again(3, 2, true),
        "the turn asks you again when 2 of them have reported, while the others keep \
         working, and again as each of the rest reports."
    );
    let all = "the turn asks you again when all 3 have reported, and not before unless a line \
               from the user arrives or another call's delegates wake it.";
    assert_eq!(when_asked_again(3, 3, true), all);
    assert_eq!(when_asked_again(3, 40, true), all);
    // A delegate's own call: no typed line ends its wait, so none is said to.
    assert_eq!(
        when_asked_again(3, 3, false),
        "the turn asks you again when all 3 have reported, and not before unless another \
         call's delegates wake it."
    );
    assert_eq!(when_asked_again(3, 1, false), first);
    for wake_after in [1, 5] {
        for typed_lines_wake in [true, false] {
            assert_eq!(
                when_asked_again(1, wake_after, typed_lines_wake),
                "the turn asks you again when the report is in."
            );
        }
    }
}

/// DELEGATE-17 and AGENT-3. The planner is offered `wake_after` as a whole number of at least
/// one, with a description saying what it does and when to use it. A field the turn reads and
/// the schema leaves out is one no planner writes.
#[test]
fn the_planner_is_offered_a_count_of_delegates_to_wake_after() {
    let tools = for_planner(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        &bravebot_core::delegate::Definitions::default(),
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = tools
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("a planner is offered a way to delegate");
    let wake_after = &spawn.function.parameters["properties"]["wake_after"];
    assert_eq!(wake_after["type"], json!("integer"));
    assert_eq!(wake_after["minimum"], json!(1));
    let described = wake_after["description"].as_str().expect("a description");
    for said in [
        "How many of this call's delegates have to end before you are asked again",
        "Left out it is 1",
        "at or above the number of delegates the call starts means all of them",
        "Use it when you need the reports together",
        "A line from the user still wakes you, and so does another call's count.",
    ] {
        assert!(described.contains(said), "{said:?} is not in {described}");
    }
    let required = spawn.function.parameters["required"]
        .as_array()
        .expect("a list");
    assert!(
        !required.contains(&json!("wake_after")),
        "wake_after is required"
    );

    // A delegate that may delegate: a typed line never ends its wait, so its description
    // says only what does.
    for name in bravebot_core::delegate::Kind::NAMES {
        let kind = bravebot_core::delegate::Kind::from_name(name).expect("enumerated");
        let held = for_delegate(
            &kind.capabilities(),
            None,
            Some(&bravebot_core::delegate::Definitions::default()),
            Deadlines::BUILT_IN,
        );
        let spawn = held
            .iter()
            .find(|tool| tool.function.name == "spawn_agent")
            .unwrap_or_else(|| panic!("a {name} above the bottom is offered spawn_agent"));
        let described = spawn.function.parameters["properties"]["wake_after"]["description"]
            .as_str()
            .expect("a description");
        assert!(
            described.contains("Use it when you need the reports together")
                && described.ends_with(" Another call's count still wakes you."),
            "{name}: {described}"
        );
        assert!(
            !described.contains("line from the user"),
            "{name}: {described}"
        );
    }
}

/// DELEGATE-17 and AGENT-3. `wake_after` left out is 1, a whole number above zero is kept as
/// written, and 0, a negative or fractional number or anything but a number is refused with a
/// sentence naming the field, so a malformed count never reaches the turn as the default.
#[test]
fn a_wake_after_that_is_not_a_count_of_delegates_is_refused() {
    assert_eq!(wake_after_asked(&json!({"task": "t"})), Ok(1));
    assert_eq!(wake_after_asked(&json!({"wake_after": null})), Ok(1));
    assert_eq!(wake_after_asked(&json!({"wake_after": 1})), Ok(1));
    assert_eq!(wake_after_asked(&json!({"wake_after": 7})), Ok(7));
    for malformed in [json!(0), json!(-2), json!(1.5), json!("3"), json!([2])] {
        let refused = wake_after_asked(&json!({ "wake_after": malformed }))
            .expect_err(&format!("{malformed} was taken for a count"));
        assert!(refused.contains("'wake_after'"), "{refused}");
    }
}

/// DELEGATE-29. A figure is a count of tokens above zero, as a number or in `/limit`'s words,
/// and anything else is refused with what the field takes, so a malformed figure never reaches
/// the kernel as "no budget".
#[test]
fn a_budget_that_is_not_a_count_of_tokens_is_refused() {
    assert!(matches!(budget_asked(&json!({"task": "t"})), Ok(None)));
    assert!(matches!(budget_asked(&json!({"budget": null})), Ok(None)));
    for (written, tokens) in [
        (json!(40_000), 40_000),
        (json!("500k"), 500_000),
        (json!("2m"), 2_000_000),
        (json!("750000"), 750_000),
    ] {
        assert_eq!(
            budget_figure(&json!({ "budget": written })),
            Ok(Some(tokens)),
            "{written}"
        );
    }
    let labelled = budget_asked(&json!({"budget": "2m"}))
        .expect("a count")
        .expect("named");
    assert_eq!(
        labelled.label(),
        bravebot_core::label::Label::untrusted_public(),
        "labelled other than as a planner's argument"
    );

    for malformed in [
        json!(0),
        json!(-5),
        json!(1.5),
        json!("0"),
        json!("lots"),
        json!("3 credits"),
        json!([5]),
        json!(true),
    ] {
        let refused = budget_asked(&json!({ "budget": malformed }))
            .err()
            .unwrap_or_else(|| panic!("{malformed} was taken for a budget"));
        assert!(refused.contains("must be a count of tokens"), "{refused}");
    }
}

/// CHECKOUT-2: the planner is told which definitions work in a checkout, since their report is
/// about the last commit and not the tree it may have just edited. A reader is given none.
#[test]
fn a_definition_asking_for_a_checkout_is_offered_as_working_in_one() {
    use bravebot_core::delegate::{Definition, Definitions, Kind};

    let mut delegates = Definitions::default();
    for (name, kind) in [("migrator", Kind::Worker), ("reviewer", Kind::Reader)] {
        delegates.insert(
            Definition::from_file(name, "Does it.", kind, None, "", "a.md").with_checkout(),
        );
    }
    delegates.insert(Definition::from_file(
        "tester",
        "Does it.",
        Kind::Checker,
        None,
        "",
        "a.md",
    ));

    let tools = for_planner(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        &delegates,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = tools
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("a planner is offered a way to delegate");
    let described = spawn.function.parameters["properties"]["kind"]["description"]
        .as_str()
        .expect("a description");
    let line = |name: &str| {
        described
            .lines()
            .find(|line| line.starts_with(&format!("- {name}: ")))
            .unwrap_or_else(|| panic!("{name} was not offered: {described}"))
    };
    assert_eq!(
        line("migrator"),
        "- migrator: Does it. (limit: 120 rounds; a round is one reply with its tool calls) \
         Its delegates work in a checkout of the last commit, which holds no change not yet \
         committed unless the call sets isolation to \"checkout_with_changes\"."
    );
    assert_eq!(
        line("reviewer"),
        "- reviewer: Does it. (limit: 60 rounds; a round is one reply with its tool calls)"
    );
    assert_eq!(
        line("tester"),
        "- tester: Does it. (limit: 80 rounds; a round is one reply with its tool calls)"
    );
}

/// DELEGATE-6: the planner sizes a task to the rounds its kind allows, so each kind's line in
/// `kind` names that kind's own figure from `Kind::rounds`, and what a round is.
///
/// Rejects: a line with no figure, one kind's figure on another's line, and a hard-coded figure
/// that drifts from the kind.
#[test]
fn each_kind_the_planner_may_pick_names_the_rounds_it_allows() {
    let tools = for_planner(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        &bravebot_core::delegate::Definitions::default(),
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = tools
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("a planner is offered a way to delegate");
    let described = spawn.function.parameters["properties"]["kind"]["description"]
        .as_str()
        .expect("a description");
    for name in bravebot_core::delegate::Kind::NAMES {
        let kind = bravebot_core::delegate::Kind::from_name(name).expect("enumerated");
        let line = described
            .lines()
            .find(|line| line.starts_with(&format!("- {name}: ")))
            .unwrap_or_else(|| panic!("no line for {name}: {described}"));
        assert!(
            line.contains(&format!(
                "(limit: {} rounds; a round is one reply with its tool calls)",
                kind.rounds()
            )),
            "{name}: {line}"
        );
    }
}

/// AGENT-5: an `each` that names no delegate is refused with a sentence saying it was empty. An
/// empty array is not read as a missing `each`, which would be one delegate on the shared task,
/// and is not accepted as naming zero delegates, which would start nothing and answer nothing.
///
/// Rejects: `tasks_in` returning `Ok` for an empty `each`, either the empty list or the shared
/// task alone, and a refusal that does not say the list was empty.
#[test]
fn an_empty_each_is_refused_with_a_sentence_saying_it_was_empty() {
    let refusal = tasks_in(&json!({"task": "t", "each": []}))
        .map(|tasks| tasks.len())
        .expect_err("an empty each named no delegate and was accepted");
    assert!(
        refusal.contains("'each' was empty"),
        "the planner is not told the list was empty: {refusal}"
    );
    assert_eq!(
        tasks_in(&json!({"task": "t"})).map(|tasks| tasks.len()),
        Ok(1),
        "a call with no each at all is the one task, which the empty list must not be read as"
    );
}

/// AGENT-5: each delegate of a fan-out is told the shared `task` followed by its own entry, so
/// the instruction says what to do before it says what differs. A delegate told its entry
/// first reads the part that differs ahead of the part that says what to do with it.
///
/// Rejects: `format!("{entry}\n\n{task}")` in `tasks_in`, and a composition that drops either
/// half or reuses one entry for every delegate.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn each_delegate_is_told_the_shared_task_followed_by_its_own_entry() {
    use bravebot_core::capability::{Capability, CapabilitySet};
    use bravebot_core::policy::{ReleasePlan, Routing};

    let tasks = tasks_in(&json!({"task": "SHARED", "each": ["FIRST", "SECOND"]}))
        .expect("two entries are accepted");
    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "compose a fan-out");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let proof = policy.authorise_display_release("what a delegate was asked to do");
    let told: Vec<String> = tasks
        .into_iter()
        .map(|task| task.declassify(&proof))
        .collect();
    assert_eq!(
        told,
        ["SHARED\n\nFIRST", "SHARED\n\nSECOND"],
        "a delegate was not told the shared task and then its own entry"
    );
}

/// AGENT-5: a planner told the most one call starts splits a fan-out into calls of that size,
/// where one told nothing sends a call that is refused and then sends the shared task again.
/// The figure in the text of `each` is the one `tasks_in` refuses past, and the text says a
/// further call starts more.
///
/// Rejects: a description with no limit, one that gives the limit and not what to do past it,
/// and a figure written into the text that differs from the one the refusal enforces.
#[test]
fn the_each_field_says_one_call_starts_at_most_the_fan_out_limit() {
    let tools = for_planner(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        &bravebot_core::delegate::Definitions::default(),
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = tools
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("a planner is offered a way to delegate");
    let described = spawn.function.parameters["properties"]["each"]["description"]
        .as_str()
        .expect("a description");

    let named: usize = described
        .split(|c: char| !c.is_ascii_digit())
        .find(|word| !word.is_empty())
        .unwrap_or_else(|| panic!("no limit in the text of each: {described}"))
        .parse()
        .expect("a count");
    assert!(
        described.contains(&format!("at most {named} delegates")),
        "the limit is not stated as the most one call starts: {described}"
    );
    assert!(
        described.contains("further call"),
        "the text does not say a further call starts more: {described}"
    );

    let entries = |count: usize| json!({"task": "t", "each": vec!["e"; count]});
    assert_eq!(
        tasks_in(&entries(named)).map(|tasks| tasks.len()),
        Ok(named),
        "the text names a count the call does not accept"
    );
    assert!(
        tasks_in(&entries(named + 1)).is_err(),
        "the text names a count below what the call accepts"
    );
    assert_eq!(named, MAX_FANOUT);

    let tool_text = &spawn.function.description;
    assert!(
        !tool_text.chars().any(|c| c.is_ascii_digit()),
        "the limit belongs in the text of each, not the tool's description: {tool_text}"
    );
}

/// CHECKOUT-1, CHECKOUT-23: the planner is offered two values for `isolation`, and told that the
/// second also carries the working directory's changes and what for.
#[test]
fn the_isolation_field_offers_a_checkout_with_the_working_directorys_changes() {
    let tools = for_planner(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        &bravebot_core::delegate::Definitions::default(),
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let spawn = tools
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("a planner is offered a way to delegate");
    let isolation = &spawn.function.parameters["properties"]["isolation"];
    assert_eq!(
        isolation["enum"],
        serde_json::json!(["checkout", "checkout_with_changes"])
    );
    let described = isolation["description"].as_str().expect("a description");
    assert!(
        described.contains("\"checkout_with_changes\" is the same, and also copies into it")
            && described.contains("the files you wrote and the ones git status lists"),
        "{described}"
    );
}

/// CHECKOUT-7: the planner reads this before it fans out, and a fetch in each checkout moves
/// the refs of the working directory and races the others. A planner without `run` cannot
/// fetch itself, so it is told to leave the fetch to one delegate. Either one writes the task
/// each delegate gets, so either one is told not to ask for a stash, whose single ref a pop in
/// any checkout takes from.
#[test]
fn the_isolation_field_says_checkouts_share_refs_and_who_fetches_once() {
    let described = |running: Running| -> String {
        let tools = for_planner(
            Scheduling::ArrangingALook,
            crate::watch::Arming::Unavailable,
            &bravebot_core::delegate::Definitions::default(),
            Deadlines::BUILT_IN,
            running,
        );
        let spawn = tools
            .iter()
            .find(|tool| tool.function.name == "spawn_agent")
            .expect("a planner is offered a way to delegate");
        spawn.function.parameters["properties"]["isolation"]["description"]
            .as_str()
            .expect("a description")
            .to_string()
    };
    let shared = "Checkouts share remote-tracking refs and tags with your working directory \
                  and with each other, so a git fetch in one updates them in all of them, and \
                  two fetches at the same time can fail.";
    let stash = "Checkouts also share one stash, so changes git stash sets aside in one can \
                 be popped in any of them. Do not ask a delegate in a checkout to use git \
                 stash.";

    let running = described(Running::Offered);
    assert!(
        running.contains(stash),
        "a planner with run is not told the checkouts share a stash: {running}"
    );
    assert!(
        running.contains(shared)
            && running.contains(
                "Where delegates need a fetch, run it once yourself before starting them \
                 rather than asking each to."
            ),
        "a planner with run is not told the checkouts share refs and to fetch first: {running}"
    );
    let withheld = described(Running::Withheld);
    assert!(
        withheld.contains(stash),
        "a planner without run is not told the checkouts share a stash: {withheld}"
    );
    assert!(
        withheld.contains(shared)
            && withheld
                .contains("Where delegates need a fetch, ask one of them for it rather than each.")
            && !withheld.contains("yourself"),
        "a planner without run is not told the checkouts share refs, or is told to fetch \
         itself: {withheld}"
    );
}

/// SCHED-6: a delegate is offered no `schedule_next`, so the descriptions it receives must not
/// send it there. `read_file` and `run` carry a paragraph about the later look that differs by
/// turn, and the delegate's list used to be built with the one for a turn that arranges it.
#[test]
fn a_delegates_descriptions_do_not_name_schedule_next() {
    use bravebot_core::capability::{Capability, CapabilitySet};
    let offered = for_delegate(
        &CapabilitySet::from_iter([Capability::FileRead, Capability::ShellExec]),
        None,
        None,
        Deadlines::BUILT_IN,
    );
    for name in ["read_file", "run"] {
        let tool = offered
            .iter()
            .find(|t| t.function.name == name)
            .unwrap_or_else(|| panic!("{name} is offered to this delegate"));
        assert!(
            !tool.function.description.contains("schedule_next"),
            "a delegate was told to call schedule_next in {name}: {}",
            tool.function.description
        );
        assert!(
            tool.function
                .description
                .to_lowercase()
                .contains("nothing will ask you again"),
            "{name} does not say no later look is coming: {}",
            tool.function.description
        );
    }
}
