//! The Sessions screen as a mode of the session: opened over the plain prompt, taking every
//! key until it is left (SCREEN-1).

use super::{Session, Status};
use crate::sessions_screen::SessionsScreen;

impl Session {
    /// The Sessions screen, while it is open.
    pub fn sessions_screen(&self) -> Option<&SessionsScreen> {
        self.sessions_screen.as_ref()
    }

    pub fn sessions_screen_mut(&mut self) -> Option<&mut SessionsScreen> {
        self.sessions_screen.as_mut()
    }

    /// Whether the Sessions screen is what is on the screen and taking the keys.
    pub fn showing_sessions(&self) -> bool {
        self.sessions_screen.is_some()
    }

    /// Whether a press of Left, which has nowhere further left to go, opens the Sessions screen
    /// (SCREEN-1).
    ///
    /// Only with nothing typed: a line with something in it is the caret's, and so is a list of
    /// commands or files being offered, a command for the shell, a brief waiting to be sent, a
    /// half-typed vi instruction, and a run of keys that arrived as one paste. At rest and while a
    /// turn runs, and not while a command the person typed runs, which has the screen to itself.
    pub fn left_opens_the_sessions_screen(&self) -> bool {
        matches!(self.status, Status::Idle | Status::Working)
            && self.sessions_screen.is_none()
            && self.key_arrived_alone
            && self.input().is_empty()
            && !self.shell
            && !self.is_completing()
            && !self.handoff_pending()
            && !self.enhanced_pending()
            && self.half_typed().is_none()
    }

    /// Show the Sessions screen over the session. The transcript and the box are kept as they were and come
    /// back when it is left.
    pub fn open_sessions_screen(&mut self, mut sessions_screen: SessionsScreen) {
        sessions_screen.hide_these(std::mem::take(&mut self.sessions_screen_hidden));
        // The keys a settings file moved, and what it asked for that could not be (SCREEN-14).
        let (keys, refused) = self.sessions_keys();
        let mut said = refused.iter().map(|refused| keys.sentence(refused));
        let first = said.next();
        let more = said.count();
        let keys = keys.clone();
        sessions_screen = sessions_screen.with_keys(keys);
        if let Some(first) = first {
            sessions_screen.say(match more {
                0 => first,
                more => format!(
                    "{first}; {}",
                    bravebot_i18n::t!(sessions_screen_keys_more, count = more)
                ),
            });
        }
        self.sessions_screen = Some(sessions_screen);
    }

    /// Keep what the Sessions screen was asked for over a running turn until the turn has ended
    /// (SCREEN-15).
    pub fn hold_for_the_end_of_the_turn(&mut self, effect: crate::sessions_screen::Effect) {
        self.after_the_turn = Some(effect);
    }

    /// What was kept for the end of the turn, once.
    pub fn take_held_for_the_end_of_the_turn(&mut self) -> Option<crate::sessions_screen::Effect> {
        self.after_the_turn.take()
    }

    /// Leave the Sessions screen. `false` where it was not open. What was hidden in it stays hidden in the
    /// next one this session opens (SCREEN-10).
    pub fn close_sessions_screen(&mut self) -> bool {
        match self.sessions_screen.take() {
            Some(mut sessions_screen) => {
                self.sessions_screen_hidden = sessions_screen.take_hidden();
                true
            }
            None => false,
        }
    }
}
