use super::arguments::{Scratch, told, with_tools};
use super::*;
use crate::recall::fact::{Fact, Scope};
use crate::recall::project::project_id;
use crate::recall::store::Store;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Confidentiality;
use bravebot_core::policy::{ReleasePlan, Routing};

const NO_MATCH: &str = "Probably nothing is stored about this. Say so rather than assume.";

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "look up what was kept");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .expect("policy")
}

struct Place {
    _scratch: Scratch,
    workspace: Workspace,
    home: std::path::PathBuf,
    held: &'static std::path::Path,
}

impl Place {
    fn new(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let project = scratch.path.join("project");
        let home = scratch.path.join("state");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let workspace = Workspace::new(&project)
            .expect("workspace")
            .with_state(&home);
        let held: &'static std::path::Path = Box::leak(home.clone().into_boxed_path());
        Place {
            _scratch: scratch,
            workspace,
            home,
            held,
        }
    }

    fn store(&self) -> Store {
        Store::new(&self.home, project_id(self.workspace.root(), false))
    }

    fn keep(&self, scope: Scope, key: &str, body: &str, edit: impl FnOnce(&mut Fact)) {
        let mut fact = Fact::parse(key, "");
        fact.body = format!("{body}\n");
        fact.verified = Some("2026-01-01".to_string());
        fact.label = Confidentiality::Public;
        edit(&mut fact);
        self.store().write(scope, &fact).unwrap();
    }

    fn fact(&self, scope: Scope, key: &str) -> Fact {
        let text = std::fs::read_to_string(self.store().fact_path(scope, key)).unwrap();
        Fact::parse(key, &text)
    }

    /// What the tool answered, whether it failed, and what the context held afterwards.
    fn call(&self, arguments: Value) -> (String, bool, Confidentiality) {
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        let produced = with_tools(&self.workspace, |tools| {
            tools.home = Some(self.held);
            recall(&mut policy, tools, &arguments)
        });
        let held = policy.context_confidentiality();
        (told(&mut policy, &produced.text), produced.failed, held)
    }
}

#[test]
fn recall_finds_a_fact_by_its_words() {
    let place = Place::new("recall-finds");
    place.keep(
        Scope::Everywhere,
        "release/forge-poll",
        "The forge is polled every minute.",
        |_| {},
    );
    place.keep(
        Scope::Project,
        "cooking/bread",
        "Proof the dough overnight.",
        |_| {},
    );
    let (told, failed, _) = place.call(json!({"query": "forge polled"}));
    assert!(!failed, "{told}");
    assert!(told.contains("release/forge-poll (strength "), "{told}");
    assert!(told.contains("verified 2026-01-01"), "{told}");
    assert!(told.contains("The forge is polled every minute."), "{told}");
    assert!(!told.contains("bread"), "{told}");
    assert!(!told.contains(NO_MATCH), "{told}");
}

#[test]
fn recall_reads_the_project_and_every_project() {
    let place = Place::new("recall-scopes");
    place.keep(
        Scope::Everywhere,
        "style/tabs",
        "Indent with tabs always.",
        |_| {},
    );
    place.keep(
        Scope::Project,
        "style/width",
        "Wrap lines at one hundred.",
        |_| {},
    );
    let (told, _, _) = place.call(json!({"query": "indent tabs wrap lines"}));
    assert!(
        told.contains("style/tabs") && told.contains("style/width"),
        "{told}"
    );
}

#[test]
fn recall_says_weak_when_nothing_matches() {
    let place = Place::new("recall-weak");
    place.keep(
        Scope::Everywhere,
        "a",
        "The forge is polled every minute.",
        |_| {},
    );
    let (told, failed, _) = place.call(json!({"query": "kubernetes ingress"}));
    assert!(!failed, "{told}");
    assert!(told.contains(NO_MATCH), "{told}");
}

#[test]
fn recall_answers_at_most_the_result_count() {
    let place = Place::new("recall-count");
    for n in 0..(crate::recall::RECALL_RESULTS + 3) {
        place.keep(
            Scope::Everywhere,
            &format!("note/n{n}"),
            "the forge polls",
            |_| {},
        );
    }
    let (told, _, _) = place.call(json!({"query": "forge polls"}));
    assert_eq!(
        told.matches("strength ").count(),
        crate::recall::RECALL_RESULTS
    );
}

