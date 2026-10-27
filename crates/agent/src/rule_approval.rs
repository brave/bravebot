//! The question `propose_rule` puts to the person, and what each answer to it does (RECALL-3).
//!
//! The rule, the file it would go to and the reason are the question's prompt. The two choices are
//! fixed here and never written by the planner, so what a yes does is picked from a list the person
//! sees in full. A reply the person types is their own wording of the rule.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use bravebot_core::ask::{Answer, Asking, Choice, Question};
use bravebot_i18n::t;

/// What the person's answer comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Write the rule as proposed.
    Keep,
    /// Write this instead: the person's own words, as typed.
    Edited(String),
    /// Write nothing.
    Refuse,
}

/// The question: the rule with the file it goes to and the reason, and the two choices.
///
/// Words that came from private material carry a notice and no keep choice: only refusing, or
/// typing the rule in the person's own words, is offered (RECALL-3).
pub fn question(text: &str, file: &Path, reason: &str, private: bool) -> Question {
    Question::new(
        t!(agent_rule_approval_header),
        format!(
            "{}{text}\n\n{}: {}\n{}: {reason}",
            if private {
                format!("{}\n\n", t!(agent_rule_approval_private))
            } else {
                String::new()
            },
            t!(agent_rule_approval_file),
            file.display(),
            t!(agent_rule_approval_why),
        ),
        match private {
            true => vec![Choice::new(t!(agent_rule_approval_refuse), None)],
            false => vec![
                Choice::new(
                    t!(agent_rule_approval_keep),
                    Some(t!(agent_rule_approval_keep_detail).to_string()),
                ),
                Choice::new(t!(agent_rule_approval_refuse), None),
            ],
        },
        false,
    )
}

/// Give each prompt a key no other prompt has had, so an interface that remembers an answer by key
/// puts this one to the person again.
///
/// A yes is a permission to write one line, given for one moment. A yes remembered under the
/// question's own key would write the same rule a second time, with no question, after the person
/// had deleted it (ASK-8).
pub fn asked_afresh(asking: &mut Asking) {
    static ASKED: AtomicU64 = AtomicU64::new(0);
    for prompt in &mut asking.prompts {
        prompt.key = format!("rule approval {}", ASKED.fetch_add(1, Ordering::Relaxed));
    }
}

/// What an answer to [`question`] does. Only the first choice and a reply the person typed write:
/// a second choice, an out-of-range pick, several picks, a skip and no answer at all write nothing.
///
/// A reply typed where the interface lists the choices and reads a line, as a session in lines
/// does, may be one of the choices: its label, its number counted from one, or a yes or a no. Such
/// a reply is that choice, so typing "no" refuses rather than keeping "no" as a rule, and keeping
/// stays impossible where it was not offered. Only other text is the person's own wording.
pub fn verdict(answer: Option<&Answer>, private: bool) -> Verdict {
    match answer {
        Some(Answer::Chosen(chosen)) => match chosen.as_slice() {
            [0] if !private => Verdict::Keep,
            _ => Verdict::Refuse,
        },
        Some(Answer::Typed(typed)) => match typed_choice(typed, private) {
            Some(true) if !private => Verdict::Keep,
            Some(_) => Verdict::Refuse,
            None => Verdict::Edited(typed.clone()),
        },
        _ => Verdict::Refuse,
    }
}

