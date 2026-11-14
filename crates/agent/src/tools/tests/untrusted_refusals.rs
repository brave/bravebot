use super::arguments::Scratch;
use super::*;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Label;
use bravebot_core::policy::{ReleasePlan, Routing};

const NOTEBOOK: &str = "refused: the outputs of this notebook are closed to read_file, \
                                label (U,pub): it is not vouched for, or untrusted content was \
                                written to it. ";
const GIT: &str = "refused: this answer is closed to read_git, label (U,pub): the \
                           repository is not vouched for, or untrusted content was written to it. ";

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "read a file");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .expect("policy")
}

/// The regression it rejects: naming `spawn_processor` or `run` whatever the turn holds,
/// which sends a delegate or a restricted turn to a tool it was never offered, or leaving
/// one out of a turn that has it.
#[test]
fn the_notebook_refusal_names_only_the_routes_the_turn_holds() {
    let cases: [(&str, bool, bool, &str); 4] = [
        (
            "everything offered",
            true,
            true,
            "Read the file with read_file and no output_of for a reference to give to \
                     spawn_processor, read it through run, or ask the person to vouch for the path",
        ),
        (
            "no run",
            true,
            false,
            "Read the file with read_file and no output_of for a reference to give to \
                     spawn_processor, or ask the person to vouch for the path",
        ),
        (
            "no spawn_processor",
            false,
            true,
            "Read it through run, or ask the person to vouch for the path",
        ),
        (
            "neither",
            false,
            false,
            "Only the person can open it, by vouching for the path",
        ),
    ];
    for (what, spawning, running, routes) in cases {
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        if !spawning {
            policy.without_spawning();
        }
        if !running {
            policy.without_running();
        }
        assert_eq!(
            closed_to_reading_outputs(&policy, Label::untrusted_public()),
            format!("{NOTEBOOK}{routes}"),
            "{what}"
        );
    }
}

/// The regression it rejects: naming `run` to a turn whose list does not hold it.
#[test]
fn the_git_refusal_names_run_only_where_the_turn_holds_it() {
    assert_eq!(
        closed_to_read_git(Running::Offered, Label::untrusted_public()),
        format!("{GIT}Ask git through run, or ask the person to vouch for the repository")
    );
    assert_eq!(
        closed_to_read_git(Running::Withheld, Label::untrusted_public()),
        format!("{GIT}Only the person can open it, by vouching for the repository")
    );
}

/// The wiring of `read_git`'s refusal. `read_git` asks only for an answer labelled trusted,
/// so a turn never gives `examine_git_answer` an untrusted one; the function that holds
/// the call to `read_trusted_content` is called with one here.
///
/// The regression it rejects: printing the denial, which ends in "Use a reference to it
/// instead" although this tool returns no reference, or carrying any byte of the answer.
#[test]
fn an_untrusted_git_answer_is_refused_in_the_helpers_words() {
    for (running, routes) in [
        (
            Running::Offered,
            "Ask git through run, or ask the person to vouch for the repository",
        ),
        (
            Running::Withheld,
            "Only the person can open it, by vouching for the repository",
        ),
    ] {
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        let answer = Labelled::new(
            crate::git::Answer {
                text: "KEEP-OUT-OF-THE-REFUSAL".to_string(),
                ..crate::git::Answer::default()
            },
            Label::untrusted_public(),
        );
        let refused = examine_git_answer(&mut policy, &answer, running)
            .expect_err("an untrusted answer was examined");
        assert_eq!(refused, format!("{GIT}{routes}"));
        assert!(
            !refused.contains("Use a reference to it instead"),
            "{refused}"
        );
        assert!(!refused.contains("KEEP-OUT-OF-THE-REFUSAL"), "{refused}");
    }

    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let trusted = Labelled::new(
        crate::git::Answer {
            text: "a trusted answer".to_string(),
            ..crate::git::Answer::default()
        },
        Label::trusted_public(),
    );
    let read = examine_git_answer(&mut policy, &trusted, Running::Offered)
        .expect("a trusted answer is examined");
    assert_eq!(read.text, "a trusted answer");
}

/// The wiring of `notebook_outputs`: a notebook the map does not vouch for gets the
/// helper's wording and not the shared sentence of the kernel, and nothing of the file.
///
/// The regression it rejects: printing the denial, which ends in "Use a reference to it
/// instead" for a turn that may hold no way to use one.
#[test]
fn a_notebook_nobody_vouched_for_is_refused_in_the_helpers_words() {
    let scratch = Scratch::new("notebook-outputs-refusal");
    std::fs::write(scratch.path.join("n.ipynb"), "KEEP-OUT-OF-THE-REFUSAL").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    for (spawning, running, routes) in [
        (
            true,
            true,
            "Read the file with read_file and no output_of for a reference to give to \
                     spawn_processor, read it through run, or ask the person to vouch for the path",
        ),
        (
            false,
            false,
            "Only the person can open it, by vouching for the path",
        ),
    ] {
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        if !spawning {
            policy.without_spawning();
        }
        if !running {
            policy.without_running();
        }
        let produced = notebook_outputs(
            &mut policy,
            &workspace,
            &Labelled::trusted("n.ipynb".to_string()),
            ("path", "n.ipynb"),
            1,
        );
        let told = arguments::told(&mut policy, &produced.text);
        // A file read is private, so the label the driver assigned reads (U,priv).
        assert_eq!(
            told,
            format!("{}{routes}", NOTEBOOK.replace("(U,pub)", "(U,priv)"))
        );
        assert!(!told.contains("Use a reference to it instead"), "{told}");
        assert!(!told.contains("KEEP-OUT-OF-THE-REFUSAL"), "{told}");
    }
}
