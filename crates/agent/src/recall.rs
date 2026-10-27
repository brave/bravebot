//! What a person has told bravebot to keep: rules and facts that outlive a session.

pub mod fact;
pub mod overview;
pub mod project;
pub mod rank;
pub mod store;

/// Every threshold recall decides by, in one place so a change is a change to one block.
mod limits {
    /// A fact scoring below this against its own recall prompts is weak.
    pub const WEAK_BELOW: u8 = 35;
    /// The strength a fact needs to be put in front of the planner.
    pub const INJECT_STRENGTH: u8 = 50;
    /// The fewest prompt terms a fact must match to be put in front of the planner.
    pub const INJECT_MIN_TERMS: usize = 2;
    /// The most facts put in front of the planner for one prompt.
    pub const INJECT_MAX: usize = 3;
    /// A prompt shorter than this many words recalls nothing.
    pub const PROMPT_MIN_WORDS: usize = 3;
    /// A candidate this similar to a fact already kept is a duplicate.
    pub const DUPLICATE_AT: u8 = 60;
    /// A candidate this similar is shown as similar to a fact already kept.
    pub const SIMILAR_ABOVE: u8 = 45;
    /// How many of the top results a link is drawn from.
    pub const LINK_FROM_TOP: usize = 3;
    /// The most links shown for one fact.
    pub const LINK_MAX: usize = 5;
    /// The most facts a `recall` query answers with.
    pub const RECALL_RESULTS: usize = 8;
    /// BM25 term saturation.
    pub const BM25_K1: f64 = 1.2;
    /// BM25 length normalisation.
    pub const BM25_B: f64 = 0.75;
}
pub use limits::*;

use fact::{Fact, Scope};
use std::collections::BTreeSet;
use std::path::Path;
use store::Loaded;

/// Every kept fact of the project and of every project, the project's own first where both scopes
/// keep a key. The one reading of the store `recall` and the prompt's injection share.
pub fn kept_facts(home: &Path, root: &Path, trusted: bool) -> Loaded {
    store::Store::new(home, project::settled(home, root, trusted)).load()
}

/// The facts of a read in the order a key resolves in: the project's own first.
pub fn in_scope_order(loaded: &Loaded) -> Vec<&(Scope, Fact)> {
    let mut kept: Vec<_> = loaded.facts.iter().collect();
    kept.sort_by_key(|(scope, _)| *scope != Scope::Project);
    kept
}

/// Whether `prompt` can have kept facts put in front of it: a prompt of [`PROMPT_MIN_WORDS`] words
/// or more that is not a command. Asked before the facts are read, since most lines are neither.
pub fn may_inject(prompt: &str) -> bool {
    prompt.split_whitespace().count() >= PROMPT_MIN_WORDS && !prompt.trim_start().starts_with('/')
}

