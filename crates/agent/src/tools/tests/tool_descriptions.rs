use super::*;

/// The tools of the planner's list, with the named ones taken off, as a definition's tools line
/// or the kind of a delegate leaves them.
pub(super) fn the_table_without(names: &[&str]) -> Vec<bravebot_aichat::protocol::Tool> {
    let mut offered = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    offered.retain(|tool| !names.contains(&tool.function.name.as_str()));
    state_what_is_beside(&mut offered);
    offered
}

/// TOOL-6. `Beside::on` reads each fact off the list it is given: the table holds every tool
/// it names, and taking one off clears that fact and no other.
#[test]
fn beside_is_read_off_the_list_the_turn_holds() {
    assert_eq!(Beside::on(&the_table_without(&[])), Beside::every());
    let without_read_output = Beside::on(&the_table_without(&["read_output"]));
    assert!(!without_read_output.holds(Named::ReadOutput));
    assert_eq!(
        Beside::every().without(Named::ReadOutput),
        without_read_output
    );
    let without_run = Beside::on(&the_table_without(&["run"]));
    assert!(!without_run.holds(Named::Run));
    assert!(without_run.holds(Named::ReadFile) && without_run.holds(Named::ReadOutput));
    assert!(!Beside::on(&the_table_without(&["read_git"])).holds(Named::ReadGit));
    assert!(!Beside::on(&the_table_without(&["spawn_processor"])).holds(Named::SpawnProcessor));
}

/// TOOL-6. The listing of the files kept beside a skill names `read_file` only where the turn's
/// list holds it, and gives the path to use either way. A turn offered every tool reads the
/// sentence it always did. The faults it rejects are a sentence that names the tool in every
/// list, and one that drops the example path where the tool is absent.
#[test]
fn the_files_beside_a_skill_name_read_file_only_where_the_list_holds_it() {
    let files = vec!["checklist.md".to_string(), "notes.md".to_string()];
    let without = Beside::every().without(Named::ReadFile);
    for (landed, link) in [
        (None, ""),
        (
            Some("real/ship-it"),
            ", a link to real/ship-it, which is where list_files shows them",
        ),
    ] {
        let at = landed.unwrap_or("skills/ship-it");
        let full = beside_the_skill("skills/ship-it", landed, &files, Beside::every());
        assert_eq!(
            full,
            format!(
                "\n\n---\nFiles kept beside this skill in skills/ship-it{link}. Read one with \
                 read_file, naming the directory and the file, for example \
                 {at}/checklist.md:\n- checklist.md\n- notes.md\n"
            ),
            "a turn offered every tool read different text"
        );
        let short = beside_the_skill("skills/ship-it", landed, &files, without);
        assert_eq!(
            short,
            format!(
                "\n\n---\nFiles kept beside this skill in skills/ship-it{link}. For example \
                 {at}/checklist.md:\n- checklist.md\n- notes.md\n"
            ),
        );
        assert!(!short.contains("read_file"), "{short}");
    }
}

pub(super) fn description_of(offered: &[bravebot_aichat::protocol::Tool], name: &str) -> String {
    offered
        .iter()
        .find(|tool| tool.function.name == name)
        .unwrap_or_else(|| panic!("{name} is not on the list"))
        .function
        .description
        .clone()
}

/// VET-5: the statement of who answers is added to the one tool that is `vet_content`, once
/// per call, and a list that lacks the tool is left as it was, with nothing added and no panic.
#[test]
fn who_answers_vet_content_is_stated_only_on_a_list_that_holds_it() {
    let sentence = "SENTENCE-OF-THE-SESSION.";
    let mut whole = the_table_without(&[]);
    let before: Vec<String> = whole
        .iter()
        .map(|tool| tool.function.description.clone())
        .collect();
    state_vet_content_answer(&mut whole, sentence);
    for (tool, was) in whole.iter().zip(&before) {
        if tool.function.name == "vet_content" {
            assert!(
                tool.function.description.ends_with(&format!(" {sentence}")),
                "the statement was not added to vet_content: {}",
                tool.function.description
            );
            assert_eq!(tool.function.description.matches(sentence).count(), 1);
        } else {
            assert_eq!(
                &tool.function.description, was,
                "{} was changed by a statement about vet_content",
                tool.function.name
            );
        }
    }

    let mut without = the_table_without(&["vet_content"]);
    let kept: Vec<String> = without
        .iter()
        .map(|tool| tool.function.description.clone())
        .collect();
    state_vet_content_answer(&mut without, sentence);
    assert!(
        without
            .iter()
            .all(|tool| tool.function.name != "vet_content"),
        "the statement put vet_content on a list that lacked it"
    );
    let after: Vec<String> = without
        .iter()
        .map(|tool| tool.function.description.clone())
        .collect();
    assert_eq!(kept, after, "a list without vet_content was changed");
}

/// VET-6, MODE-3 and CHECK-9: `fetch_url` is offered `read` only on a list that holds
/// `vet_content`, and what it says about the argument is the same on every turn that has it.
/// It names no mode and no word a check answers with, so it cannot be read as saying which of
/// them will answer, and a list without `vet_content` is told nothing about either.
#[test]
fn fetch_url_is_offered_read_only_where_vet_content_is_and_names_no_mode_or_check_word() {
    let properties_of = |offered: &[bravebot_aichat::protocol::Tool]| {
        offered
            .iter()
            .find(|tool| tool.function.name == "fetch_url")
            .expect("fetch_url is offered")
            .function
            .parameters["properties"]
            .clone()
    };
    let words_of = |text: &str| -> Vec<String> {
        text.to_ascii_lowercase()
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .map(str::to_string)
            .collect()
    };

    let with = the_table_without(&[]);
    let properties = properties_of(&with);
    assert_eq!(
        properties["read"]["type"], "boolean",
        "a list holding vet_content was not offered read"
    );

    let without = the_table_without(&["vet_content"]);
    let kept = properties_of(&without);
    assert!(
        kept.get("read").is_none(),
        "read was offered on a list with no vet_content: {kept}"
    );
    assert!(
        kept.get("url").is_some() && kept.get("why").is_some(),
        "the other arguments were lost with read: {kept}"
    );
    assert!(
        !names_the_tool(&description_of(&without, "fetch_url"), "vet_content"),
        "a list without vet_content was pointed at it"
    );
    assert!(
        !names_the_tool(&properties_of(&without).to_string(), "vet_content"),
        "an argument of a list without vet_content names it"
    );

    let mentioned = format!(
        "{} {}",
        description_of(&with, "fetch_url"),
        properties["read"]["description"].as_str().unwrap_or("")
    );
    assert!(
        mentioned.contains("vet_content"),
        "read does not say which tool it asks the release of: {mentioned}"
    );
    let words = words_of(&mentioned);
    for forbidden in [
        "bypass",
        "bypassing",
        "bypassed",
        "auto-vetting",
        "screening",
        "screened",
        "permissions",
        "plan",
        "safe",
        "unsafe",
        "inconclusive",
        "verdict",
        "classifier",
        "injection",
    ] {
        assert!(
            !words.iter().any(|word| word == forbidden),
            "the fetch_url text names '{forbidden}': {mentioned}"
        );
    }
}

