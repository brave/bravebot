//! The tools the relay offers, held to the rule every tool is added under.
//!
//! A person answers for a call from the question BraveBot asks before it, which shows the tool as
//! `alias:tool` and the arguments as the planner wrote them (SERVERS-7). Routing is the alias and
//! the tool and never the arguments (ROUTE-7), so what has to hold of a tool is that its name is
//! one a person can read in a list and that each argument is one they can judge in that question.
//! A tab id, a node id or a selector read from a page is not, since it says nothing about which
//! page it is or where an action lands (BROWSER-13).

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_browser::tools::TOOLS;
use serde_json::Value;
use std::collections::BTreeSet;

/// Every argument a tool takes today, each one a person can judge in the question before the call:
/// an address, a name or a short text they can read, a number, a time or a size, one of a short
/// list of words, or yes or no. The design document says what the question shows for each
/// (`docs/design/browser-extension.md`, "Tool surface").
///
/// A name is added here by a person who has read ROUTE-7 and BROWSER-13 and can say what the
/// question shows for it, and the design document gets the same row. A name that no tool takes is
/// not kept, so this list cannot hold one ahead of the tool that would use it.
const JUDGEABLE: &[&str] = &[
    // An address, an origin or the exact title or name of one thing, as the person would write it.
    "filename",
    "folder",
    "frame_url",
    "name",
    "option",
    "parent",
    "query",
    "onto_name",
    "text",
    "title",
    "to",
    "url",
    "urls",
    // The name of a file in the uploads directory, or its absolute path inside it, as the person
    // reads it in the question and in the window. It is not a selector or an id read from a page:
    // the planner wrote it, the server looks at the file it leads to before anything is sent
    // (`uploads` in the relay), and a person judges it from the path, which the window shows again
    // in full with the file's size.
    "path",
    // The code `run_script` runs, at most 2,000 characters. It is the planner's own text, and the
    // question draws every line of it with what cannot be seen written out (SERVERS-7), and the
    // window shows it again with the exact address before anything runs (BROWSER-46).
    "code",
    // A count, a position, a size, a factor or a time the person can read. `element_number` is
    // the number of a line of the list `list_page_elements` made, which the person read in the
    // transcript, next to the role and name of the element on that line; the extension shows the
    // element itself in a window, and the number says nothing the person cannot check against
    // the list.
    "count",
    "element_number",
    "end",
    "factor",
    "frames",
    "height",
    "index",
    "max_entries",
    "max_results",
    "minutes",
    "pages",
    "seconds",
    "since",
    "start",
    "width",
    // One of a list of words, a list of them, or yes or no.
    "button",
    "collapsed",
    "color",
    "direction",
    "key",
    "level",
    "levels",
    "muted",
    "onto_role",
    "pinned",
    "read",
    "role",
    "types",
];

const WHEN_ADDING: &str = "A tool takes only arguments a person can judge in the question before the call, such as a URL, \
    a name, a short text, a number or a time they can read, and never a tab id, a node id or a \
    selector read from a page. Read ROUTE-7 in docs/specs/routing.md and BROWSER-13 in \
    docs/specs/browser.md, and then name the argument in JUDGEABLE and in the \"Tool surface\" \
    table of docs/design/browser-extension.md if it is one";

