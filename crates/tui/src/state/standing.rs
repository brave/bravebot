//! What stands over a session between turns: loops, file watches and background jobs.

use super::*;

impl Session {
    /// Start looking again because the turn that just ended asked to, repeating the person's line.
    ///
    /// Nothing is sent now. The turn that asked has just taken the look it is reporting, so the
    /// first tick is the wait away rather than immediate, and the person reads one answer rather
    /// than the same answer twice.
    ///
    /// Refused while a goal is set, because a goal is the one thing a session is working towards
    /// and a watch nobody typed is not a reason to drop it. A person's own `/loop` may replace a
    /// goal, since they are there to mean it.
    pub fn watch_again(&mut self, prompt: &str, wakeup: crate::loops::Wakeup) {
        if self.goal.is_some() {
            self.note(t!(loop_not_armed_under_a_goal));
            return;
        }
        // And refused while a standing watch is live, for the same reason: a session does one
        // thing at a time that happens without anybody typing, and a turn that took somebody's
        // watch off to start a loop would be ending the standing answer to replace it with the
        // repeating one.
        if !self.watches.is_empty() {
            self.note(t!(loop_not_armed_under_a_watch));
            return;
        }
        let after = crate::loops::spell(wakeup.after);
        self.looping = Some(crate::loops::Running::armed(
            prompt.to_string(),
            wakeup,
            Instant::now(),
        ));
        self.note(t!(loop_armed_by_the_turn, after = after));
    }

    /// Say what is repeating and when the next tick is due, or that nothing is.
    ///
    /// What the bare command answers. The line is part of it because a loop is announced once, at
    /// the top of a session that has since scrolled, and somebody who has come back to a room with
    /// a loop running in it is asking what they set going as much as when it next goes.
    ///
    /// Its first line only, the way `/status` carries it: a prompt is whatever somebody typed, and
    /// the rest of a pasted one would arrive as a wall of transcript nobody asked for.
    pub fn report_loop(&mut self) {
        let Some(running) = self.looping.as_ref() else {
            self.note(t!(loop_none));
            return;
        };
        let prompt = crate::render::one_line(running.prompt());
        let pace = running.pace();
        let when = running.when(Instant::now());
        self.note(t!(
            loop_active,
            prompt = &prompt,
            pace = &pace,
            when = &when
        ));
        self.note(t!(loop_ends_with));
    }

    /// Stop the loop, and say whether there was one.
    pub fn stop_loop(&mut self) -> bool {
        let stopped = self.looping.take().is_some();
        if stopped {
            self.note(t!(loop_stopped));
        }
        stopped
    }

    /// Every live watch, oldest first, for the report that lists them.
    pub fn watches(&self) -> &[watch::Watch] {
        self.watches.live()
    }

    /// Whether this turn may arm a standing watch, and why not where it may not.
    ///
    /// Read once, as the turn is started, and handed to it: the tool answers out of this rather
    /// than guessing, so what the planner is told matches what the session will actually do.
    pub fn arming(&self) -> watch::Arming {
        use watch::Arming;
        if self.looping.is_some() {
            return Arming::UnderALoop;
        }
        if self.goal.is_some() {
            return Arming::UnderAGoal;
        }
        match watch::MAX_LIVE.saturating_sub(self.watches.live().len()) {
            0 => Arming::Full,
            free => Arming::Allowed { free },
        }
    }

    /// Arm a standing watch on a path the turn that has just ended asked to be told about.
    ///
    /// The turn already went through the gate a read of the path goes through, so nothing here
    /// asks a second time. What is decided here is what only the session can decide: whether it
    /// is already doing something that happens without anybody typing, whether it has room, and
    /// what the first look at the path saw.
    ///
    /// `under` is the working directory the first look was taken in, and it travels with the
    /// watch so that every later look is taken against the same directory. Taken from the caller
    /// that took the look rather than from this session's own record of where it is working: one
    /// fact read twice is two answers waiting to differ, and the one that matters is the
    /// workspace's.
    pub fn arm_watch(
        &mut self,
        path: &str,
        under: impl Into<std::path::PathBuf>,
        first: watch::Looked,
    ) {
        if self.looping.is_some() {
            self.note(t!(watch_not_armed_under_a_loop));
            return;
        }
        if self.goal.is_some() {
            self.note(t!(watch_not_armed_under_a_goal));
            return;
        }
        match self.watches.arm(
            path.to_string(),
            under.into(),
            self.turns,
            first,
            Instant::now(),
        ) {
            Ok(number) => self.note(t!(watch_armed, number = number, path = path)),
            Err(watch::Refused::Full) => {
                self.note(t!(watch_not_armed_full, count = watch::MAX_LIVE))
            }
            Err(watch::Refused::NothingToLookAt) => {
                self.note(t!(watch_not_armed_unreadable, path = path))
            }
        }
    }

