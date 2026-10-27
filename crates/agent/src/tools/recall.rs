//! `remember`, `propose_rule` and `recall`: the planner keeps a fact, proposes a rule the person
//! approves, and reads kept facts back (RECALL).

use super::{Produced, Tools, tally, workspace_is_trusted};
use crate::confirm::Confirmer;
use bravebot_core::ask::{self, Series};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use serde_json::Value;

/// The store the session may write for the working directory's project, or none where it writes
/// nothing under the state directory, as in an incognito session.
fn writable_store<S: Sink>(
    policy: &Policy<'_, S>,
    tools: &Tools,
) -> Option<crate::recall::store::Store> {
    let home = tools.home.filter(|_| crate::home::writable().is_some())?;
    let project = crate::recall::project::settled(
        home,
        tools.workspace.root(),
        workspace_is_trusted(policy, tools.workspace),
    );
    Some(crate::recall::store::Store::new(home, project))
}

/// The most characters of a saved fact's first line the transcript shows.
const REMEMBERED_LINE_SHOWN: usize = 100;

/// Keep a fact the planner wrote, shown in the transcript and undoable like a file write (RECALL-5).
///
/// The text is the planner's own and nothing quarantined: there is no reference argument, and a
/// context that took in something untrusted is refused, since a fact is trusted (RECALL-8). The
/// answer names other facts by key and percentage and quotes none of them, so saving reads no
/// fact's text into the context. A fact is only ever saved as the planner's (`source: agent`):
/// the person asking for it to be remembered is not something this tool can be told.
#[expect(
    clippy::too_many_lines,
    reason = "one save: each check and refusal in the order RECALL-4 to RECALL-8 state them"
)]
pub(super) fn remember<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools,
    arguments: &Value,
) -> Produced {
    use crate::recall::fact::{Fact, Scope, Source, is_a_key};
    use crate::recall::rank::term_share;
    use bravebot_core::label::Confidentiality;

    if tools.delegated {
        return Produced::problem("error: no such tool 'remember'");
    }
    let Some(store) = writable_store(policy, tools) else {
        return Produced::problem(
            "remember kept nothing: this session writes nothing to the user's state directory. \
             Do not retry; say the fact in your reply if it matters.",
        );
    };
    let text_of = |name: &str| arguments.get(name).and_then(Value::as_str);
    let (Some(key), Some(text)) = (text_of("key"), text_of("text")) else {
        return Produced::problem("error: 'key', 'scope' and 'text' are required strings");
    };
    let scope = match text_of("scope") {
        Some("everywhere") => Scope::Everywhere,
        Some("project") => Scope::Project,
        _ => return Produced::problem("error: 'scope' is \"everywhere\" or \"project\""),
    };
    if !is_a_key(key) {
        return Produced::problem(format!(
            "error: {key:?} is not a key. A key is lower-case words joined by hyphens, grouped \
             with slashes, such as release/forge-poll."
        ));
    }
    let supersedes = text_of("supersedes");
    if let Some(old) = supersedes
        && (!is_a_key(old) || old == key)
    {
        return Produced::problem(format!(
            "error: {old:?} is not the key of another fact, so nothing was kept"
        ));
    }
    if text.trim().is_empty() {
        return Produced::problem("error: 'text' is empty, so nothing was kept");
    }
    let new = arguments.get("new").and_then(Value::as_bool) == Some(true);

    // The planner's words at the confidentiality of its context, and refused where the context is
    // not trusted. Everything above only chose names; nothing is written before this.
    let words = policy.label_model_output("remember", text.to_string());
    let (text, mut label) = match policy.release_into_kept_fact("remember", words) {
        Ok(released) => released,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    let kept = store.load();
    let exists = kept.facts.iter().any(|(s, f)| *s == scope && f.key == key);
    let compared: Vec<(&str, u8, Confidentiality)> = kept
        .facts
        .iter()
        .filter(|(s, f)| {
            *s == scope && f.is_live() && f.key != key && supersedes != Some(f.key.as_str())
        })
        .map(|(_, f)| (f.key.as_str(), term_share(&text, &f.body), f.label))
        .collect();
    // Which facts an answer lists, and whether a save is refused, rest on every fact compared, so
    // every answer is held at the join of all their labels and not only of those it names
    // (RECALL-6, RECALL-8).
    let compared_labels: Vec<Confidentiality> =
        compared.iter().map(|(_, _, label)| *label).collect();
    let mut similar: Vec<(&str, u8, Confidentiality)> = compared
        .into_iter()
        .filter(|(_, share, _)| *share > crate::recall::SIMILAR_ABOVE)
        .collect();
    similar.sort_by_key(|(_, share, _)| std::cmp::Reverse(*share));
    // What the answer says about a kept fact (its key and how much of the new text it holds) is
    // derived from that fact, so the context takes the fact's confidentiality before it is
    // written, and the answer carries it (RECALL-8).
    let refused = |policy: &mut Policy<'_, S>, told: String, named: &[Confidentiality]| Produced {
        failed: true,
        ..about_facts(
            policy,
            "remember",
            told.clone(),
            told,
            named.iter().copied(),
        )
    };

    let mut created = None;
    let mut tags = Vec::new();
    if let Some((_, old)) = kept.facts.iter().find(|(s, f)| *s == scope && f.key == key) {
        if old.source == Source::Person {
            return refused(
                policy,
                format!(
                    "refused: {key} is a fact the user asked to keep, so it is not overwritten \
                     and nothing was kept. Pass supersedes: {key} with a new key to replace it."
                ),
                &[old.label],
            );
        }
        if let Some(by) = &old.superseded_by {
            return refused(
                policy,
                format!(
                    "refused: {key} was replaced by {by}, so it is not saved over and nothing \
                     was kept. Save under {by}, or use a new key."
                ),
                &[old.label],
            );
        }
        created = old.created.clone();
        tags = old.tags.clone();
        // The tags and the date carried over are the old fact's, so the new one is held at its
        // label too: a private fact saved over from a public context stays private.
        label = label.join(old.label);
    }

    if !new
        && !exists
        && let Some((closest, share, closest_label)) = similar.first()
        && *share >= crate::recall::DUPLICATE_AT
    {
        return refused(
            policy,
            format!(
                "refused: this says {share}% of what the kept fact {closest} already says, so \
                 nothing was kept. To correct it, save again under the key {closest}. To \
                 replace it with this fact because it stopped being true, pass supersedes: \
                 {closest}. To keep both anyway, pass new: true."
            ),
            &[&[*closest_label], compared_labels.as_slice()].concat(),
        );
    }

    // The fact to be marked as replaced, from the same scope first.
    let replaced = match supersedes {
        None => None,
        Some(old) => match kept
            .facts
            .iter()
            .filter(|(_, f)| f.key == old)
            .min_by_key(|(held, _)| *held != scope)
        {
            Some((held, f)) => Some((*held, f.clone())),
            None => {
                return Produced::problem(format!(
                    "error: no kept fact has the key {old:?}, so nothing was kept"
                ));
            }
        },
    };

    let today = crate::preamble::today();
    let fact = Fact {
        key: key.to_string(),
        source: Source::Agent,
        label,
        created: created.or_else(|| Some(today.clone())),
        verified: Some(today),
        superseded_by: None,
        // A field is one line of front matter, so an id that could end it is left out.
        session: tools
            .remembering
            .filter(|id| !id.is_empty() && !id.chars().any(char::is_control))
            .map(str::to_string),
        tags,
        body: if text.ends_with('\n') {
            text
        } else {
            format!("{text}\n")
        },
    };

    // Each file is backed up before it is written, once and under the spelling the store gives it,
    // so the session's undo takes the save apart.
    let path = store.fact_path(scope, key);
    tools.workspace.record_home_backup(&path);
    if scope == Scope::Project {
        tools.workspace.record_home_backup(&store.id_path());
    }
    if let Some((old_scope, old)) = &replaced {
        tools
            .workspace
            .record_home_backup(&store.fact_path(*old_scope, &old.key));
    }
    if let Err(failure) = store.write(scope, &fact) {
        return Produced::problem(format!("error: the fact was not kept: {failure}"));
    }
    let replaced_label = replaced.as_ref().map(|(_, old)| old.label);
    if let Some((old_scope, old)) = replaced {
        let marked = Fact {
            superseded_by: Some(key.to_string()),
            ..old
        };
        if let Err(failure) = store.write(old_scope, &marked) {
            return Produced::problem(format!(
                "error: {key} was kept, but {} could not be marked as replaced: {failure}",
                marked.key
            ));
        }
    }

    let shown: String = fact
        .body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(REMEMBERED_LINE_SHOWN)
        .collect();
    let mut told = format!("kept {key}");
    if let Some(old) = supersedes {
        told.push_str(&format!(" and marked {old} as replaced"));
    }
    told.push('.');
    if !similar.is_empty() {
        let shown_keys: Vec<String> = similar
            .iter()
            .map(|(other, share, _)| format!("{other} ({share}%)"))
            .collect();
        told.push_str(&format!(
            " Kept facts close to this one: {}. Supersede one if this replaces it.",
            shown_keys.join(", ")
        ));
    }
    let named = similar
        .iter()
        .map(|(_, _, label)| *label)
        .chain(replaced_label)
        .chain(compared_labels);
    about_facts(
        policy,
        "remember",
        told,
        format!("remembered {key}: {shown}"),
        named,
    )
}

