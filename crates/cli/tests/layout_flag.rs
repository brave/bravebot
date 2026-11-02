//! BOT-17.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self {
            path: path.canonicalize().expect("canonical scratch"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn bravebot(home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bravebot"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_name_no_layout_answers_to_is_refused_with_the_names_that_exist() {
    let home = Scratch::new("layout-flag-unknown");

    let output = bravebot(&home.path, &["--layout", "nowhere"]);

    let stderr = stderr_of(&output);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("nowhere") && stderr.contains("default, bots"),
        "{stderr}"
    );
}

#[test]
fn a_line_that_is_not_a_bare_start_is_refused_naming_both_flags() {
    let home = Scratch::new("layout-flag-refused");
    for (arguments, other) in [
        (&["--layout", "bots", "-p", "hi"][..], "-p"),
        (&["--layout", "bots", "--model", "m"][..], "--model"),
        (&["--layout", "bots", "--resume"][..], "--resume"),
        (&["--layout", "bots", "-c"][..], "-c"),
        (&["--layout", "bots", "--fork", "x"][..], "--fork"),
        (&["--layout", "bots", "-i", "task"][..], "-i"),
        (&["--layout", "bots", "--plain"][..], "--plain"),
        (&["--layout", "bots", "--bg", "task"][..], "--bg"),
        (&["--layout", "bots", "doctor"][..], "doctor"),
        (&["--layout", "bots", "do the thing"][..], "a task"),
        (&["--layout", "bots", "--agent", "reviewer"][..], "--agent"),
        (&["--layout", "bots", "--incognito"][..], "--incognito"),
        (
            &["--layout", "bots", "--system-prompt", "words"][..],
            "--system-prompt",
        ),
        (&["--layout", "default", "--resume"][..], "--resume"),
    ] {
        let output = bravebot(&home.path, arguments);

        let stderr = stderr_of(&output);
        assert!(!output.status.success(), "{arguments:?}: {stderr}");
        assert!(
            stderr.contains("--layout") && stderr.contains(other),
            "{arguments:?}: {stderr}"
        );
    }
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_order_of_the_arguments_does_not_change_the_refusal() {
    let home = Scratch::new("layout-flag-order");
    for (arguments, other) in [
        (&["--model", "m", "--layout", "bots"][..], "--model"),
        (&["-p", "hi", "--layout", "bots"][..], "-p"),
        (&["--incognito", "--layout", "bots"][..], "--incognito"),
        (&["--layout", "bots", "--model", "m"][..], "--model"),
        (&["--resume", "--layout", "bots"][..], "--resume"),
    ] {
        let output = bravebot(&home.path, arguments);

        let stderr = stderr_of(&output);
        assert!(!output.status.success(), "{arguments:?}: {stderr}");
        assert!(
            stderr.contains("--layout") && stderr.contains(other),
            "{arguments:?}: {stderr}"
        );
    }
}

#[test]
fn the_version_and_the_help_win_over_the_layout_wherever_it_is_given() {
    let home = Scratch::new("layout-flag-version");
    for arguments in [
        &["--layout", "bots", "--version"][..],
        &["--version", "--layout", "bots"][..],
    ] {
        let output = bravebot(&home.path, arguments);

        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            stderr_of(&output)
        );
        assert!(
            stdout_of(&output).starts_with("bravebot "),
            "{arguments:?}: {}",
            stdout_of(&output)
        );
    }
    for arguments in [
        &["--layout", "bots", "--help"][..],
        &["--help", "--layout", "bots"][..],
    ] {
        let output = bravebot(&home.path, arguments);

        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            stderr_of(&output)
        );
        assert!(
            stdout_of(&output).contains("--layout"),
            "{arguments:?}: {}",
            stdout_of(&output)
        );
    }
}

#[test]
fn the_usage_text_lists_every_layout_name() {
    let home = Scratch::new("layout-flag-usage");

    let output = bravebot(&home.path, &["--help"]);

    let stdout = stdout_of(&output);
    let line = stdout
        .lines()
        .find(|line| line.contains("--layout <name>"))
        .unwrap_or_else(|| panic!("no line for --layout:\n{stdout}"));
    assert!(
        line.contains(&bravebot_tui::layout::Layout::names()),
        "{line}"
    );
}
