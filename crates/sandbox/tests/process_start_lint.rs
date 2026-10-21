//! SANDBOX-30: `clippy.toml` refuses a program start whatever names the program.
//!
//! A scratch crate holds a start of each shape that reached the tree unresolved: a string literal,
//! a name held in a variable, an alias of `Command`, a helper that passes the program on, and a
//! type alias. The repository's own `clippy.toml` is copied into it, so what is checked is the
//! configuration `make check` and CI run, and `cargo clippy -- -D warnings` is what refuses the
//! start there.
#![forbid(unsafe_code)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A directory removed when the test ends, whether or not it passed.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One start per line carrying the `start:<shape>` marker, each in a function of its own. `{mark}`
/// is where a function's allow goes, and a helper's caller never has one.
const SOURCE: &str = r#"
use std::path::Path;

type Process = std::process::Command;
use std::process::Command as Spawn;

{mark}
pub fn by_literal() {
    let _ = std::process::Command::new("ssh-keygen"); // start:literal
}

{mark}
pub fn by_variable(program: &str) {
    let _ = std::process::Command::new(program); // start:variable
}

{mark}
pub fn by_alias(program: &Path) {
    let _ = Spawn::new(program); // start:alias
}

{mark}
pub fn by_type_alias(program: &Path) {
    let _ = Process::new(program); // start:type-alias
}

{mark}
pub fn started(program: &Path) -> std::process::Command {
    std::process::Command::new(program) // start:helper
}

pub fn through_the_helper(program: &Path) {
    let _ = started(program); // start:caller-of-helper
}
"#;

const MARK: &str =
    r#"#[allow(clippy::disallowed_methods, reason = "a person named the program in a setting")]"#;

/// Every shape that holds a start, in the order the source writes them.
const SHAPES: [&str; 5] = ["literal", "variable", "alias", "type-alias", "helper"];

/// A scratch crate holding `SOURCE` with `mark` above each start, linted with the repository's
/// `clippy.toml`. `None` where `cargo clippy` is not installed, which is a machine this cannot
/// ask: the repository's own clippy run still refuses every start there. A CI machine without it
/// fails instead, since a skip there would report green for a test that never ran.
fn lint(name: &str, mark: &str) -> Option<(Output, Vec<usize>, String)> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let installed = Command::new(&cargo)
        .args(["clippy", "--version"])
        .output()
        .is_ok_and(|output| output.status.success());
    if !installed {
        assert!(
            std::env::var_os("CI").is_none(),
            "cargo clippy is not installed on a CI machine, so the refusal was not tested"
        );
        eprintln!("skipped: cargo clippy is not installed");
        return None;
    }

    let scratch = Scratch(
        std::env::temp_dir().join(format!("bravebot-start-lint-{}-{name}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::create_dir_all(scratch.0.join("src")).unwrap();
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname = \"start-lint-scratch\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
         [lib]\npath = \"src/lib.rs\"\n\n[workspace]\n",
    )
    .unwrap();
    let source = SOURCE.replace("{mark}\n", &format!("{mark}\n"));
    std::fs::write(scratch.0.join("src/lib.rs"), &source).unwrap();
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clippy.toml");
    std::fs::copy(&config, scratch.0.join("clippy.toml"))
        .expect("the repository has a clippy.toml");

    let mut clippy = Command::new(cargo);
    clippy
        .args([
            "clippy",
            "--offline",
            "--message-format",
            "short",
            "--",
            "-D",
            "clippy::disallowed_methods",
        ])
        .current_dir(&scratch.0)
        .env("CARGO_TARGET_DIR", scratch.0.join("target"));
    // What the person's environment adds to a build or moves clippy's configuration is not what
    // the repository's `clippy.toml` says.
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            "CLIPPY_CONF_DIR"
                | "RUSTFLAGS"
                | "CARGO_ENCODED_RUSTFLAGS"
                | "CARGO_BUILD_RUSTFLAGS"
                | "CARGO_BUILD_TARGET"
                | "RUSTC_WRAPPER"
                | "RUSTC_WORKSPACE_WRAPPER"
        ) || (name.starts_with("CARGO_TARGET_") && name.ends_with("_RUSTFLAGS"))
        {
            clippy.env_remove(name.as_ref());
        }
    }
    let output = clippy.output().expect("cargo starts");
    let report = String::from_utf8_lossy(&output.stderr).into_owned();
    let refused = report
        .lines()
        .filter(|line| line.contains("disallowed method"))
        .filter_map(|line| {
            // Cargo writes the path with the separator the platform has.
            line.replace('\\', "/")
                .strip_prefix("src/lib.rs:")?
                .split(':')
                .next()?
                .parse()
                .ok()
        })
        .collect();
    Some((output, refused, source))
}

/// The line a shape's start is written on.
fn line_of(source: &str, shape: &str) -> usize {
    let marker = format!("// start:{shape}");
    source
        .lines()
        .position(|line| line.ends_with(&marker))
        .map(|index| index + 1)
        .unwrap_or_else(|| panic!("the source has no start of shape {shape}"))
}

/// The regression this rejects: a check that reads only `Command::new` followed by a string
/// literal, which passes a program named by a variable, through an alias or through a helper.
#[test]
fn a_start_is_refused_whatever_names_its_program() {
    let Some((output, refused, source)) = lint("unmarked", "") else {
        return;
    };
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "clippy passed starts nobody marked:\n{report}"
    );
    for shape in SHAPES {
        let line = line_of(&source, shape);
        assert!(
            refused.contains(&line),
            "the start by {shape} (line {line}) was not refused; refused lines {refused:?}:\n{report}"
        );
    }
    let caller = line_of(&source, "caller-of-helper");
    assert!(
        !refused.contains(&caller),
        "a call of a helper was refused, so the start would be marked at every caller instead \
         of where the program is chosen:\n{report}"
    );
    assert_eq!(
        refused.len(),
        SHAPES.len(),
        "a start was refused twice or something else was; refused lines {refused:?}:\n{report}"
    );
}

/// The regression this rejects: a refusal that a mark with a reason does not lift, which would make
/// a start the tree needs unwritable, or one lifted for the whole crate rather than the start.
#[test]
fn the_same_starts_with_a_mark_and_a_reason_pass() {
    let Some((output, refused, _)) = lint("marked", MARK) else {
        return;
    };
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && refused.is_empty(),
        "a marked start was refused:\n{report}"
    );
}