/// VET-6 and TOOL-6 for `search` and `list_files` (R12, R17 of #3995). `read` is offered only
/// on a list that holds `vet_content`, the other arguments stay where it is removed, nothing
/// on a list without it names `vet_content`, and the text names `vet_content` and neither the
/// mode nor the check (VET-5).
///
/// A wrong implementation offers `read` to a delegate, removes the routing arguments with it,
/// or says in the shared text that a check reads the result.
#[test]
fn search_and_list_files_are_offered_read_only_where_vet_content_is_and_name_no_check_word() {
    for (tool, kept) in [("search", "pattern"), ("list_files", "directory")] {
        let properties_of = |offered: &[bravebot_aichat::protocol::Tool]| {
            offered
                .iter()
                .find(|offered| offered.function.name == tool)
                .expect("the tool is offered")
                .function
                .parameters["properties"]
                .clone()
        };
        let with = the_table_without(&[]);
        let properties = properties_of(&with);
        assert_eq!(
            properties["read"]["type"], "boolean",
            "{tool}: a list holding vet_content was not offered read"
        );
        let without = the_table_without(&["vet_content"]);
        let left = properties_of(&without);
        assert!(
            left.get("read").is_none(),
            "{tool}: read was offered on a list with no vet_content: {left}"
        );
        assert!(
            left.get(kept).is_some() && left.get("why").is_some(),
            "{tool}: the other arguments were lost with read: {left}"
        );
        assert!(
            !names_the_tool(&description_of(&without, tool), "vet_content")
                && !names_the_tool(&left.to_string(), "vet_content"),
            "{tool}: a list without vet_content was pointed at it"
        );
        let read = properties["read"]["description"].as_str().unwrap_or("");
        assert!(
            names_the_tool(read, "vet_content"),
            "{tool}: read does not say which tool it asks the release of: {read}"
        );
        let words: Vec<String> = read
            .to_ascii_lowercase()
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .map(str::to_string)
            .collect();
        for forbidden in [
            "bypass",
            "bypassed",
            "auto-vetting",
            "screening",
            "permissions",
            "check",
            "checked",
            "safe",
            "unsafe",
            "verdict",
            "classifier",
            "injection",
        ] {
            assert!(
                !words.iter().any(|word| word == forbidden),
                "{tool}: read names '{forbidden}': {read}"
            );
        }
    }
}

/// VET-6. The `read` argument of `fetch_url` and of `web_search` says what a result may hold,
/// so a planner that asks to read a page it can tell is large is not left to find the cap out
/// from a reference.
///
/// A wrong implementation keeps the wording that says only "too long for one result", which
/// names no figure, or states a figure other than the cap the turn runs under by default.
#[test]
fn the_read_argument_of_fetch_url_and_web_search_states_the_cap() {
    let mut offered = the_table_without(&[]);
    offer_web_search(&mut offered);
    assert_eq!(OUTPUT_CAP, 16 * 1024, "the descriptions say 16 KiB");
    for tool in ["fetch_url", "web_search"] {
        let on_the_list = offered.iter().find(|offered| offered.function.name == tool);
        assert!(on_the_list.is_some(), "{tool} is not on the list");
        let read = on_the_list.unwrap().function.parameters["properties"]["read"]["description"]
            .as_str()
            .unwrap_or("")
            .to_string();
        assert!(
            read.contains("16 KiB") && !read.contains("too long for one result"),
            "{tool}: read does not state the cap: {read}"
        );
    }
}

/// The words a description names a tool with: the tool's name as a word of its own.
fn names_the_tool(description: &str, tool: &str) -> bool {
    description
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|word| word == tool)
}

/// The planner on an ordinary turn reads the two descriptions as the table wrote them. These
/// are the words they had before a description followed the list, so a sentence that changed
/// for the session is found here.
#[test]
fn a_list_holding_every_tool_gets_the_processor_and_read_output_descriptions_it_always_had() {
    let everything = the_table_without(&[]);
    assert_eq!(
        description_of(&everything, "spawn_processor"),
        "Transform quarantined content you were not shown. An isolated model with no tools \
         reads only the references you name and follows your instruction; its output is \
         quarantined as a new reference. To change a file you cannot see, name its \
         reference, say what must be true of it afterwards, and pass the reference that comes \
         back to write_file as contents_ref. Nobody reads the output before it is written, so \
         ask for the complete document and nothing else. It decides what the change is, and \
         may leave a file alone that is not the one you are after. A summary, a translation, \
         a rewrite, an explanation or a list of the points made of content you were not shown \
         is written by this tool, since you have not read it. Ask for the form wanted, for example a plain-text summary of each of six stories, two \
         sentences each, with each link kept. Do not leave it to a script you write, which \
         can only cut the original down. A fetched page or a run's output needs no file: \
         name its reference in reads. Then give the answer's reference to run as stdin_ref, \
         or write the answer with write_file and contents_ref to a new file in the scratch \
         directory and run the program that sends it with the file's path as an argument, so \
         it reads the body from the file. The recipient and the subject are yours and only \
         the body comes from the processor. Your final message says what was sent and to \
         whom. Call the text a summary in your final message only if a processor wrote one."
    );
    assert_eq!(
        description_of(&everything, "read_output"),
        "Ask to be shown what a command printed: give the reference a run returned. A \
         run's output is quarantined by default, so ask for any result you need, errors \
         included, and do not claim a run succeeded without seeing it. Only for run output, \
         not a quarantined file. For anything else you are holding a reference to, \
         vet_content is the question to ask. Output shown only in part because it was too \
         long may be read without anybody being asked, a page at a time from offset, or \
         searched with a pattern, which returns the matching lines numbered by the byte each \
         starts at. A pattern on output you may not read is refused. Called on output you \
         may not read, with no offset or pattern, it returns all of it, and what it returns \
         stays in every later request at about a token for every four bytes; for hundreds \
         of kilobytes, run tail or grep with the reference as stdin_ref and read: true \
         instead."
    );
    let table = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    for name in ["spawn_processor", "read_output"] {
        assert_eq!(
            description_of(&table, name),
            description_of(&everything, name),
            "the table and a pass over a list holding every tool disagree about {name}"
        );
    }
}

/// A run is not told in the description of `spawn_processor` to use a tool it is not offered:
/// `write_file` is not on a reader's list, and a definition's tools line can leave out `run`.
/// Each is taken off alone, and the description keeps naming the one that remains.
#[test]
fn the_processor_description_names_only_the_tools_the_list_holds() {
    let all = ["write_file", "run"];
    for gone in all {
        let described = description_of(&the_table_without(&[gone]), "spawn_processor");
        assert!(
            !names_the_tool(&described, gone),
            "a list without {gone} was sent to it: {described}"
        );
        for kept in all.into_iter().filter(|name| *name != gone) {
            assert!(
                names_the_tool(&described, kept),
                "taking {gone} off took {kept} out of the description too: {described}"
            );
        }
    }

    // Everything taken off at once leaves a sentence that still reads, naming none of them.
    let bare = description_of(&the_table_without(&all), "spawn_processor");
    for gone in all {
        assert!(!names_the_tool(&bare, gone), "{gone} remains: {bare}");
    }
    // Nothing is written, so the description does not say what is written.
    assert!(
        !bare.contains("before it is written") && !bare.contains("contents_ref"),
        "{bare}"
    );
}

