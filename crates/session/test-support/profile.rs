//! Private profiles for tests that use the on-disk session store.

use std::path::PathBuf;

/// Run each test in its own process so profile variables never affect parallel tests.
/// The parent owns cleanup, including when a child test panics.
pub fn in_isolated_profile() -> bool {
    const CHILD_TEST: &str = "BRAVEBOT_SESSION_TEST";
    let thread = std::thread::current();
    let name = thread.name().expect("a named test thread");
    if std::env::var(CHILD_TEST).as_deref() == Ok(name) {
        return true;
    }

    let profiles = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/test-scratch/session-profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    let profile = tempfile::Builder::new()
        .prefix("test-")
        .tempdir_in(profiles)
        .unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", name, "--nocapture"]);
    child.env(CHILD_TEST, name);
    for variable in bravebot_agent::home::PROFILE_VARIABLES {
        child.env(variable, profile.path());
    }
    let output = child.output().expect("run isolated session test");
    profile.close().expect("remove isolated session profile");
    assert!(
        output.status.success(),
        "isolated test {name} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    false
}

/// Projects share the child's profile lifetime, including cleanup after a panic.
pub fn project(name: &str) -> PathBuf {
    bravebot_agent::home::profile()
        .unwrap()
        .join("projects")
        .join(name)
}
