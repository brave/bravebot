//! A condition the session works towards.
//!
//! The state machine lives in [`bravebot_agent::goal`], so the terminal and the desktop bridge
//! share one implementation. This module re-exports it under the name the terminal uses, and its
//! tests stay here so the names `docs/specs/goal.md` cites still resolve.

pub use bravebot_agent::goal::{Asked, MAX_IDLE_ROUNDS, MAX_ROUNDS, Next, Running, parse};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bare_command_asks_what_the_goal_is() {
        assert_eq!(parse(""), Asked::Report);
        assert_eq!(parse("   "), Asked::Report);
    }

    #[test]
    fn the_reserved_word_takes_the_goal_off() {
        assert_eq!(parse("clear"), Asked::Clear);
        assert_eq!(parse("  clear  "), Asked::Clear);
    }

    /// The reservation is one word and not a prefix, for the reason a command name is the whole
    /// word: a person asking for the build directory to be cleaned must not have their goal
    /// silently taken off instead.
    #[test]
    fn a_condition_that_begins_with_the_reserved_word_is_still_a_condition() {
        assert_eq!(
            parse("clear the build directory and cargo test exits 0"),
            Asked::Set("clear the build directory and cargo test exits 0".to_string())
        );
    }

    /// Same reservation as `clear`: the whole word, so a condition that opens with one is a
    /// condition, and a person describing work is not silently holding their goal still.
    #[test]
    fn pause_and_resume_are_whole_words_and_not_prefixes() {
        assert_eq!(parse("pause"), Asked::Pause);
        assert_eq!(parse("  resume  "), Asked::Resume);
        assert_eq!(
            parse("pause the deploy before the tests pass"),
            Asked::Set("pause the deploy before the tests pass".to_string())
        );
        assert_eq!(
            parse("resume.txt exists"),
            Asked::Set("resume.txt exists".to_string())
        );
    }

    #[test]
    fn pausing_keeps_the_condition_and_the_rounds_and_resuming_arms_it_again() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        goal.not_met("nothing ran them".to_string(), 1);
        assert!(goal.pause());
        assert!(!goal.pause(), "a second pause reported a change");
        assert!(goal.is_paused());
        assert_eq!(goal.condition(), "the tests pass");
        assert_eq!(goal.rounds(), 1);
        assert_eq!(goal.last_reason(), Some("nothing ran them"));
        assert!(goal.resume());
        assert!(!goal.resume(), "a second resume reported a change");
        assert!(!goal.is_paused());
        assert_eq!(goal.rounds(), 1);
    }

    #[test]
    fn anything_else_is_the_condition_verbatim() {
        assert_eq!(
            parse("  cargo test -p bravebot-tui exits 0  "),
            Asked::Set("cargo test -p bravebot-tui exits 0".to_string())
        );
    }

    #[test]
    fn a_fresh_goal_has_sent_nothing_back_and_has_heard_nothing() {
        let goal = Running::begin("the tests pass".to_string(), 0, false);
        assert_eq!(goal.rounds(), 0);
        assert_eq!(goal.last_reason(), None);
        assert_eq!(goal.left(), MAX_ROUNDS);
    }

    #[test]
    fn a_verdict_of_not_yet_is_kept_and_counted() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        assert_eq!(
            goal.not_met("nothing ran them".to_string(), 1),
            Next::SendBack
        );
        assert_eq!(goal.rounds(), 1);
        assert_eq!(goal.last_reason(), Some("nothing ran them"));
    }

    /// A condition nobody can satisfy would otherwise spend a session's whole budget re-sending a
    /// conversation that grows every round.
    #[test]
    fn a_goal_stops_sending_the_work_back_once_its_rounds_are_spent() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        for round in 1..=MAX_ROUNDS {
            assert_eq!(
                goal.not_met("still nothing".to_string(), 1),
                Next::SendBack,
                "the goal gave up on round {round} of {MAX_ROUNDS}"
            );
        }
        assert_eq!(goal.left(), 0);
        assert_eq!(goal.not_met("still nothing".to_string(), 1), Next::Spent);
    }

    /// The round that runs out of budget is the one whose reason a person most wants to read, so
    /// giving up must not also throw away what the judge said.
    #[test]
    fn the_reason_from_the_round_that_gave_up_is_still_kept() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        for _ in 0..MAX_ROUNDS {
            goal.not_met("still nothing".to_string(), 1);
        }
        goal.not_met("the linker is missing".to_string(), 1);
        assert_eq!(goal.last_reason(), Some("the linker is missing"));
    }

    /// What a goal has cost is counted from when it was armed, not from the start of the session,
    /// and the time from the same moment.
    #[test]
    fn a_goal_counts_what_was_spent_and_how_long_from_the_moment_it_was_armed() {
        let goal = Running::begin("the tests pass".to_string(), 4_000, false);

        assert_eq!(goal.spent(4_000), 0);
        assert_eq!(goal.spent(9_500), 5_500);
        assert_eq!(goal.spent(100), 0, "a total that went down underflowed");
        assert_eq!(
            goal.elapsed(goal.started() + std::time::Duration::from_secs(125)),
            std::time::Duration::from_secs(125)
        );
    }

    /// A turn that answers in prose is sent back, and one that keeps doing it is not: the goal
    /// stays set for the next prompt and the count starts again.
    #[test]
    fn rounds_with_no_tool_call_in_a_row_stop_the_goal_without_clearing_it() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        for _ in 1..MAX_IDLE_ROUNDS {
            assert_eq!(goal.not_met("say it".to_string(), 0), Next::SendBack);
        }
        assert!(!goal.is_stalled());
        assert_eq!(goal.not_met("say it".to_string(), 0), Next::Stalled);
        assert!(goal.is_stalled(), "a stalled goal is an open loop (OPEN-1)");
        assert_eq!(goal.last_reason(), Some("say it"));
        assert_eq!(goal.not_met("say it".to_string(), 0), Next::SendBack);
        assert!(!goal.is_stalled());
    }

    /// Only consecutive rounds count: a round that ran something starts the count over.
    #[test]
    fn a_round_that_made_a_call_starts_the_count_again() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        for calls in [0, 0, 1, 0, 0, 1] {
            assert_eq!(goal.not_met("say it".to_string(), calls), Next::SendBack);
        }
    }

    /// Two writes and a command after them, as a turn that edited and then built.
    const BUILT: bravebot_agent::Work = bravebot_agent::Work {
        writes: 2,
        commands: 1,
        ran_after_last_write: true,
        last_command_failed: false,
    };

    /// GROUND-4. Every turn the goal drives adds to its record, a later write after an earlier
    /// command included.
    #[test]
    fn the_record_adds_every_turn_the_goal_drives() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        goal.worked(BUILT);
        goal.worked(bravebot_agent::Work {
            writes: 1,
            commands: 0,
            ran_after_last_write: false,
            last_command_failed: false,
        });
        assert_eq!(
            goal.record(),
            bravebot_agent::Work {
                writes: 3,
                commands: 1,
                ran_after_last_write: false,
                last_command_failed: false,
            }
        );
    }

    /// GROUND-4. The turn that was running when the goal was set began before it, so it is left
    /// out, and the next one counts.
    #[test]
    fn a_turn_running_when_the_goal_was_set_is_not_counted() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, true);
        goal.worked(BUILT);
        assert_eq!(goal.record(), bravebot_agent::Work::default());
        goal.worked(BUILT);
        assert_eq!(goal.record(), BUILT);
    }

    /// GROUND-4. A paused goal still counts, since the work is done whether or not it is judged.
    #[test]
    fn a_turn_while_the_goal_is_paused_is_counted() {
        let mut goal = Running::begin("the tests pass".to_string(), 0, false);
        goal.pause();
        goal.worked(BUILT);
        assert_eq!(goal.record(), BUILT);
    }
}