/// Which choice a typed reply names: `Some(true)` for keeping, `Some(false)` for refusing, and
/// nothing where it names neither and is the person's own wording.
fn typed_choice(typed: &str, private: bool) -> Option<bool> {
    let typed = typed.trim().to_lowercase();
    let words = |list: String| -> Vec<String> {
        list.split(',')
            .map(|word| word.trim().to_lowercase())
            .filter(|word| !word.is_empty())
            .collect()
    };
    let keep = t!(agent_rule_approval_keep).to_lowercase();
    let refuse = t!(agent_rule_approval_refuse).to_lowercase();
    // The choices in the order [`question`] offers them, each with whether it keeps.
    let offered: Vec<(String, bool)> = match private {
        true => vec![(refuse.clone(), false)],
        false => vec![(keep.clone(), true), (refuse.clone(), false)],
    };
    if let Some((_, keeps)) = offered
        .iter()
        .enumerate()
        .find(|(at, (label, _))| *label == typed || (at + 1).to_string() == typed)
        .map(|(_, choice)| choice)
    {
        return Some(*keeps);
    }
    if typed == keep || words(t!(agent_rule_approval_yes_words).to_string()).contains(&typed) {
        return Some(true);
    }
    if typed == refuse || words(t!(agent_rule_approval_no_words).to_string()).contains(&typed) {
        return Some(false);
    }
    // A number that names no choice offered is a choice mistyped, never the wording of a rule.
    if !typed.is_empty() && typed.chars().all(|c| c.is_ascii_digit()) {
        return Some(false);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_choice_and_typed_words_write() {
        assert_eq!(
            verdict(Some(&Answer::Chosen(vec![0])), false),
            Verdict::Keep
        );
        for refused in [
            Some(Answer::Chosen(vec![1])),
            Some(Answer::Chosen(vec![2])),
            Some(Answer::Chosen(vec![])),
            Some(Answer::Chosen(vec![0, 1])),
            Some(Answer::Declined),
            None,
        ] {
            assert_eq!(
                verdict(refused.as_ref(), false),
                Verdict::Refuse,
                "{refused:?}"
            );
        }
        assert_eq!(
            verdict(Some(&Answer::Typed("Use tabs.".into())), true),
            Verdict::Edited("Use tabs.".into())
        );
    }

    /// RECALL-3: a reply typed where the choices are listed and a line is read names a choice when
    /// it is a label, a number or a yes or a no, and only other text is a rule in the person's
    /// own words. The fault this rejects is a typed refusal kept as the rule "no".
    #[test]
    fn a_typed_reply_that_names_a_choice_is_that_choice() {
        let typed = |text: &str, private: bool| verdict(Some(&Answer::Typed(text.into())), private);
        let keep = t!(agent_rule_approval_keep).to_string();
        let refuse = t!(agent_rule_approval_refuse).to_string();
        for kept in [keep.as_str(), "1", " yes ", "Y", "keep"] {
            assert_eq!(typed(kept, false), Verdict::Keep, "{kept:?}");
        }
        for refused in [refuse.as_str(), " Don't keep it ", "2", "no", "N"] {
            assert_eq!(typed(refused, false), Verdict::Refuse, "{refused:?}");
        }
        // Where the words were private, keeping was never offered: naming it refuses, and the one
        // choice offered is the refusal.
        for refused in [keep.as_str(), "yes", "keep", "1", refuse.as_str(), "no"] {
            assert_eq!(typed(refused, true), Verdict::Refuse, "{refused:?}");
        }
        // A number names a choice, and one not offered refuses rather than becoming a rule.
        for mistyped in ["2", "3", " 12 "] {
            assert_eq!(typed(mistyped, true), Verdict::Refuse, "{mistyped:?}");
        }
        assert_eq!(typed("3", false), Verdict::Refuse);
        assert_eq!(
            typed("Never push to main.", true),
            Verdict::Edited("Never push to main.".into())
        );
    }

    #[test]
    fn the_question_carries_the_rule_the_file_and_the_reason() {
        let asked = question(
            "Use tabs.",
            Path::new("/state/AGENTS.md"),
            "they said so",
            false,
        );
        for shown in ["Use tabs.", "/state/AGENTS.md", "they said so"] {
            assert!(asked.prompt.contains(shown), "{}", asked.prompt);
        }
        assert_eq!(asked.choices.len(), 2);
        let private = question("Use tabs.", Path::new("/state/AGENTS.md"), "x", true);
        assert_eq!(private.choices.len(), 1);
        assert_eq!(
            verdict(Some(&Answer::Chosen(vec![0])), true),
            Verdict::Refuse
        );
        assert!(!asked.multiple);
    }
}
