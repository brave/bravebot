//! Which of this crate's words are prose for a person and which are interface to the planner.
//!
//! The distinction is not visible in a type: both are `&str`, both end up in a `format!`, and the
//! natural reflex when a repository becomes localizable is to reach for every string literal in
//! it. Doing that to a tool's description would change what the model does, in a language the
//! person who changed it does not read, and no test of behaviour would catch it because the
//! behaviour would still be self-consistent.
//!
//! So the boundary is drawn where it can be checked: the modules below talk to the planner and
//! to nobody else, and nothing in them is looked up in a catalog. A string in this crate that
//! reaches the screen belongs somewhere else, which is why the word a transcript line begins
//! with lives with the rest of the reporting surface rather than beside the tool table.

use bravebot_agent::exec::Deadlines;
use bravebot_agent::tools::{self, Running, Scheduling};
use bravebot_agent::watch::Arming;
use bravebot_core::delegate::Definitions;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

/// The modules whose every string is read by the model rather than by a person.
const PLANNER_FACING: [&str; 8] = [
    "src/tools.rs",
    "src/tools/offered.rs",
    "src/preamble.rs",
    "src/compact.rs",
    "src/processor.rs",
    "src/programs.rs",
    "src/goal.rs",
    "src/turn/prompts.rs",
];

/// A guard is worth nothing if it is silently covering a file that no longer exists.
#[test]
fn every_planner_facing_module_named_here_is_a_module_that_exists() {
    for name in PLANNER_FACING {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        assert!(
            path.exists(),
            "{name} is listed as planner-facing and is not here. Either it moved, in which case \
             follow it, or it went away, in which case take it off the list"
        );
    }
}

/// The rule this file exists for: a translation must not be able to reach the model.
///
/// Written as a search of the source rather than as a comparison of two rendered requests
/// because the failure it guards against is somebody adding one lookup, and a request built
/// under the only catalog that has shipped so far would read identically either way.
#[test]
fn what_the_planner_reads_is_not_taken_from_a_catalog() {
    for name in PLANNER_FACING {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let source = std::fs::read_to_string(&path).expect("read a module of this crate");
        for (number, line) in source.lines().enumerate() {
            assert!(
                !looks_up_a_message(line),
                "{name}:{} looks up a message in a catalog, and everything in this module is \
                 read by the planner. Translating it would change what the model does. If the \
                 string is shown to a person, it belongs in a module that talks to people.\n\
                 {line}",
                number + 1
            );
        }
    }
}

/// Every tool the planner can be offered, serialized as it is sent: each combination of what a turn
/// may say about when it runs again, whether the session keeps watches and whether `run` is on the
/// list, with the tools and sentences that only some turns add.
fn tools_as_sent() -> Vec<Value> {
    let mut sent = Vec::new();
    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::NoLaterLook,
    ] {
        for arming in [
            Arming::Allowed { free: 1 },
            Arming::UnderALoop,
            Arming::UnderAGoal,
            Arming::Full,
            Arming::Unavailable,
        ] {
            for running in [Running::Offered, Running::Withheld] {
                let mut table = tools::for_planner(
                    scheduling,
                    arming,
                    &Definitions::default(),
                    Deadlines::BUILT_IN,
                    running,
                );
                tools::offer_advisor(&mut table);
                tools::offer_propose_plan(&mut table);
                tools::offer_message_session(&mut table);
                tools::offer_tool_loader(&mut table, &["alias:tool".to_string()]);
                tools::state_withheld_variables(&mut table);
                sent.extend(
                    table
                        .iter()
                        .map(|tool| serde_json::to_value(tool).expect("a tool serializes")),
                );
            }
        }
    }
    sent
}

/// Every string in `value`, with where it is.
fn strings_in<'a>(at: String, value: &'a Value, found: &mut Vec<(String, &'a str)>) {
    match value {
        Value::String(text) => found.push((at, text)),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                strings_in(format!("{at}[{index}]"), item, found);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                strings_in(format!("{at}.{key}"), item, found);
            }
        }
        _ => {}
    }
}

/// A description written across source lines ends each one with a backslash, which drops the next
/// line's indentation. One written without it keeps that indentation as text, and the planner is
/// sent runs of spaces in the middle of a sentence.
///
/// Read from the tools as the planner receives them rather than from the source, so a description
/// that runs over several lines is checked to its last one, and a sentence a function assembles
/// is checked as it is sent.
///
/// Rejects: a source line of a description left without its trailing backslash, wherever it falls
/// in the literal, and a run of spaces written inside a description on one source line.
#[test]
fn a_tool_description_holds_no_run_of_spaces() {
    let mut offenders = BTreeSet::new();
    for tool in tools_as_sent() {
        let name = tool["function"]["name"]
            .as_str()
            .expect("a tool has a name");
        let mut found = Vec::new();
        strings_in(String::new(), &tool, &mut found);
        for (at, text) in found {
            if text.contains("  ") {
                offenders.insert(format!("{name}{at}: {text}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a tool description has a run of spaces, which is the next source line's indentation \
         kept as text. End each source line of a multi-line description with a backslash in \
         src/tools/offered.rs so the indentation is dropped.\n{}",
        offenders.into_iter().collect::<Vec<_>>().join("\n")
    );
}

/// Whether the line calls the catalog lookup, as opposed to merely ending a longer macro name
/// with the same three characters, which `format!(` and `assert!(` both do. The lookup written
/// with its crate path, `bravebot_i18n::t!(`, is a lookup too.
fn looks_up_a_message(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(offset) = line[from..].find("t!(") {
        let at = from + offset;
        match at.checked_sub(1).map(|before| bytes[before]) {
            Some(b':') if line[..at].ends_with("bravebot_i18n::") => return true,
            Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b':' => {}
            _ => return true,
        }
        from = at + 3;
    }
    false
}

/// The search is only worth its name if it recognises each way a lookup is written, and passes
/// the macros that merely end in the same characters.
#[test]
fn a_lookup_is_recognised_however_it_is_written() {
    for lookup in [
        "let line = t!(hello);",
        "t!(hello, name = who)",
        "let line = bravebot_i18n::t!(hello);",
        "use_it(bravebot_i18n::t!(hello, name = who))",
    ] {
        assert!(looks_up_a_message(lookup), "{lookup}");
    }
    for other in [
        "let line = format!(\"{x}\");",
        "assert!(ready);",
        "std::print!(\"x\")",
        "let line = other_crate::t!(hello);",
    ] {
        assert!(!looks_up_a_message(other), "{other}");
    }
}