/// A planner writes a delegate's task, so the description of `spawn_agent` tells it, after the
/// sentence on approvals, that a delegate cannot call `vet_content`, `ask_user`, `fetch_url`
/// or `download_url` and that work needing one is the planner's own to do before delegating or
/// instead. Each of the four is named only where the turn's list holds it (TOOL-6), and a
/// delegate's own list, which holds none, is told nothing of it.
///
/// Rejects: a description silent about the tools, one that names them but gives no way on, one
/// that names all four in every list, and one that is written for the planner's list and left
/// as it was on a delegate's.
#[test]
fn the_planner_is_told_a_delegate_cannot_call_vet_content_ask_the_user_or_fetch() {
    use bravebot_core::delegate::{Definition, Definitions, Kind};

    // What follows the sentence on approvals, up to the sentence on cost.
    fn lacks(description: &str) -> String {
        let (_, after) = description
            .split_once("so this saves you context and never an approval.")
            .expect("the approvals sentence is in the description");
        let (lacks, _) = after
            .split_once(" Do not use it for something one read would answer")
            .expect("the sentence on cost is in the description");
        lacks.to_string()
    }

    let everything = lacks(&description_of(&the_table_without(&[]), "spawn_agent"));
    assert_eq!(
        everything,
        " A delegate cannot call vet_content, ask_user, fetch_url or download_url, so do \
         work that needs one of them yourself, before you delegate or instead of \
         delegating. A reference that has to be read whole is read by you with vet_content."
    );

    let four = ["vet_content", "ask_user", "fetch_url", "download_url"];
    for gone in four {
        let said = lacks(&description_of(&the_table_without(&[gone]), "spawn_agent"));
        assert!(
            !names_the_tool(&said, gone),
            "{gone} is not on the list: {said}"
        );
        for kept in four.iter().filter(|kept| **kept != gone) {
            assert!(
                names_the_tool(&said, kept),
                "taking {gone} off the list dropped {kept}: {said}"
            );
        }
        assert_eq!(
            names_the_tool(&said, "vet_content"),
            gone != "vet_content",
            "{said}"
        );
        assert_eq!(
            said.contains("read by you with vet_content"),
            gone != "vet_content",
            "{said}"
        );
    }
    let one = lacks(&description_of(
        &the_table_without(&["ask_user", "fetch_url", "download_url"]),
        "spawn_agent",
    ));
    assert_eq!(
        one,
        " A delegate cannot call vet_content, so do work that needs it yourself, before you \
         delegate or instead of delegating. A reference that has to be read whole is read by \
         you with vet_content."
    );
    let none = description_of(&the_table_without(&four), "spawn_agent");
    assert!(!none.contains("A delegate cannot call"), "{none}");
    assert_eq!(lacks(&none), "");

    // A delegate that may delegate is offered none of the four, so it is told nothing of them.
    let mut delegates = Definitions::default();
    delegates.insert(Definition::from_file(
        "rule-reviewer",
        "Checks a diff against the rule.",
        Kind::Reader,
        None,
        "",
        ".bravebot/agents/rule-reviewer.md",
    ));
    let mut held = for_delegate(
        &Kind::Worker.capabilities(),
        None,
        Some(&delegates),
        Deadlines::BUILT_IN,
    );
    state_what_is_beside(&mut held);
    let delegate = description_of(&held, "spawn_agent");
    assert!(!delegate.contains("A delegate cannot call"), "{delegate}");
    assert_eq!(lacks(&delegate), "");
}

/// brave/bravebot#3986. A delegate is quarantined exactly as the planner is (DELEGATE-1), so
/// the description of `spawn_agent` promises a build's failures only for output the planner
/// may read, says that delegating a run whose output it may not read does not make that output
/// readable, and points to `spawn_processor` and to vouching for the command instead. It names
/// `spawn_processor` only where the turn's list holds it (TOOL-6).
///
/// Rejects: the build example promising what failed whatever the label, so a planner holding a
/// reference delegates the run again and its delegate gets the same reference; a description
/// that names the rule and no way on; and one that names `spawn_processor` to a list without it.
#[test]
fn the_planner_is_told_a_delegate_cannot_read_output_it_may_not_read_either() {
    let everything = description_of(&the_table_without(&[]), "spawn_agent");
    assert!(
        everything.contains(
            "That is what this is for: running a build reads the whole log, and asking a \
             delegate to run a build whose output you may read tells you what failed. A \
             delegate is quarantined exactly as you are, so delegating a run whose output \
             you may not read does not make that output readable. Give its reference to \
             spawn_processor for a bounded question about it instead, or have a person vouch \
             for the command. Use it when finding"
        ),
        "{everything}"
    );
    assert!(
        !everything.contains("run the build tells you"),
        "{everything}"
    );

    let without = description_of(&the_table_without(&["spawn_processor"]), "spawn_agent");
    assert!(
        without.contains(
            "does not make that output readable. Have a person vouch for the command \
             instead. Use it when finding"
        ),
        "{without}"
    );
    assert!(!names_the_tool(&without, "spawn_processor"), "{without}");
}

/// Being shown private content does not close the processor, so no description tells the
/// planner it does: not the tools that show output, not `spawn_agent`, and not the kind text
/// that says what a reader does.
///
/// Rejects: a description that sends the planner round a closure the policy no longer makes,
/// so that it plans around a tool that is open.
#[test]
fn no_description_says_that_private_content_closes_processors() {
    let everything = the_table_without(&[]);
    for tool in &everything {
        let described = description_of(&everything, &tool.function.name);
        for stale in [
            "closes spawn_processor",
            "cannot run processors",
            "a processor takes no private instruction",
            "spawn_processor call is refused",
        ] {
            assert!(
                !described.contains(stale),
                "{} says {stale:?}: {described}",
                tool.function.name
            );
        }
    }

    let kind = everything
        .iter()
        .find(|tool| tool.function.name == "spawn_agent")
        .expect("spawn_agent is on the list")
        .function
        .parameters["properties"]["kind"]["description"]
        .as_str()
        .expect("a kind description")
        .to_string();
    assert!(
        kind.contains("lists, searches and runs processors: use it to find something out"),
        "{kind}"
    );
    assert!(!kind.contains("holds nothing private"), "{kind}");
}

/// SPAWN-6. A processor's answer about a reference that is no file is written to a new file with
/// `write_file`, so a list without it is told that the answer goes to the program as
/// `stdin_ref`, which needs no file, and is sent to neither the write nor the file. The route
/// reads no file first, so a list without `read_file` is told the same as one with it. A list
/// without `run` is told the route to no program at all.
#[test]
fn the_write_to_a_file_is_described_only_where_the_list_holds_write_file() {
    let with = description_of(&the_table_without(&[]), "spawn_processor");
    let to_a_file = "Then give the answer's reference to run as stdin_ref, or write the \
                     answer with write_file and contents_ref to a new file in the scratch \
                     directory and run the program that sends it with the file's path as an \
                     argument, so it reads the body from the file.";
    let stdin_alone = "Then give the answer's reference to run as stdin_ref, in the line of \
                       the program that sends it.";
    assert!(
        with.contains(to_a_file) && names_the_tool(&with, "write_file"),
        "a list holding every tool is not told it may write the answer to a file: {with}"
    );
    assert!(
        !names_the_tool(&with, "read_file") && !with.contains("save the original"),
        "the description sends the planner to save the original first: {with}"
    );

    // Nothing is read first, so the file tool a reference to it would come from is no part of
    // the route.
    let unreadable = description_of(&the_table_without(&["read_file"]), "spawn_processor");
    assert_eq!(
        unreadable, with,
        "taking read_file off changed the description"
    );

    // Without `write_file` there is no file to write the answer to.
    let unwritable = description_of(&the_table_without(&["write_file"]), "spawn_processor");
    for gone in ["write_file", "contents_ref"] {
        assert!(!names_the_tool(&unwritable, gone), "{unwritable}");
    }
    assert!(
        unwritable.contains(stdin_alone)
            && !unwritable.contains("scratch directory")
            && unwritable.contains("Do not leave it to a script you write")
            && unwritable.contains("The recipient and the subject are yours"),
        "a list without write_file lost more than the write to a file: {unwritable}"
    );

    // A list without `run` is sent to no route, so the program is not named for one.
    let no_run = description_of(&the_table_without(&["run"]), "spawn_processor");
    assert!(!names_the_tool(&no_run, "stdin_ref"), "{no_run}");
}

/// `read_output` points a run at `vet_content` for what it is holding besides a command's
/// output, so a list without it is not pointed there. The rest of the description is the same.
#[test]
fn the_read_output_description_names_vet_content_only_where_the_list_holds_it() {
    let with = description_of(&the_table_without(&[]), "read_output");
    let without = description_of(&the_table_without(&["vet_content"]), "read_output");
    assert!(names_the_tool(&with, "vet_content"), "{with}");
    assert!(!names_the_tool(&without, "vet_content"), "{without}");
    assert_eq!(
        without,
        with.replace(
            "For anything else you are holding a reference to, vet_content is the question \
             to ask. ",
            ""
        ),
        "taking vet_content off changed more than the sentence about it"
    );
    // Taking another tool off leaves the sentence.
    let other = description_of(&the_table_without(&["write_file"]), "read_output");
    assert_eq!(other, with);
}

/// What the description of `run` says to do with the output that comes back as a reference.
fn run_says_of_its_reference(offered: &[bravebot_aichat::protocol::Tool]) -> String {
    let run = description_of(offered, "run");
    let from = "otherwise it comes back as a reference, like a file you may not read";
    let start = run
        .find(from)
        .unwrap_or_else(|| panic!("{from:?} is not in {run}"));
    let end = run[start..]
        .find(" Do use it to compile and test what you changed.")
        .unwrap_or_else(|| panic!("the sentence about the output does not end: {run}"))
        + start;
    run[start..end].to_string()
}

