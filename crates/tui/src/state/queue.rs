//! The lines typed while a turn runs, and the queue that holds them until it is over.

use super::attachments::previews_restaged;
#[cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "a child of state.rs, which holds Session and the types every sibling shares"
    )
)]
use super::*;

/// How long the turn's command runs before a prompt waiting for the turn moves it to the background.
///
/// Long enough that a command about to finish returns its output to the turn instead of a job to
/// ask about.
pub(super) const GIVE_WAY_AFTER: Duration = Duration::from_secs(10);

/// A line typed while a turn was running, waiting for it to end.
///
/// Settled when it was queued rather than when it is sent, because what it names is what the box
/// held at that moment. A file the user took off the line afterwards was never part of this
/// prompt, and one they added belongs to whatever they type next.
#[derive(Debug, Clone)]
pub struct Queued {
    /// The line, as it was typed.
    pub prompt: String,
    /// Files it named, settled at the moment it was queued.
    attached: Vec<Attached>,
    /// Pictures it named, settled at the same moment and for the same reason.
    pasted: Vec<AttachedImage>,
    /// What the line is waiting to become.
    ///
    /// Decided by the caller, since which words are commands and which mode the box was in are
    /// the input box's to know and not this type's.
    waiting: Waiting,
    /// The history entry the line made, for a line that was typed. A line a click queued was not
    /// typed and leaves none.
    recall: Option<crate::history::Ticket>,
    /// Whether Ctrl-Enter asked for this prompt to go now, with the others it asked for.
    ///
    /// On the line rather than the session, because what the press asked for is the lines waiting
    /// when it was made: one typed after it was not part of that, and one taken back with Up is no
    /// longer anywhere to be sent.
    hurried: bool,
    /// When the line was queued, which is when the person typed it.
    ///
    /// Carried to the command it becomes so that a delay the line names counts from here and not
    /// from the moment the turn ends and the queue reaches it ([LOOP-18](../../../../docs/specs/loop.md#LOOP-18)).
    typed_at: std::time::Instant,
}

impl Queued {
    /// Whether the line was typed, which a line a click on the info panel queued was not
    /// (MOUSE-29).
    fn was_typed(&self) -> bool {
        self.recall.is_some()
    }
}

/// Where a waiting line is going, which is what its row under the box says.
///
/// What a person watching a turn needs before deciding whether to stop it: whether the line they
/// sent is about to be read, is waiting for the turn to end, or is carried out rather than sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// A prompt the turn in flight takes at its next round boundary.
    IntoThisTurn,
    /// A prompt that waits for what is running to end and then becomes a turn of its own.
    ItsOwnTurn,
    /// A prompt behind one of those, which the turn that one begins takes at its first round
    /// boundary, since only the prompt that begins a turn leaves the buffer the turn reads.
    IntoTheNextTurn,
    /// A command, carried out when the queue reaches it.
    CarriedOut,
    /// A command line, run through the shell when the queue reaches it.
    ///
    /// Apart from [`Bound::CarriedOut`] because the two go to different places, and a command line
    /// drawn as anything else would read as words on their way to the model.
    Run,
}

/// What a queued line is waiting to become when the turn in flight ends.
///
/// Two of the three are never offered to the turn, because the only thing that could do with a
/// line there is the planner, and neither a command nor a command line is something the planner is
/// asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    /// A prompt: offered to the turn in flight as an interjection, and a turn of its own if the
    /// turn ends before it is taken.
    Prompt,
    /// A command: carried out when the queue reaches it, exactly as Enter on it at rest would
    /// have carried it out.
    Command,
    /// A command line, typed with shell mode armed: run through the shell when the queue reaches
    /// it, exactly as Enter on it at rest would have run it.
    Shell,
    /// A command line typed `!!`: run like [`Waiting::Shell`], with its output drawn and never
    /// recorded.
    ShellPrivate,
}

impl Waiting {
    /// Whether the line is one the planner may be given.
    fn is_sent(self) -> bool {
        self == Waiting::Prompt
    }
}

