//! The agents a conversation has started, and where each stands.
//!
//! One list for every front end, so the rules for it are written once: a start appends, a finish
//! marks the latest agent with that number that is still working, a turn ending marks every agent
//! still working as not finished, and the counts are read off the list. A front end hands it the
//! driver's events and draws what it holds.
//!
//! Numbers count from one in every turn, so one can name an agent that ended in an earlier turn.
//! A finish therefore goes to the latest agent with that number that is still working, and an
//! agent that has ended is never changed by a later finish.
//!
//! Everything here comes from the driver. The kind and the task are carried for display and are
//! never compared or branched on, and nothing a delegate printed is given to this type.

use crate::report::DelegateId;

/// Where one agent stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Started, and no finish has been reported for it.
    Working,
    /// Finished, and the driver did not report a failure.
    Finished,
    /// Finished with a failure, or still working when its turn ended (DELEGATE-17).
    Failed,
}

/// One agent: the driver's number for it, its kind, what it was asked and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// The driver's number for it.
    pub id: DelegateId,
    /// The kind of delegate, as the driver named it. Display only.
    pub kind: String,
    /// What it was asked to do, as the planner wrote it. Display only.
    pub task: String,
    /// Where it stands.
    pub state: AgentState,
}

/// How many agents have been started and how many of those are still working.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Every agent in the list.
    pub started: usize,
    /// The ones whose state is [`AgentState::Working`].
    pub working: usize,
}

/// The agents a conversation has started, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roster {
    agents: Vec<Agent>,
}

impl Roster {
    /// A roster with no agents.
    pub fn new() -> Self {
        Self::default()
    }

    /// An agent began. Added whether or not its number was seen before.
    pub fn start(&mut self, id: DelegateId, kind: String, task: String) {
        self.agents.push(Agent {
            id,
            kind,
            task,
            state: AgentState::Working,
        });
    }

    /// An agent ended, named by its number.
    ///
    /// Returns the place in the list of the agent it ended, or `None` where no agent with that
    /// number is still working, in which case nothing changes.
    pub fn finish(&mut self, id: DelegateId, failed: bool) -> Option<usize> {
        let at = self
            .agents
            .iter()
            .rposition(|agent| agent.id == id && agent.state == AgentState::Working)?;
        self.agents[at].state = if failed {
            AgentState::Failed
        } else {
            AgentState::Finished
        };
        Some(at)
    }

    /// The turn is over, and an agent does not outlive the turn that started it (DELEGATE-17).
    ///
    /// Every agent still working is marked failed. Returns the place in the list of each one
    /// changed, oldest first.
    pub fn turn_ended(&mut self) -> Vec<usize> {
        let mut changed = Vec::new();
        for (at, agent) in self.agents.iter_mut().enumerate() {
            if agent.state == AgentState::Working {
                agent.state = AgentState::Failed;
                changed.push(at);
            }
        }
        changed
    }

    /// Keep the first `len` agents. A front end that drops the part of a conversation after a
    /// point drops the agents it started with it.
    pub fn truncate(&mut self, len: usize) {
        self.agents.truncate(len);
    }

    /// Forget every agent, for a conversation that begins again.
    pub fn clear(&mut self) {
        self.agents.clear();
    }

    /// Every agent, oldest first.
    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    /// How many have been started and how many of those are still working.
    pub fn counts(&self) -> Counts {
        Counts {
            started: self.agents.len(),
            working: self
                .agents
                .iter()
                .filter(|agent| agent.state == AgentState::Working)
                .count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(roster: &Roster) -> Vec<(String, AgentState)> {
        roster
            .agents()
            .iter()
            .map(|agent| (agent.id.to_string(), agent.state))
            .collect()
    }

    fn started(roster: &mut Roster, number: u32, task: &str) {
        roster.start(DelegateId::nth(number), "reader".into(), task.into());
    }

    #[test]
    fn a_start_appends_a_working_agent() {
        let mut roster = Roster::new();
        started(&mut roster, 1, "first");
        started(&mut roster, 2, "second");
        assert_eq!(
            states(&roster),
            [
                ("d1".to_string(), AgentState::Working),
                ("d2".to_string(), AgentState::Working)
            ]
        );
        assert_eq!(roster.agents()[1].task, "second");
        assert_eq!(roster.agents()[1].kind, "reader");
    }

    #[test]
    fn a_finish_marks_the_agent_with_that_number_whatever_order_they_end_in() {
        let mut roster = Roster::new();
        started(&mut roster, 1, "first");
        started(&mut roster, 2, "second");
        assert_eq!(roster.finish(DelegateId::nth(2), false), Some(1));
        assert_eq!(
            states(&roster),
            [
                ("d1".to_string(), AgentState::Working),
                ("d2".to_string(), AgentState::Finished)
            ]
        );
        assert_eq!(roster.finish(DelegateId::nth(1), true), Some(0));
        assert_eq!(
            states(&roster),
            [
                ("d1".to_string(), AgentState::Failed),
                ("d2".to_string(), AgentState::Finished)
            ]
        );
    }

    #[test]
    fn a_number_used_again_finishes_the_latest_agent_still_working() {
        let mut roster = Roster::new();
        started(&mut roster, 1, "earlier turn");
        assert_eq!(roster.finish(DelegateId::nth(1), false), Some(0));
        started(&mut roster, 1, "later turn");
        assert_eq!(roster.finish(DelegateId::nth(1), true), Some(1));
        assert_eq!(
            states(&roster),
            [
                ("d1".to_string(), AgentState::Finished),
                ("d1".to_string(), AgentState::Failed)
            ]
        );
    }

    #[test]
    fn a_finish_for_a_number_nothing_is_working_under_changes_nothing() {
        let mut roster = Roster::new();
        started(&mut roster, 1, "first");
        roster.finish(DelegateId::nth(1), false);
        let before = roster.clone();
        assert_eq!(roster.finish(DelegateId::nth(1), true), None);
        assert_eq!(roster.finish(DelegateId::nth(9), true), None);
        assert_eq!(roster, before);
    }

    #[test]
    fn a_turn_ending_marks_every_agent_still_working_as_failed() {
        let mut roster = Roster::new();
        started(&mut roster, 1, "ended");
        started(&mut roster, 2, "still working");
        started(&mut roster, 3, "also still working");
        roster.finish(DelegateId::nth(1), false);
        assert_eq!(roster.turn_ended(), [1, 2]);
        assert_eq!(
            states(&roster),
            [
                ("d1".to_string(), AgentState::Finished),
                ("d2".to_string(), AgentState::Failed),
                ("d3".to_string(), AgentState::Failed)
            ]
        );
        assert!(roster.turn_ended().is_empty());
    }

    #[test]
    fn the_counts_are_the_agents_started_and_the_ones_working() {
        let mut roster = Roster::new();
        assert_eq!(roster.counts(), Counts::default());
        started(&mut roster, 1, "a");
        started(&mut roster, 2, "b");
        started(&mut roster, 3, "c");
        roster.finish(DelegateId::nth(2), false);
        assert_eq!(
            roster.counts(),
            Counts {
                started: 3,
                working: 2
            }
        );
        roster.turn_ended();
        assert_eq!(
            roster.counts(),
            Counts {
                started: 3,
                working: 0
            }
        );
    }

    #[test]
    fn truncating_and_clearing_drop_the_agents_after_a_point() {
        let mut roster = Roster::new();
        started(&mut roster, 1, "a");
        started(&mut roster, 2, "b");
        roster.truncate(1);
        assert_eq!(roster.counts().started, 1);
        roster.clear();
        assert_eq!(roster.counts(), Counts::default());
    }
}