/// The planner on an ordinary turn reads `fetch_url`, `job_output` and `run` as the table
/// wrote them before they followed the list. These are the words they had, so a sentence that
/// changed for the session is found here.
#[test]
fn a_list_holding_every_tool_gets_the_fetch_job_and_run_texts_it_always_had() {
    let everything = the_table_without(&[]);
    assert_eq!(
        description_of(&everything, "fetch_url"),
        "Fetch an http or https URL. Not the way to read a github.com pull request or issue \
         where the prompt says the GitHub CLI is on PATH, which names the commands that are. \
         What comes back is quarantined, like a file nobody vouched for: you get a reference \
         rather than the text, and you cannot read it or be told what it says. Hand the \
         reference to spawn_processor to have a question answered about it, or to write_file \
         as contents_ref to save it. Where you have to read the page yourself, vet_content is \
         the question to ask. The user is asked before anything leaves the machine."
    );
    assert_eq!(
        description_of(&everything, "job_output"),
        "See what a background job printed since your last look, and whether it has \
         ended. Give the job name run returned. Each call hands back a slice of what the job \
         printed since your previous look. The result gives the range of lines the slice \
         covers, and the earlier slices are in the earlier references the result lists. \
         Output is quarantined exactly as a run's is: it comes back as text where the user \
         vouched for every command in the line, and otherwise as a reference to pass to \
         read_output, spawn_processor or write_file. Wait with wait_seconds rather than \
         calling again and again, and add until_exit: true where you need the job's end and \
         not its output as it arrives. It is still a wait inside this turn, so it cannot \
         tell you about anything that happens after the turn ends, and the account of the \
         job's start says whether it is killed then or kept. Repeated waits on a running job \
         are not held; the same call without wait_seconds, or on a job that has ended, is \
         held at the third in a row. wait_seconds is bounded under the prompt cache \
         lifetime, since a longer wait makes the next request write the whole conversation \
         to the cache again."
    );
    let run = description_of(&everything, "run");
    assert!(
        run.contains(
            "Output comes back as text you can read where the user has vouched for every \
             command in the line; otherwise it comes back as a reference, like a file you may \
             not read: pass it to read_output to see it, hand it to spawn_processor, or write \
             it to a file with write_file. A summary, a translation or other text made from \
             it is written by spawn_processor, whose description says how that text reaches a \
             program. Do use it to compile and test what you changed."
        ),
        "{run}"
    );
    let background = everything
        .iter()
        .find(|tool| tool.function.name == "run")
        .expect("run is on the list")
        .function
        .parameters["properties"]["background"]["description"]
        .as_str()
        .expect("a description")
        .to_string();
    assert_eq!(
        background,
        "Leave it running and return a job name, for a server, a watcher or a log \
         follower. You are told when it ends during this turn. Call job_output with the name \
         to see what it has printed before then. One pipeline with no redirection; killed \
         when this turn ends. Defaults to false."
    );

    // The pass over a list holding every tool is the table's own text.
    let table = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    for name in ["fetch_url", "job_output", "run"] {
        let in_table = table.iter().find(|t| t.function.name == name).unwrap();
        let after = everything.iter().find(|t| t.function.name == name).unwrap();
        assert_eq!(
            in_table.function.description, after.function.description,
            "the table and a pass over a list holding every tool disagree about {name}"
        );
        assert_eq!(
            in_table.function.parameters, after.function.parameters,
            "the table and a pass over a list holding every tool disagree about the \
             arguments of {name}"
        );
    }
}

/// `fetch_url` hands back a reference and says what to do with it. A list without
/// `spawn_processor`, `write_file` or `vet_content` is not sent to it, the moves it does have
/// stay, and nothing else in the description changes.
#[test]
fn the_fetch_description_names_only_the_tools_the_list_holds() {
    let with = description_of(&the_table_without(&[]), "fetch_url");
    let moves = " Hand the reference to spawn_processor to have a question answered about it, \
                 or to write_file as contents_ref to save it.";
    let reading = " Where you have to read the page yourself, vet_content is the question to \
                   ask.";
    assert!(with.contains(moves) && with.contains(reading), "{with}");

    for (gone, expected) in [
        (
            "spawn_processor",
            " Hand the reference to write_file as contents_ref to save it.",
        ),
        (
            "write_file",
            " Hand the reference to spawn_processor to have a question answered about it.",
        ),
    ] {
        let without = description_of(&the_table_without(&[gone]), "fetch_url");
        assert!(!names_the_tool(&without, gone), "{without}");
        assert_eq!(without, with.replace(moves, expected), "without {gone}");
    }
    let unvetted = description_of(&the_table_without(&["vet_content"]), "fetch_url");
    assert!(!names_the_tool(&unvetted, "vet_content"), "{unvetted}");
    assert_eq!(unvetted, with.replace(reading, ""));

    // Neither move and no check: the description still says what comes back.
    let bare = description_of(
        &the_table_without(&["spawn_processor", "write_file", "vet_content"]),
        "fetch_url",
    );
    assert_eq!(
        bare,
        with.replace(moves, "").replace(reading, ""),
        "taking all three off changed more than the sentences about them"
    );
    assert!(
        bare.contains("you cannot read it or be told what it says. The user is asked before"),
        "{bare}"
    );
}

/// `web_search` hands back a reference and says what to do with it, in its own words about
/// "the results". Each of `spawn_processor`, `write_file` and `vet_content` is named only where
/// the list holds it, and every combination reads exactly as written here. The regression it
/// rejects is `web_search` borrowing the wording of `fetch_url` ("the page") or a sentence for
/// one combination of the neighbours being sent for another.
#[test]
fn the_web_search_description_names_only_the_tools_the_list_holds() {
    let head = "Search the web with the Brave Search API. 'service' picks what is asked: web, \
                news, videos, images, llm_context (pre-extracted page text for grounding), \
                suggest, spellcheck or places. What comes back is quarantined, like a file \
                nobody vouched for: you get a reference rather than the results, and you \
                cannot read it or be told what it says.";
    let tail = " The user is asked before the query leaves the machine, and it is sent to the \
                Brave Search API and nowhere else.";
    let reading = " Where you have to read the results yourself, vet_content is the question \
                   to ask.";
    for (spawn, write, moves) in [
        (
            true,
            true,
            " Hand the reference to spawn_processor to have a question answered about it, or \
             to write_file as contents_ref to save it.",
        ),
        (
            true,
            false,
            " Hand the reference to spawn_processor to have a question answered about it.",
        ),
        (
            false,
            true,
            " Hand the reference to write_file as contents_ref to save it.",
        ),
        (false, false, ""),
    ] {
        for vet in [true, false] {
            let mut gone = Vec::new();
            if !spawn {
                gone.push("spawn_processor");
            }
            if !write {
                gone.push("write_file");
            }
            if !vet {
                gone.push("vet_content");
            }
            let mut offered = the_table_without(&gone);
            offer_web_search(&mut offered);
            let expected = format!("{head}{moves}{}{tail}", if vet { reading } else { "" });
            assert_eq!(
                description_of(&offered, "web_search"),
                expected,
                "without {gone:?}"
            );
        }
    }
}

