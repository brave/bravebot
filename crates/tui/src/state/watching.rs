//! Watching a delegate or a job, and the rows that list them.

use super::*;

/// The delegate view, while it is open.
///
/// Two levels rather than one, because a turn has one delegate or nine. The list is the way in
/// where there are several, and one delegate's own lines are what somebody came to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watching {
    /// Which row this is about, by its place in the list: the delegates in the order they were
    /// spawned, then the commands in the order they ran.
    ///
    /// A position rather than a name, because it is also where the highlight sits in the list, and
    /// the two must not be able to disagree. What that costs is that a row arriving ahead of this
    /// one moves it, so whatever inserts the row moves this with it: a position left where it was
    /// is a different row from the one somebody opened.
    pub at: usize,
    /// Whether the list is what is on the screen, rather than the row at `at`.
    pub listing: bool,
    /// Whether the list's highlight is on the session rather than on one of the rows.
    ///
    /// Kept beside `at` rather than folded into it so that `at` stays the row somebody was last
    /// reading: coming back into the list puts the highlight where they left it, and the way out
    /// is a row rather than a position nothing else can name.
    pub on_session: bool,
}

impl Session {
    /// Whose work the reports that follow are, as the driver said.
    ///
    /// The one thing that decides where a line lands. Delegates work alongside the turn and
    /// alongside each other, so the lines arrive interleaved: nothing here reads a line to work
    /// out whose it was, because a line is prose a model had a hand in.
    pub fn reporting_for(&mut self, delegate: Option<bravebot_agent::report::DelegateId>) {
        self.attributed_to = delegate;
    }

    /// Where a report lands: under the delegate whose work it is, or in the turn's transcript.
    ///
    /// A slice rather than the vector, so nothing can add a line here. A delegate's lines are
    /// bounded, and the bound is enforced where a line enters rather than by every caller
    /// remembering: [`Session::hold`] is that entrance.
    pub(super) fn working_lines(&mut self) -> &mut [Entry] {
        match self.attributed_at() {
            Some(at) => &mut self.delegate_mut(at).lines,
            None => &mut self.transcript,
        }
    }

    /// Where the delegate the current report is attributed to has its block, if there is one.
    pub(super) fn attributed_at(&self) -> Option<usize> {
        self.attributed_to.and_then(|id| self.at(id))
    }

    /// The delegate held by the transcript entry at `at`, a position [`Session::at`] returned.
    ///
    /// Takes a position rather than returning an option, because a caller that falls back to the
    /// transcript in the `None` arm cannot hold the borrow this would return.
    pub(super) fn delegate_mut(&mut self, at: usize) -> &mut Delegate {
        self.transcript[at]
            .delegate
            .as_mut()
            .expect("a delegate entry holds its delegate")
    }

    /// The last tool call of the current block that satisfies `keep`.
    ///
    /// Looks only through the trailing run of delegate and tool entries, so a call from before the
    /// latest thing the person or the session said is never the one a report lands on.
    pub(super) fn call_where(&mut self, keep: impl Fn(&Entry) -> bool) -> Option<&mut Entry> {
        self.working_lines()
            .iter_mut()
            .rev()
            .take_while(|entry| {
                entry.speaker == Speaker::Delegate || entry.speaker == Speaker::Tool
            })
            .find(|entry| entry.speaker == Speaker::Tool && keep(entry))
    }

    /// Hold a line that is not a call: under the delegate whose work it is, to the same bound its
    /// calls are held to, or in the turn's transcript.
    ///
    /// Not [`Session::working_lines`] and a push, which reaches the delegate's lines around the
    /// only thing that bounds them.
    pub(super) fn hold(&mut self, entry: Entry) {
        match self.attributed_at() {
            Some(at) => self.delegate_mut(at).hold(entry),
            None => self.transcript.push(entry),
        }
    }

    /// Where a delegate's block is, by the number the driver gave it.
    ///
    /// Searched from the end, because the one being reported on is almost always the one most
    /// recently started.
    pub(super) fn at(&self, id: bravebot_agent::report::DelegateId) -> Option<usize> {
        self.transcript
            .iter()
            .rposition(|entry| entry.delegate.as_ref().is_some_and(|held| held.id == id))
    }