#[test]
fn recall_leaves_superseded_facts_out_of_a_query() {
    let place = Place::new("recall-superseded");
    place.keep(
        Scope::Everywhere,
        "old",
        "The forge is polled every minute.",
        |f| {
            f.superseded_by = Some("new".to_string());
        },
    );
    place.keep(
        Scope::Everywhere,
        "new",
        "The forge is watched by webhook.",
        |_| {},
    );
    let (told, _, _) = place.call(json!({"query": "forge polled watched"}));
    assert!(told.contains("new (strength"), "{told}");
    assert!(!told.contains("old ("), "{told}");
    assert!(!told.contains("polled every minute"), "{told}");
}

#[test]
fn reading_a_superseded_fact_by_key_names_its_replacement() {
    let place = Place::new("recall-by-key");
    place.keep(
        Scope::Everywhere,
        "old",
        "The forge is polled every minute.",
        |f| {
            f.superseded_by = Some("release/forge-watch".to_string());
        },
    );
    let (told, failed, _) = place.call(json!({"key": "old"}));
    assert!(!failed, "{told}");
    assert!(told.contains("superseded by release/forge-watch"), "{told}");
    assert!(told.contains("polled every minute"), "{told}");
    let (told, failed, _) = place.call(json!({"key": "absent"}));
    assert!(failed, "{told}");
}

#[test]
fn recall_takes_exactly_one_of_query_and_key() {
    let place = Place::new("recall-one-of");
    for arguments in [json!({}), json!({"query": "a b", "key": "a"})] {
        let (told, failed, _) = place.call(arguments);
        assert!(failed && told.contains("exactly one"), "{told}");
    }
}

#[test]
fn recall_follows_links_one_hop_from_the_top_three() {
    let place = Place::new("recall-links");
    place.keep(
        Scope::Everywhere,
        "zebra/steps",
        "Deploy the zebra service by hand. See [[ops/rollback]].",
        |_| {},
    );
    place.keep(
        Scope::Everywhere,
        "ops/rollback",
        "Roll back with the saved tag.\nSee [[ops/second-hop]].",
        |_| {},
    );
    place.keep(
        Scope::Everywhere,
        "ops/second-hop",
        "Two hops away.",
        |_| {},
    );
    place.keep(Scope::Everywhere, "ops/gone", "Nothing here.", |f| {
        f.superseded_by = Some("zebra/steps".to_string());
    });
    place.keep(
        Scope::Everywhere,
        "zebra/links-gone",
        "Deploy the zebra service. See [[ops/gone]].",
        |_| {},
    );
    let (told, _, _) = place.call(json!({"query": "zebra service"}));
    assert!(
        told.contains("ops/rollback (via zebra/steps, verified"),
        "{told}"
    );
    assert!(!told.contains("ops/second-hop ("), "{told}");
    assert!(!told.contains("ops/gone ("), "{told}");
}

#[test]
fn recall_follows_at_most_the_link_cap() {
    let place = Place::new("recall-link-cap");
    let targets: Vec<String> = (0..crate::recall::LINK_MAX + 2)
        .map(|n| format!("[[target/t{n}]]"))
        .collect();
    place.keep(
        Scope::Everywhere,
        "hub",
        &format!("The zebra hub. {}", targets.join(" ")),
        |_| {},
    );
    for n in 0..crate::recall::LINK_MAX + 2 {
        place.keep(
            Scope::Everywhere,
            &format!("target/t{n}"),
            "elsewhere",
            |_| {},
        );
    }
    let (told, _, _) = place.call(json!({"query": "zebra hub"}));
    assert_eq!(
        told.matches("via hub").count(),
        crate::recall::LINK_MAX,
        "{told}"
    );
}

#[test]
fn recalling_a_private_fact_makes_the_context_private() {
    let place = Place::new("recall-private");
    place.keep(
        Scope::Everywhere,
        "health/pill",
        "Takes a pill at nine.",
        |f| {
            f.label = Confidentiality::Private;
        },
    );
    place.keep(
        Scope::Everywhere,
        "open/fact",
        "The forge is polled.",
        |_| {},
    );
    let (_, _, held) = place.call(json!({"query": "pill nine"}));
    assert_eq!(held, Confidentiality::Private);
    let (_, _, held) = place.call(json!({"key": "health/pill"}));
    assert_eq!(held, Confidentiality::Private);
    let (_, _, held) = place.call(json!({"key": "open/fact"}));
    assert_eq!(held, Confidentiality::Public);
    let (told, _, held) = place.call(json!({"query": "kubernetes ingress"}));
    assert!(told.contains(NO_MATCH), "{told}");
    assert_eq!(held, Confidentiality::Private);
}

