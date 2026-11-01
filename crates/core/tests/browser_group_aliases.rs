//! The browser relay's groups declared as servers of their own (BROWSER-19), held to what the
//! driver does with an alias.
//!
//! `bravebot-browser install --groups` makes one alias for each group, `brave-tabs-read`,
//! `brave-remove-data` and so on, and `brave` for the one that offers every tool. The grant
//! (SERVERS-9), the `Mcp(alias)` rules (SERVERS-7, PERM-9), the endorsement of a call, and the
//! servers a delegate holds (DELEGATE-4, DELEGATE-24) all key on the alias, so a person can accept
//! one group and refuse another. Each test names two aliases that differ only in their group.

#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::delegate::{Definition, Kind};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Label;
use bravebot_core::permissions::{Anchors, Decision, Permissions, Ruling};
use bravebot_core::policy::{McpStanding, Policy, ReleasePlan, Routing};
use bravebot_core::value::Labelled;

fn grant(alias: &str) -> Capability {
    Capability::McpCall(ServerAlias::new(alias))
}

fn rules(deny: &[&str], ask: &[&str], allow: &[&str]) -> Permissions {
    let owned = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let (permissions, rejected) =
        Permissions::parse(&owned(deny), &owned(ask), &owned(allow), &Anchors::none());
    assert!(rejected.is_empty(), "a rule in this test did not parse");
    permissions
}

fn policy<'a>(
    sink: &'a mut RecordingSink,
    held: impl IntoIterator<Item = Capability>,
) -> Policy<'a, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "list the open tabs");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter(held),
        sink,
    )
    .unwrap()
}

/// Granting the call of one group's server grants nothing about another's, and taking one back
/// leaves the other. The fault this rejects is a grant compared by the program a server runs, which
/// every group alias shares, or by a prefix of the alias, which `brave` and `brave-page-read` share.
#[test]
fn a_grant_for_one_group_is_not_a_grant_for_another() {
    let mut held =
        CapabilitySet::from_iter([grant("brave-tabs-read"), grant("brave-history-read")]);

    assert!(held.contains(&grant("brave-tabs-read")));
    for other in [
        "brave-page-read",
        "brave-tabs-manage",
        "brave",
        "brave-tabs",
    ] {
        assert!(!held.contains(&grant(other)), "{other}");
    }

    assert!(held.revoke_mcp_call(&ServerAlias::new("brave-tabs-read")));
    assert!(!held.contains(&grant("brave-tabs-read")));
    assert!(held.contains(&grant("brave-history-read")));
}

/// A rule names one alias, or one tool of it, and the alias is matched whole: a rule about the
/// server that offers every tool does not reach a group's server, one about a group does not reach
/// its neighbour, and `brave` is not a prefix of `brave-page-read` for it.
#[test]
fn an_mcp_rule_about_one_group_reaches_no_other_group() {
    let permissions = rules(
        &["Mcp(brave-remove-data)"],
        &["Mcp(brave-page-read:read_page)"],
        &["Mcp(brave-tabs-read)"],
    );

    assert_eq!(
        permissions.for_mcp("brave-remove-data", "clear"),
        Decision::Ruled(Ruling::Deny)
    );
    assert_eq!(
        permissions.for_mcp("brave-page-read", "read_page"),
        Decision::Ruled(Ruling::Ask)
    );
    assert_eq!(
        permissions.for_mcp("brave-tabs-read", "list_tabs"),
        Decision::Ruled(Ruling::Allow)
    );
    for (alias, tool) in [
        ("brave-cookies", "clear"),
        ("brave-page-read", "list_tabs"),
        ("brave-history-read", "read_page"),
        ("brave", "read_page"),
        ("brave", "list_tabs"),
        ("brave-tabs-read-2", "list_tabs"),
    ] {
        assert_eq!(
            permissions.for_mcp(alias, tool),
            Decision::Unmatched,
            "{alias}:{tool} was matched by a rule about another alias"
        );
    }

    let for_all = rules(&["Mcp(brave)"], &[], &[]);
    assert_eq!(
        for_all.for_mcp("brave", "read_page"),
        Decision::Ruled(Ruling::Deny)
    );
    for group in ["brave-page-read", "brave-tabs-read"] {
        assert_eq!(
            for_all.for_mcp(group, "read_page"),
            Decision::Unmatched,
            "{group}"
        );
    }
}

