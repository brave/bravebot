// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The stored-login `gh` the sandbox usability suite starts is handed none of the dynamic loader's
//! variables ([SANDBOX-21]).
//!
//! # Why a binary of its own
//!
//! These tests set `DYLD_INSERT_LIBRARIES`, `LD_PRELOAD` and other loader variables in the
//! environment of this process, and the environment belongs to the whole process. A program
//! another test started while they were set would inherit them. Where System Integrity Protection
//! is off, as on the hosted macOS runners, that program aborts trying to load `/etc/hosts`. Kept
//! apart from the rest of the suite, the variables reach no program but the ones these tests
//! start. Each test holds one lock for its whole body, so under `cargo test`, which runs both on
//! threads of one process, neither starts a program while the other has the variables set.
//!
//! [SANDBOX-21]: ../../../docs/specs/sandboxing.md
#![cfg(unix)]

use bravebot_agent::usability::{self, Outcome};
use bravebot_sandbox::rules::Lists;
use std::path::{Path, PathBuf};

/// Taken first by every test here and held for the whole of it, which covers the availability
/// check, the fixture's `codesign` and the probe as well as the variables.
fn environment_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Holds the given variables in the environment of this process while it is alive, and puts back
/// what was there. The caller holds [`environment_lock`] for longer than this lives.
struct Environment {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Environment {
    fn held(set: &[(&'static str, &str)]) -> Self {
        let previous = set
            .iter()
            .map(|(name, _)| (*name, std::env::var_os(name)))
            .collect();
        // SAFETY: the caller holds the lock every test here takes, and `drop` puts them back.
        unsafe {
            for (name, value) in set {
                std::env::set_var(name, value);
            }
        }
        Self { previous }
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: as in `held`.
        for (name, previous) in self.previous.iter().rev() {
            match previous {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

#[path = "../../sandbox/test-support/scratch_root.rs"]
mod scratch_root;

fn root(name: &str) -> PathBuf {
    scratch_root::scratch_dir(&format!("usability-{name}"))
}

/// A scratch home holding a credential file the sandbox refuses and one it allows, and a `gh`
/// whose `auth token` runs `script`. Returns the root, the stub and the home.
fn login_fixture(name: &str, script: &str) -> (PathBuf, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".ssh")).expect("ssh dir");
    std::fs::create_dir_all(home.join(".config/gh")).expect("gh dir");
    std::fs::write(home.join(".ssh/id_ed25519"), "TOKEN-IN-A-REFUSED-FILE").expect("key");
    std::fs::create_dir_all(home.join(".aws")).expect("aws dir");
    std::fs::write(home.join(".aws/credentials"), "TOKEN-IN-A-REFUSED-FILE").expect("aws");
    std::fs::write(home.join(".config/gh/hosts.yml"), "TOKEN-IN-HOSTS").expect("hosts");
    let gh = root.join("gh");
    std::fs::write(&gh, format!("#!/bin/sh\n{script}\n")).expect("stub");
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("mode");
    (root.join("run"), gh, home)
}

/// A `gh` whose `auth token` runs `script` under a shell that keeps the dynamic loader's variables.
/// macOS removes `DYLD_` names when it starts a program under `/bin/sh`, which would make a stub
/// there pass whatever the check does, so a copy of the shell signed for this machine runs it
/// instead. Returns why where that copy cannot be made.
fn loader_fixture(name: &str, script: &str) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let (run, gh, home) = login_fixture(name, script);
    if !cfg!(target_os = "macos") {
        return Ok((run, gh, home));
    }
    let shell = gh.with_file_name("sh");
    std::fs::copy("/bin/sh", &shell).map_err(|e| format!("/bin/sh was not copied: {e}"))?;
    let signed = bravebot_sandbox::programs::start_against(
        &bravebot_sandbox::programs::Resolved::written_in_source("/usr/bin/codesign"),
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
        bravebot_sandbox::programs::CodeSelectors::Kept,
        std::iter::empty(),
        &|_: &Path| true,
    )
    .args(["--force", "--sign", "-"])
    .arg(&shell)
    .output()
    .map_err(|e| format!("codesign did not start: {e}"))?;
    if !signed.status.success() {
        return Err(format!(
            "codesign failed with {}: {}",
            signed.status,
            String::from_utf8_lossy(&signed.stderr).trim()
        ));
    }
    std::fs::write(&gh, format!("#!{}\n{script}\n", shell.display()))
        .map_err(|e| format!("the stub was not written: {e}"))?;
    Ok((run, gh, home))
}

/// A test that cannot run on this machine says so. On a CI machine it fails instead, since a skip
/// there reports green for a check that never ran.
fn skipped(why: &str) {
    assert!(
        std::env::var_os("CI").is_none(),
        "a CI machine must run this check, and cannot: {why}"
    );
    eprintln!("skipped: {why}");
}

/// SANDBOX-21: `gh auth token` is given none of the dynamic loader's variables, in the run outside
/// the sandbox, whose failure gives no row, and in the run under it, whose failure is the row's.
/// The values are files that exist outside every directory a stage writes, which the check every
/// start makes keeps, so only a removal of the `DYLD_` names does it. The regression it rejects:
/// handing the stored-login `gh` `DYLD_INSERT_LIBRARIES`, which loads a library into it.
#[test]
fn the_gh_the_login_row_starts_is_not_handed_a_dyld_variable() {
    let _lock = environment_lock();
    if !usability::available() {
        return;
    }
    let script =
        r#"test -z "$DYLD_INSERT_LIBRARIES" && test -z "$DYLD_FALLBACK_LIBRARY_PATH" || exit 1"#;
    let (root, gh, home) = match loader_fixture("login-dyld", script) {
        Ok(fixture) => fixture,
        Err(why) => {
            skipped(&format!(
                "no shell that keeps DYLD_ names can be made here: {why}"
            ));
            return;
        }
    };
    let _environment = Environment::held(&[
        ("DYLD_INSERT_LIBRARIES", "/etc/hosts"),
        ("DYLD_FALLBACK_LIBRARY_PATH", "/etc"),
    ]);

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("the stub failed, so it was handed a DYLD_ variable");

    assert_eq!(row.outcome, Outcome::Passed);
}

/// SANDBOX-21: as for the `DYLD_` names, with `LD_PRELOAD`, which the shared check would keep for
/// this value, and two `LD_` names it does not list, so a removal of a fixed list does not pass.
/// The regression it rejects: removing `DYLD_` names and leaving the `LD_` ones.
#[test]
fn the_gh_the_login_row_starts_is_not_handed_an_ld_variable() {
    let _lock = environment_lock();
    if !usability::available() {
        return;
    }
    let script = r#"test -z "$LD_PRELOAD" && test -z "$LD_LIBRARY_PATH" && test -z "$LD_DEBUG" && test -z "$LD_BIND_NOW" || exit 1"#;
    let (root, gh, home) = match loader_fixture("login-ld", script) {
        Ok(fixture) => fixture,
        Err(why) => {
            skipped(&format!(
                "no shell that keeps loader names can be made here: {why}"
            ));
            return;
        }
    };
    let _environment = Environment::held(&[
        ("LD_PRELOAD", "/etc/hosts"),
        ("LD_LIBRARY_PATH", "/etc"),
        ("LD_DEBUG", "all"),
        ("LD_BIND_NOW", "1"),
    ]);

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("the stub failed, so it was handed an LD_ variable");

    assert_eq!(row.outcome, Outcome::Passed);
}
