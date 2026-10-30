//! Where a session of the ChatGPT account is begun, and that what a front end builds is in one
//! ([CHATGPT-10]).
//!
//! The identifier a request carries is the one of the session that makes it. A task is in the
//! session it was told, and a task told none begins one of its own with its first request, so a front
//! end that builds a task without saying which session it is for gets one identifier for each task. A
//! backend given no session carries an identifier of its own, so one built for a request without
//! `.in_session` gets one for each call. Both compile and pass every test of what they send, and the
//! service finds no cache for either, so these tests read the code: every place that begins a session
//! is named, and every task and every backend the code builds is either in a session or listed here
//! with the reason it is not. The terminal makes five requests off the thread that owns the screen,
//! which no test reaches without a terminal, and the second test is what holds them.
//!
//! The reading has to be right about what is code, so the tests after those two hold it. A call's
//! window ends at the call, so a session named for the task beside it is not credited to it. A
//! comment or a string that holds the words of a session names none. A `#[cfg(test)]` takes out its
//! own item and no code after it, and a task built inside one is allowed only in a module of tests.
//!
//! A front end that holds a session of its own does three things, and a test here fails until it has
//! done them. It begins an `AccountSession` where the session begins and lists that place in the
//! first test. It says `.in_account_session(session.clone())` of every task it builds. It gives
//! `&session` to `turn::compact`, `aside`, `goal` and `commit_message` and to `Chat::new`, which take
//! it as an argument.
//!
//! [CHATGPT-10]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10

mod shipped;

use std::collections::BTreeMap;

/// Each source file of a crate that begins an account session, with how many times.
fn begun() -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    for source in shipped::sources() {
        let count = source.code.matches("AccountSession::begin").count();
        if count > 0 {
            found.insert(source.name, count);
        }
    }
    found
}

/// Every place that begins a session, and why it is one. A change that adds a place, or takes one
/// away, is a change to what begins a session and to this list. A place that is not on it begins an
/// identifier for something that may belong to a session already. A front end that is not on it has
/// no session to give its tasks, and the second test fails for each task it builds.
#[test]
fn a_session_of_the_account_is_begun_only_where_a_session_begins() {
    let expected: BTreeMap<String, usize> = [
        // A request made in no session is given one, so it shares no cache with another.
        ("crates/agent/src/backend.rs", 1),
        // A task told no session begins one with its first request, which is a delegate's and a
        // task built for one turn.
        ("crates/agent/src/turn/task.rs", 1),
        // A plain or background session, where it is assembled.
        ("crates/cli/src/plain.rs", 1),
        // The terminal's session, when it opens and when `/clear` or a handoff begins another.
        ("crates/tui/src/state.rs", 1),
        ("crates/tui/src/state/settings.rs", 1),
        // The desktop's session, begun once in `State::blank`, which the opened, resumed and
        // forked constructors all use.
        ("crates/ui-bridge/src/running.rs", 1),
    ]
    .into_iter()
    .map(|(file, count)| (file.to_string(), count))
    .collect();

    assert_eq!(begun(), expected);
}

/// What a place that builds a task or a backend does about the session its requests are made in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Does {
    /// Names the session: `.in_account_session(..)` of a task, `.in_session(..)` of a backend.
    Says,
    /// Names none of a task, which begins a session of its own with its first request.
    BeginsOwn,
    /// Names none of a backend, which a request through it carries an identifier of its own for.
    CarriesOwn,
    /// Builds a backend to ask what it is, as `spends_a_subscription` and `service_name` do, and
    /// sends nothing.
    Asks,
}

/// The calls the second test follows, and whether each builds a task.
const TRACKED: [(&str, bool); 3] = [
    ("Task::new", true),
    ("Task::delegated", true),
    ("Backend::select", false),
];