/// `job_output` says what a job's output is passed to when it comes back as a reference. Each of
/// the three tools is named only where the list holds it, and a list with none still says that
/// the output comes back as a reference.
#[test]
fn the_job_output_description_names_only_the_tools_the_list_holds() {
    let full = description_of(&the_table_without(&[]), "job_output");
    let sentence = "otherwise as a reference to pass to read_output, spawn_processor or \
                    write_file.";
    assert!(full.contains(sentence), "{full}");
    for (gone, expected) in [
        (
            "read_output",
            "otherwise as a reference to pass to spawn_processor or write_file.",
        ),
        (
            "spawn_processor",
            "otherwise as a reference to pass to read_output or write_file.",
        ),
        (
            "write_file",
            "otherwise as a reference to pass to read_output or spawn_processor.",
        ),
    ] {
        let without = description_of(&the_table_without(&[gone]), "job_output");
        assert!(!names_the_tool(&without, gone), "{without}");
        assert_eq!(without, full.replace(sentence, expected), "without {gone}");
    }
    for (kept, expected) in [
        (
            "read_output",
            "otherwise as a reference to pass to read_output.",
        ),
        (
            "spawn_processor",
            "otherwise as a reference to pass to spawn_processor.",
        ),
        (
            "write_file",
            "otherwise as a reference to pass to write_file.",
        ),
    ] {
        let all = ["read_output", "spawn_processor", "write_file"];
        let gone: Vec<&str> = all.into_iter().filter(|name| *name != kept).collect();
        let alone = description_of(&the_table_without(&gone), "job_output");
        assert_eq!(alone, full.replace(sentence, expected), "only {kept}");
    }
    let bare = description_of(
        &the_table_without(&["read_output", "spawn_processor", "write_file"]),
        "job_output",
    );
    assert_eq!(
        bare,
        full.replace(sentence, "otherwise as a reference."),
        "taking all three off changed more than the sentence about them"
    );
}

/// A look at a job hands over a slice, and the result lists the references of the looks before
/// it. A description that says each call reports "what is new" leaves a planner reading the
/// last reference as the whole of the output, and searching it alone for a line printed in an
/// earlier look. The wording is the same for a list that holds none of the tools a reference
/// is passed to, and it names none of them.
#[test]
fn the_job_output_description_says_a_look_is_a_slice_and_earlier_slices_are_earlier_references() {
    let bare_list = ["read_output", "spawn_processor", "write_file"];
    for (which, offered) in [
        ("a list holding every tool", the_table_without(&[])),
        (
            "a list holding none of the tools a reference goes to",
            the_table_without(&bare_list),
        ),
        (
            "the table as built",
            available(
                Scheduling::ArrangingALook,
                crate::watch::Arming::Unavailable,
                Deadlines::BUILT_IN,
                Running::Offered,
            ),
        ),
    ] {
        let said = description_of(&offered, "job_output");
        for stated in [
            "a slice of what the job printed since your previous look",
            "gives the range of lines the slice covers",
            "earlier slices are in the earlier references the result lists",
        ] {
            assert!(
                said.contains(stated),
                "{which}: job_output no longer says '{stated}': {said}"
            );
        }
        assert!(
            !said.contains("Each call reports what is new"),
            "{which}: job_output still says each call reports what is new: {said}"
        );
    }
    let bare = description_of(&the_table_without(&bare_list), "job_output");
    for tool in bare_list {
        assert!(
            !names_the_tool(&bare, tool),
            "a list without {tool} is sent to it: {bare}"
        );
    }
}

/// `run` says what to do with output that comes back as a reference, and how to see what a
/// background job has printed so far. Each tool is named only where the list holds it. The
/// sentence that sends text made from the output to `spawn_processor` follows the moves, and is
/// said where that tool is on the list.
#[test]
fn the_run_description_names_only_the_tools_the_list_holds() {
    let full = description_of(&the_table_without(&[]), "run");
    let in_full = run_says_of_its_reference(&the_table_without(&[]));
    assert_eq!(
        in_full,
        "otherwise it comes back as a reference, like a file you may not read: pass it to \
         read_output to see it, hand it to spawn_processor, or write it to a file with \
         write_file. A summary, a translation or other text made from it is written by \
         spawn_processor, whose description says how that text reaches a program."
    );
    let made_from_it = " A summary, a translation or other text made from it is written by \
                        spawn_processor, whose description says how that text reaches a \
                        program.";
    let sentence = |moves: &str, processor: bool| {
        let then = if processor { made_from_it } else { "" };
        format!(
            "otherwise it comes back as a reference, like a file you may not read{moves}.{then}"
        )
    };
    let all = ["read_output", "spawn_processor", "write_file"];
    for (gone, moves) in [
        (
            "read_output",
            ": hand it to spawn_processor, or write it to a file with write_file",
        ),
        (
            "spawn_processor",
            ": pass it to read_output to see it, or write it to a file with write_file",
        ),
        (
            "write_file",
            ": pass it to read_output to see it, or hand it to spawn_processor",
        ),
    ] {
        let offered = the_table_without(&[gone]);
        let processor = gone != "spawn_processor";
        assert_eq!(
            run_says_of_its_reference(&offered),
            sentence(moves, processor),
            "without {gone}"
        );
        // Only the sentence about the output changed.
        assert_eq!(
            description_of(&offered, "run"),
            full.replace(&in_full, &sentence(moves, processor)),
            "taking {gone} off changed more than the sentence about the output"
        );
        assert_eq!(
            names_the_tool(&description_of(&offered, "run"), "spawn_processor"),
            processor,
            "without {gone}"
        );
    }
    for (kept, moves) in [
        ("read_output", ": pass it to read_output to see it"),
        ("spawn_processor", ": hand it to spawn_processor"),
        ("write_file", ": write it to a file with write_file"),
        ("", ""),
    ] {
        let gone: Vec<&str> = all.into_iter().filter(|name| *name != kept).collect();
        assert_eq!(
            run_says_of_its_reference(&the_table_without(&gone)),
            sentence(moves, kept == "spawn_processor"),
            "only {kept:?}"
        );
    }

    // The first look at what a background job has printed is job_output's, named where it is
    // held.
    let background = |offered: &[bravebot_aichat::protocol::Tool]| {
        offered
            .iter()
            .find(|tool| tool.function.name == "run")
            .expect("run is on the list")
            .function
            .parameters["properties"]["background"]["description"]
            .as_str()
            .expect("a description")
            .to_string()
    };
    let with_it = background(&the_table_without(&[]));
    let without_it = background(&the_table_without(&["job_output"]));
    assert!(names_the_tool(&with_it, "job_output"), "{with_it}");
    assert!(!names_the_tool(&without_it, "job_output"), "{without_it}");
    assert_eq!(
        without_it,
        with_it.replace(
            " Call job_output with the name to see what it has printed before then.",
            ""
        ),
        "taking job_output off changed more than the sentence about it"
    );
}

/// The description of one argument of a tool.
fn argument_of(offered: &[bravebot_aichat::protocol::Tool], tool: &str, argument: &str) -> String {
    offered
        .iter()
        .find(|candidate| candidate.function.name == tool)
        .unwrap_or_else(|| panic!("{tool} is not on the list"))
        .function
        .parameters["properties"][argument]["description"]
        .as_str()
        .unwrap_or_else(|| panic!("{tool} has no description for {argument}"))
        .to_string()
}

/// TOOL-6, RUN-18. `run` sends a request to watch something to the built-in skill `watching`.
/// Where the turn cannot load it, because its catalogue lacks the skill or its list lacks
/// `load_skill`, that sentence is replaced by the paragraph the skill stands for, which names
/// `read_file` and `job_output` only where the list holds each. The regressions it rejects are
/// a turn sent to a skill it cannot load, a turn that can load it sent the paragraph anyway,
/// and a paragraph naming a tool the list lacks.
#[test]
fn the_run_description_on_watching_names_only_the_tools_the_list_holds() {
    let route = "load the built-in skill watching";
    let full = description_of(&the_table_without(&[]), "run");
    assert!(full.contains(route), "{full}");
    let mut loadable = the_table_without(&[]);
    state_watching_route(&mut loadable, true);
    assert_eq!(
        description_of(&loadable, "run"),
        full,
        "a turn that can load the skill was not sent to it"
    );

    for (listed, gone) in [(false, &[][..]), (true, &["load_skill"][..])] {
        let mut offered = the_table_without(gone);
        state_watching_route(&mut offered, listed);
        let said = description_of(&offered, "run");
        assert!(
            !said.contains(route),
            "listed {listed}, without {gone:?}: {said}"
        );
        for stated in [
            "decide first what is being watched",
            "compare read_file's change token",
            "background: true and then job_output with wait_seconds",
            "take the first look now",
        ] {
            assert!(
                said.contains(stated),
                "listed {listed}, without {gone:?}: no '{stated}': {said}"
            );
        }
    }
    for gone in ["read_file", "job_output"] {
        let mut offered = the_table_without(&["load_skill", gone]);
        state_watching_route(&mut offered, true);
        let said = description_of(&offered, "run");
        assert!(!names_the_tool(&said, gone), "{gone} remains: {said}");
        assert!(
            said.contains("decide first what is being watched"),
            "{said}"
        );
    }
}

