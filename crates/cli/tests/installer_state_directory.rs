//! Where `install.sh` keeps the record of what it installed, which is the state directory the
//! program itself reads (STATE-2).
//!
//! The script is shell, so nothing in this workspace calls it and running it is the only way a Rust
//! test can hold it to the rule. It is sourced with the switch that stops it installing and asked
//! what it resolved the state directory to, since a full install needs a release to serve and the
//! resolution is made before anything is fetched.
#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::Path;
use std::process::Command;

/// What the script resolved as the state directory and the record inside it, each empty where it
/// resolved none, for an environment holding only the variables given.
fn resolved(variables: &[(&str, &str)]) -> (String, String) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install.sh");
    let output = Command::new("sh")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("BRAVEBOT_INSTALL_SH_TEST", "1")
        .envs(variables.iter().copied())
        .arg("-c")
        .arg(r#". "$1"; printf '%s\n%s\n' "$STATE_DIR" "$INSTALLED_BY""#)
        .arg("sh")
        .arg(script)
        .output()
        .expect("run the script");
    assert!(
        output.status.success(),
        "the script did not source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut lines = said.lines();
    (
        lines.next().unwrap_or_default().to_owned(),
        lines.next().unwrap_or_default().to_owned(),
    )
}

/// STATE-2: the directory a person names in `BRAVEBOT_HOME` is the one the record goes in, as it is
/// the one the program reads `installed-by` from. A record under `~/.bravebot` while the program
/// looks in the named directory leaves a script install with no update command.
#[test]
fn the_record_goes_in_the_directory_a_person_named() {
    assert_eq!(
        resolved(&[
            ("BRAVEBOT_HOME", "/srv/ci/state"),
            ("HOME", "/home/someone")
        ]),
        (
            "/srv/ci/state".to_owned(),
            "/srv/ci/state/installed-by".to_owned()
        ),
        "the named directory did not answer, or had `.bravebot` joined onto it"
    );
}

/// STATE-2: the variable is the only thing that moves the directory, so without it the profile
/// directory answers as before.
#[test]
fn the_record_goes_under_home_where_no_directory_is_named() {
    assert_eq!(
        resolved(&[("HOME", "/home/someone")]),
        (
            "/home/someone/.bravebot".to_owned(),
            "/home/someone/.bravebot/installed-by".to_owned()
        )
    );
}

/// STATE-2: a value that is empty or not an absolute path names nothing and is passed over, so the
/// profile directory answers. A relative value taken as a directory would put the record under
/// wherever the person ran the script, which nobody stated.
#[test]
fn a_value_that_names_no_directory_is_passed_over() {
    for value in ["", "state", "./state", "../state", "~/state", "$HOME/state"] {
        assert_eq!(
            resolved(&[("BRAVEBOT_HOME", value), ("HOME", "/home/someone")]),
            (
                "/home/someone/.bravebot".to_owned(),
                "/home/someone/.bravebot/installed-by".to_owned()
            ),
            "{value:?} was taken as a directory"
        );
    }
}

/// STATE-2: passing a value over leaves a machine that names nothing else with no state directory,
/// and then no record is written, as for an empty `HOME`.
#[test]
fn a_value_that_names_no_directory_leaves_no_state_directory_to_guess() {
    for value in ["", "state", "./state", "~/state"] {
        assert_eq!(
            resolved(&[("BRAVEBOT_HOME", value), ("HOME", "")]),
            (String::new(), String::new()),
            "{value:?} produced a location nothing stated"
        );
    }
}