    /// Look at every watched path, and send the fire that is due where one is.
    ///
    /// The sibling of [`Session::loop_tick`], and called from the same place for the same reason:
    /// nobody is going to press anything to make a fire happen, so a pass taken only after input
    /// arrives would sit there until somebody typed something unrelated.
    ///
    /// A fire waits for an idle session and for the queue to empty, exactly as a tick does. A
    /// filesystem event is not a licence to interrupt: the person is still the one using this
    /// session.
    ///
    /// `look` is the caller's, because what this session may still reach is the workspace's
    /// question rather than this one's, and it is handed the working directory each watch was
    /// armed under along with the path: a relative path is the file it was armed on only while
    /// that is still the working directory. `now` is the caller's for the same reason the registry
    /// takes one: the interval between two looks is five seconds, and a test that had to wait
    /// them out would be a test nobody runs.
    pub fn watch_fired(
        &mut self,
        now: Instant,
        look: impl FnMut(&str, &std::path::Path) -> watch::Looked,
    ) -> Option<String> {
        for (number, why) in self.watches.look(now, look) {
            match why {
                watch::Reaped::Aged => self.note(t!(watch_aged_out, number = number)),
                watch::Reaped::OutOfReach => self.note(t!(watch_out_of_reach, number = number)),
            }
        }
        if self.status != Status::Idle || !self.queued.is_empty() {
            return None;
        }
        let watch = self.watches.due(now)?;
        let change = watch.change()?;
        let (number, path) = (watch.number(), watch.path().to_string());
        self.watches.dispatched(number);
        match change {
            watch::Change::Written => self.note(t!(watch_fired, number = number, path = &path)),
            watch::Change::Removed => {
                self.note(t!(watch_fired_removed, number = number, path = &path))
            }
            watch::Change::Appeared => {
                self.note(t!(watch_fired_appeared, number = number, path = &path))
            }
        }
        let prompt = watch::fired(number, &path, change);
        Some(self.begin_turn(prompt, (Vec::new(), Vec::new()), Vec::new()))
    }

    /// Whether the turn running now is a watch's fire.
    ///
    /// What makes a fire's prompt the driver's rather than the person's: an `@` in the sentence
    /// this program wrote names nothing, and a wait the turn asks for leaves no loop repeating
    /// it.
    pub fn watch_is_firing(&self) -> bool {
        self.watches.firing().is_some()
    }

    /// The number and path of the watch whose fire the turn starting now is, for the tag its
    /// prompt is recorded with (LAYER-6).
    pub fn firing_watch(&self) -> Option<(usize, String)> {
        self.watches
            .firing_watch()
            .map(|watch| (watch.number(), watch.path().to_string()))
    }

    /// Record that the turn a fire started has ended, which is where the gap to the next fire is
    /// measured from.
    pub fn watch_turn_ended(&mut self) {
        self.watches.turn_ended(Instant::now());
    }

    /// End the watch whose fire is the turn being stopped, and say whether there was one.
    ///
    /// Without this the key never reaches a watch that fires often: every press lands on a turn,
    /// and the next fire arrives seconds later. A turn that was not a fire ends no watch, because
    /// that press is a person steering their own work.
    pub fn stop_firing_watch(&mut self) -> bool {
        let Some(number) = self.watches.stop_firing() else {
            return false;
        };
        self.note(t!(watch_stopped_with_its_turn, number = number));
        true
    }

    /// End one watch because somebody named it, and say whether there was one.
    pub fn stop_watch(&mut self, number: usize) -> bool {
        let stopped = self.watches.stop(number);
        if stopped {
            self.note(t!(watch_stopped, number = number));
        } else {
            self.note(t!(watch_no_such, number = number));
        }
        stopped
    }

