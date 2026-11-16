//! SANDBOX-30: the copy of ratatui-image in `third_party/ratatui-image` starts no program.
//!
//! ratatui-image 11.1.0 started `tmux` by bare name, with the whole environment and the `PATH` the
//! process has, and without `bravebot_sandbox::programs::start`. `clippy.toml` cannot refuse a call
//! inside a dependency, so the clause that a dependency is audited for a bare-name start when it is
//! added or raised is held here for this crate: the Rust source it compiles, its `build.rs`
//! included, must not name `Command`, which is the one way the standard library starts a program.
//!
//! A raise of the crate that brings back the start, or adds another, fails here until the new
//! release is read and the difference is removed or recorded in SANDBOX-30. The scan reads source.
//! It does not see a start made through a macro, a name the source computes, or a dependency of the
//! crate, which `make check-deps` and the audit of DEP-001 cover.
//!
//! The property is the workspace's, and it lives here because this is the crate the shipped binary
//! is built from.

mod shipped;

use shipped::{code_of, relative, rust_files, workspace};
use std::path::PathBuf;

/// Where the vendored crate is.
const CRATE: &str = "third_party/ratatui-image";

/// Whether the source starts a program: the standard library's `Command` is the one way it has to,
/// and the type has to be named whatever path or alias brought it in.
fn starts_a_program(source: &str) -> bool {
    code_of(source).without_test_modules.contains("Command")
}

/// The files of the crate that are compiled: everything under `src` and the build script.
fn sources() -> Vec<PathBuf> {
    let root = workspace().join(CRATE);
    let mut found = rust_files(&root.join("src"));
    found.push(root.join("build.rs"));
    found
}

/// SANDBOX-30: no source file of the vendored ratatui-image names `Command`.
///
/// The regression it rejects is the published crate's start, `std::process::Command::new("tmux")`
/// in `detect_tmux_and_outer_protocol_from_env`, coming back with a copy of a newer release, or
/// any other start added the same way.
#[test]
fn the_vendored_ratatui_image_starts_no_program() {
    let files = sources();
    let scanned: Vec<String> = files.iter().map(|file| relative(file)).collect();
    assert!(
        scanned
            .iter()
            .any(|name| name == &format!("{CRATE}/src/picker.rs")),
        "the sources of {CRATE} were not found, or src/picker.rs is not among them: {scanned:?}"
    );
    assert!(
        scanned
            .iter()
            .any(|name| name == &format!("{CRATE}/build.rs")),
        "build.rs of {CRATE} was not scanned: {scanned:?}"
    );
    let mut starting = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a source file of the vendored crate");
        if starts_a_program(&text) {
            starting.push(relative(file));
        }
    }
    assert!(
        starting.is_empty(),
        "SANDBOX-30: these files of {CRATE} name Command, so they can start a program by bare name. \
         Remove the start, as third_party/README.md describes, or record it in the clause:\n{}",
        starting.join("\n")
    );
}

/// The scan sees a start under any name and not a comment or a test module.
///
/// The regression it rejects is a scan that reads only `Command::new(`, which a `use
/// std::process::Command as Spawn;` followed by `Spawn::new("tmux")` would get past, and one that
/// reads a comment explaining the removed start as the start.
#[test]
fn the_scan_of_the_vendored_crate_sees_a_command_under_any_name_and_not_a_comment() {
    for starting in [
        "let c = std::process::Command::new(\"tmux\");",
        "use std::process::Command;",
        "use std::process::Command as Spawn;\nlet c = Spawn::new(\"tmux\");",
        "fn f() -> std::process::Command { todo() }",
    ] {
        assert!(starts_a_program(starting), "{starting}");
    }
    for quiet in [
        "// The published crate ran std::process::Command here.\nfn f() {}",
        "let c = std::env::var_os(\"TERM\");",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    use std::process::Command;\n}\n",
    ] {
        assert!(!starts_a_program(quiet), "{quiet}");
    }
}