impl Session {
    /// Move the turn's command in flight to the background for a prompt waiting to go into the
    /// turn, once the command has run for [`GIVE_WAY_AFTER`].
    ///
    /// The turn takes a waiting prompt only between rounds, and a round lasts as long as its
    /// slowest command, which can be minutes. Nothing happens for a command or a command line
    /// waiting, since those wait for the turn to end, nor for a turn being stopped, which takes
    /// nothing more.
    ///
    /// Whether a command was moved.
    pub fn give_way_to_the_queue(&mut self, now: Instant) -> bool {
        let waiting = self.a_turn_is_running()
            && !self.stopping
            && self.queued.iter().any(|line| line.waiting.is_sent());
        match self.movable {
            Some((_, since))
                if waiting && now.saturating_duration_since(since) >= GIVE_WAY_AFTER =>
            {
                self.move_to_background()
            }
            _ => false,
        }
    }

    /// Take the current line as a prompt to send when the turn in flight has finished.
    ///
    /// The line leaves the box exactly as it would on sending, and is remembered in the history
    /// the same way, because from the person's side they have sent it. What has not happened yet
    /// is the turn, so nothing goes into the transcript until this one's own turn begins.
    ///
    /// Only while a turn is running. With none there is nothing to wait for and
    /// [`Session::submit`] is what Enter means.
    pub fn queue(&mut self) -> bool {
        self.queue_line(Waiting::Prompt)
    }

    /// Take the current line as a command to carry out when the turn in flight has finished.
    ///
    /// The same wait a prompt gets, and for the same reason: the person pressed Enter, so the line
    /// leaves the box and is remembered, and what has not happened yet is the turn. The difference is
    /// at the far end. A prompt that was waiting becomes a turn of its own; a command that was waiting
    /// is dispatched, which is what pressing Enter on it at rest would have done.
    ///
    /// Which words are commands is the caller's to say. This only promises what a line marked one is
    /// spared: it is never put where the running turn can reach it, so nothing about it is sent
    /// anywhere, and nothing about it is in the conversation while it waits.
    pub fn queue_command(&mut self) -> bool {
        self.queue_line(Waiting::Command)
    }

    /// The lines waiting ahead of position `at`, as they were typed and in that order, with where
    /// each is going. `at` is a line's own position, or the length of the queue for one about to join it.
    pub fn queued_ahead(&self, at: usize) -> impl Iterator<Item = (Bound, &str)> {
        self.queued
            .iter()
            .take(at)
            .enumerate()
            .map(|(position, line)| (self.where_it_goes(position), line.prompt.as_str()))
    }

    /// The commands waiting for the turn to end, as they were typed and in that order.
    pub fn commands_waiting(&self) -> impl Iterator<Item = &str> {
        self.queued
            .iter()
            .filter(|line| line.waiting == Waiting::Command)
            .map(|line| line.prompt.as_str())
    }

    /// Move the time every waiting line was typed back by `by`, for a test that needs a turn to have
    /// run longer than the real clock can be made to.
    #[cfg(test)]
    pub(crate) fn backdate_queued(&mut self, by: Duration) {
        for line in &mut self.queued {
            line.typed_at = line
                .typed_at
                .checked_sub(by)
                .expect("the clock reaches back that far");
        }
    }

    /// Take the current line as a command line to run when the turn in flight has finished.
    ///
    /// The same wait the other two get, for the same reason, and what is different is again at the
    /// far end: this line is run through the shell, which is what pressing Enter on it at rest
    /// would have done with it.
    ///
    /// Shell mode is how a person said the line was for a shell, and a turn running changes
    /// nothing about that: a turn can begin without anybody pressing anything, from a loop's tick
    /// or a watch firing, so the mode is armed mid-turn over a line that was typed to be run. What
    /// this may never do is hand it to the turn, since the planner would then be sent a command
    /// line as a sentence somebody said.
    ///
    /// Leaves shell mode, exactly as running one at rest does: the mode lasts one command, and one
    /// still armed over an emptied box would claim whatever is typed next.
    pub fn queue_shell(&mut self) -> bool {
        let waiting = match self.shell_private() {
            true => Waiting::ShellPrivate,
            false => Waiting::Shell,
        };
        if !self.shell || !self.queue_line(waiting) {
            return false;
        }
        self.shell = false;
        true
    }

