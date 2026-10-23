//! Whether a path a test builds under the shared temporary directory names the run.
//!
//! Every process on a machine shares one temporary directory. A test that builds
//! `temp_dir().join("bravebot-thing")` and removes it before it starts deletes the directory of
//! another run of the same test, whether that is a second session on the machine or a test binary
//! started again before the first has finished, and each then fails for a reason that is nowhere in
//! the code under test. A name that holds the process id, a counter, a timestamp or a `tempfile`
//! directory is the run's own.
//!
//! The property belongs to the workspace rather than to any one crate, and it lives here because
//! this is the crate the shipped binary is built from.

use std::path::Path;
use std::path::PathBuf;

/// What makes a name the run's own, found anywhere in the statement that builds the path.
const UNIQUE: [&str; 14] = [
    "::id()",
    "pid",
    "nanos",
    "SystemTime",
    "Instant",
    "stamp",
    "counter",
    "COUNTER",
    "Atomic",
    "unique",
    "rand",
    "tempfile",
    "run_id",
    "fetch_add",
];

/// The comment that marks a name that has to be the same each run, with the reason after it.
const FIXED: &str = "shared-temp-name:";

/// The statement a use of `temp_dir()` is in: from the start of its line to the `;` that ends it,
/// or to the bracket that closes what it is inside.
fn statement(source: &str, at: usize) -> &str {
    let start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let mut depth = 0_i32;
    for (offset, c) in source[at..].char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return &source[start..at + offset],
            ')' | ']' | '}' => depth -= 1,
            ';' if depth == 0 => return &source[start..at + offset],
            _ => {}
        }
    }
    &source[start..]
}

/// The 1-based lines of `source` that build a named path under `temp_dir()` with nothing in the
/// name that is the run's own, and no marker saying the name has to be fixed.
fn fixed_names(source: &str) -> Vec<usize> {
    const CALL: &str = "temp_dir()";
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (at, _) in source.match_indices(CALL) {
        let statement = statement(source, at);
        let after = &source[at + CALL.len()..];
        let ends = after.find(';').unwrap_or(after.len());
        if !after[..ends].contains(".join(") && !statement.contains(".join(") {
            continue;
        }
        if UNIQUE.iter().any(|token| statement.contains(token)) {
            continue;
        }
        let line = source[..at].matches('\n').count() + 1;
        // The marker is on the line itself or the line above it.
        let marked = (line.saturating_sub(2)..line)
            .any(|index| lines.get(index).is_some_and(|text| text.contains(FIXED)));
        if !marked {
            found.push(line);
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
fn a_path_a_test_builds_under_the_temporary_directory_names_the_run() {
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
    let mut shared = Vec::new();
    for file in &files {
        // The fixtures of this check's own tests are strings that name `temp_dir()`.
        if file.file_name().is_some_and(|name| name == "temp_names.rs") {
            continue;
        }
        let source = std::fs::read_to_string(file).expect("a Rust source file");
        for line in fixed_names(&source) {
            shared.push(format!(
                "{}:{line}",
                file.strip_prefix(root).unwrap().display()
            ));
        }
    }
    assert!(
        shared.is_empty(),
        "these build a path under temp_dir() with a name every run shares. Put std::process::id() \
         in the name, or mark a name that has to be fixed with `// {FIXED} <why>`:\n{}",
        shared.join("\n")
    );
}

#[test]
fn a_name_is_shared_when_nothing_in_it_belongs_to_the_run() {
    let shared = [
        r#"let p = std::env::temp_dir().join("bravebot-thing");"#,
        r#"let p = std::env::temp_dir().join(format!("bravebot-thing-{name}"));"#,
        "let p = std::env::temp_dir()\n    .join(format!(\"bravebot-{name}\"))\n    .join(\"sub\");",
    ];
    for source in shared {
        assert_eq!(fixed_names(source), [1], "{source}");
    }
}

#[test]
fn a_name_is_the_runs_own_when_it_holds_an_id_a_counter_or_a_timestamp() {
    let own = [
        r#"let p = std::env::temp_dir().join(format!("bravebot-{}", std::process::id()));"#,
        r#"let p = std::env::temp_dir().join(format!("bravebot-{name}-{}", std::process::id()));"#,
        r#"let p = std::env::temp_dir().join(format!("bravebot-{}", NEXT.fetch_add(1, Ordering::Relaxed)));"#,
        r#"let p = std::env::temp_dir().join(format!("bravebot-{stamp}"));"#,
        "let p = std::env::temp_dir().join(format!(\n    \"bravebot-{}\",\n    std::process::id()\n));",
    ];
    for source in own {
        assert_eq!(fixed_names(source), Vec::<usize>::new(), "{source}");
    }
}

#[test]
fn the_directory_alone_and_a_marked_name_are_not_reported() {
    let allowed = [
        "let root = std::env::temp_dir();",
        "let root = std::env::temp_dir().canonicalize().expect(\"the directory\");",
        "// shared-temp-name: a test of the path a person types\nlet p = std::env::temp_dir().join(\"x\");",
        "let p = std::env::temp_dir().join(\"x\"); // shared-temp-name: the well-known path",
    ];
    for source in allowed {
        assert_eq!(fixed_names(source), Vec::<usize>::new(), "{source}");
    }
}
