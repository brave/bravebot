//! When a recap is owed to a person who has been away ([CMD-20](../../../docs/specs/commands.md#CMD-20)).
//!
//! Only what the interface itself saw is read here: the clock, whether the terminal reported being
//! left, and how many turns there have been. Nothing from the conversation reaches the decision,
//! so there is nothing untrusted to branch on.

use std::time::{Duration, Instant};

/// How long after the last completed turn, with the terminal left, a recap is owed.
pub const IDLE: Duration = Duration::from_secs(5 * 60);

/// The fewest turns a session has to have had for a recap to be worth the request.
pub const FEWEST_TURNS: usize = 3;

/// What the interface knows about the person's absence.
#[derive(Debug, Clone)]
pub struct Away {
    /// Whether the settings leave the automatic recap on.
    enabled: bool,
    /// Whether the terminal is believed to be in front of the person. True until it reports
    /// otherwise, so a terminal that reports no focus changes never recaps by itself.
    focused: bool,
    /// When the last turn completed.
    last_turn_done: Option<Instant>,
    /// Whether a recap has been asked for since the last turn completed.
    recapped_since: bool,
}

impl Default for Away {
    fn default() -> Self {
        Self {
            enabled: true,
            focused: true,
            last_turn_done: None,
            recapped_since: false,
        }
    }
}

impl Away {
    /// Take what the settings said. Only a real `false` turns it off.
    pub fn adopt(&mut self, setting: Option<bool>) {
        self.enabled = setting.unwrap_or(true);
    }

    /// The terminal was left or came back to.
    pub fn focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// A turn completed at `now`, which makes the next recap allowed again.
    pub fn turn_done(&mut self, now: Instant) {
        self.last_turn_done = Some(now);
        self.recapped_since = false;
    }

    /// A recap was asked for, by the person or by [`Away::due`].
    pub fn recapped(&mut self) {
        self.recapped_since = true;
    }

    /// Whether to recap now, for a session that has had `turns` turns.
    pub fn due(&self, now: Instant, turns: usize) -> bool {
        self.enabled
            && !self.focused
            && !self.recapped_since
            && turns >= FEWEST_TURNS
            && self
                .last_turn_done
                .is_some_and(|done| now.saturating_duration_since(done) >= IDLE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An away session in which every condition holds, for a test to take one away.
    fn owed() -> (Away, Instant) {
        let start = Instant::now();
        let mut away = Away::default();
        away.turn_done(start);
        away.focus(false);
        (away, start + IDLE)
    }

    #[test]
    fn a_recap_is_due_once_every_condition_holds() {
        let (away, now) = owed();
        assert!(away.due(now, FEWEST_TURNS));
    }

    #[test]
    fn a_recap_waits_for_the_idle_time() {
        let (away, now) = owed();
        assert!(!away.due(now - Duration::from_secs(1), FEWEST_TURNS));
    }

    #[test]
    fn a_recap_waits_for_the_terminal_to_be_left() {
        let (mut away, now) = owed();
        away.focus(true);
        assert!(!away.due(now, FEWEST_TURNS));
    }

    #[test]
    fn a_recap_waits_for_enough_turns() {
        let (away, now) = owed();
        assert!(!away.due(now, FEWEST_TURNS - 1));
    }

    #[test]
    fn a_recap_is_not_made_twice_in_a_row() {
        let (mut away, now) = owed();
        away.recapped();
        assert!(!away.due(now + IDLE, FEWEST_TURNS));
        away.turn_done(now);
        assert!(away.due(now + IDLE, FEWEST_TURNS));
    }

    #[test]
    fn a_recap_is_not_made_where_the_setting_turned_it_off() {
        let (mut away, now) = owed();
        away.adopt(Some(false));
        assert!(!away.due(now, FEWEST_TURNS));
        away.adopt(None);
        assert!(away.due(now, FEWEST_TURNS));
    }

    #[test]
    fn a_session_with_no_completed_turn_owes_nothing() {
        let mut away = Away::default();
        away.focus(false);
        assert!(!away.due(Instant::now() + IDLE * 10, FEWEST_TURNS));
    }
}