/// Which facts survive a phrase filter depends on the private one too, so the answer
/// carries its label even when it is not shown.
#[test]
fn a_private_fact_removed_by_a_phrase_still_makes_the_context_private() {
    let place = Place::new("recall-private-phrase");
    place.keep(
        Scope::Everywhere,
        "open",
        "The zebra hub runs daily.",
        |_| {},
    );
    place.keep(
        Scope::Everywhere,
        "secret",
        "The zebra hub is hidden.",
        |f| {
            f.label = Confidentiality::Private;
        },
    );
    let (told, _, held) = place.call(json!({"query": "zebra \"hub runs\""}));
    assert!(told.contains("open (strength"), "{told}");
    assert!(!told.contains("secret ("), "{told}");
    assert_eq!(held, Confidentiality::Private);
}

/// The cut at the result count depends on every ranked fact, so a private fact below it
/// still labels the answer.
#[test]
fn a_private_fact_cut_off_past_the_result_count_still_makes_the_context_private() {
    let place = Place::new("recall-private-cut");
    for n in 0..crate::recall::RECALL_RESULTS {
        place.keep(
            Scope::Everywhere,
            &format!("note/n{n}"),
            "zebra zebra zebra hub",
            |_| {},
        );
    }
    place.keep(Scope::Everywhere, "secret", "a zebra", |f| {
        f.label = Confidentiality::Private;
    });
    let (told, _, held) = place.call(json!({"query": "zebra hub"}));
    assert!(!told.contains("secret ("), "{told}");
    assert_eq!(held, Confidentiality::Private);
}

#[test]
fn a_corpus_of_public_facts_leaves_the_context_public() {
    let place = Place::new("recall-public-corpus");
    place.keep(Scope::Everywhere, "a", "The zebra hub runs daily.", |_| {});
    place.keep(Scope::Project, "b", "Something else entirely.", |_| {});
    let (_, _, held) = place.call(json!({"query": "zebra hub"}));
    assert_eq!(held, Confidentiality::Public);
    let (_, _, held) = place.call(json!({"query": "kubernetes ingress"}));
    assert_eq!(held, Confidentiality::Public);
}

#[test]
fn a_private_fact_linked_to_makes_the_context_private() {
    let place = Place::new("recall-private-link");
    place.keep(
        Scope::Everywhere,
        "open",
        "The zebra hub. See [[secret]].",
        |_| {},
    );
    place.keep(Scope::Everywhere, "secret", "Hidden.", |f| {
        f.label = Confidentiality::Private;
    });
    let (told, _, held) = place.call(json!({"query": "zebra hub"}));
    assert!(told.contains("secret (via open"), "{told}");
    assert_eq!(held, Confidentiality::Private);
}

#[test]
fn a_delegate_cannot_recall() {
    let place = Place::new("recall-delegate");
    place.keep(Scope::Everywhere, "k", "a b c", |_| {});
    {
        let call: ToolCall = serde_json::from_value(json!({
            "id": "1",
            "function": {"name": "recall", "arguments": json!({"key": "k"}).to_string()}
        }))
        .expect("a call");
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        let told = with_tools(&place.workspace, |tools| {
            tools.home = Some(place.held);
            tools.delegated = true;
            let output = dispatch(
                &mut policy,
                tools,
                &mut crate::confirm::ApproveWrites,
                &mut crate::report::IgnoreReports,
                &call,
            );
            told(&mut policy, &output.text)
        });
        assert!(told.starts_with("error: no such tool"), "{told}");
        assert_eq!(policy.context_confidentiality(), Confidentiality::Public);
    }
    assert_eq!(
        place.fact(Scope::Everywhere, "k").verified.as_deref(),
        Some("2026-01-01")
    );
}

#[test]
fn a_delegate_reaching_the_recall_handlers_anyway_is_refused() {
    let place = Place::new("recall-delegate-direct");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let read = with_tools(&place.workspace, |tools| {
        tools.home = Some(place.held);
        tools.delegated = true;
        recall(&mut policy, tools, &json!({"key": "k"}))
    });
    assert!(read.failed);
}