/// TOOL-6. The `read` argument of `job_output` names `read_output` only where the list holds
/// it, as `run`'s does, and says the account of the job's end follows the latest look.
#[test]
fn the_job_output_read_argument_names_read_output_only_where_the_list_holds_it() {
    let full = argument_of(&the_table_without(&[]), "job_output", "read");
    assert!(names_the_tool(&full, "read_output"), "{full}");
    assert!(
        full.contains("The account of the job's end follows your latest look's read."),
        "{full}"
    );
    let without = argument_of(&the_table_without(&["read_output"]), "job_output", "read");
    assert!(!names_the_tool(&without, "read_output"), "{without}");
    assert_eq!(
        without,
        "Ask to read what the job printed in this result. This changes nothing in this turn: \
         output you may not read comes back as a reference. Defaults to false."
    );
}

/// TOOL-6. The `read` argument of `run` names `read_output` only where the list holds it. A
/// list without it releases nothing to `read`, so the argument says it changes nothing there.
#[test]
fn the_run_read_argument_names_read_output_only_where_the_list_holds_it() {
    let full = argument_of(&the_table_without(&[]), "run", "read");
    assert_eq!(
        full,
        "Ask for the output as text in this result where read_output would give it to you \
         without asking the user. Where it would be kept back, or is too long for one \
         result, it still comes back as a reference. Defaults to false. Not with \
         background: true."
    );
    let without = argument_of(&the_table_without(&["read_output"]), "run", "read");
    assert!(!names_the_tool(&without, "read_output"), "{without}");
    assert_eq!(
        without,
        "Ask to read what the command prints in this result. This changes nothing in this \
         turn: output you may not read comes back as a reference. Defaults to false. Not \
         with background: true."
    );
    // Taking another tool off leaves the words alone.
    for other in ["spawn_processor", "job_output", "write_file", "read_file"] {
        assert_eq!(
            argument_of(&the_table_without(&[other]), "run", "read"),
            full,
            "taking {other} off changed the read argument"
        );
    }
}

/// TOOL-6. `read_file` sends a picture, a PDF and the saved outputs of a notebook cell to
/// `spawn_processor`, and a notebook's cells to `edit_file`, each only where the list holds it.
/// A list holding both reads the words the table always wrote.
#[test]
fn the_read_file_description_names_only_the_tools_the_list_holds() {
    let full = description_of(&the_table_without(&[]), "read_file");
    let to_the_processor = " Give that reference to spawn_processor with a question about it \
                            and the answer comes back the way any processor's does. That is \
                            how to check a screenshot or read a scanned page.";
    let to_edit = ": edit_file changes a cell by that number.";
    assert!(
        full.contains(&format!("whoever vouched for the directory it is in.{to_the_processor} A .ipynb notebook comes back as its cells, numbered from 1, with their source only{to_edit} Saved outputs")),
        "a list holding both lost the words it had: {full}"
    );
    let output_of = |offered: &[bravebot_aichat::protocol::Tool]| {
        argument_of(offered, "read_file", "output_of")
    };
    let full_output_of = output_of(&the_table_without(&[]));
    assert!(
        full_output_of.contains("They come back as a reference for spawn_processor, never as text"),
        "{full_output_of}"
    );

    let without_processor = the_table_without(&["spawn_processor"]);
    let described = description_of(&without_processor, "read_file");
    assert!(
        !names_the_tool(&described, "spawn_processor"),
        "{described}"
    );
    assert_eq!(described, full.replace(to_the_processor, ""));
    let argument = output_of(&without_processor);
    assert!(!names_the_tool(&argument, "spawn_processor"), "{argument}");
    assert_eq!(
        argument,
        full_output_of.replace("a reference for spawn_processor", "a reference")
    );

    let without_edit = the_table_without(&["edit_file"]);
    let described = description_of(&without_edit, "read_file");
    assert!(!names_the_tool(&described, "edit_file"), "{described}");
    assert_eq!(described, full.replace(to_edit, "."));
    assert_eq!(output_of(&without_edit), full_output_of);

    let neither = the_table_without(&["spawn_processor", "edit_file"]);
    let bare = description_of(&neither, "read_file");
    assert_eq!(
        bare,
        full.replace(to_the_processor, "").replace(to_edit, ".")
    );
}

/// TOOL-6. The `contents_ref` argument of `write_file` says where a processor's answer may be
/// written only where the list holds `spawn_processor`. A list without it has no answer of one
/// to write, and the driver refuses such a write whatever the words say (PROC-6).
#[test]
fn the_write_file_reference_argument_names_spawn_processor_only_where_the_list_holds_it() {
    let full = argument_of(&the_table_without(&[]), "write_file", "contents_ref");
    let lands = " What spawn_processor produced may be written only over the file that call \
                 was about. When the call read only references that are no file, such as a \
                 fetched page or a run's output, it may be written only to a new file in \
                 the scratch directory.";
    assert_eq!(
        full,
        format!(
            "A reference whose quarantined content becomes the whole file, e.g. \"ref:2\". \
             This is how to write out something you were never shown, such as a fetched \
             page.{lands} A picture or PDF reference is written as the file it encodes, to a \
             path you name ending in its extension, such as .png."
        )
    );
    let without = argument_of(
        &the_table_without(&["spawn_processor"]),
        "write_file",
        "contents_ref",
    );
    assert!(!names_the_tool(&without, "spawn_processor"), "{without}");
    assert_eq!(without, full.replace(lands, ""));
    for other in ["run", "read_output", "edit_file", "fetch_url"] {
        assert_eq!(
            argument_of(&the_table_without(&[other]), "write_file", "contents_ref"),
            full,
            "taking {other} off changed contents_ref"
        );
    }
}

/// TOOL-6. `edit_file` recommends `write_file` for a change to most of a file only where the
/// list holds it. A list holding it reads the words the table always wrote.
#[test]
fn the_edit_file_description_names_write_file_only_where_the_list_holds_it() {
    let full = description_of(&the_table_without(&[]), "edit_file");
    let prefer = " Prefer this to write_file when changing part of a file: a diff is easier \
                  to review than a whole body.";
    assert_eq!(
        full,
        format!(
            "Replace an exact passage of text in an existing workspace file.{prefer} An \
             edit inside a directory the user vouched for is applied without a prompt; \
             elsewhere the user approves each edit as a diff before it happens unless a \
             permission rule or mode answers for them. Explain what you are changing \
             either way. To change several places in one file, give them together in \
             edits: they are one diff. In a .ipynb notebook, name a cell \
             by the number a read showed instead of a passage: give cell and new_text to \
             replace that cell's source (its saved outputs are dropped), or cell_action \
             insert_after or delete."
        )
    );
    let without = description_of(&the_table_without(&["write_file"]), "edit_file");
    assert!(!names_the_tool(&without, "write_file"), "{without}");
    assert_eq!(without, full.replace(prefer, ""));
    for other in ["read_file", "spawn_processor", "run"] {
        assert_eq!(
            description_of(&the_table_without(&[other]), "edit_file"),
            full,
            "taking {other} off changed the edit_file description"
        );
    }
}

/// WRITE-3. The descriptions of the two writing tools do not say every write is put to the
/// user: a write to a trusted path with trusted data is applied unasked (TRUST-4). write_file
/// also says it replaces an existing file whole and that the path should be read or listed
/// first when the file may exist.
#[test]
fn the_writing_tools_describe_when_the_user_is_asked_and_what_a_write_replaces() {
    let table = the_table_without(&[]);
    let write = description_of(&table, "write_file");
    let edit = description_of(&table, "edit_file");
    for (name, text) in [("write_file", &write), ("edit_file", &edit)] {
        assert!(
            text.contains("inside a directory the user vouched for is applied without a prompt"),
            "{name}: {text}"
        );
        assert!(
            !text.contains("must approve"),
            "{name} still says every call needs approval: {text}"
        );
    }
    assert!(
        write.contains("replaces it whole and the old contents are gone"),
        "{write}"
    );
    assert!(
        write.contains("read or list the path first when the file may exist"),
        "{write}"
    );
}

