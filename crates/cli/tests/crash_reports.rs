//! Which executables of this workspace report crashes, and that each does so the one way.
//!
//! [CRASH-5] holds because the directory a report goes in is decided inside the shared start
//! function, from the environment and incognito mode, so an executable has no directory to pass.
//! What is left for an executable to get wrong is whether it calls the function at all, and with
//! which word. A launch of the real executables cannot be run here with an address baked in, so
//! this reads their sources, and it names every executable the workspace builds so that a new one
//! has to say which side of the line it is on.
//!
//! [CRASH-5]: ../../../docs/specs/crash-reports.md

use std::path::{Path, PathBuf};

/// The executables that report, and the word each says about itself.
const REPORTING: [(&str, &str); 4] = [
    ("crates/cli/src/main.rs", "Cli"),
    ("crates/ui-bridge/src/bin/bravebot-host.rs", "Host"),
    ("crates/ui-bridge/src/bin/bravebot-rpc.rs", "Rpc"),
    ("crates/ui-bridge/src/bin/bravebot-acp.rs", "Acp"),
];

/// The executables that do not, each with the reason. Neither starts a session or holds anything a
/// panic report would say more about than the program it relays or the file it opens.
const NOT_REPORTING: [(&str, &str); 2] = [
    (
        "crates/browser/src/main.rs",
        "relays messages between a browser extension and a server and links no workspace crate",
    ),
    (
        "crates/ui-files/src/main.rs",
        "opens and writes the files the window names, under a directory it is handed, and depends on nothing but the file type crate",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `src/main.rs` and every file in `src/bin` of a member crate, relative to the workspace,
/// the way Cargo finds an executable without the manifest naming it.
fn executables() -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root().join("crates")).expect("the crates directory") {
        let member = entry.expect("a directory entry").path();
        let name = member
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        if member.join("src/main.rs").is_file() {
            found.push(format!("crates/{name}/src/main.rs"));
        }
        if let Ok(bins) = std::fs::read_dir(member.join("src/bin")) {
            for bin in bins {
                let bin = bin.expect("a directory entry").path();
                if bin.extension().is_some_and(|extension| extension == "rs") {
                    let file = bin
                        .file_name()
                        .expect("a name")
                        .to_string_lossy()
                        .into_owned();
                    found.push(format!("crates/{name}/src/bin/{file}"));
                }
            }
        }
    }
    found.sort();
    found
}

/// What the source says to this crate, with comments dropped and whitespace removed so a call that
/// rustfmt split across lines reads as one.
fn code_of(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<String>()
        .split_whitespace()
        .collect()
}

/// The text of each call to `start` and of anything else that begins reporting, between its
/// opening parenthesis and the one that closes it.
fn calls_in(source: &str) -> Vec<(String, String)> {
    let code = code_of(source);
    let mut calls = Vec::new();
    for function in ["start", "start_with", "begin"] {
        let needle = format!("bravebot_backtrace::{function}(");
        let mut from = 0;
        while let Some(at) = code[from..].find(&needle) {
            let open = from + at + needle.len();
            let mut depth = 1;
            let mut end = open;
            for (offset, character) in code[open..].char_indices() {
                match character {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    end = open + offset;
                    break;
                }
            }
            calls.push((function.to_string(), code[open..end].to_string()));
            from = end;
        }
    }
    calls
}

/// CRASH-5: each executable that reports calls the shared function once, with its own word and no
/// directory, and calls nothing that would let it choose an address or a directory.
#[test]
fn every_reporting_executable_starts_it_once_with_its_own_word() {
    for (file, word) in REPORTING {
        let source = std::fs::read_to_string(root().join(file)).expect("an executable's source");
        let calls = calls_in(&source);
        assert_eq!(
            calls,
            [(
                "start".to_string(),
                format!("bravebot_backtrace::Process::{word}")
            )],
            "{file} does not start crash reports once, as {word}, and nothing else"
        );
    }
}

/// An executable that does not report names nothing from the crate, so a person reading it is not
/// left to wonder what it does about a panic.
#[test]
fn an_executable_that_does_not_report_names_nothing_of_the_crate() {
    for (file, why) in NOT_REPORTING {
        let source = std::fs::read_to_string(root().join(file)).expect("an executable's source");
        assert!(
            !code_of(&source).contains("bravebot_backtrace"),
            "{file} ({why}) uses the crash report crate"
        );
    }
}

/// Every executable the workspace builds is on one list or the other. A new one fails here until
/// whoever adds it says which, and why if it does not report.
#[test]
fn every_executable_the_workspace_builds_says_whether_it_reports() {
    let known: Vec<&str> = REPORTING
        .iter()
        .map(|(file, _)| *file)
        .chain(NOT_REPORTING.iter().map(|(file, _)| *file))
        .collect();
    for file in executables() {
        assert!(
            known.contains(&file.as_str()),
            "{file} is an executable that neither reports crashes nor says why it does not: \
             add it to REPORTING or to NOT_REPORTING in crates/cli/tests/crash_reports.rs"
        );
    }
    for file in known {
        assert!(
            executables().iter().any(|found| found == file),
            "{file} is listed here and is no longer an executable"
        );
    }
}

/// The reader sees a call split across lines, ignores one in a comment, and sees every call of the
/// three functions, so a second start or a start with an address is not missed.
#[test]
fn the_scan_sees_a_split_call_skips_comments_and_finds_every_function() {
    let source = "
        // bravebot_backtrace::start(bravebot_backtrace::Process::Host);
        fn main() {
            bravebot_backtrace::start(
                bravebot_backtrace::Process::Cli,
            );
            bravebot_backtrace::start_with(Process::Cli, Some(url()));
            bravebot_backtrace::begin(Process::Rpc, dir, address);
        }";
    assert_eq!(
        calls_in(source),
        [
            (
                "start".to_string(),
                "bravebot_backtrace::Process::Cli,".to_string()
            ),
            (
                "start_with".to_string(),
                "Process::Cli,Some(url())".to_string()
            ),
            ("begin".to_string(), "Process::Rpc,dir,address".to_string()),
        ]
    );
    assert_eq!(calls_in("fn main() {}"), []);
}
