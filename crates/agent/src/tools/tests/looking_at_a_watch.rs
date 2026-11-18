use super::arguments::Scratch;
use super::*;
use crate::watch::Looked;
use crate::{LiveMode, PermissionMode};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::file_authority::FileAuthority;
use bravebot_core::policy::{ReleasePlan, Routing};

/// A project, a directory beside it holding a file, and the workspace over the project.
fn places(name: &str) -> (Scratch, Workspace, String) {
    let scratch = Scratch::new(&format!("watch-look-{name}"));
    let project = scratch.path.join("project");
    let beside = scratch.path.join("beside");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&beside).unwrap();
    let outside = beside.canonicalize().unwrap().join("notes.txt");
    std::fs::write(&outside, "one").unwrap();
    let workspace = Workspace::new(project.canonicalize().unwrap()).unwrap();
    (scratch, workspace, outside.display().to_string())
}

/// A watch beside the project is looked at under bypass where the working directory is
/// trusted, and is out of reach under any other mode and where it is not trusted. The
/// regressions it rejects: a look that takes the reach from the mode alone, which reaches
/// beside an untrusted project, and a look that never gives the reach, which ends the
/// watch the moment bypass is on.
#[test]
fn a_watch_beside_the_project_is_reached_under_bypass_in_a_trusted_project_only() {
    let (_scratch, workspace, outside) = places("reach");
    let trusting = crate::trusted::trusting_the_workspace(workspace.root());
    let not_trusting = crate::workspace::trust_store(workspace.root());
    let bypass = LiveMode::new(PermissionMode::Bypass);
    let ask = LiveMode::new(PermissionMode::Ask);
    let look = |mode: &LiveMode, trust| {
        look_at_a_watch(&workspace, mode, trust, &outside, workspace.root())
    };

    assert!(matches!(look(&bypass, &trusting), Looked::Saw(_)));
    assert!(matches!(look(&ask, &trusting), Looked::OutOfReach));
    assert!(matches!(look(&bypass, &not_trusting), Looked::OutOfReach));
}

/// A path inside the project is seen under every mode and with no trust, since a look of
/// it asks nothing of the map. The regression it rejects: a look that is out of reach
/// for everything unless bypass gives it a reach.
#[test]
fn a_watch_inside_the_project_is_seen_whatever_the_mode_and_the_map() {
    let (_scratch, workspace, _) = places("inside");
    std::fs::write(workspace.root().join("inside.txt"), "one").unwrap();
    let not_trusting = crate::workspace::trust_store(workspace.root());
    for mode in [PermissionMode::Ask, PermissionMode::Bypass] {
        let looked = look_at_a_watch(
            &workspace,
            &LiveMode::new(mode),
            &not_trusting,
            "inside.txt",
            workspace.root(),
        );
        assert!(matches!(looked, Looked::Saw(_)), "{mode:?}");
    }
}

/// The trust a watch is judged by is the answer a call is judged by (TRUST-7), for a map
/// that trusts the working directory and one that does not. The regression it rejects: a
/// look that asks the map under another key than the one `workspace_is_trusted` asks, so
/// a call and a watch in one project disagree about whether it is trusted.
#[test]
fn a_watch_asks_the_trust_question_a_call_asks() {
    let (_scratch, workspace, outside) = places("agree");
    let bypass = LiveMode::new(PermissionMode::Bypass);
    for (trust, trusted) in [
        (
            crate::trusted::trusting_the_workspace(workspace.root()),
            true,
        ),
        (crate::workspace::trust_store(workspace.root()), false),
    ] {
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "ask whether the project is trusted");
        let policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy")
        .with_file_authority(FileAuthority::new(trust.clone()));
        let call_trusts = workspace_is_trusted(&policy, &workspace);
        assert_eq!(call_trusts, trusted);
        let looked = look_at_a_watch(&workspace, &bypass, &trust, &outside, workspace.root());
        assert_eq!(matches!(looked, Looked::Saw(_)), call_trusts);
    }
}