    fn queue_line(&mut self, waiting: Waiting) -> bool {
        if self.status != Status::Working {
            return false;
        }
        let typed = self.input.trim().to_string();
        if typed.is_empty() {
            return false;
        }
        // Resolved before the line is taken, because taking it clears what the markers stand for.
        let resolved = self.resolved(&typed);
        let prompt = self.unfolded(&typed);
        let recall = self.remember(&prompt);
        let (attached, pasted) = self.take_line(&typed);
        // Into the turn's reach the moment it is typed, rather than when the turn next asks. The
        // turn asks between rounds and a round can be a long wait; put there now, the line is
        // taken at the first boundary after it was sent, which is the soonest anything could act
        // on it.
        //
        // The resolved copy lives only there. What is kept here is what the person typed, because
        // that is what the screen and the history are for, and a second copy of the resolved line
        // would be one for the two to disagree over.
        //
        // A prompt and nothing else. That buffer is the one thing that reaches the planner from
        // here, and handing it a command is how one used to be answered as a question about
        // itself, while handing it a command line is how `echo pwned` used to reach the planner as
        // something a person had said. Both wait in the queue below and nowhere else.
        if waiting.is_sent() {
            // Behind a line held back from the turn in flight, the buffer holds it too: the turn
            // taking it ahead of the older one would send what was typed last first.
            self.pending.push_behind(resolved);
        }
        self.queued.push(Queued {
            prompt,
            attached,
            pasted,
            waiting,
            recall: Some(recall),
            hurried: false,
            typed_at: std::time::Instant::now(),
        });
        self.scroll = 0;
        true
    }

    /// Queue `line` as a command to carry out when the turn in flight has finished, for a click
    /// that stands for the command ([MOUSE-29](../../../docs/specs/mouse.md#MOUSE-29)).
    ///
    /// The line a person typed would be taken off the box and remembered. This one was never in the
    /// box, which is left as it is, and was not typed, so the history is too. Everything else about
    /// the wait is the same: where it goes, what it waits behind and how it is carried out are read
    /// from the queue and not from how the line got there. While the session is working, as
    /// [`Session::queue_command`] requires, and while a command the person typed is running, which
    /// the queue is carried out after as it is after a turn (MOUSE-29).
    pub fn queue_command_line(&mut self, line: &str) -> bool {
        if !matches!(self.status, Status::Working | Status::Running) {
            return false;
        }
        self.queued.push(Queued {
            prompt: line.to_string(),
            attached: Vec::new(),
            pasted: Vec::new(),
            waiting: Waiting::Command,
            recall: None,
            hurried: false,
            typed_at: std::time::Instant::now(),
        });
        self.scroll = 0;
        true
    }

    /// Ask for every prompt waiting to go now, as one turn, once the turn in flight has stopped.
    ///
    /// `true` where there is a turn to stop and anything waiting behind it, which is when the caller
    /// stops it. A command or a command line waiting counts, since stopping the turn is what lets the
    /// queue reach it, but only a prompt is marked: only a prompt is sent, and the others are carried
    /// out one at a time as they always are.
    pub fn hurry(&mut self) -> bool {
        if !self.a_turn_is_running() || self.queued.is_empty() {
            return false;
        }
        for line in &mut self.queued {
            line.hurried |= line.waiting.is_sent();
        }
        true
    }