/// The `properties` of a tool's input schema, or none where it has none.
fn arguments(schema: &Value) -> Vec<(String, Value)> {
    schema["properties"]
        .as_object()
        .map(|properties| {
            properties
                .iter()
                .map(|(name, property)| (name.clone(), property.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// A name is lower-case words joined by underscores, at least two, as `list_tabs` and
/// `get_visits` are: a verb and what it acts on.
fn is_verb_noun(name: &str) -> bool {
    let words: Vec<&str> = name.split('_').collect();
    words.len() >= 2
        && words
            .iter()
            .all(|word| !word.is_empty() && word.chars().all(|letter| letter.is_ascii_lowercase()))
}

/// A tool named in another style is one a person reads differently in the list and in the
/// question, and one a rule that matches a name by its words does not treat as the others.
#[test]
fn every_tool_is_named_in_lower_case_words_joined_by_underscores() {
    for tool in TOOLS {
        assert!(
            is_verb_noun(tool.name),
            "`{}` is not named like the others: lower-case words joined by underscores, a verb \
             and what it acts on, as in `list_tabs` (BROWSER-10)",
            tool.name
        );
    }
    for wrong in [
        "listTabs",
        "ListTabs",
        "list-tabs",
        "tabs",
        "list__tabs",
        "_tabs",
        "tabs_",
    ] {
        assert!(!is_verb_noun(wrong), "{wrong}");
    }
}

/// An argument that is not on the list is one nobody decided a person can judge, which is how a
/// tab id, a node id or a selector would enter. One that no tool takes is not kept either, so the
/// list is the arguments in use and not a stock of names for a tool to take later.
#[test]
fn every_argument_of_every_tool_is_one_a_person_can_judge_in_the_question() {
    let allowed: BTreeSet<&str> = JUDGEABLE.iter().copied().collect();
    assert_eq!(allowed.len(), JUDGEABLE.len(), "JUDGEABLE names one twice");
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for tool in TOOLS {
        for (name, _) in arguments(&tool.listed()["inputSchema"]) {
            assert!(
                allowed.contains(name.as_str()),
                "{} takes `{name}`, which is not on the list of arguments a person can judge. \
                 {WHEN_ADDING}",
                tool.name
            );
            taken.insert(name);
        }
    }
    let unused: Vec<&&str> = allowed
        .iter()
        .filter(|name| !taken.contains(**name))
        .collect();
    assert!(
        unused.is_empty(),
        "JUDGEABLE names {unused:?}, which no tool takes: remove it, and its row of the design \
         document"
    );
}

/// A schema that names arguments by a pattern, by a reference, or by allowing any other one would
/// take an argument the list above never sees, and one that is an object or a list of objects can
/// hold an id or a selector inside a name that is on the list.
#[test]
fn a_schema_names_its_arguments_in_properties_and_each_is_a_word_a_number_a_choice_or_a_list_of_words()
 {
    for tool in TOOLS {
        let listed = tool.listed();
        let schema = &listed["inputSchema"];
        for key in schema.as_object().unwrap().keys() {
            assert!(
                ["type", "properties", "required"].contains(&key.as_str()),
                "{} has `{key}` in its schema, which can take an argument that `properties` does \
                 not name. {WHEN_ADDING}",
                tool.name
            );
        }
        for (name, property) in arguments(schema) {
            let scalar = |kind: &Value| {
                ["string", "integer", "number", "boolean"]
                    .iter()
                    .any(|allowed| kind == allowed)
            };
            let shape = &property["type"];
            let fits = scalar(shape) || (shape == "array" && property["items"]["type"] == "string");
            assert!(
                fits,
                "{} takes `{name}` as {property}, which is not a string, a number, a boolean or a \
                 list of strings. {WHEN_ADDING}",
                tool.name
            );
        }
        for required in schema["required"].as_array().into_iter().flatten() {
            assert!(
                schema["properties"][required.as_str().unwrap()].is_object(),
                "{} requires {required}, which it does not take",
                tool.name
            );
        }
    }
}

/// A listing that declared the shape of a result would invite a client to read the result as data
/// of a known kind. A result is untrusted and private whatever the tool is (MCP-1), so the listing
/// holds a name, a description and what the call takes, and nothing about what comes back.
#[test]
fn a_tool_is_listed_with_a_name_a_description_and_an_input_schema_and_no_result_shape() {
    for tool in TOOLS {
        let listed = tool.listed();
        let keys: BTreeSet<&str> = listed
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            BTreeSet::from(["description", "inputSchema", "name"]),
            "{} is listed with a field other than its name, description and input schema. A \
             result is untrusted and private (MCP-1) and the planner never reads it (BROWSER-13), \
             so a tool says nothing of the shape of one",
            tool.name
        );
    }
}