    /// A delegate has begun, drawn where the call that started it happened.
    ///
    /// Nothing about the view moves for it. Where the view is open, the row it is on is a place
    /// in a list this inserts a row into, so a place at or after the new delegate's moves with
    /// it: without that, a delegate starting takes the screen from somebody reading a command,
    /// since the commands are listed after the delegates.
    pub fn delegate_started(&mut self, delegation: bravebot_agent::report::Delegation) {
        let inserted = self.asides.len() + self.delegates().len();
        if let Some(watching) = &mut self.watching
            && watching.at >= inserted
        {
            watching.at += 1;
        }
        self.transcript.push(Entry::delegate(Delegate {
            id: delegation.id,
            kind: delegation.kind,
            task: delegation.task,
            lines: Vec::new(),
            calls: 0,
            note: None,
            reported: None,
            failed: false,
            stop: delegation.stop,
        }));
    }

    /// One delegate has finished, with what the turn was told about it and what it reported.
    ///
    /// Its block collapses to those two: what it did is behind it, and what it concluded is the
    /// whole of what anybody acts on. Named rather than taken to be whichever was working,
    /// because the one that finishes is not the one that started last.
    pub fn delegate_finished(
        &mut self,
        id: bravebot_agent::report::DelegateId,
        note: String,
        failed: bool,
        reported: Option<Reported>,
    ) {
        if self.attributed_to == Some(id) {
            self.attributed_to = None;
        }
        if self.waiting_on == Some(id) {
            self.waiting_on = None;
        }
        if let Some(at) = self.at(id)
            && let Some(delegate) = self.transcript[at].delegate.as_mut()
        {
            delegate.note = Some(note);
            delegate.reported = reported;
            delegate.failed = failed;
        }
    }

    /// Every delegate this session has started, oldest first.
    pub fn delegates(&self) -> Vec<&Delegate> {
        self.transcript
            .iter()
            .filter_map(|entry| entry.delegate.as_ref())
            .collect()
    }

    /// A background job started, ended, or is being stopped with its turn (RUN-26).
    ///
    /// Its row is made when it starts, with nothing printed yet, and every later event changes that
    /// row. Whose job it is comes from the driver's attribution, never from anything in the event.
    pub fn job(&mut self, event: bravebot_agent::report::JobEvent) {
        use bravebot_agent::report::JobEvent;
        match event {
            JobEvent::Started {
                name,
                line,
                moved_after,
                stop,
            } => self.outputs.push(Output {
                command: line,
                lines: Vec::new(),
                total: 0,
                // True so the first look's standing is the row's: looks are merged with `&=`.
                read_by_the_planner: true,
                outcome: bravebot_agent::report::Outcome::Running {
                    ran_for: moved_after.unwrap_or_default(),
                    waited: None,
                },
                job: Some(JobView {
                    name,
                    delegate: self.attributed_to,
                    since: Instant::now(),
                    moved_after,
                    state: JobState::Running,
                    stop,
                }),
            }),
            JobEvent::Ended { name, outcome } => {
                if let Some(row) = self.job_row(&name) {
                    row.outcome = outcome;
                    if let Some(job) = row.job.as_mut() {
                        job.state = JobState::Ended;
                    }
                }
            }
            JobEvent::Dropped { name } => {
                if let Some(job) = self.job_row(&name).and_then(|row| row.job.as_mut()) {
                    job.state = JobState::EndedWithTurn;
                }
            }
        }
    }

    /// The newest row of the job by this name that belongs to whoever the driver last said the
    /// reports are from.
    ///
    /// The newest, because a job's start makes its row before anything else is said about it.
    /// Searched from the first row and not from where the list of jobs starts: a kept job the person
    /// stopped between turns is out of the next turn's list when that turn reports its finish.
    pub(super) fn job_row(&mut self, name: &str) -> Option<&mut Output> {
        let whose = self.attributed_to;
        self.outputs.iter_mut().rev().find(|row| {
            row.job
                .as_ref()
                .is_some_and(|job| job.name == name && job.delegate == whose)
        })
    }

    /// Every command this session ran, oldest first.
    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    /// The background jobs of the turn in flight, or of the last one, in the order they started.
    pub fn jobs(&self) -> impl Iterator<Item = (&Output, &JobView)> {
        self.outputs[self.jobs_from..]
            .iter()
            .filter_map(|row| row.job.as_ref().map(|job| (row, job)))
    }

    /// How many background jobs are running now, for the hint line.
    pub fn jobs_running(&self) -> usize {
        self.jobs().filter(|(_, job)| job.is_running()).count()
    }