/// TOOL-6. `repo_map`, `lsp` and `vet_content` name `read_file`, `read_file` and
/// `spawn_processor` only where the list holds them. A list holding them reads the words the
/// table always wrote.
#[test]
fn the_repo_map_lsp_and_vet_content_descriptions_name_only_the_tools_the_list_holds() {
    let every = the_table_without(&[]);
    for (tool, missing, sentence, kept) in [
        (
            "repo_map",
            "read_file",
            "never bodies: read_file reads the lines it points at.",
            "never bodies.",
        ),
        (
            "lsp",
            "read_file",
            "the line says so, and read_file will not open it.",
            "the line says so.",
        ),
        (
            "vet_content",
            "spawn_processor",
            "Say plainly in your reply why you need to read it rather than pass it to \
             spawn_processor.",
            "Say plainly in your reply why you need to read it.",
        ),
    ] {
        let full = description_of(&every, tool);
        assert!(full.contains(sentence), "{tool}: {full}");
        let without = description_of(&the_table_without(&[missing]), tool);
        assert!(!names_the_tool(&without, missing), "{tool}: {without}");
        assert_eq!(without, full.replace(sentence, kept), "{tool}");
        // A list without a tool this text does not name reads the full text.
        assert_eq!(
            description_of(&the_table_without(&["edit_file"]), tool),
            full,
            "{tool}: taking edit_file off changed it"
        );
    }
}

/// TOOL-6. A `spawn_agent` call's notice of the checkouts it made names `lsp` only where the
/// collecting turn's own list holds it, since a delegate's checkout strips `lsp` from the
/// delegate's own list unconditionally (CHECKOUT setup) but that says nothing about whether
/// the turn reading this notice, which may itself be a delegate, has `lsp` on its list.
#[test]
fn the_checkout_notice_names_lsp_only_where_the_list_holds_it() {
    let holding = Beside::every();
    let without = holding.without(Named::Lsp);

    assert_eq!(lsp_clause(holding, false), " and is not offered lsp");
    assert_eq!(lsp_clause(holding, true), " or is offered lsp");
    assert_eq!(lsp_clause(without, false), "");
    assert_eq!(lsp_clause(without, true), "");
}

/// TOOL-6. A checkout refusal names `read_git` only where this turn's list holds it, and a
/// name this turn can act on otherwise: the turn asking `spawn_agent` or `apply_checkout` for
/// a checkout is not always the turn that would have called `read_git`.
#[test]
fn a_checkout_refusal_names_read_git_only_where_the_list_holds_it() {
    assert_eq!(checkout_reader(Beside::every()), "read_git");
    assert_ne!(
        checkout_reader(Beside::every().without(Named::ReadGit)),
        "read_git"
    );
}

/// The processor's description as the turn it is read in is offered it.
fn processor_description(running: Running) -> String {
    available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        running,
    )
    .into_iter()
    .find(|t| t.function.name == "spawn_processor")
    .expect("the turn is offered a processor")
    .function
    .description
}

/// SPAWN-6. A request for a summary, a translation, a rewrite, an explanation or a list of the
/// points made of content the planner was not shown is done by a processor, and the
/// description is where a planner choosing between its tools reads that. A planner asked to
/// email a summary of a page had a processor and `write_file` and used neither: it wrote a
/// script that cut the page's own sentences into the body, and said it had not read them.
///
/// The route is the one the tools accept. A processor's answer about a page or a run's output is
/// for no file, so it is given to `run` as `stdin_ref` or written to a new file in the scratch
/// directory (PROC-6). The page is not saved to a file first.
#[test]
fn a_processor_is_described_as_the_writer_of_text_made_from_content_not_shown() {
    let description = processor_description(Running::Offered);
    for stated in [
        // The kinds of text, in the words a person asks for them in.
        "A summary, a translation, a rewrite, an explanation or a list of the points made of \
         content you were not shown is written by this tool",
        // The form is named in the instruction, so a summary comes back as the one asked for.
        "Ask for the form wanted, for example a plain-text summary of each of six stories, \
         two sentences each, with each link kept",
        // A script that scrapes cannot summarise, which is the defect the planner showed.
        "Do not leave it to a script you write, which can only cut the original down",
        // A reference that is no file is named in reads as it is.
        "A fetched page or a run's output needs no file: name its reference in reads",
        // Where the answer goes (PROC-6).
        "give the answer's reference to run as stdin_ref, or write the answer with write_file \
         and contents_ref to a new file in the scratch directory and run the program that \
         sends it with the file's path as an argument, so it reads the body from the file",
        // The head of the message is the planner's and only the body is derived.
        "The recipient and the subject are yours and only the body comes from the processor",
        // What the person is told afterwards.
        "Your final message says what was sent and to whom",
        "Call the text a summary in your final message only if a processor wrote one",
    ] {
        assert!(
            description.contains(stated),
            "the description does not say '{stated}': {description}"
        );
    }
    for gone in [
        "save the original to a file first",
        "cannot be written anywhere",
        "Do not save it with a script",
    ] {
        assert!(
            !description.contains(gone),
            "the description still tells the planner '{gone}': {description}"
        );
    }
}

/// TOOL-6. The same description is read by a turn that holds no `run`: a reader delegate, and a
/// turn addressed to a definition that dropped it. It is told who writes derived text and what
/// to call the result, and is not sent to a program that sends it.
#[test]
fn the_text_made_from_content_not_shown_names_run_only_where_the_turn_holds_one() {
    let withheld = processor_description(Running::Withheld);
    for stated in [
        "A summary, a translation, a rewrite, an explanation or a list of the points made of \
         content you were not shown is written by this tool",
        "Ask for the form wanted, for example a plain-text summary of each of six stories",
        "Call the text a summary in your final message only if a processor wrote one",
    ] {
        assert!(
            withheld.contains(stated),
            "a turn without run is not told '{stated}': {withheld}"
        );
    }
    let offered = processor_description(Running::Offered);
    for run_only in ["run", "script", "stdin_ref", "sends it"] {
        assert!(
            offered.contains(run_only),
            "'{run_only}' is not in the text of a turn with run, so the check below shows \
             nothing: {offered}"
        );
        assert!(
            !withheld.contains(run_only),
            "a turn without run is sent to '{run_only}': {withheld}"
        );
    }
    assert!(
        offered.contains("give the answer's reference to run as stdin_ref"),
        "a turn with run is not sent to it: {offered}"
    );
}

/// SPAWN-6. The `instruction` parameter said "no explanation, no summary", which is the advice
/// for a change to a file and read, by a planner asked for a summary, as a ban on asking for
/// one. It means no remarks about what the processor did, and the planner is told to name the
/// form when the document is itself derived text. The planning prompt carries the same advice
/// for a changed file and says it the same way.
#[test]
fn the_processor_instruction_does_not_forbid_asking_for_a_summary() {
    let offered = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let instruction = offered
        .iter()
        .find(|t| t.function.name == "spawn_processor")
        .expect("the turn is offered a processor")
        .function
        .parameters["properties"]["instruction"]["description"]
        .as_str()
        .expect("a description")
        .to_string();
    assert!(
        !instruction.contains("no summary") && !instruction.contains("no explanation"),
        "the instruction forbids the document a person may have asked for: {instruction}"
    );
    assert!(
        instruction.contains("no remarks about what it did, no code fence"),
        "the instruction does not ask for the document alone: {instruction}"
    );
    assert!(
        instruction.contains(
            "Where the document is itself a summary, a translation or an explanation, name \
             that form and its length"
        ),
        "the instruction does not tell the planner to name the form: {instruction}"
    );

    let planning = crate::turn::PLANNING;
    assert!(
        !planning.contains("no explanation, no summary"),
        "the planning prompt forbids the document a person may have asked for"
    );
    assert!(
        planning.contains("the whole document, no commentary on what was changed or why"),
        "the planning prompt does not say what to leave out of a changed file"
    );
}