    /// End every live watch because somebody pressed the key that stops things, and say whether
    /// there were any.
    pub fn stop_watches(&mut self) -> bool {
        let stopped = self.watches.stop_all();
        if stopped > 0 {
            self.note(t!(watches_stopped, count = stopped));
        }
        stopped > 0
    }

    /// End every watch the working directory moving has closed the answer for, and say so.
    ///
    /// A watch is armed on the path as the turn wrote it, which for the tool that arms one is a
    /// relative path, and a relative path means the working directory. So a move leaves it naming
    /// a file in the new directory that nobody armed a watch on, and the answer that allowed it
    /// was about the directory that has just closed. Said one watch at a time, as the closed
    /// directories are, because each one is something the session was going to tell the person
    /// about and now will not.
    pub fn end_watches_left_behind(&mut self, root: &std::path::Path) {
        for number in self.watches.stop_moved(root) {
            self.note(t!(watch_out_of_reach, number = number));
        }
    }

    /// End every live watch because the session is about to do one of the other two things that
    /// happen without anybody typing.
    ///
    /// Silent where there are none, so a person who never armed one is not told about a feature
    /// every time they type `/loop`.
    pub(super) fn end_watches_for_another_kind(&mut self) {
        let stopped = self.watches.stop_all();
        if stopped > 0 {
            self.note(t!(watches_replaced, count = stopped));
        }
    }

    /// Say what is being watched, or that nothing is.
    ///
    /// What the bare command answers, and the same list `/status` draws. A watch that ended in
    /// silence is indistinguishable from one that is live and has seen nothing, so the way to
    /// tell them apart has to be askable.
    pub fn report_watches(&mut self) {
        let now = Instant::now();
        let lines: Vec<String> = self
            .watches
            .live()
            .iter()
            .map(|watch| {
                t!(
                    watch_listed,
                    number = watch.number(),
                    path = watch.path(),
                    turn = watch.armed_by(),
                    left = crate::loops::spell(watch.left(now))
                )
            })
            .collect();
        if lines.is_empty() {
            self.note(t!(watch_none));
            return;
        }
        for line in lines {
            self.note(line);
        }
    }

    /// Say which background jobs the turn in flight has, or the last one had, and where each is,
    /// or that there are none (RUN-27).
    ///
    /// Named as `/jobs stop` takes them, with the words `/status` uses for where each one is. Every
    /// word is the driver's or this end's clock, and none is anything a job printed.
    pub fn report_jobs(&mut self) {
        let lines: Vec<String> = self
            .jobs()
            .map(|(row, job)| {
                let command = crate::status::cut(&row.command, crate::status::JOB_LINE);
                let standing = job.standing(&row.outcome);
                match job.delegate {
                    Some(delegate) => t!(
                        jobs_listed_of_delegate,
                        name = job.name.clone(),
                        number = delegate.to_string(),
                        command = command,
                        standing = standing
                    )
                    .to_string(),
                    None => t!(
                        jobs_listed,
                        name = job.name.clone(),
                        command = command,
                        standing = standing
                    )
                    .to_string(),
                }
            })
            .collect();
        if lines.is_empty() {
            self.note(t!(jobs_none));
            return;
        }
        for line in lines {
            self.note(line);
        }
    }