/// The statement that starts at `from` in `code`, as the code of it alone.
///
/// It ends where the call ends: at the `;` that ends the statement, the `{` that opens a block, the
/// `,` or the `=>` that ends an argument, an element or an arm, or the bracket that closes what the
/// call was inside, each at the depth the call started at. What follows belongs to something else,
/// which may be another call beside this one that does name its session. A comment is left out, and
/// what is between the quotes of a string, so that neither can hold the name of a session or hide a
/// bracket.
fn statement(code: &str, from: usize) -> String {
    let bytes = code.as_bytes();
    let mut cleaned = String::new();
    let (mut depth, mut at) = (0usize, from);
    while at < bytes.len() {
        let byte = bytes[at];
        match byte {
            b'"' => {
                cleaned.push_str("\"\"");
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
            }
            b'r' if at == 0
                || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') =>
            {
                // A raw string, `r"` or `r#"`, which ends at the quote and the hashes it began with.
                let hashes = bytes[at + 1..]
                    .iter()
                    .take_while(|byte| **byte == b'#')
                    .count();
                if bytes.get(at + 1 + hashes) == Some(&b'"') {
                    let closing = format!("\"{}", "#".repeat(hashes));
                    cleaned.push_str("\"\"");
                    at += 2 + hashes;
                    while at < bytes.len() && !code[at..].starts_with(&closing) {
                        at += 1;
                    }
                    at += closing.len() - 1;
                } else {
                    cleaned.push('r');
                }
            }
            b'\'' => {
                // A character, whose quote may be a `"`, and not a lifetime.
                if bytes.get(at + 1) == Some(&b'\\') {
                    while at < bytes.len() && bytes[at + 2..].first() != Some(&b'\'') {
                        at += 1;
                    }
                    at += 2;
                } else if bytes.get(at + 2) == Some(&b'\'') {
                    at += 2;
                } else {
                    cleaned.push('\'');
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                while at + 1 < bytes.len() && !(bytes[at] == b'*' && bytes[at + 1] == b'/') {
                    at += 1;
                }
                at += 1;
            }
            b'(' | b'[' => {
                depth += 1;
                cleaned.push(char::from(byte));
            }
            b')' | b']' | b'}' if depth == 0 => break,
            b')' | b']' | b'}' => {
                depth -= 1;
                cleaned.push(char::from(byte));
            }
            b'{' if depth == 0 => break,
            b'{' => {
                depth += 1;
                cleaned.push('{');
            }
            b';' | b',' if depth == 0 => break,
            b'=' if depth == 0 && bytes.get(at + 1) == Some(&b'>') => break,
            _ => cleaned.push(char::from(byte)),
        }
        at += 1;
    }
    cleaned
}

/// Every call to build a task or a backend in `code`, as its offset, what it builds and what it does
/// about the session.
fn sites_in(code: &str) -> Vec<(usize, &'static str, Does)> {
    let mut found = Vec::new();
    for (builds, is_a_task) in TRACKED {
        for (offset, _) in code.match_indices(&format!("{builds}(")) {
            let statement = statement(code, offset);
            let does = match (is_a_task, statement.as_str()) {
                (true, text) if text.contains(".in_account_session(") => Does::Says,
                (true, _) => Does::BeginsOwn,
                (false, text) if text.contains(".in_session(") => Does::Says,
                (false, text)
                    if text.contains(".spends_a_subscription(")
                        || text.contains(".service_name(") =>
                {
                    Does::Asks
                }
                (false, _) => Does::CarriesOwn,
            };
            found.push((offset, builds, does));
        }
    }
    found.sort();
    found
}

/// Every place the shipped code builds a task or a backend, as the file, the line, what is built
/// and what it does about the session.
fn built() -> Vec<(String, usize, &'static str, Does)> {
    let mut found = Vec::new();
    for source in shipped::sources() {
        for (offset, builds, does) in sites_in(&source.code) {
            let line = source.numbers[source.code[..offset].matches('\n').count()];
            found.push((source.name.clone(), line, builds, does));
        }
    }
    found
}

/// Every task and every backend the code builds is in a session or is listed here with the reason
/// it is not.
///
/// A task built without `.in_account_session` begins a session of its own with its first request,
/// and a backend built without `.in_session` carries an identifier of its own in each request, so
/// the requests of one session reach the account under several identifiers and none of them finds
/// the cache of another. The failure names the file and the line. The remedy for a front end that
/// holds a session is to name it there, and a place that is one whole session of its own on purpose
/// is listed below with why.
#[test]
fn every_task_and_every_backend_the_code_builds_is_in_a_session_or_listed_as_not() {
    use Does::{Asks, BeginsOwn, CarriesOwn, Says};
    let listed: &[(&str, &str, Does, usize)] = &[
        // A delegate is a session of its own, begun with its first request, and is built for one
        // run and never cloned.
        (
            "crates/agent/src/delegate.rs",
            "Task::delegated",
            BeginsOwn,
            1,
        ),
        // A one-shot run is one session, begun with its first request. Every round of a goal run is
        // made from a clone of that task, which is in the same session.
        ("crates/cli/src/main.rs", "Task::new", BeginsOwn, 1),
        // The sessions that hold one, which they name in every task they build: a plain or
        // background session, the terminal's turns and its `/checkouts apply`, and the desktop's
        // turns and manifest runs.
        ("crates/cli/src/plain.rs", "Task::new", Says, 1),
        ("crates/tui/src/app.rs", "Task::new", Says, 1),
        ("crates/tui/src/app/actions.rs", "Task::new", Says, 1),
        ("crates/ui-bridge/src/bridge.rs", "Task::new", Says, 1),
        ("crates/ui-bridge/src/manifest.rs", "Task::new", Says, 1),
        // Every request the agent makes names the session of the task or the chat it is made for,
        // and the places that only ask what a model is, or which service it is sent to, send
        // nothing.
        ("crates/agent/src/backend.rs", "Backend::select", Asks, 1),
        ("crates/ui-bridge/src/bridge.rs", "Backend::select", Asks, 1),
        ("crates/agent/src/manifest.rs", "Backend::select", Says, 1),
        ("crates/agent/src/processor.rs", "Backend::select", Says, 1),
        ("crates/agent/src/turn.rs", "Backend::select", Says, 1),
        ("crates/agent/src/turn.rs", "Backend::select", Asks, 1),
    ];

    let mut found: BTreeMap<(String, &str, Does), Vec<usize>> = BTreeMap::new();
    for (name, line, builds, does) in built() {
        found.entry((name, builds, does)).or_default().push(line);
    }
    let mut expected: BTreeMap<(String, &str, Does), usize> = BTreeMap::new();
    for (name, builds, does, count) in listed {
        expected.insert((name.to_string(), *builds, *does), *count);
    }

    let mut problems = Vec::new();
    for ((name, builds, does), lines) in &found {
        let listed = expected
            .get(&(name.clone(), *builds, *does))
            .copied()
            .unwrap_or(0);
        if lines.len() != listed {
            for line in lines {
                problems.push(format!(
                    "{name}:{line}: {builds} {}, which is not listed ({listed} listed, {} found)",
                    match does {
                        Says => "names its session",
                        BeginsOwn => "names no session, so the task begins one of its own",
                        CarriesOwn =>
                            "names no session, so each request carries an identifier of its own",
                        Asks => "only asks what the model is",
                    },
                    lines.len()
                ));
            }
        }
    }
    for (key, listed) in &expected {
        if !found.contains_key(key) {
            problems.push(format!(
                "{}: {} {:?} is listed {listed} times and is not there",
                key.0, key.1, key.2
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "a task or a backend is built in no session. Name the session with `.in_account_session(..)` \
         or `.in_session(..)`, or list the place in this test with the reason it is not in one:\n{}",
        problems.join("\n")
    );
}

/// Whether `item`, the first line of what a `#[cfg(test)]` is on, opens a module.
fn opens_a_module(item: &str) -> bool {
    let item = item.strip_prefix("pub").map_or(item, |rest| {
        let rest = rest.trim_start();
        match rest.strip_prefix('(') {
            Some(inside) => inside.split_once(')').map_or(rest, |(_, after)| after),
            None => rest,
        }
        .trim_start()
    });
    item.starts_with("mod ") && item.trim_end().ends_with('{')
}

/// A `#[cfg(test)]` hides the module of tests it is on and no code that ships.
///
/// The scan above reads what is left when each item marked for tests is taken out, so an attribute
/// that took out more than its item would take shipped code out of both tests with it, and nothing
/// would say so. The item ends where `rustfmt` ends it, which is a convention and not a parse, so
/// this also looks at what each attribute took out: a task or a backend built inside it is allowed
/// only in a module of tests, where it is test code. Anything else that builds one under an
/// attribute is shown with its file and line, for the author to put in a module of tests, where it
/// is not read, or to take the attribute off.
#[test]
fn a_test_attribute_hides_a_module_of_tests_and_no_task_or_backend_the_code_builds() {
    let mut problems = Vec::new();
    for source in shipped::sources() {
        for hidden in &source.hidden {
            let builds = TRACKED
                .iter()
                .any(|(call, _)| hidden.text.contains(&format!("{call}(")));
            if builds && !opens_a_module(&hidden.item) {
                problems.push(format!(
                    "{}:{}: `#[cfg(test)]` on `{}` takes out {} lines that build a task or a backend",
                    source.name,
                    hidden.line,
                    hidden.item,
                    hidden.text.lines().count()
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A session named for another task beside a call is not named for it. In a match arm, a guard and
/// its arm, a tuple, a `vec!` and the arguments of a call, the second task has the session and the
/// first has none, and the first is the one the scan reports.
#[test]
fn a_session_named_for_the_task_beside_a_task_is_not_credited_to_it() {
    let code = "\
match kind {
    A => Task::new(\"a\"),
    B => Task::new(\"b\").in_account_session(s),
}
let pair = (Task::new(\"c\"), Task::new(\"d\").in_account_session(s));
let many = vec![Task::new(\"e\"), Task::new(\"f\").in_account_session(s)];
f(Task::new(\"g\"), Task::new(\"h\").in_account_session(s));
let asked = (Backend::select(c, e, m), Backend::select(c, e, m).in_session(s));
match kind {
    _ if Task::new(\"i\").is_empty() => Task::new(\"j\").in_account_session(s),
}
";
    let does: Vec<Does> = sites_in(code)
        .into_iter()
        .map(|(_, _, does)| does)
        .collect();
    assert_eq!(
        does,
        [
            Does::BeginsOwn,
            Does::Says,
            Does::BeginsOwn,
            Does::Says,
            Does::BeginsOwn,
            Does::Says,
            Does::BeginsOwn,
            Does::Says,
            Does::CarriesOwn,
            Does::Says,
            Does::BeginsOwn,
            Does::Says,
        ]
    );
}

/// A comment or a string that holds the words of a session does not name one, whether the comment
/// ends the line or sits between two calls, and whether the string is an ordinary one, a raw one or
/// a character beside a quote. A session named after a comment still counts.
#[test]
fn a_comment_or_a_string_that_names_a_session_does_not_name_it() {
    let code = "\
let a = Task::new(\"a\") // .in_account_session(s)
    .with_model(None);
let b = Task::new(\"b\").with_model(Some(\".in_account_session(\".into()));
let c = Task::new(\"c\") /* .in_account_session(s) */ .with_model(None);
let d = Task::new(\"d\").with_model(Some(r#\".in_account_session(\"#.into()));
let e = Task::new(\"e\").with_model(Some('\"'.to_string())).with_effort(None);
let f = Task::new(\"f\")
    // the session of the person
    .in_account_session(s);
let g = Task::new(\"g\").with_model(Some(\"a // b\".into())).in_account_session(s);
";
    let does: Vec<Does> = sites_in(code)
        .into_iter()
        .map(|(_, _, does)| does)
        .collect();
    assert_eq!(
        does,
        [
            Does::BeginsOwn,
            Does::BeginsOwn,
            Does::BeginsOwn,
            Does::BeginsOwn,
            Does::BeginsOwn,
            Does::Says,
            Does::Says,
        ]
    );
}

/// A `#[cfg(test)]` on a field, a `use`, a constant or a variant takes out that item and not what
/// follows, and on a module takes out the module, as does `#[cfg(all(test, unix))]`, which gates
/// the item on a test build as well.
#[test]
fn a_test_attribute_takes_out_its_own_item_and_nothing_after_it() {
    let text = "\
struct Held {
    #[cfg(test)]
    pub field: u8,
    other: u8,
}

fn after_a_field() {
    Task::new(\"x\");
}

#[cfg(test)]
use somewhere::Thing;

fn after_a_use() {}

#[cfg(test)]
const SOMETHING: u8 = 1;

fn after_a_constant() {}

enum Kind {
    #[cfg(test)]
    Only,
    #[cfg(test)]
    Wrapped {
        inside: u8,
    },
    Shipped,
}

fn after_a_variant() {}

fn call() {
    #[cfg(test)]
    let seen = record(
        \"x\",
    );
    after_a_statement();
}

#[cfg(test)]
mod tests {
    fn hidden() {
        Task::new(\"hidden\");
    }
}

#[cfg(all(test, unix))]
mod only_on_unix {
    fn hidden_on_unix() {
        Task::new(\"hidden\");
    }
}

fn last() {}
";
    let source = shipped::code_of(text);
    for kept in [
        "after_a_field",
        "Task::new(\"x\")",
        "after_a_use",
        "after_a_constant",
        "other: u8",
        "Shipped,",
        "after_a_variant",
        "after_a_statement",
        "fn last",
    ] {
        assert!(source.code.contains(kept), "{kept} was taken out");
    }
    for taken in [
        "pub field",
        "Thing",
        "SOMETHING",
        "Only",
        "inside",
        "seen",
        "hidden",
        "hidden_on_unix",
    ] {
        assert!(!source.code.contains(taken), "{taken} was kept");
    }
    assert_eq!(source.hidden.len(), 8);
    assert!(opens_a_module(&source.hidden[6].item));
    assert!(opens_a_module(&source.hidden[7].item));
    assert!(!opens_a_module(&source.hidden[0].item));
}