    /// Whether Ctrl-Enter would stop the turn in flight and send what is waiting, which is when the
    /// row under the queue says so.
    ///
    /// Not once a stop has been asked for: the queue goes when the turn has stopped all the same, so
    /// the key would stop a turn already stopping.
    pub fn offers_to_send_now(&self) -> bool {
        self.ctrl_enter_arrives
            && self.a_turn_is_running()
            && !self.stopping
            && !self.queued.is_empty()
    }

    /// Where the line waiting at `at` is going, for the row under the box to say.
    ///
    /// A prompt goes into the turn in flight only where that turn will ask for it. An aside is
    /// handed a queue of its own and takes nothing, and a turn being stopped takes nothing at the
    /// boundary it stops at, so the first prompt waiting behind either becomes a turn of its own and
    /// the ones after it go into that turn.
    pub fn where_it_goes(&self, at: usize) -> Bound {
        match self.queued[at].waiting {
            Waiting::Command => Bound::CarriedOut,
            Waiting::Shell | Waiting::ShellPrivate => Bound::Run,
            Waiting::Prompt if self.a_turn_is_running() && !self.stopping && !self.is_held(at) => {
                Bound::IntoThisTurn
            }
            Waiting::Prompt if self.queued[..at].iter().any(|line| line.waiting.is_sent()) => {
                Bound::IntoTheNextTurn
            }
            Waiting::Prompt => Bound::ItsOwnTurn,
        }
    }

    /// Whether the prompt waiting at `at` is one whose copy is held back from the turn in flight.
    fn is_held(&self, at: usize) -> bool {
        let sent = |line: &&Queued| line.waiting.is_sent();
        let total = self.queued.iter().filter(sent).count();
        let before = self.queued[..at].iter().filter(sent).count();
        before >= total.saturating_sub(self.pending.held())
    }

    /// Record that the person asked for what is running to stop.
    ///
    /// At the press, rather than when the turn reports that it stopped, because a turn can take a
    /// while to get there and a prompt sent in between would otherwise be drawn as going into it.
    pub fn stop_asked(&mut self) {
        self.stopping = true;
    }

    /// The buffer the running turn takes interjections from.
    ///
    /// Handed to the worker when a turn starts. Cloning shares the buffer rather than copying it,
    /// which is the point: what the interface adds is what the turn takes.
    pub fn interjections(&self) -> crate::remote_confirm::Interjections {
        self.pending.clone()
    }

    /// Record that the prompt queued longest ago has reached the planner mid-turn.
    ///
    /// Called when the turn says it took one, rather than when the line was put where the turn
    /// could reach it: until it is taken the prompt is still waiting, and it belongs above the box
    /// where a waiting prompt is drawn. This is the moment it becomes part of the conversation, so
    /// this is the moment it joins the transcript, which reads in the order things happened.
    ///
    /// The oldest prompt rather than the oldest line, because neither a command nor a command line
    /// was ever offered to the turn: what the turn just took is the oldest line that had a copy in
    /// the buffer, and either of those queued ahead of it has one waiting there still.
    ///
    /// Nothing about the view moves for it, for the reason [`Session::back_to_the_tail`] gives: the
    /// prompt was sent rounds ago, and the turn taking it is the turn's doing rather than a press.
    pub fn interjected(&mut self) {
        let Some(taken) = self.queued.iter().position(|line| line.waiting.is_sent()) else {
            return;
        };
        let gone = self.queued.remove(taken);
        self.transcript.push(Entry::user(gone.prompt));
    }