    /// Keep a question asked beside the work, and open the view on it.
    ///
    /// Opened rather than left behind a key, because the person asked a question and an answer
    /// they are not shown is not an answer. Nothing moves under a reader doing it: the press that
    /// asked came from the input box, which this mode does not draw.
    ///
    /// The answer arrives out of a wait loop that answers keys, so a mode a person opened while
    /// they waited is standing over the session when it lands. Each is put away rather than
    /// opened around: the scroller is drawn in front of this view, so an answer under one is on
    /// no screen at all, and a search left open below would take the letters that walk this list.
    ///
    /// The turn's own view is remembered only where this view was not already open, the way
    /// [`Session::back_to_the_tail`] guards on the same question: while it is open `scroll` is an
    /// offset into a watched row's lines, and the offset the transcript comes back on is the one
    /// already held.
    pub fn asked_aside(&mut self, aside: Aside) {
        self.asides.push(aside);
        let at = self.asides.len() - 1;
        self.close_scroller();
        self.close_history_search();
        if self.watching.is_none() {
            self.held_view = Some(self.scroll);
        }
        self.scroll = 0;
        self.watching = Some(Watching {
            at,
            listing: false,
            on_session: false,
        });
    }

    /// Put back the asides a resumed session had, oldest first.
    ///
    /// The one thing the view holds that outlives the session that produced it. A delegate's
    /// lines and what a command printed are not written down at all, so a resume brings back
    /// these and nothing else.
    pub fn restore_asides(&mut self, asides: Vec<Aside>) {
        self.asides = asides;
    }

    /// Every question asked beside the work, oldest first.
    pub fn asides(&self) -> &[Aside] {
        &self.asides
    }

    /// The aside the view is on, where the row it is on is one.
    pub fn watched_aside(&self) -> Option<&Aside> {
        match self.watched() {
            Some(Watched::Aside(aside)) => Some(aside),
            _ => None,
        }
    }

