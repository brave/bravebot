//! What the planner is told `run` removes from a program's environment (RUN-12).
//!
//! # Why a binary of its own
//!
//! The project settings layer is the one in the process's working directory, the person's own layer
//! is under `HOME`, and the filtering has a switch in the environment. All three are process-wide,
//! so a test that sets them cannot share a binary with tests that read any of them. An integration
//! test is its own process and this file holds one test, so nothing else reads the environment or
//! the directory while it changes them.

use bravebot_agent::exec::Deadlines;
use bravebot_agent::tools::{self, Running, Scheduling};
use bravebot_agent::watch::Arming;
use std::path::PathBuf;

/// A scratch directory that removes itself, named for this process so concurrent runs of the suite
/// on one machine do not share it.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "bravebot-agent-withheld-variables-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn run_description() -> String {
    let mut table = tools::for_planner(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        &bravebot_core::delegate::Definitions::default(),
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    tools::state_withheld_variables(&mut table);
    table
        .iter()
        .find(|tool| tool.function.name == "run")
        .map(|tool| tool.function.description.clone())
        .expect("run is offered")
}

/// The planner is told the two credentials fixed in the source, and no string a settings file
/// wrote. The files do reach the filtering, which is what the controls below establish: a
/// provider's `env` entry and a checkout's `run.scrubEnv` entry are both withheld from a program, so
/// their absence from the planner's text is a decision and not an accident of a file going unread.
///
/// The provider entry is shaped like a key a person pasted into the wrong field. Listing the
/// entries of `env` would send it to the model provider with every request, and a checkout's own
/// `run.scrubEnv` entry would put a string its author chose in the planner's context.
///
/// With the filtering switched off nothing is removed, so nothing is said.
#[test]
fn the_planner_is_told_the_fixed_credentials_and_no_string_a_settings_file_wrote() {
    let scratch = Scratch::new();
    let home = scratch.path.join("home");
    let checkout = scratch.path.join("checkout");
    std::fs::create_dir_all(home.join(".bravebot")).expect("create the home");
    std::fs::create_dir_all(checkout.join(".bravebot")).expect("create the checkout");
    std::fs::write(
        home.join(".bravebot").join("settings.json"),
        r#"{"provider": {"acme": {
            "options": {"baseURL": "https://gateway.acme.invalid/v1"},
            "env": ["ACME_GATEWAY_TOKEN", "sk-live-0123456789abcdef"]
        }}}"#,
    )
    .expect("write the person's settings");
    std::fs::write(
        checkout.join(".bravebot").join("settings.json"),
        r#"{"run": {"scrubEnv": ["CHECKOUT_CHOSEN_NAME"]}}"#,
    )
    .expect("write the checkout's settings");

    // SAFETY: this binary holds this one test, so nothing else reads or writes the environment or
    // the working directory while they are changed.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::remove_var("BRAVEBOT_SUBPROCESS_ENV_SCRUB");
    }
    std::env::set_current_dir(&checkout).expect("enter the checkout");

    let removed_from_a_program = bravebot_config::scrub::withheld();
    for written in [
        "ACME_GATEWAY_TOKEN",
        "sk-live-0123456789abcdef",
        "CHECKOUT_CHOSEN_NAME",
    ] {
        assert!(
            removed_from_a_program.iter().any(|name| name == written),
            "{written} was not read from a settings file, so this test cannot tell a decision from \
             an accident"
        );
    }

    let described = run_description();
    for told in ["`SERVICES_KEY_AICHAT`", "`BRAVE_SERVICES_KEY_ID`"] {
        assert!(described.contains(told), "{told} is not named: {described}");
    }
    for written in [
        "ACME_GATEWAY_TOKEN",
        "sk-live-0123456789abcdef",
        "CHECKOUT_CHOSEN_NAME",
    ] {
        assert!(
            !described.contains(written),
            "a string a settings file wrote reached the planner: {written} in {described}"
        );
    }

    // SAFETY: as above.
    unsafe { std::env::set_var("BRAVEBOT_SUBPROCESS_ENV_SCRUB", "0") };
    let unfiltered = run_description();
    unsafe { std::env::remove_var("BRAVEBOT_SUBPROCESS_ENV_SCRUB") };
    assert!(
        !unfiltered.contains("SERVICES_KEY_AICHAT")
            && !unfiltered.contains("says nothing about whether the person has set it"),
        "the planner was told of a removal that the switch turned off: {unfiltered}"
    );
}
