//! Whether a test that points `HOME` at a scratch directory also clears `BRAVEBOT_HOME`.
//!
//! `BRAVEBOT_HOME` names the state directory and answers before `HOME`. A test that sets `HOME`
//! and leaves the other variable alone reads and writes the directory a developer exported in their
//! shell, or a CI job set, and each then fails for a reason that is nowhere in the code under test
//! while it overwrites that person's own history, settings and sessions.
//!
//! The property belongs to the workspace rather than to any one crate, and it lives here because
//! this is the crate the shipped binary is built from.

use std::path::Path;
use std::path::PathBuf;

const STATE_VARIABLE: &str = "BRAVEBOT_HOME";

/// What beside a `HOME` shows the state variable is not inherited: its name, the constant the
/// crates hold it in, or an environment cleared before anything is set.
const CLEARED_BY: [&str; 3] = [STATE_VARIABLE, "STATE_VARIABLE", "env_clear()"];

/// What marks a line as setting `HOME` for a test: on a child it starts, in this process, or as a
/// pair in a list of variables to set. A restore arm (`Some(value) => ...`) puts a value back and
/// is not one.
const SETS_HOME: [&str; 3] = [r#".env("HOME""#, r#"set_var("HOME""#, r#"("HOME", Some("#];

/// How many lines either side of the one that sets `HOME` may name the other variable.
const REACH: usize = 6;

/// The comment that marks a `HOME` a test sets without clearing the other variable, with the reason
/// after it.
const KEPT: &str = "state-home-kept:";

/// Where the test code of `source` starts, as a line index: the whole file when it is a test
/// target, and the first `#[cfg(test)]` otherwise.
fn test_code_from(path: &Path, source: &str) -> Option<usize> {
    let whole = path
        .components()
        .any(|part| part.as_os_str() == "tests" || part.as_os_str() == "test-support")
        || path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("_tests.rs"));
    if whole {
        return Some(0);
    }
    source
        .lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
}

/// The 1-based lines of `source` from `first` on that set `HOME` with no mention of
/// `BRAVEBOT_HOME` close by and no marker saying the state variable is meant to stay.
fn homes_set_alone(source: &str, first: usize) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate().skip(first) {
        if !SETS_HOME.iter().any(|token| line.contains(token)) || line.contains("=>") {
            continue;
        }
        let near = &lines[index.saturating_sub(REACH)..(index + REACH + 1).min(lines.len())];
        let cleared = near
            .iter()
            .any(|text| text.contains(KEPT) || CLEARED_BY.iter().any(|token| text.contains(token)));
        if !cleared {
            found.push(index + 1);
        }
    }
    found
}

fn rust_files(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a directory under crates/") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                rust_files(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn a_test_that_sets_home_clears_the_state_directory_variable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("this crate is two directories below the workspace root");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    assert!(
        files.len() > 100,
        "the crates were not found: {}",
        files.len()
    );
    let mut alone = Vec::new();
    for file in &files {
        // The fixtures of this check's own tests are strings that set `HOME`.
        if file.file_name().is_some_and(|name| name == "test_home.rs") {
            continue;
        }
        let source = std::fs::read_to_string(file).expect("a Rust source file");
        let Some(first) = test_code_from(file, &source) else {
            continue;
        };
        for line in homes_set_alone(&source, first) {
            alone.push(format!(
                "{}:{line}",
                file.strip_prefix(root).unwrap().display()
            ));
        }
    }
    assert!(
        alone.is_empty(),
        "these set HOME for a test and leave {STATE_VARIABLE} alone, so with it exported the test \
         reads and writes the directory it names. Remove it beside the HOME (`.env_remove`, \
         `std::env::remove_var`, or `(\"{STATE_VARIABLE}\", None)`), or mark one that has to keep \
         it with `// {KEPT} <why>`:\n{}",
        alone.join("\n")
    );
}

#[test]
fn a_home_set_with_no_state_variable_beside_it_is_reported() {
    let alone = [
        "let mut command = Command::new(x);\ncommand.env(\"HOME\", &home);\n",
        "unsafe { std::env::set_var(\"HOME\", &dir) };\n",
        "with_env(&[\n    (\"HOME\", Some(home)),\n    (\"PATH\", None),\n]);\n",
    ];
    for source in alone {
        assert_eq!(homes_set_alone(source, 0).len(), 1, "{source}");
    }
}

#[test]
fn a_home_set_beside_a_cleared_state_variable_a_restore_and_a_marked_one_are_not() {
    let allowed = [
        "command.env(\"HOME\", &home).env_remove(\"BRAVEBOT_HOME\");\n",
        "unsafe { std::env::set_var(\"HOME\", &dir) };\nunsafe { std::env::remove_var(\"BRAVEBOT_HOME\") };\n",
        "(\"HOME\", Some(home)),\n(\"BRAVEBOT_HOME\", None),\n",
        "Command::new(x).env_clear().env(\"HOME\", &home);\n",
        "Some(value) => unsafe { std::env::set_var(\"HOME\", value) },\n",
        "// state-home-kept: a test of the fallback\ncommand.env(\"HOME\", &home);\n",
    ];
    for source in allowed {
        assert_eq!(homes_set_alone(source, 0), Vec::<usize>::new(), "{source}");
    }
}

#[test]
fn only_the_test_code_of_a_source_file_is_read() {
    let source = "fn run() { c.env(\"HOME\", x); }\n#[cfg(test)]\nmod tests {}\n";
    assert_eq!(
        test_code_from(Path::new("crates/a/src/lib.rs"), source),
        Some(1)
    );
    assert_eq!(
        test_code_from(Path::new("crates/a/tests/b.rs"), source),
        Some(0)
    );
    assert_eq!(
        test_code_from(Path::new("crates/a/src/lib.rs"), "fn x() {}"),
        None
    );
}
