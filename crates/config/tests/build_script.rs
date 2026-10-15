//! What the build script says when it stops for missing configuration.
//!
//! The script is compiled and started on its own rather than through cargo, because what it reports
//! depends on the environment it is handed and a test controls that only by starting the program
//! itself. It stops before it reads `OUT_DIR`, so nothing else of cargo's is needed.

use bravebot_config::env_var;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The script compiled for one test, in a directory of that test's own that is removed after it.
///
/// Named for the process and the test: a runner that starts each test as its own process would
/// otherwise have them all write one output path, and one would run the script while another
/// replaced it.
struct Script {
    directory: PathBuf,
    binary: PathBuf,
}

impl Script {
    fn compile(test: &str) -> Self {
        let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("build-script-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory to compile the script in");
        let binary = directory.join(if cfg!(windows) {
            "build-script.exe"
        } else {
            "build-script"
        });
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let compiled = Command::new(rustc)
            .args(["--edition", "2024", "--crate-type", "bin", "-o"])
            .arg(&binary)
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
            .output()
            .expect("rustc starts");
        assert!(
            compiled.status.success(),
            "build.rs does not compile on its own: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        Self { directory, binary }
    }

    /// What the script says on standard error when it stops, started with exactly `set` in its
    /// environment.
    fn stopped_saying(&self, set: &[(&str, &str)]) -> String {
        let output = Command::new(&self.binary)
            .env_clear()
            .envs(set.iter().copied())
            .output()
            .expect("the build script starts");
        assert!(
            !output.status.success(),
            "the script went on with configuration missing"
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// A build started by `run` has the signing key and its id removed from its environment, so the
/// script reports them missing whatever the person's shell holds. A planner that reads only
/// "not set" tells the person their machine is misconfigured, which is the wrong diagnosis; the
/// message has to name the removal.
#[test]
fn a_build_missing_the_withheld_credentials_says_run_removes_them() {
    let said = Script::compile("withheld").stopped_saying(&[]);

    assert!(
        said.contains(&format!(
            "missing configuration: {}, {}, {}.",
            env_var::SIGNING_KEY,
            env_var::KEY_ID,
            env_var::ENDPOINT
        )),
        "{said}"
    );
    assert!(
        said.contains(&format!(
            "run tool started this build, it removed {}, {} from the environment of every program \
             it starts",
            env_var::SIGNING_KEY,
            env_var::KEY_ID
        )),
        "the message does not say run removed them: {said}"
    );
    assert!(
        said.contains(env_var::ALLOW_UNCONFIGURED_BUILD),
        "the message names no way to build from there: {said}"
    );
}

/// The endpoint is not among the variables `run` removes, so a build missing only that is missing
/// it for a reason `run` did not cause. Blaming `run` there would send a person looking for a
/// removal that did not happen.
#[test]
fn a_build_missing_only_a_variable_run_leaves_alone_does_not_blame_run() {
    let said = Script::compile("endpoint-only").stopped_saying(&[
        (env_var::SIGNING_KEY, "a value"),
        (env_var::KEY_ID, "a value"),
    ]);

    assert!(
        said.contains(&format!("missing configuration: {}.", env_var::ENDPOINT)),
        "{said}"
    );
    assert!(
        !said.contains("run tool"),
        "the message blames run for a variable it does not remove: {said}"
    );
}

/// Of what is missing, only the names `run` removes are said to be removed. Naming every missing
/// variable, or every variable `run` removes, tells a person that one they had set was taken.
#[test]
fn a_build_names_only_the_missing_variables_run_removes() {
    let said = Script::compile("partly-set").stopped_saying(&[(env_var::SIGNING_KEY, "a value")]);

    assert!(
        said.contains(&format!(
            "run tool started this build, it removed {} from the environment",
            env_var::KEY_ID
        )),
        "{said}"
    );
    let removal = &said[said
        .find("run tool started this build")
        .expect("the removal")..];
    let removal = &removal[..removal.find("from the environment").expect("its end")];
    for present in [env_var::SIGNING_KEY, env_var::ENDPOINT] {
        assert!(
            !removal.contains(present),
            "{present} is not both missing and removed by run, yet it is listed as removed: {removal}"
        );
    }
}