/// The same rules decide the calls the policy puts to a person: a deny rule about one group refuses
/// its tools before anybody is asked and leaves the other group's alone, an ask rule asks for one
/// group whatever answer stands for another, and a standing answer removes the question only for
/// the alias it was given about.
#[test]
fn the_policy_asks_and_refuses_each_group_by_its_own_rules() {
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink, []).with_permissions(rules(
        &["Mcp(brave-remove-data)"],
        &["Mcp(brave-page-read)"],
        &[],
    ));
    let public = Label::trusted_public();

    assert!(
        policy
            .before_mcp_call_rules("brave-remove-data", "clear")
            .is_err()
    );
    assert!(
        policy
            .before_mcp_call_rules("brave-cookies", "clear")
            .is_ok()
    );

    let stopped = McpStanding::StoppedAsking;
    assert!(policy.mcp_call_needs_approval("brave-page-read", "read_page", public, stopped));
    assert!(!policy.mcp_call_needs_approval("brave-tabs-read", "list_tabs", public, stopped));
    assert!(policy.mcp_call_needs_approval(
        "brave-tabs-read",
        "list_tabs",
        public,
        McpStanding::Nothing
    ));
}

/// A person's yes to one call is an endorsement of that alias and that tool, spent once. The one
/// given for `brave-tabs-read` does not send the arguments of a call to `brave-page-read`, even
/// where a tool has the same word.
#[test]
fn an_endorsement_for_one_groups_tool_is_not_spent_on_another_groups() {
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink, []);
    let arguments = || Labelled::new("{}", Label::trusted_public());

    policy.endorse_mcp_call("brave-tabs-read", "list_tabs");
    assert!(
        policy
            .before_mcp_call("brave-page-read", "list_tabs", arguments())
            .is_err()
    );
    assert!(
        policy
            .before_mcp_call("brave", "list_tabs", arguments())
            .is_err()
    );
    assert!(
        policy
            .before_mcp_call("brave-tabs-read", "list_tabs", arguments())
            .is_ok(),
        "the endorsement was spent by a call to another alias"
    );
}

/// A worker whose definition names one group's server holds that grant and not the other groups',
/// and a definition that names none holds every group its parent does. The line selects out of
/// what the parent holds, so a group the parent holds no grant for is not handed over by naming it.
#[test]
fn a_worker_naming_one_group_holds_that_groups_server_and_no_other() {
    let parent = CapabilitySet::from_iter([
        grant("brave-tabs-read"),
        grant("brave-page-read"),
        grant("brave-history-read"),
    ]);
    let worker = |servers: Option<Vec<&str>>| {
        let definition = Definition::from_file("looker", "looks", Kind::Worker, None, "", "test");
        match servers {
            Some(names) => definition.with_servers(names.into_iter().map(String::from).collect()),
            None => definition,
        }
    };
    let held = |definition: &Definition| -> Vec<String> {
        definition
            .held_out_of(&parent)
            .iter()
            .filter(|capability| matches!(capability, Capability::McpCall(_)))
            .map(|capability| capability.to_string())
            .collect()
    };

    assert_eq!(
        held(&worker(Some(vec!["brave-tabs-read"]))),
        ["mcp_call:brave-tabs-read"]
    );
    assert_eq!(
        held(&worker(Some(vec!["brave-tabs-read", "brave-history-read"]))),
        ["mcp_call:brave-history-read", "mcp_call:brave-tabs-read"]
    );
    assert_eq!(held(&worker(None)).len(), 3);

    let naming_one_not_held = worker(Some(vec!["brave-tabs-read", "brave-cookies"]));
    assert_eq!(held(&naming_one_not_held), ["mcp_call:brave-tabs-read"]);
    assert_eq!(
        naming_one_not_held.servers_not_held(&parent),
        ["brave-cookies"]
    );

    // A shorter alias is not a way to name a longer one.
    assert!(held(&worker(Some(vec!["brave"]))).is_empty());
    assert!(held(&worker(Some(vec!["brave-tabs"]))).is_empty());
}

/// A reader and a checker hold no server whatever their definition names, so a group's server is
/// never reached by a delegate that cannot write or run anything.
#[test]
fn a_reader_or_a_checker_naming_a_group_holds_no_server() {
    let parent = CapabilitySet::from_iter([grant("brave-tabs-read")]);
    for kind in [Kind::Reader, Kind::Checker] {
        let definition = Definition::from_file("looker", "looks", kind, None, "", "test")
            .with_servers(vec!["brave-tabs-read".to_string()]);
        let held = definition.held_out_of(&parent);
        assert!(!held.contains(&grant("brave-tabs-read")), "{kind}");
    }
}
