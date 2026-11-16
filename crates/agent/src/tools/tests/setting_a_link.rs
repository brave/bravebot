use super::arguments::{Scratch, told, with_tools_typed};
use super::*;
use crate::links::LinkKind;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Integrity;
use bravebot_core::policy::{ReleasePlan, Routing};

const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "record a link");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .expect("policy")
}

/// What the call answered, whether it failed and the link it asked to have recorded.
fn call(
    typed: &[&str],
    delegated: bool,
    untrusted: bool,
    arguments: Value,
) -> (String, bool, Option<crate::links::LinkSet>) {
    call_in(typed, delegated, untrusted, false, arguments)
}

/// As [`call`], in a context that may also already hold private content.
fn call_in(
    typed: &[&str],
    delegated: bool,
    untrusted: bool,
    private: bool,
    arguments: Value,
) -> (String, bool, Option<crate::links::LinkSet>) {
    let scratch = Scratch::new("set-link");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let typed: Vec<String> = typed.iter().map(|line| line.to_string()).collect();
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    if untrusted {
        policy = policy.resuming(Integrity::Untrusted);
    }
    if private {
        policy = policy.holding(bravebot_core::label::Confidentiality::Private);
    }
    let produced = with_tools_typed(&workspace, &typed, |tools| {
        tools.delegated = delegated;
        set_link(&mut policy, tools, &arguments)
    });
    (
        told(&mut policy, &produced.text),
        produced.failed,
        produced.link,
    )
}

fn asking(kind: &str, url: &str) -> Value {
    json!({"kind": kind, "url": url, "why": "the user named it"})
}

/// The regression it rejects: a tool that records nothing, or the wrong kind.
#[test]
fn an_address_the_person_typed_is_recorded_under_the_kind_asked_for() {
    let line = format!("fix this {ISSUE}, see also https://github.com/brave/bravebot/issues/1000");
    let (said, failed, link) = call(&[&line], false, false, asking("issue", ISSUE));
    assert!(!failed, "{said}");
    let link = link.expect("a link");
    assert_eq!(link.kind, LinkKind::Issue);
    assert_eq!(link.url, ISSUE);

    let pull = "https://github.com/brave/bravebot/pull/4309";
    let (said, failed, link) = call(&[pull], false, false, asking("pull_request", pull));
    assert!(!failed, "{said}");
    assert_eq!(link.expect("a link").kind, LinkKind::PullRequest);
}

/// The regression it rejects: no comparison with the person's lines, so the planner can
/// record an address that came from a file or a tool result.
#[test]
fn an_address_the_person_did_not_type_is_refused_and_nothing_is_recorded() {
    let (said, failed, link) = call(
        &["fix the build"],
        false,
        false,
        asking("issue", "https://evil.example/x"),
    );
    assert!(failed, "{said}");
    assert!(link.is_none(), "an address nobody typed was recorded");
    assert!(said.contains("not in a line the user typed"), "{said}");

    let (_, failed, link) = call(&[], false, false, asking("issue", ISSUE));
    assert!(
        failed && link.is_none(),
        "a session with no typed line recorded one"
    );
}

/// The regression it rejects: a substring test, which lets the start of a longer address
/// the person wrote stand for an address they did not.
#[test]
fn the_start_of_a_longer_typed_address_is_refused() {
    let (said, failed, link) = call(
        &["see https://github.com/brave/bravebot/issues/12670"],
        false,
        false,
        asking("issue", ISSUE),
    );
    assert!(failed && link.is_none(), "{said}");
}

/// The regression it rejects: skipping the PANEL-12 check, so a typed word that is not a
/// web address is recorded.
#[test]
fn a_typed_word_that_is_not_a_web_address_is_refused() {
    for bad in [
        "ftp://a.example/x",
        "javascript:alert(1)",
        "https://",
        "two words",
    ] {
        let line = format!("use {bad} please");
        let (said, failed, link) = call(&[&line], false, false, asking("issue", bad));
        assert!(failed && link.is_none(), "{bad}: {said}");
        assert!(!said.contains(bad), "the refusal repeated {bad}: {said}");
    }
}

/// The regression it rejects: accepting any `kind`, or none.
#[test]
fn a_kind_other_than_issue_or_pull_request_is_refused() {
    for arguments in [
        asking("ticket", ISSUE),
        json!({"url": ISSUE, "why": "the user named it"}),
    ] {
        let (said, failed, link) = call(&[ISSUE], false, false, arguments);
        assert!(failed && link.is_none(), "{said}");
        assert!(said.contains("'kind'"), "{said}");
    }
}

/// The regression it rejects: a missing `delegated` guard, which would let a delegate set
/// the parent session's link.
#[test]
fn a_delegate_cannot_set_a_link() {
    let (said, failed, link) = call(&[ISSUE], true, false, asking("issue", ISSUE));
    assert!(failed && link.is_none(), "{said}");
    assert!(said.starts_with("error: no such tool"), "{said}");
}

/// The regression it rejects: reading `url` without the planner-argument gate, so a
/// context that has met untrusted content still chooses what the session records.
#[test]
fn a_context_that_has_met_something_untrusted_cannot_set_a_link() {
    let (said, failed, link) = call(&[ISSUE], false, true, asking("issue", ISSUE));
    assert!(failed && link.is_none(), "{said}");
    assert!(said.starts_with("refused:"), "{said}");
}

/// SETLINK-1: the gate asks about integrity only (`read_planner_argument`), and the recorded
/// value is one of the person's own words, so what the context holds does not change the
/// outcome: a typed address is recorded and an untyped one is still refused.
///
/// The regression it rejects: refusing on the confidentiality axis, which would stop the
/// tool after any vetted read, or skipping the typed-line check in a private context.
#[test]
fn a_context_holding_private_content_still_records_a_typed_address_and_nothing_else() {
    let (said, failed, link) = call_in(&[ISSUE], false, false, true, asking("issue", ISSUE));
    assert!(!failed, "{said}");
    assert_eq!(link.expect("a link").url, ISSUE);

    let other = "https://github.com/brave/bravebot/issues/9";
    let (said, failed, link) = call_in(&[ISSUE], false, false, true, asking("issue", other));
    assert!(failed && link.is_none(), "{said}");
}

/// An address on a host other than GitHub is recorded when the person typed it.
///
/// The regression it rejects: a check that tests the host for GitHub.
#[test]
fn an_address_on_another_tracker_is_recorded() {
    let other = "https://gitlab.example.org/team/project/-/issues/7";
    let line = format!("work on {other}");
    let (said, failed, link) = call(&[&line], false, false, asking("issue", other));
    assert!(!failed, "{said}");
    assert_eq!(link.expect("a link").url, other);
}

/// A line with several addresses records only the one the planner names, and the planner
/// cannot name one that is in no line.
///
/// The regression it rejects: taking the first address of a line, or all of them.
#[test]
fn only_the_address_the_planner_names_is_recorded_from_a_line_with_several() {
    let other = "https://github.com/brave/bravebot/issues/1000";
    let line = format!("fix {ISSUE} and see {other}");
    let (said, failed, link) = call(&[&line], false, false, asking("issue", other));
    assert!(!failed, "{said}");
    assert_eq!(link.expect("a link").url, other);

    let (said, failed, link) = call(
        &[&line],
        false,
        false,
        asking("issue", "https://github.com/brave/bravebot/issues/1001"),
    );
    assert!(failed && link.is_none(), "{said}");
}