/// The kept facts to put in front of the planner for `prompt` (RECALL-9): at most
/// [`INJECT_MAX`] live facts, each at [`INJECT_STRENGTH`] or more, matching at least
/// [`INJECT_MIN_TERMS`] distinct words of the prompt, and none whose key is in `already`.
///
/// A word counts when any of its terms is one the fact holds. Counted by words rather than by
/// ranked terms because a compound such as `~ricsul-bilwyt` ranks as the whole word and each of
/// its parts, and one word must not pass a floor meant to refuse a match on one word.
pub fn facts_to_inject<'a>(
    prompt: &str,
    loaded: &'a Loaded,
    already: &BTreeSet<String>,
) -> Vec<&'a Fact> {
    if !may_inject(prompt) {
        return Vec::new();
    }
    // A key resolves to the project's own fact first, so an everywhere fact under the same key is
    // not put beside it on the same prompt.
    let mut keys = BTreeSet::new();
    let live: Vec<&Fact> = in_scope_order(loaded)
        .into_iter()
        .map(|(_, fact)| fact)
        .filter(|fact| fact.is_live())
        .filter(|fact| keys.insert(fact.key.as_str()))
        .collect();
    let docs: Vec<rank::Doc> = live
        .iter()
        .map(|fact| rank::Doc {
            key: &fact.key,
            tags: &fact.tags,
            body: &fact.body,
        })
        .collect();
    // A set of words, so a word the prompt repeats is one of the two (RECALL-9).
    let words: BTreeSet<BTreeSet<String>> = prompt
        .split_whitespace()
        .map(|word| rank::terms(word).into_iter().collect())
        .collect();
    rank::rank(prompt, &docs)
        .hits
        .iter()
        .filter(|hit| hit.strength >= INJECT_STRENGTH)
        .map(|hit| live[hit.index])
        .filter(|fact| !already.contains(&fact.key))
        .filter(|fact| {
            let held: BTreeSet<String> = rank::terms(&fact.key)
                .into_iter()
                .chain(fact.tags.iter().flat_map(|tag| rank::terms(tag)))
                .chain(rank::terms(&fact.body))
                .collect();
            words.iter().filter(|word| !word.is_disjoint(&held)).count() >= INJECT_MIN_TERMS
        })
        .take(INJECT_MAX)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(facts: &[(&str, &str)]) -> Loaded {
        Loaded {
            facts: facts
                .iter()
                .map(|(key, body)| {
                    (
                        Scope::Project,
                        Fact::parse(
                            key,
                            &format!("---\nsource: person\nlabel: public\nverified: 2026-10-01\n---\n{body}\n"),
                        ),
                    )
                })
                .collect(),
            notices: Vec::new(),
        }
    }

    fn keys<'a>(facts: &[&'a Fact]) -> Vec<&'a str> {
        facts.iter().map(|fact| fact.key.as_str()).collect()
    }

    const NONE: BTreeSet<String> = BTreeSet::new();

    #[test]
    fn a_short_prompt_injects_nothing() {
        let kept = loaded(&[("deploy", "deploy the staging forge")]);
        assert!(facts_to_inject("deploy staging", &kept, &NONE).is_empty());
    }

    #[test]
    fn a_slash_command_injects_nothing() {
        let kept = loaded(&[("deploy", "deploy the staging forge")]);
        assert!(facts_to_inject("/deploy staging forge now", &kept, &NONE).is_empty());
    }

    #[test]
    fn one_common_word_is_not_enough() {
        let kept = loaded(&[("deploy", "deploy"), ("other", "unrelated words")]);
        let found = facts_to_inject("what about the deploy", &kept, &NONE);
        assert!(found.is_empty(), "{:?}", keys(&found));
        // Nor does one word said twice.
        let found = facts_to_inject("deploy the deploy, please", &kept, &NONE);
        assert!(found.is_empty(), "{:?}", keys(&found));
        // Two words clear it.
        let kept = loaded(&[("deploy", "deploy staging forge")]);
        let found = facts_to_inject("deploy the staging forge", &kept, &NONE);
        assert_eq!(keys(&found), ["deploy"]);
    }

    #[test]
    fn one_compound_word_is_not_enough() {
        let kept = loaded(&[("ship", "~ricsul-bilwyt"), ("other", "unrelated words")]);
        let found = facts_to_inject("what about the ~ricsul-bilwyt", &kept, &NONE);
        assert!(found.is_empty(), "{:?}", keys(&found));
    }

    #[test]
    fn a_project_fact_shadows_an_everywhere_fact_under_its_key() {
        let mut kept = loaded(&[("deploy", "deploy staging forge from the project")]);
        let everywhere = loaded(&[("deploy", "deploy staging forge from everywhere")]);
        kept.facts
            .insert(0, (Scope::Everywhere, everywhere.facts[0].1.clone()));
        let found = facts_to_inject("deploy the staging forge", &kept, &NONE);
        assert_eq!(found.len(), 1, "{:?}", keys(&found));
        assert!(
            found[0].body.contains("from the project"),
            "{}",
            found[0].body
        );
    }

    #[test]
    fn at_most_three_are_injected() {
        let kept = loaded(&[
            ("a", "deploy staging forge"),
            ("b", "deploy staging forge"),
            ("c", "deploy staging forge"),
            ("d", "deploy staging forge"),
        ]);
        assert_eq!(
            facts_to_inject("deploy staging forge", &kept, &NONE).len(),
            3
        );
    }

    #[test]
    fn a_fact_is_injected_once_per_session() {
        let kept = loaded(&[("a", "deploy staging forge")]);
        let already: BTreeSet<String> = ["a".to_string()].into();
        assert!(facts_to_inject("deploy staging forge", &kept, &already).is_empty());
    }

    #[test]
    fn a_replaced_fact_is_not_injected() {
        let mut kept = loaded(&[("a", "deploy staging forge")]);
        kept.facts[0].1.superseded_by = Some("b".to_string());
        assert!(facts_to_inject("deploy staging forge", &kept, &NONE).is_empty());
    }
}
