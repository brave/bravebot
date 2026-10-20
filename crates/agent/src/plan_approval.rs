//! The question `propose_plan` puts to the person, and what each answer to it does to the mode.
//!
//! The plan is the planner's own text and is the one question's prompt. The three choices are
//! fixed here and never written by the planner, so the mode the session moves to is picked from a
//! list the person sees in full (MODE-14). Bypassing is not on the list (MODE-5).

use std::sync::atomic::{AtomicU64, Ordering};

use crate::PermissionMode;
use bravebot_core::ask::{Answer, Asking, Choice, Question};
use bravebot_i18n::t;

/// What the person's answer comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    /// Plan mode ends and the session continues in this mode.
    LeaveFor(PermissionMode),
    /// The session stays in plan mode.
    KeepPlanning,
}

/// The modes the prompt offers, in the order its choices are drawn.
const OFFERED: [PermissionMode; 2] = [PermissionMode::AcceptEdits, PermissionMode::Ask];

/// The question: the plan, with the three choices.
pub fn question(plan: &str) -> Question {
    Question::new(
        t!(agent_plan_approval_header),
        plan,
        vec![
            Choice::new(
                t!(agent_plan_approval_accept_edits),
                Some(t!(agent_plan_approval_accept_edits_detail).to_string()),
            ),
            Choice::new(
                t!(agent_plan_approval_ask),
                Some(t!(agent_plan_approval_ask_detail).to_string()),
            ),
            Choice::new(t!(agent_plan_approval_keep_planning), None),
        ],
        false,
    )
}

/// Give each prompt a key no other prompt has had, so an interface that remembers an answer by key
/// puts this one to the person again.
///
/// An approval is a permission to change the mode, given for one moment. A yes remembered under the
/// plan's own key would end plan mode a second time, with no question, for the same plan proposed
/// after the person had gone back to planning.
pub fn asked_afresh(asking: &mut Asking) {
    static ASKED: AtomicU64 = AtomicU64::new(0);
    for prompt in &mut asking.prompts {
        prompt.key = format!("plan approval {}", ASKED.fetch_add(1, Ordering::Relaxed));
    }
}

/// What an answer to [`question`] does. Anything other than one of the two approving choices
/// keeps planning: a typed reply is feedback for the planner, and a skip approves nothing.
pub fn approval(answer: Option<&Answer>) -> Approval {
    match answer {
        Some(Answer::Chosen(chosen)) => match chosen.as_slice() {
            [index] => OFFERED
                .get(*index)
                .map_or(Approval::KeepPlanning, |mode| Approval::LeaveFor(*mode)),
            _ => Approval::KeepPlanning,
        },
        _ => Approval::KeepPlanning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_two_choices_leave_plan_mode_and_the_third_does_not() {
        let pick = |index| approval(Some(&Answer::Chosen(vec![index])));
        assert_eq!(pick(0), Approval::LeaveFor(PermissionMode::AcceptEdits));
        assert_eq!(pick(1), Approval::LeaveFor(PermissionMode::Ask));
        assert_eq!(pick(2), Approval::KeepPlanning);
    }

    #[test]
    fn no_answer_selects_bypassing_or_leaves_plan_mode_without_a_choice() {
        for index in 0..8 {
            assert_ne!(
                approval(Some(&Answer::Chosen(vec![index]))),
                Approval::LeaveFor(PermissionMode::Bypass)
            );
        }
        assert_eq!(
            approval(Some(&Answer::Chosen(vec![]))),
            Approval::KeepPlanning
        );
        assert_eq!(
            approval(Some(&Answer::Chosen(vec![0, 1]))),
            Approval::KeepPlanning
        );
        assert_eq!(
            approval(Some(&Answer::Typed("0".into()))),
            Approval::KeepPlanning
        );
        assert_eq!(approval(Some(&Answer::Declined)), Approval::KeepPlanning);
        assert_eq!(approval(None), Approval::KeepPlanning);
    }

    #[test]
    fn the_question_carries_the_plan_and_one_choice_per_outcome() {
        let asked = question("1. edit a.rs");
        assert_eq!(asked.prompt, "1. edit a.rs");
        assert_eq!(asked.choices.len(), OFFERED.len() + 1);
        assert!(!asked.multiple);
    }
}