    /// Everything the view can open, in the order the list draws it.
    ///
    /// Grouped by kind rather than ordered by when each happened, so a row's place does not move
    /// when the next thing of a different kind arrives. The asides come first because they are
    /// the only rows that survive a resume: a resumed session's list is asides alone, and every
    /// delegate and command the session goes on to produce appends after them.
    ///
    /// All three are work that happened outside the transcript, which is what the view is for.
    pub fn watchable(&self) -> Vec<Watched<'_>> {
        self.asides
            .iter()
            .map(Watched::Aside)
            .chain(self.delegates().into_iter().map(Watched::Delegate))
            .chain(self.outputs.iter().map(Watched::Output))
            .collect()
    }

    /// Open the view: on the list where there are several rows, and on the one where there is one.
    ///
    /// `false` where there is nothing to look at, which leaves the key doing nothing at all. A
    /// mode that opened on an empty screen would be worse than a key that did not answer.
    pub fn watch(&mut self) -> bool {
        let rows = self.watchable();
        let Some(last) = rows.len().checked_sub(1) else {
            return false;
        };
        // The delegate working, or the most recent row where none is. Somebody pressing the key
        // while something is happening means that one, and there is nothing else it could mean
        // when nothing is.
        let at = rows
            .iter()
            .rposition(|row| matches!(row, Watched::Delegate(delegate) if delegate.is_running()))
            .unwrap_or(last);
        let listing = last > 0;
        self.held_view = Some(self.scroll);
        self.scroll = 0;
        self.watching = Some(Watching {
            at,
            listing,
            on_session: false,
        });
        true
    }

    /// Close it, putting the turn's own view back where it was left.
    ///
    /// `false` where it was not open, so a key can tell whether it was the one that closed
    /// something from whether it has still to be answered by the ladder below.
    pub fn stop_watching(&mut self) -> bool {
        if self.watching.take().is_none() {
            return false;
        }
        self.scroll = self.held_view.take().unwrap_or(0);
        true
    }

    /// The delegate view, while it is open.
    pub fn watching(&self) -> Option<Watching> {
        self.watching
    }

    /// Whether one row's own lines are what is on the screen.
    pub fn watching_a_delegate(&self) -> bool {
        self.watching.is_some_and(|watching| !watching.listing)
    }

    /// Whether the list is what is on the screen.
    pub fn listing_delegates(&self) -> bool {
        self.watching.is_some_and(|watching| watching.listing)
    }

    /// The row the view is on, whether it is open or highlighted in the list.
    ///
    /// `None` where the list's highlight is on the session, which is a row and not one of these.
    pub fn watched(&self) -> Option<Watched<'_>> {
        let watching = self.watching.filter(|watching| !watching.on_session)?;
        self.watchable().get(watching.at).copied()
    }

    /// The delegate the view is on, where the row it is on is a delegate.
    pub fn watched_delegate(&self) -> Option<&Delegate> {
        match self.watched() {
            Some(Watched::Delegate(delegate)) => Some(delegate),
            _ => None,
        }
    }

    /// Ask the delegate the view is on to stop, leaving the turn and every other delegate going.
    ///
    /// Does nothing where the row is not a delegate, or where it has finished or been asked
    /// already. Returns whether a stop was asked for.
    pub fn stop_watched_delegate(&mut self) -> bool {
        match self.watched_delegate() {
            Some(delegate) if delegate.is_running() && !delegate.is_stopping() => {
                delegate.stop.request();
                true
            }
            _ => false,
        }
    }

    /// What a command printed, where the row the view is on is a command.
    pub fn watched_output(&self) -> Option<&Output> {
        match self.watched() {
            Some(Watched::Output(output)) => Some(output),
            _ => None,
        }
    }

    /// Whether the list's highlight is on the session rather than on one of the delegates.
    pub fn listing_on_the_session(&self) -> bool {
        self.watching
            .is_some_and(|watching| watching.listing && watching.on_session)
    }

    /// Where the highlight sits among the list's rows, the session being the first of them.
    pub fn list_highlight(&self) -> usize {
        match self.watching {
            Some(watching) if !watching.on_session => watching.at + 1,
            _ => 0,
        }
    }

    /// Open the delegate the list is on.
    pub fn open_watched(&mut self) {
        if let Some(watching) = &mut self.watching {
            watching.listing = false;
            self.scroll = 0;
        }
    }

    /// Go back to the list from one row's lines.
    ///
    /// `false` where there is only one row: there is no list behind it, and the key that would
    /// have gone back is the key that closes.
    pub fn list_delegates(&mut self) -> bool {
        if self.watchable().len() < 2 {
            return false;
        }
        match &mut self.watching {
            Some(watching) if !watching.listing => {
                watching.listing = true;
                watching.on_session = false;
                self.scroll = 0;
                true
            }
            _ => false,
        }
    }

    /// Move to the next delegate, or to the previous one, in the order they were spawned.
    ///
    /// Each stops at its end rather than wrapping: somebody stepping through wants to arrive at
    /// the last one and know that it is the last.
    ///
    /// The session is one of the rows in the list, above the first delegate, so moving up from
    /// that one reaches it. It is not one of the steps in a delegate's own view: what `n` and `p`
    /// are for there is comparing two runs, and a key that stepped out of the mode partway
    /// through would be a different key wearing the same name.
    pub fn watch_next(&mut self) {
        let last = self.watchable().len().saturating_sub(1);
        let Some(watching) = &mut self.watching else {
            return;
        };
        if watching.on_session {
            watching.on_session = false;
            watching.at = 0;
            self.scroll = 0;
        } else if watching.at < last {
            watching.at += 1;
            self.scroll = 0;
        }
    }

    pub fn watch_previous(&mut self) {
        let Some(watching) = &mut self.watching else {
            return;
        };
        if watching.on_session {
            return;
        }
        if watching.at > 0 {
            watching.at -= 1;
            self.scroll = 0;
        } else if watching.listing {
            watching.on_session = true;
            self.scroll = 0;
        }
    }

    /// The entries the screen is drawn from: one delegate's own, or the turn's.
    ///
    /// The single place the two views part company, so everything that lays out a transcript does
    /// it the same way for both and neither can drift from the other.
    pub fn viewed(&self) -> &[Entry] {
        if self.watching.is_none_or(|watching| watching.listing) {
            return &self.transcript;
        }
        // Through the list rather than into the delegates, because a position is a place in the
        // list and the list holds three kinds of row. Indexing the delegates with it would draw
        // one delegate's lines under another kind of row every time the rows before it were not
        // delegates.
        match self.watched() {
            Some(Watched::Delegate(delegate)) => &delegate.lines,
            _ => &self.transcript,
        }
    }
}