    /// Begin the turn for the prompt queued longest ago, if the session is free to start one.
    ///
    /// What a prompt still waiting when the turn ended does, which is what every queued prompt used
    /// to do. It becomes a turn of its own, and as its own turn it gets what a turn gets: routing
    /// precommitted from it, and the files and pictures it named carried with it. That is why one
    /// left over is better off here than interjected, and why nothing but the person hurries it.
    ///
    /// Where they did, with Ctrl-Enter, the prompts it marked go together as one turn, one to a
    /// line, carrying everything each of them named: what Up and Enter would have sent, without the
    /// box in between. Only as far as the first line that is not one of them, so what was typed first
    /// still happens first.
    ///
    /// A command or a command line at the head of the queue stops this, rather than being sent: the
    /// queue is drained in the order it was typed, and [`Session::take_queued_command`] and
    /// [`Session::take_queued_shell`] are what take those.
    pub fn send_queued(&mut self) -> Option<String> {
        if self.status != Status::Idle
            || self
                .queued
                .first()
                .is_none_or(|next| !next.waiting.is_sent())
        {
            return None;
        }
        let together = self
            .queued
            .iter()
            .take_while(|line| line.waiting.is_sent() && line.hurried)
            .count()
            .max(1);
        let mut prompts = Vec::new();
        let mut attached = Vec::new();
        let mut pasted = Vec::new();
        let mut recall = Vec::new();
        let hurried = self.queued[0].hurried;
        for next in self.queued.drain(..together) {
            prompts.push(next.prompt);
            attached.extend(next.attached);
            pasted.extend(next.pasted);
            recall.extend(next.recall);
        }
        // The copies of the prompts going now leave the buffer: a copy still there would reach the
        // planner a second time, as an interjection into the very turn this line started. What
        // waits behind a hurried batch goes after whatever stops it, a command or a turn of its
        // own, so the turn begun now may not take it. Behind a prompt sent alone, the rest is
        // for that turn to take, as it always was.
        self.pending.begin_turn(together, hurried);
        // A line sent together with others is no longer a lone word, so only one prompt can be.
        let lone = matches!(
            prompts.as_slice(),
            [only] if crate::command_table::is_a_lone_leave_word(only)
        );
        let begun = self.begin_turn(prompts.join("\n"), (attached, pasted), recall);
        self.leave_word_sent = lone;
        Some(begun)
    }

    /// Take the command waiting longest, if the session is free to carry one out.
    ///
    /// The line as it was typed, with everything it named, for the caller to dispatch exactly as it
    /// dispatches one typed at rest: waiting changes nothing about what a command can carry, so the
    /// same caller settles the same markers for the same commands. Nothing here decides what any
    /// command does and nothing here is written down: a command is not part of the conversation, and
    /// it was not while it waited either.
    ///
    /// Only from the head of the queue, so the order somebody typed things in is the order they
    /// happen in: a command behind a prompt waits for that prompt's turn, the same way the prompt
    /// waited for the turn that was running when it was typed.
    pub fn take_queued_command(&mut self) -> Option<Commanded> {
        if self.status != Status::Idle || self.queued.first()?.waiting != Waiting::Command {
            return None;
        }
        let taken = self.queued.remove(0);
        Some(Commanded {
            line: taken.prompt,
            pasted: taken.pasted,
            attached: taken.attached,
            typed_at: taken.typed_at,
        })
    }

    /// Take the command line waiting longest, if the session is free to run one.
    ///
    /// The line as it was typed, for the caller to run exactly as it runs one typed at rest. It
    /// joins the transcript here, the way one submitted at rest joins it as it is submitted: this
    /// is the moment it stops waiting and starts being run, and nothing about it was in the
    /// transcript while it waited.
    ///
    /// Only from the head of the queue, for the reason [`Session::take_queued_command`] takes only
    /// from there: what somebody typed first happens first.
    pub fn take_queued_shell(&mut self) -> Option<String> {
        self.take_queued_shell_of(Waiting::Shell)
    }

    /// Take the command line waiting longest, as [`Session::take_queued_shell`] does, where it was
    /// typed `!!` and so is run with its output kept out of the conversation.
    pub fn take_queued_private_shell(&mut self) -> Option<String> {
        self.take_queued_shell_of(Waiting::ShellPrivate)
    }

    fn take_queued_shell_of(&mut self, waiting: Waiting) -> Option<String> {
        if self.status != Status::Idle || self.queued.first()?.waiting != waiting {
            return None;
        }
        let line = self.queued.remove(0).prompt;
        self.transcript.push(Entry::shell(line.clone()));
        // Through [`Session::back_to_the_tail`], so an open view stays where its reader put it. The
        // queue giving this line up is the turn ending rather than a press: the line was typed
        // rounds ago and nothing closes a view when a turn ends.
        self.back_to_the_tail();
        Some(line)
    }