    /// Ask for one background job to be stopped, and say whether it is (RUN-27).
    ///
    /// Sets the job's token and nothing else: the turn reads it at its next step and does the
    /// stopping, so the row says the job is being stopped until the driver says it was. A job of
    /// another delegate by the same name is a different job, which is why the delegate is part of
    /// the match.
    pub fn stop_job(&mut self, name: &str, delegate: Option<&str>) -> bool {
        let label = match delegate {
            Some(number) => t!(
                job_of_delegate,
                name = name.to_string(),
                number = number.to_string()
            )
            .to_string(),
            None => name.to_string(),
        };
        let turn_is_running = self.a_turn_is_running();
        let job = self.outputs[self.jobs_from..]
            .iter_mut()
            .rev()
            .filter_map(|row| row.job.as_mut())
            .find(|job| {
                job.name == name && job.delegate.map(|id| id.to_string()).as_deref() == delegate
            });
        let (said, asked) = match job {
            None => (t!(jobs_no_such, name = label).to_string(), false),
            Some(job) => match job.state {
                JobState::Running => {
                    job.stop.request();
                    job.state = JobState::Stopping;
                    (t!(job_stop_asked, name = label).to_string(), true)
                }
                // The token is already set. With a turn in flight it reads it; with none, nothing
                // will, so the stop is carried out below rather than answered with a refusal.
                JobState::Stopping if !turn_is_running && self.kept_jobs.is_some() => {
                    (t!(job_stop_asked, name = label).to_string(), true)
                }
                JobState::Stopping => (t!(job_stop_already_asked, name = label).to_string(), false),
                JobState::Ended | JobState::EndedWithTurn => {
                    (t!(jobs_already_ended, name = label).to_string(), false)
                }
            },
        };
        self.note(said);
        // Between turns nothing reads the token, so the stop is carried out here and the row says
        // so. The planner is told at the next turn's first round, as it is of any finish (RUN-27).
        if asked && !turn_is_running {
            self.carry_out_stops();
        }
        asked
    }

    /// Kill every job whose stop token is set and say each was the person's stop (RUN-27).
    ///
    /// For the moments no turn is left to read a token: between turns, and at a turn's end, where a
    /// reply with no tool calls ends the turn without another step and a kept session's jobs
    /// outlive it.
    pub(super) fn carry_out_stops(&mut self) {
        let Some(kept) = &self.kept_jobs else {
            return;
        };
        for (name, ran_for) in kept.stop_requested() {
            self.job(bravebot_agent::report::JobEvent::Ended {
                name,
                outcome: bravebot_agent::report::Outcome::StoppedByTheUser(ran_for),
            });
        }
    }

    /// Send the next tick, if one is due and the session is free to take it.
    ///
    /// Called on the way round the interface's own loop, so a tick waits for the turn in flight
    /// and for everything the person queued behind it. A schedule is a request to be asked
    /// again, not a licence to interrupt.
    pub fn loop_tick(&mut self) -> Option<String> {
        self.loop_tick_at(Instant::now())
    }

    pub(super) fn loop_tick_at(&mut self, now: Instant) -> Option<String> {
        let running = self.looping.as_ref()?;
        if running.aged_out(now) {
            self.looping = None;
            self.note(t!(loop_aged_out));
            return None;
        }
        if self.status != Status::Idle || !self.queued.is_empty() || !running.due(now) {
            return None;
        }
        self.dispatch_tick()
    }

    /// Arm the next tick from the turn that has just ended.
    ///
    /// `wakeup` is what a self-paced turn asked for. Nothing happens where the finished turn was
    /// not a tick: a person typing their own prompt in the middle of a loop is not a tick of it,
    /// and must not reset the clock.
    pub fn loop_turn_ended(&mut self, wakeup: Option<crate::loops::Wakeup>) {
        let Some(running) = self.looping.as_mut() else {
            return;
        };
        if !running.ended(wakeup, Instant::now()) {
            self.looping = None;
            if wakeup.is_some_and(|wakeup| wakeup.stop) {
                self.note(t!(loop_finished));
            } else {
                self.note(t!(loop_unpaced));
            }
        }
    }

    /// Send a tick, announcing which one it is.
    ///
    /// The line is the loop's own, taken from what the person typed, and it carries no files: one
    /// staged in the box belonged to the line it was staged in. A picture is the exception the loop
    /// itself makes, and only once: [`crate::loops::Running::dispatching`] hands it to the first tick
    /// and has nothing to hand any tick after that. A `@path` in the prompt is read back out of it
    /// every tick, the way it is for any other prompt.
    pub(super) fn dispatch_tick(&mut self) -> Option<String> {
        let running = self.looping.as_mut()?;
        let (prompt, pasted, attached) = running.dispatching();
        let count = running.ticks();
        let quiet = running.quiet();
        if quiet > 0 {
            self.note(t!(loop_tick_quiet, count = count, quiet = quiet));
        } else {
            self.note(t!(loop_tick, count = count));
        }
        Some(self.begin_turn(prompt, (attached, pasted), Vec::new()))
    }
}
