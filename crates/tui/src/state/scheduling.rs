//! `/schedule`: the shared crate's subcommands run for a line a person typed, and their answers
//! drawn as notes.

use super::{Session, t};
use bravebot_schedule::{InSession, Outcome, Reply, TypedLine};

impl Session {
    /// Run `/schedule <argument>`.
    ///
    /// Reached only from the dispatch of a line a person typed into the box (CMD-1), which is why
    /// [`TypedLine::of_a_person`] is made here and nowhere a tool call can reach. The session's
    /// sandbox mode is not an input: an entry added here holds read mode whatever mode this
    /// session is in.
    pub(crate) fn schedule_asked(&mut self, argument: &str) {
        let project = if self.workspace.as_os_str().is_empty() {
            match std::env::current_dir() {
                Ok(directory) => directory,
                Err(_) => {
                    self.note_refused(t!(cli_directory_unknown));
                    return;
                }
            }
        } else {
            self.workspace.clone()
        };
        match bravebot_schedule::typed_in_session(
            argument,
            TypedLine::of_a_person(),
            &project,
            self.schedule,
        ) {
            InSession::Ended(reply) => self.show_schedule_reply(&reply),
            InSession::Running(receiver) => {
                self.schedule_runs.push(receiver);
                self.note(t!(schedule_command_running));
            }
        }
    }

    /// Take the answer of every `run-now` that has one, and show it. `true` where one was shown, so
    /// the pass draws.
    pub fn settle_schedule_runs(&mut self) -> bool {
        let mut shown = Vec::new();
        self.schedule_runs
            .retain(|receiver| match receiver.try_recv() {
                Ok(reply) => {
                    shown.push(reply);
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                // The thread ended without answering: nothing more will come.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
            });
        let any = !shown.is_empty();
        for reply in &shown {
            self.show_schedule_reply(reply);
        }
        any
    }

    /// How many `run-now` runs are waiting for an answer.
    pub fn schedule_runs_waiting(&self) -> usize {
        self.schedule_runs.len()
    }

    /// Draw a reply as one note.
    ///
    /// A system note, so it is on the screen and never in the messages the planner is sent: a
    /// note is a transcript entry of the system speaker, and a turn is built from the prompt a
    /// person sent. The lines are shown only to the person who typed the command and are not drawn
    /// behind the quarantine margin (SCHEDULE-8).
    /// Every line goes through [`crate::render::printable`], because a line can carry text a
    /// program chose, and a control character in it would move the cursor or rewrite the screen.
    /// The reply's own outcome decides how the note is drawn; no line is read to decide it. Every
    /// reply `bravebot_schedule` returns holds at least one line (a listing with nothing in it says
    /// so), so the guard on an empty one is only a defence against drawing an empty note.
    fn show_schedule_reply(&mut self, reply: &Reply) {
        let lines: Vec<String> = reply
            .lines()
            .iter()
            .map(|line| crate::render::printable(line))
            .collect();
        if lines.is_empty() {
            return;
        }
        let text = lines.join("\n");
        match reply.outcome {
            Outcome::Done => self.note(text),
            Outcome::Argument | Outcome::Failed | Outcome::Refused => self.note_refused(text),
        }
    }
}