    /// Take every waiting prompt back out of the queue and into the box.
    ///
    /// All of them, in the order they were typed, one to a line, and the half-written line already
    /// in the box stays below them: it was typed after they were, and it is where the caret goes,
    /// so somebody carries on where they left off. What each of them named comes back with it, the
    /// way a stashed line's markers do, since a marker with nothing behind it would send a prompt
    /// pointing at a file that is no longer staged.
    ///
    /// The prompts stay in the history. From the person's side they were sent, and taking them back
    /// does not unsay them.
    pub fn unqueue(&mut self) -> bool {
        self.unqueue_from(0)
    }

    /// Take the waiting prompt at `first` back out of the queue and into the box, and every prompt
    /// typed after it (MOUSE-28).
    ///
    /// What the key does with the whole queue, stopped at the line pointed to: the queue gives up its
    /// lines from the end the person types at, so a prompt has gone back only once the ones after it
    /// have, and the order they were typed in is the order they come back in. A click on the oldest
    /// is the key. Says whether anything came back, which it does not where nothing waits at `first`
    /// or the turn has taken the newest line already.
    pub fn unqueue_from(&mut self, first: usize) -> bool {
        if self.queued.len() <= first {
            return false;
        }
        // From the back, which is the end this key takes from, and only as far as the turn has not
        // already reached. A prompt comes back only if its copy was still there to drop: a line the
        // planner has been given cannot be unsaid, and offering it back to the box would leave the
        // person editing a prompt that had already gone. The two threads move independently, so a
        // line can be taken between the key press and this loop.
        //
        // Before anything is disturbed, so that finding nothing left to take leaves the box exactly
        // as it was rather than half rewritten.
        //
        // A command and a command line come back whatever the turn has reached, because there is
        // nothing to take back from: neither was ever offered to the turn, so no copy of either is
        // anywhere for the planner to have been given. What makes a prompt unreclaimable is that it
        // has already gone.
        //
        // A line a click queued does not come back at all. Nobody typed it, so the box would hold a
        // command the person never wrote, and Enter on it would send the line to the planner as
        // words. It keeps its place among what is left and is carried out in its turn.
        let mut reclaimed = Vec::new();
        let mut stays = Vec::new();
        while self.queued.len() > first
            && let Some((waiting, typed)) = self
                .queued
                .last()
                .map(|line| (line.waiting, line.was_typed()))
        {
            if typed && waiting.is_sent() && !self.pending.forget_last() {
                break;
            }
            let line = self.queued.pop().expect("the queue was not empty");
            match typed {
                true => reclaimed.push(line),
                false => stays.push(line),
            }
        }
        // Taken from the end, so the order they waited in is the reverse of the order they were taken.
        self.queued.extend(stays.into_iter().rev());
        if reclaimed.is_empty() {
            return false;
        }

        // The box is about to be rewritten, so the things standing over it that belong to the line
        // it held go, exactly as they do for anything else that writes a whole line.
        self.history.leave();
        self.completion = 0;

        let mut lines = Vec::new();
        let mut attached = Vec::new();
        let mut pasted = Vec::new();
        for waiting in reclaimed.into_iter().rev() {
            lines.push(waiting.prompt);
            attached.extend(waiting.attached);
            pasted.extend(waiting.pasted);
        }
        if !self.input.is_empty() {
            lines.push(std::mem::take(&mut self.input));
        }
        // Only what the reclaimed lines named: what the box already held still has its own.
        let restaged = previews_restaged(&pasted, &attached);
        self.previews.extend(restaged);
        attached.append(&mut self.attached);
        self.attached = attached;
        pasted.append(&mut self.pasted);
        self.pasted = pasted;
        self.set_input(lines.join("\n"));
        true
    }
}