/// One line: every control character and every kind of line break becomes a space, and runs of
/// white space become one, so nothing in a rule's text can start a second rule line.
fn on_one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Put a rule the planner wrote to the person, and append it to their rule file only on a yes
/// (RECALL-3).
///
/// The text and the reason are the planner's own and nothing quarantined: there is no reference
/// argument, and a context that took in something untrusted is refused before anything is asked,
/// since a rule is trusted. What the person is offered is fixed by [`crate::rule_approval`]. A
/// reply they type is their own wording and replaces the text. Nowhere to ask, as in a one-shot
/// run, is a refusal, and so is a session that writes nothing under the state directory.
#[expect(
    clippy::too_many_lines,
    reason = "one proposal: release, ask, read the answer, back up and append, as RECALL-3 orders them"
)]
pub(super) fn propose_rule<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> (Produced, Option<std::path::PathBuf>) {
    use crate::recall::fact::Scope;
    use crate::rule_approval::{Verdict, asked_afresh, question, verdict};

    if tools.delegated {
        return (
            Produced::problem("error: no such tool 'propose_rule'"),
            None,
        );
    }
    let Some(store) = writable_store(policy, tools) else {
        return (
            Produced::problem(
                "propose_rule wrote nothing: this session writes nothing to the user's state \
                 directory. Do not retry; say the rule in your reply if it matters.",
            ),
            None,
        );
    };
    let text_of = |name: &str| arguments.get(name).and_then(Value::as_str);
    let (Some(text), Some(reason)) = (text_of("text"), text_of("reason")) else {
        return (
            Produced::problem("error: 'text', 'scope' and 'reason' are required strings"),
            None,
        );
    };
    let scope = match text_of("scope") {
        Some("home") => Scope::Everywhere,
        Some("project") => Scope::Project,
        _ => {
            return (
                Produced::problem("error: 'scope' is \"home\" or \"project\""),
                None,
            );
        }
    };
    let (text, reason) = (on_one_line(text), on_one_line(reason));
    if text.is_empty() || reason.is_empty() {
        return (
            Produced::problem("error: 'text' and 'reason' must not be empty, so nothing was asked"),
            None,
        );
    }

    // The planner's words, refused where its context is not trusted. Nothing is asked before this.
    let mut release = |words: String| {
        let words = policy.label_model_output("propose_rule", words);
        policy.release_into_kept_fact("propose_rule", words)
    };
    let ((text, text_label), (reason, reason_label)) = match (release(text), release(reason)) {
        (Ok(text), Ok(reason)) => (text, reason),
        (Err(denial), _) | (_, Err(denial)) => {
            return (Produced::problem(format!("refused: {denial}")), None);
        }
    };
    // Words from a private context cannot become a standing rule that every later turn reads as
    // public (RECALL-8). The person can still type the rule in their own words.
    let private =
        [text_label, reason_label].contains(&bravebot_core::label::Confidentiality::Private);

    let file = store.rules_path(scope);

    let series = policy.label_model_output(
        "propose_rule",
        Series::new(vec![question(&text, &file, &reason, private)]),
    );
    let canonical = policy.render_in_place("propose_rule", &series, |s| ask::canonical_series(&s));
    if let Err(denial) = policy.before_asking("propose_rule", &canonical) {
        return (
            Produced::problem(format!(
                "refused: {denial}. A rule can only be put to the user before anything untrusted \
                 has reached your context. Say the rule in your reply instead."
            )),
            None,
        );
    }
    let shaped = policy.render_in_place("propose_rule", &series, |s| ask::asking(&s));
    let proof = policy.authorise_display_release("the proposed rule for the user");
    let mut asking = shaped.declassify(&proof);
    asked_afresh(&mut asking);
    let answers = confirmer.ask_user(&asking);

    let said = match policy.record_answers("propose_rule", &series, &answers) {
        Ok(said) => said,
        Err(denial) => return (Produced::problem(format!("refused: {denial}")), None),
    };
    let refused = match answers.first() {
        None => {
            "Nobody could be asked to keep the rule, so nothing was written. Say the rule in \
             your reply if it matters."
        }
        Some(_) => {
            "The user did not keep the rule, and nothing was written. Do not propose it again \
             unless they ask."
        }
    };
    let (kept, edited) = match verdict(answers.first(), private) {
        Verdict::Keep => (Some(text), false),
        // The person's own words are the rule, on one line like any other.
        Verdict::Edited(typed) => match on_one_line(&typed) {
            typed if typed.is_empty() => (None, false),
            typed => (Some(typed), true),
        },
        Verdict::Refuse => (None, false),
    };
    let Some(kept) = kept else {
        let told =
            policy.render_in_place("propose_rule", &said, |said| format!("{said}\n{refused}"));
        return (Produced::new(told, "", String::new()), None);
    };

    // Each file is backed up before it is written, so the session's undo takes the rule back. A
    // home rules file that is a link is backed up where it lands now, which is where the append
    // goes, and undo puts it back only while the link still lands there (RECALL-5).
    tools
        .workspace
        .record_home_backup(&crate::workspace::destination(&file).unwrap_or_else(|| file.clone()));
    if scope == Scope::Project {
        tools.workspace.record_home_backup(&store.id_path());
    }
    // The reason is the planner's, so beside the person's own words from a private context it is
    // not written.
    let why = if private {
        "typed by the person"
    } else {
        &reason
    };
    let line = format!("- {kept} ({}) why: {why}", crate::preamble::today());
    if let Err(failure) = store.append_rule(scope, &line) {
        return (
            Produced::problem(format!("error: the rule was not written: {failure}")),
            None,
        );
    }
    let outcome = match edited {
        true => "The user kept the rule in their own words and it was added to their rules.",
        false => "The user kept the rule and it was added to their rules.",
    };
    let told = policy.render_in_place("propose_rule", &said, |said| format!("{said}\n{outcome}"));
    (
        Produced::new(told, "", format!("kept a rule: {kept}")),
        Some(file),
    )
}

