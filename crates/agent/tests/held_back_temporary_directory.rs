//! SANDBOX-22: the person's own settings file holding the temporary directory back reaches the
//! programs `run` starts and the description the planner is given, through the settled setting.
//!
//! # Why a binary of its own
//!
//! The setting is read once for the process ([`bravebot_config::settle_sandbox_filesystem_in`]). A
//! test in a binary shared with others would hold the directory back for all of them. An
//! integration test is its own process.
#![cfg(unix)]

use bravebot_agent::RunDecision;
use bravebot_sandbox::SandboxMode;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{Scratch, a_turn_with_result_of, trust_of};

/// The file is made by the line that `run` starts, with the setting read from a settings file and
/// settled the way the entry point settles it. The regressions it rejects: the settled value never
/// reaching the confinement a `run` call builds, which leaves the file written, and one that
/// reaches it too widely, which refuses the session's own directory. The planner's description is
/// read from the same settled value. A line writing the session directory is the control that a
/// refusal is the setting's.
#[test]
fn the_settled_setting_holds_the_temporary_directory_back_from_run_and_is_told_to_the_planner() {
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let scratch = Scratch::new("agent-held-back-temporary");
    let home = scratch.path.join("home");
    let file = bravebot_config::user_settings_file(&home);
    std::fs::create_dir_all(file.parent().expect("a directory")).expect("home");
    std::fs::write(
        &file,
        r#"{"sandbox": {"holdBackTemporaryDirectory": true}}"#,
    )
    .expect("file");
    let settings = bravebot_config::Settings::layered(Some(home), None, None);
    assert!(settings.sandbox_holds_back_temporary_directory());
    bravebot_config::settle_sandbox_filesystem_in(
        &bravebot_sandbox::rules::Lists::default(),
        &settings,
    );
    assert!(bravebot_config::holds_back_temporary_directory());

    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    let planted = std::env::temp_dir().join(format!("bravebot-settled-{}", std::process::id()));
    let _ = std::fs::remove_file(&planted);

    let (_, inside) = a_turn_with_result_of(
        &work,
        "touch made.txt",
        RunDecision::approve(),
        |workspace| trust_of(workspace, false),
    );
    let (_, outside) = a_turn_with_result_of(
        &work,
        &format!("touch '{}'", planted.display()),
        RunDecision::approve(),
        |workspace| trust_of(workspace, false),
    );
    let wrote = planted.exists();
    let _ = std::fs::remove_file(&planted);

    assert!(work.join("made.txt").exists(), "the control: {inside}");
    assert!(
        !wrote,
        "a line wrote the temporary directory the person held back: {outside}"
    );

    let said = bravebot_agent::confine::stated_to_the_planner(true, SandboxMode::WorkspaceWrite)
        .expect("a statement");
    assert!(
        said.contains("holds the temporary directory back"),
        "{said}"
    );
}
