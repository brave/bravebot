//! What `install.sh` registers so that a `bravebot://` link starts the program (LINK-6).
//!
//! The script is shell, so the way to hold it to the rule is to source it with the switch that stops
//! it installing and call the registration itself, in a scratch data directory.
#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A directory no earlier call left an entry in, so a refusal is not read off the one before it.
fn scratch(name: &str) -> PathBuf {
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("bravebot-{name}-{}-{call}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("make the scratch directory");
    path
}

/// The entry the script wrote for a binary at `dest`, or `None` where it wrote nothing.
fn registered(
    dest: &str,
    variables: &[(&str, &Path)],
    switches: &[(&str, &str)],
) -> Option<String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install.sh");
    let data = scratch("deep-link-data");
    let output = Command::new("sh")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("BRAVEBOT_INSTALL_SH_TEST", "1")
        .env("XDG_DATA_HOME", &data)
        .envs(variables.iter().map(|(name, value)| (*name, *value)))
        .envs(switches.iter().copied())
        .arg("-c")
        .arg(r#". "$1"; register_deep_link "$2""#)
        .arg("sh")
        .arg(script)
        .arg(dest)
        .output()
        .expect("run the script");
    assert!(
        output.status.success(),
        "registration failed the install: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entry = std::fs::read_to_string(data.join("applications/bravebot-handler.desktop")).ok();
    let _ = std::fs::remove_dir_all(&data);
    entry
}

/// LINK-6: the entry runs the installed binary in a terminal with the link as its only argument, and
/// claims the one scheme.
#[test]
fn the_entry_runs_the_binary_in_a_terminal_with_the_link() {
    let entry =
        registered("/home/someone/.local/bin/bravebot", &[], &[]).expect("no entry written");
    for line in [
        r#"Exec="/home/someone/.local/bin/bravebot" --handle-uri %u"#,
        "Terminal=true",
        "MimeType=x-scheme-handler/bravebot;",
    ] {
        assert!(entry.lines().any(|found| found == line), "{line}\n{entry}");
    }
}

/// LINK-6: the switch stops registration, and a path a launcher would read as something else is not
/// written into a line it would run.
#[test]
fn nothing_is_registered_where_it_is_switched_off_or_the_path_is_unsafe() {
    assert_eq!(
        registered(
            "/usr/local/bin/bravebot",
            &[],
            &[("BRAVEBOT_NO_DEEP_LINKS", "1")]
        ),
        None,
        "the switch did not stop it"
    );
    for dest in [
        "bravebot",
        "/opt/a\"b/bravebot",
        "/opt/a%ub/bravebot",
        "/opt/$HOME/bravebot",
        "/opt/a`b/bravebot",
        "/opt/a\\b/bravebot",
        "/opt/a\nb/bravebot",
    ] {
        assert_eq!(
            registered(dest, &[], &[]),
            None,
            "{dest:?} was written into an Exec line"
        );
    }
}