/// The answer of a tool that reads or names kept facts, held at the confidentiality of every fact
/// it names (RECALL-8).
///
/// Whatever the answer says about a fact (its key, its strength, its date, its text) is derived
/// from that fact, so the context takes the join of their labels before the answer is returned,
/// and the answer carries it. An answer that names no fact names nothing private.
fn about_facts<S: Sink>(
    policy: &mut Policy<'_, S>,
    origin: &str,
    told: String,
    note: String,
    named: impl IntoIterator<Item = bravebot_core::label::Confidentiality>,
) -> Produced {
    use bravebot_core::label::Confidentiality;

    let held = named
        .into_iter()
        .fold(Confidentiality::Public, |held, next| held.join(next));
    let told = policy.label_kept_fact(origin, told, held);
    policy.read_kept_fact(&told);
    Produced::new(told, "", note)
}

/// One fact as `recall` shows it: its key, how it was found, when it was last checked, then its
/// text.
fn recalled_fact(fact: &crate::recall::fact::Fact, found: &str) -> String {
    let mut out = format!(
        "{} ({found}verified {})\n",
        fact.key,
        fact.verified.as_deref().unwrap_or("never")
    );
    if let Some(by) = &fact.superseded_by {
        out.push_str(&format!("superseded by {by}\n"));
    }
    out.push_str(fact.body.trim_end());
    out.push('\n');
    out
}