/// SPAWN-6. `write_file` said `contents_ref` is how to write out "such as what spawn_processor
/// produced", which a planner reads as the way to save any processor's answer anywhere. An
/// answer is written only over the file the call was about, and one about a fetched page or a
/// run's output only to a new file in the scratch directory (PROC-6). The parameter says both,
/// to a turn with `run` and to one without, since writing the file needs no program.
#[test]
fn the_write_file_reference_text_does_not_offer_a_processors_answer_as_a_free_write() {
    let contents_ref = |running: Running| -> String {
        available(
            Scheduling::ArrangingALook,
            crate::watch::Arming::Unavailable,
            Deadlines::BUILT_IN,
            running,
        )
        .iter()
        .find(|t| t.function.name == "write_file")
        .expect("the turn is offered write_file")
        .function
        .parameters["properties"]["contents_ref"]["description"]
            .as_str()
            .expect("a description")
            .to_string()
    };
    for running in [Running::Offered, Running::Withheld] {
        let described = contents_ref(running);
        assert!(
            !described.contains("such as what spawn_processor produced"),
            "contents_ref offers a processor's answer as something to write anywhere \
             ({running:?}): {described}"
        );
        assert!(
            described.contains("such as a fetched page"),
            "contents_ref does not name the page a fetch gave ({running:?}): {described}"
        );
        assert!(
            described.contains(
                "What spawn_processor produced may be written only over the file that call \
                 was about. When the call read only references that are no file, such as a \
                 fetched page or a run's output, it may be written only to a new file in \
                 the scratch directory."
            ),
            "contents_ref does not say where a processor's answer may go ({running:?}): \
             {described}"
        );
        assert!(
            !described.contains("save a page to a file"),
            "contents_ref sends the planner to saving a page first ({running:?}): {described}"
        );
    }
}

/// TOOL-6, TOOL-7. The descriptions of `load_skill`, `search_skills`, `download_url` and
/// `ask_user` are written from the list: each names another tool only where the list holds it,
/// and a list holding every tool names every other tool. The regression it rejects is a
/// description that names `search_skills`, `load_skill`, `fetch_url` or a reading tool to a turn
/// whose list lacks it, as a definition's `tools:` line can leave it.
#[test]
fn the_skill_download_and_ask_user_descriptions_name_only_the_tools_the_list_holds() {
    let every = the_table_without(&[]);
    assert!(names_the_tool(
        &description_of(&every, "load_skill"),
        "search_skills"
    ));
    assert!(names_the_tool(
        &description_of(&every, "search_skills"),
        "load_skill"
    ));
    assert!(names_the_tool(
        &description_of(&every, "download_url"),
        "fetch_url"
    ));
    for reading in ["list_files", "search", "read_file", "run"] {
        assert!(
            names_the_tool(&description_of(&every, "ask_user"), reading),
            "{reading}"
        );
    }

    let without_search = the_table_without(&["search_skills"]);
    let load = without_search
        .iter()
        .find(|tool| tool.function.name == "load_skill")
        .expect("load_skill");
    assert!(
        !load.function.description.contains("search_skills")
            && !load
                .function
                .parameters
                .to_string()
                .contains("search_skills"),
        "{load:?}"
    );
    let without_load = description_of(&the_table_without(&["load_skill"]), "search_skills");
    assert!(
        !names_the_tool(&without_load, "load_skill"),
        "{without_load}"
    );
    let without_fetch = description_of(&the_table_without(&["fetch_url"]), "download_url");
    assert!(
        !names_the_tool(&without_fetch, "fetch_url"),
        "{without_fetch}"
    );
    for gone in ["list_files", "search", "read_file", "run"] {
        let asked = description_of(&the_table_without(&[gone]), "ask_user");
        assert!(!names_the_tool(&asked, gone), "{gone}: {asked}");
    }
    let none = description_of(
        &the_table_without(&["list_files", "search", "read_file", "run"]),
        "ask_user",
    );
    assert!(
        none.contains("say in your answer that you could not find it out instead"),
        "{none}"
    );
}

/// TOOL-6, TOOL-7. The descriptions of tools a turn may hold back for `load_tool` name
/// each other only where the list holds the other: `watch_file` names `schedule_next`,
/// `remember` names `recall` and `propose_rule` names `remember`. A held tool is written beside
/// the list the turn offers and not beside the other held tools. The regression it rejects is
/// a loaded tool whose description sends the planner to another that is still held back.
#[test]
fn a_held_back_tool_names_no_tool_the_offered_list_lacks() {
    let mut every = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    offer_remember(&mut every);
    offer_propose_rule(&mut every);
    offer_recall(&mut every);
    let mut full = every.clone();
    state_what_is_beside(&mut full);
    assert!(names_the_tool(
        &description_of(&full, "watch_file"),
        "schedule_next"
    ));
    assert!(
        full.iter()
            .find(|tool| tool.function.name == "remember")
            .expect("remember")
            .function
            .parameters
            .to_string()
            .contains("left out of recall")
    );
    assert!(names_the_tool(
        &description_of(&full, "propose_rule"),
        "remember"
    ));

    let mut offered = every.clone();
    let mut deferred = Deferred::split(&mut offered, &[]);
    state_what_is_beside(&mut offered);
    deferred.state_what_is_beside(&offered);
    let held = deferred.held();
    let watch = description_of(held, "watch_file");
    assert!(!names_the_tool(&watch, "schedule_next"), "{watch}");
    let remember = held
        .iter()
        .find(|tool| tool.function.name == "remember")
        .expect("remember is held back");
    assert!(
        !remember.function.parameters.to_string().contains("recall"),
        "{remember:?}"
    );
    let rule = description_of(held, "propose_rule");
    assert!(!names_the_tool(&rule, "remember"), "{rule}");
}

/// TOOL-6, TOOL-7. Where `watch_file` and `schedule_next` are held back for `load_tool`,
/// the `read_file` and `run` descriptions of the core list name neither, and the sentence that
/// arranges the next look sends the planner to `load_tool`. Where neither is held back they name
/// each directly. The regressions it rejects are a core description naming a tool
/// the list holds back, and the route to a later look dropped altogether.
#[test]
fn the_read_and_run_descriptions_name_a_held_back_watch_or_schedule_through_load_tool() {
    let every = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let mut unheld = every.clone();
    state_what_is_held(&mut unheld, &["repo_map".to_string()]);
    assert_eq!(
        description_of(&unheld, "read_file"),
        description_of(&every, "read_file")
    );
    assert_eq!(
        description_of(&unheld, "run"),
        description_of(&every, "run")
    );

    let mut offered = every.clone();
    let deferred = Deferred::split(&mut offered, &[]);
    state_what_is_beside(&mut offered);
    state_what_is_held(&mut offered, &deferred.names());
    for tool in ["read_file", "run"] {
        let said = description_of(&offered, tool);
        for held in ["watch_file", "schedule_next"] {
            assert!(!names_the_tool(&said, held), "{tool} names {held}: {said}");
        }
    }
    // `run` sends a watch request to the skill `watching`, so only `read_file` names the route.
    let read = description_of(&offered, "read_file");
    assert!(names_the_tool(&read, "load_tool"), "{read}");
    assert!(
        description_of(&offered, "read_file").contains("The next look is yours to arrange"),
        "the route to a later look was dropped"
    );

    // `schedule_next` held back and `watch_file` offered in full, as a `tools:` line naming
    // `watch_file` leaves it: the watch is still the better answer, without its second half.
    let mut named = every.clone();
    let deferred = Deferred::split(&mut named, &["watch_file".to_string()]);
    state_what_is_held(&mut named, &deferred.names());
    let read = description_of(&named, "read_file");
    assert!(read.contains("watch_file is the better answer"), "{read}");
    assert!(!names_the_tool(&read, "schedule_next"), "{read}");
}
