//! A safe session reads no `AGENTS.md` and no skills directory below the project root (INSTR-14).
//!
//! # Why a binary of its own
//!
//! Engaging safe mode is a one-way door for the life of a process, so these cannot share a binary
//! with the tests that assert the ordinary behaviour. The fixtures are the ones
//! `tests/preamble.rs` and `tests/skills.rs` show are read in an ordinary session.

use bravebot_agent::preamble;
use bravebot_agent::skills::{self, Catalogue};
use bravebot_agent::workspace::Workspace;
use bravebot_config::Attribution;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::trust::TrustStore;

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "do the work");
    let mut store = TrustStore::new("/work");
    store.trust(".");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
        sink,
    )
    .expect("policy")
    .with_trust(store)
}

/// A project whose `pkg` holds an `AGENTS.md` and a skill, after the session worked in `pkg`.
fn worked_in_a_package(name: &str) -> Workspace {
    bravebot_core::safe::engage();
    let project = std::env::temp_dir().join(format!("bravebot-agent-safe-{name}"));
    let _ = std::fs::remove_dir_all(&project);
    let skill = project.join("pkg/.bravebot/skills/only");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(project.join("pkg/AGENTS.md"), "PKG-RULES").unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: only\ndescription: a package skill\n---\n\nPKG-BODY",
    )
    .unwrap();
    let workspace = Workspace::new(&project).expect("workspace");
    workspace.record_touch("pkg/src/lib.rs");
    workspace
}

/// A safe session offers no skill from a directory it worked in.
#[test]
fn a_safe_session_offers_no_nested_skill() {
    let workspace = worked_in_a_package("skill");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);

    let (catalogue, _) = skills::discover(&mut policy, &workspace, None);

    assert!(
        catalogue
            .iter()
            .all(|skill| skill.origin == "built-in" && skill.name != "only"),
        "a safe session offered a nested skill"
    );
}

/// A safe session reads no `AGENTS.md` from a directory it worked in. What the command line named
/// still reaches the prompt, which shows the prompt was composed and only the file was left out.
#[test]
fn a_safe_session_reads_no_nested_agents_file() {
    let workspace = worked_in_a_package("agents");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);

    let composed = preamble::compose_in(
        &mut policy,
        &workspace,
        &workspace,
        None,
        &Catalogue::default(),
        None,
        None,
        &Attribution::default(),
        Some("COMMAND-LINE-WORDS"),
    );

    assert!(
        composed.text.contains("COMMAND-LINE-WORDS"),
        "{}",
        composed.text
    );
    assert!(
        !composed.text.contains("PKG-RULES"),
        "a safe session read a nested AGENTS.md: {}",
        composed.text
    );
}