/// Read the person's kept facts by a query or by one key (RECALL-10).
///
/// A query ranks the live facts of the project and of every project; a key reads that one fact
/// even where it has been replaced. Every fact returned is trusted text at the confidentiality
/// its file states, and the answer is held at the join of them.
pub(super) fn recall<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools,
    arguments: &Value,
) -> Produced {
    use crate::recall::rank::{Doc, links, rank};

    if tools.delegated {
        return Produced::problem("error: no such tool 'recall'");
    }
    let Some(home) = tools.home else {
        return Produced::problem("recall found nothing: this session has no state directory.");
    };
    let text_of = |name: &str| arguments.get(name).and_then(Value::as_str);
    let (query, key) = (text_of("query"), text_of("key"));
    if query.is_some() == key.is_some() {
        return Produced::problem("error: give exactly one of 'query' and 'key'");
    }
    let loaded = crate::recall::kept_facts(
        home,
        tools.workspace.root(),
        workspace_is_trusted(policy, tools.workspace),
    );
    // The project's own fact first where both scopes keep the key.
    let kept = crate::recall::in_scope_order(&loaded);

    if let Some(key) = key {
        let Some((_, fact)) = kept.iter().find(|(_, fact)| fact.key == key) else {
            return Produced::problem(format!("error: no kept fact has the key {key:?}"));
        };
        return about_facts(
            policy,
            "recall",
            recalled_fact(fact, ""),
            format!("recalled {}", fact.key),
            [fact.label],
        );
    }

    let query = query.unwrap_or_default();
    if query.trim().is_empty() {
        return Produced::problem("error: 'query' is empty");
    }
    let live: Vec<_> = kept.iter().filter(|(_, fact)| fact.is_live()).collect();
    let docs: Vec<Doc> = live
        .iter()
        .map(|(_, fact)| Doc {
            key: &fact.key,
            tags: &fact.tags,
            body: &fact.body,
        })
        .collect();
    let ranked = rank(query, &docs);
    let shown: Vec<_> = ranked
        .hits
        .iter()
        .take(crate::recall::RECALL_RESULTS)
        .collect();

    let mut told = String::new();
    if ranked.weak {
        told.push_str("Probably nothing is stored about this. Say so rather than assume.\n\n");
    }
    // Ranking weighs every live fact (corpus size, average length, how common each term is), so
    // what the answer shows of a public fact depends on private ones that were filtered out or
    // cut off. The answer therefore takes the label of everything that was ranked.
    let mut named: Vec<_> = live.iter().map(|(_, fact)| fact.label).collect();
    let mut seen: Vec<&str> = Vec::new();
    for hit in &shown {
        let (_, fact) = live[hit.index];
        told.push_str(&recalled_fact(
            fact,
            &format!("strength {}, ", hit.strength),
        ));
        told.push('\n');
        seen.push(&fact.key);
    }
    // One hop from the best few, each marked with the fact it came from.
    let mut linked = 0;
    for hit in shown.iter().take(crate::recall::LINK_FROM_TOP) {
        let (_, from) = live[hit.index];
        for target in links(&from.body) {
            if linked == crate::recall::LINK_MAX {
                break;
            }
            let Some((_, fact)) = live.iter().find(|(_, fact)| fact.key == target) else {
                continue;
            };
            if seen.contains(&fact.key.as_str()) {
                continue;
            }
            told.push_str(&recalled_fact(fact, &format!("via {}, ", from.key)));
            told.push('\n');
            named.push(fact.label);
            seen.push(&fact.key);
            linked += 1;
        }
    }
    if shown.is_empty() {
        told.push_str("No kept fact matches.\n");
    }
    let note = format!("recalled {}", tally(seen.len(), "fact", "facts"));
    about_facts(policy, "recall", told, note, named)
}
