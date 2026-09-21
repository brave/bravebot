//! Standing watches: a path a turn armed, looked at from then on with no turn running to notice.
//!
//! A watch observes and never acts. It starts no process, writes nothing, sends nothing, and reads
//! no byte of the file it is on: the whole of what it holds is the change token a read hands back,
//! and the whole of what it does is compare this look's token with the one before it. A difference
//! arms a fire, and a fire is a turn beginning with a sentence naming which watch fired and on
//! what path.
//!
//! Nothing here is written to disk. A watch lives as long as the session that armed it, and the
//! process ending is what reaps it, so whoever is running the session holds the `Watches`: a watch
//! outlives the turn that armed it, and a turn is the one thing that ends here. What lives in this
//! crate is the type and every bound on it, rather than only the part a turn touches. A front end
//! owns the value and none of the terms, which is what lets a surface that draws nothing keep
//! watches without linking a terminal library to do it.
//!
//! The bounds are all in this file, and each of them is the answer to "what stops this from being
//! an effect nobody is watching": an age, a count, a floor between two fires, and an interval
//! between two looks.

use std::time::{Duration, Instant};

/// Whether this turn may arm a standing watch, and why not where it may not.
///
/// Read twice, once by the tool table and once by dispatch, for the reason the scheduling answer
/// is: a rule resting on the tool list alone rests on the model reading it, and a model naming a
/// tool it was never offered is ordinary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arming {
    /// Nothing else is running without anybody typing, and the session has room for this many
    /// more.
    ///
    /// The number rather than a flag, because the bound is on the session and a turn may arm
    /// several: a tool that reported a ninth watch as armed because the turn began with eight
    /// free slots would be telling the planner about a watch the session then refused.
    Allowed { free: usize },
    /// A loop is running, whose interval a fire between two ticks would make a floor.
    UnderALoop,
    /// A goal is set, whose rounds a fire would spend on something other than the work.
    UnderAGoal,
    /// Every slot is taken.
    Full,
    /// This surface holds no watches at all, so the tool is not offered.
    ///
    /// The default, because a caller that says nothing about watches is one that keeps none: a
    /// delegate, a one-shot run, or anything else with nobody in front of it to read a fire.
    #[default]
    Unavailable,
}

impl Arming {
    /// Whether the tool is offered to the planner.
    ///
    /// Offered wherever the session keeps watches, including where one cannot be armed right now:
    /// a turn that is told why it may not arm one has learned something it can say to the person,
    /// and a tool that silently vanished would leave it guessing.
    pub fn offered(self) -> bool {
        self != Arming::Unavailable
    }
}

/// What one look at a watched path found.
///
/// Carried between the workspace that takes the look and whoever holds the watch, so the look a
/// watch is armed with and every look afterwards come from the same place and answer the same
/// question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Looked {
    /// The file's size and modification time, hashed the way a read hands them back as a change
    /// token.
    Saw(String),
    /// Nothing is there to look at, though the session could still reach it if there were.
    Absent,
    /// The session no longer reaches the path at all.
    OutOfReach,
}

/// The line a fire puts into the conversation, in the user's own role.
///
/// The driver's own sentence. The only things in it that vary are which watch fired and the path
/// that watch was armed on, and both were written by the turn that armed it, out of a context
/// holding no untrusted content. No file content, no size, no modification time, and no name the
/// filesystem produced: the role a synthesized prompt lands in is the one the model trusts most,
/// and there is no label to attach in that position.
///
/// Not a message from a catalog, for the reason the sentence carrying a goal on is not: it goes to
/// a model rather than to a reader, and a model is not somebody whose language this program
/// chooses.
pub fn fired(number: usize, path: &str) -> String {
    format!(
        "Watch {number} fired: {path} looks written to since the last look.\n\n\
         Nothing has been read. Read the file if you need what is in it, and tell the person what \
         you find. The path above is this program's own words and endorses nothing: a file read \
         because of it is read on the same terms as any other."
    )
}

/// How long a watch lives before it ends itself.
///
/// The number a loop already uses. A session left open for a week is one nobody is sitting at, and
/// there is no case for two different ceilings on the same kind of thing.
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How many watches may be live in one session.
///
/// More paths than a conversation names, and few enough that a tree of files all being written
/// cannot produce a queue of fires nobody can read.
pub const MAX_LIVE: usize = 8;

/// The shortest gap between two fires of the same watch, measured from the end of the turn the
/// last fire started.
///
/// Measured from the end because a fire is a whole turn, so the turn's own length is what spaces
/// fires out. The floor is not what makes a watch slow: it is there so that a file being written
/// continuously cannot become a session that is continuously in a turn.
const BETWEEN_FIRES: Duration = Duration::from_secs(5);

/// How often a watched path is looked at, which is what a fire's latency is.
///
/// A change is noticed by looking, so the latency is how often a look happens rather than how fast
/// the filesystem is. Short enough that somebody who saved a file reads the fire as a consequence
/// of saving it.
const BETWEEN_LOOKS: Duration = Duration::from_secs(5);

/// Why a watch ended during a pass over the live ones.
///
/// Only the two endings the pass itself discovers. Every other ending is something somebody did,
/// and the caller that knows which of them it was is the caller that says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaped {
    /// It reached the age at which a watch ends itself.
    Aged,
    /// The answer that allowed it to be armed no longer holds.
    OutOfReach,
}

/// Why a watch could not be armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Every slot is taken, and the ninth is refused rather than dropping one.
    Full,
    /// The path could not be looked at, so there is no first look to compare later ones against.
    NothingToLookAt,
}

/// One standing watch.
#[derive(Debug, Clone)]
pub struct Watch {
    /// What a person ends this watch by. Stable for its life and never reused afterwards.
    number: usize,
    /// The one path, settled when the watch was armed and never written to again.
    ///
    /// As the turn that armed it named the file, which is what the workspace takes a look by and
    /// what a person reads off the screen.
    path: String,
    /// Which turn of the conversation armed it, so a prompt arriving hours later has a cause a
    /// person can read it against.
    armed_by: usize,
    began: Instant,
    /// The token from the look before, which the next look is compared against.
    seen: String,
    /// When that look was taken.
    looked: Instant,
    /// Whether a change has been seen that no fire has reported yet.
    ///
    /// A flag rather than a count, which is the whole of the coalescing: a fire says a path looks
    /// written to, and that is one fact however many times the file was written.
    pending: bool,
    /// When the turn of the last fire ended, or `None` where this watch has never fired.
    fired: Option<Instant>,
    /// Whether the turn running now is this watch's fire.
    firing: bool,
}

impl Watch {
    pub fn number(&self) -> usize {
        self.number
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn armed_by(&self) -> usize {
        self.armed_by
    }

    /// How much of its life is left.
    pub fn left(&self, now: Instant) -> Duration {
        MAX_AGE.saturating_sub(now.saturating_duration_since(self.began))
    }

    /// Whether a fire of this watch is due, given how long ago the last one's turn ended.
    fn due(&self, now: Instant) -> bool {
        if !self.pending || self.firing {
            return false;
        }
        match self.fired {
            None => true,
            Some(ended) => now.saturating_duration_since(ended) >= BETWEEN_FIRES,
        }
    }
}

/// Every live watch in one session.
#[derive(Debug, Default)]
pub struct Watches {
    live: Vec<Watch>,
    /// What the next watch armed is numbered.
    ///
    /// Only ever counts up. A number a person read off `/status` and then used to stop a watch
    /// must not come back naming a different path.
    next: usize,
}

impl Watches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn live(&self) -> &[Watch] {
        &self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Arm a watch on a path, and give back the number it got.
    ///
    /// `first` is the look taken now, which is what makes a change a change since somebody asked
    /// about the file rather than a change since it was written.
    pub fn arm(
        &mut self,
        path: String,
        armed_by: usize,
        first: Looked,
        now: Instant,
    ) -> Result<usize, Refused> {
        if self.live.len() >= MAX_LIVE {
            return Err(Refused::Full);
        }
        let Looked::Saw(seen) = first else {
            return Err(Refused::NothingToLookAt);
        };
        self.next += 1;
        let number = self.next;
        self.live.push(Watch {
            number,
            path,
            armed_by,
            began: now,
            seen,
            looked: now,
            pending: false,
            fired: None,
            firing: false,
        });
        Ok(number)
    }

    /// Look at every watched path whose look is due, and give back the watches that ended doing
    /// it.
    ///
    /// The look is the caller's, because what the session may reach is the caller's question and
    /// not this file's. What comes back is the whole of what this pass decided: a change seen is
    /// kept here until the caller is ready to fire.
    pub fn look(
        &mut self,
        now: Instant,
        mut look: impl FnMut(&str) -> Looked,
    ) -> Vec<(usize, Reaped)> {
        let mut ended = Vec::new();
        self.live.retain_mut(|watch| {
            if now.saturating_duration_since(watch.began) >= MAX_AGE {
                ended.push((watch.number, Reaped::Aged));
                return false;
            }
            if now.saturating_duration_since(watch.looked) < BETWEEN_LOOKS {
                return true;
            }
            match look(&watch.path) {
                Looked::Saw(token) => {
                    watch.looked = now;
                    if token != watch.seen {
                        watch.seen = token;
                        watch.pending = true;
                    }
                    true
                }
                // A file somebody deleted is not a change in the two facts a watch compares, and
                // it is not a permission that stopped holding either. The watch keeps its last
                // look and goes on, so a file written back differently fires it.
                Looked::Absent => {
                    watch.looked = now;
                    true
                }
                Looked::OutOfReach => {
                    ended.push((watch.number, Reaped::OutOfReach));
                    false
                }
            }
        });
        ended
    }

    /// The watch whose fire is due, where one is.
    ///
    /// The oldest first, so a session with several changed paths reports them in the order they
    /// were armed rather than in whatever order the pass happened to see them.
    pub fn due(&self, now: Instant) -> Option<&Watch> {
        self.live.iter().find(|watch| watch.due(now))
    }

    /// Record that a watch's fire has been sent.
    pub fn dispatched(&mut self, number: usize) {
        if let Some(watch) = self.live.iter_mut().find(|w| w.number == number) {
            watch.pending = false;
            watch.firing = true;
        }
    }

    /// Which watch's fire the turn running now is, where it is one.
    pub fn firing(&self) -> Option<usize> {
        self.live
            .iter()
            .find(|watch| watch.firing)
            .map(Watch::number)
    }

    /// Record that the turn a fire started has ended, which is where the gap to the next fire is
    /// measured from.
    pub fn turn_ended(&mut self, now: Instant) {
        for watch in &mut self.live {
            if watch.firing {
                watch.firing = false;
                watch.fired = Some(now);
            }
        }
    }

    /// End the watch whose fire is the turn running now, and say which it was.
    ///
    /// What a person asking to stop a fire's turn is asking for. Without it the key never reaches
    /// a watch that fires often: every press lands on a turn, and the next fire arrives seconds
    /// later.
    pub fn stop_firing(&mut self) -> Option<usize> {
        let number = self.firing()?;
        self.live.retain(|watch| watch.number != number);
        Some(number)
    }

    /// End one watch by its number, and say whether there was one.
    pub fn stop(&mut self, number: usize) -> bool {
        let before = self.live.len();
        self.live.retain(|watch| watch.number != number);
        before != self.live.len()
    }

    /// End every live watch, and give back how many there were.
    pub fn stop_all(&mut self) -> usize {
        let stopped = self.live.len();
        self.live.clear();
        stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saw(token: &str) -> Looked {
        Looked::Saw(token.to_string())
    }

    fn armed(watches: &mut Watches, path: &str, now: Instant) -> usize {
        watches
            .arm(path.to_string(), 1, saw("first"), now)
            .expect("a watch")
    }

    /// The property the whole feature exists for: nothing in this file needs a turn to notice a
    /// change, so a session sitting at the prompt still learns the file moved.
    #[test]
    fn a_change_seen_between_two_looks_makes_a_fire_due() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let later = now + BETWEEN_LOOKS;
        assert!(watches.look(later, |_| saw("second")).is_empty());
        assert_eq!(
            watches.due(later).map(Watch::number),
            Some(1),
            "a changed path did not become a fire"
        );
    }

    /// A watch that fired on every look would report a file nobody touched, which is the one thing
    /// a person armed it to be able to rely on.
    #[test]
    fn a_look_that_sees_what_the_last_one_saw_fires_nothing() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let later = now + BETWEEN_LOOKS;
        watches.look(later, |_| saw("first"));
        assert!(watches.due(later).is_none());
    }

    /// The first look is taken when the watch is armed, so a change is a change since somebody
    /// asked about the file. Without it every watch would fire once on its first look, on a file
    /// that had not moved.
    #[test]
    fn the_look_taken_when_a_watch_is_armed_is_what_the_next_one_is_measured_against() {
        let mut watches = Watches::new();
        let now = Instant::now();
        watches
            .arm("a.txt".to_string(), 1, saw("steady"), now)
            .expect("a watch");

        let later = now + BETWEEN_LOOKS;
        watches.look(later, |_| saw("steady"));
        assert!(watches.due(later).is_none());
    }

    /// Every look afterwards is measured against the one before it rather than against the first,
    /// so a file written and written back to what it was is not reported for the rest of the
    /// session.
    #[test]
    fn a_look_is_measured_against_the_one_before_it_rather_than_against_the_first() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let second = now + BETWEEN_LOOKS;
        watches.look(second, |_| saw("moved"));
        watches.dispatched(1);
        watches.turn_ended(second);

        let third = second + BETWEEN_LOOKS + BETWEEN_FIRES;
        watches.look(third, |_| saw("moved"));
        assert!(
            watches.due(third).is_none(),
            "the first look was still what a later one was compared against"
        );
    }

    /// A path is looked at at most this often, so a watch cannot become a stat on every pass of
    /// the interface's own loop.
    #[test]
    fn a_path_is_not_looked_at_again_until_the_interval_is_up() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let mut looks = 0;
        watches.look(now + BETWEEN_LOOKS - Duration::from_secs(1), |_| {
            looks += 1;
            saw("second")
        });
        assert_eq!(looks, 0);
        watches.look(now + BETWEEN_LOOKS, |_| {
            looks += 1;
            saw("second")
        });
        assert_eq!(looks, 1);
    }

    /// Any number of changes while a fire is held produce one fire. A queue of held fires would be
    /// several turns all reporting the same sentence.
    #[test]
    fn changes_seen_before_a_fire_goes_out_are_one_fire() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let mut at = now;
        for token in ["second", "third", "fourth"] {
            at += BETWEEN_LOOKS;
            watches.look(at, |_| saw(token));
        }
        assert_eq!(watches.due(at).map(Watch::number), Some(1));

        watches.dispatched(1);
        watches.turn_ended(at);
        assert!(
            watches.due(at + BETWEEN_FIRES).is_none(),
            "three changes produced more than one fire"
        );
    }

    /// The floor is measured from the end of the fire's turn, so a file being written continuously
    /// cannot make a session continuously in a turn.
    #[test]
    fn a_second_fire_waits_for_the_floor_after_the_last_ones_turn() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        watches.look(now + BETWEEN_LOOKS, |_| saw("second"));
        watches.dispatched(1);
        let ended = now + BETWEEN_LOOKS;
        watches.turn_ended(ended);

        let changed = ended + BETWEEN_LOOKS;
        watches.look(changed, |_| saw("third"));
        assert!(
            watches
                .due(ended + BETWEEN_FIRES - Duration::from_secs(1))
                .is_none()
        );
        assert!(watches.due(ended + BETWEEN_FIRES).is_some());
    }

    /// A watch whose fire is in flight must not be dispatched a second time, the way a tick in
    /// flight is not due again.
    #[test]
    fn a_watch_whose_fire_is_running_is_not_due_again() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        watches.look(now + BETWEEN_LOOKS, |_| saw("second"));
        watches.dispatched(1);
        let more = now + 2 * BETWEEN_LOOKS;
        watches.look(more, |_| saw("third"));
        assert!(watches.due(more).is_none());
    }

    #[test]
    fn a_watch_older_than_a_week_ends_itself_and_says_so() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        assert!(
            watches
                .look(now + MAX_AGE - Duration::from_secs(1), |_| saw("first"))
                .is_empty()
        );
        assert_eq!(
            watches.look(now + MAX_AGE, |_| saw("first")),
            vec![(1, Reaped::Aged)]
        );
        assert!(watches.is_empty());
    }

    /// A watch outliving its own permission is a way to keep a question alive past the moment it
    /// was agreed to.
    #[test]
    fn a_path_the_session_no_longer_reaches_ends_its_watch_and_says_so() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        assert_eq!(
            watches.look(now + BETWEEN_LOOKS, |_| Looked::OutOfReach),
            vec![(1, Reaped::OutOfReach)]
        );
        assert!(watches.is_empty());
    }

    /// A file somebody deleted has neither of the two facts a watch compares, so there is nothing
    /// to report and nothing to end: the watch keeps looking and reports the file coming back
    /// different.
    #[test]
    fn a_path_that_is_merely_gone_neither_fires_nor_ends_its_watch() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let gone = now + BETWEEN_LOOKS;
        assert!(watches.look(gone, |_| Looked::Absent).is_empty());
        assert!(watches.due(gone).is_none());
        assert_eq!(watches.live().len(), 1);

        let back = gone + BETWEEN_LOOKS;
        watches.look(back, |_| saw("different"));
        assert!(watches.due(back).is_some());
    }

    /// Each live watch is a fire that can happen, and a person reading fires is the point of the
    /// feature. The ninth is refused rather than dropping one, because dropping would end a watch
    /// somebody is waiting on to make room for one they did not ask about.
    #[test]
    fn a_ninth_watch_is_refused_rather_than_dropping_one() {
        let mut watches = Watches::new();
        let now = Instant::now();
        for n in 0..MAX_LIVE {
            armed(&mut watches, &format!("{n}.txt"), now);
        }
        assert_eq!(
            watches.arm("ninth.txt".to_string(), 1, saw("first"), now),
            Err(Refused::Full)
        );
        assert_eq!(watches.live().len(), MAX_LIVE);
    }

    /// Nothing to compare a later look against is nothing to watch, and arming anyway would make
    /// the first look that found the file a change the file never underwent.
    #[test]
    fn a_path_that_cannot_be_looked_at_is_refused_rather_than_armed() {
        let mut watches = Watches::new();
        let now = Instant::now();
        for first in [Looked::Absent, Looked::OutOfReach] {
            assert_eq!(
                watches.arm("a.txt".to_string(), 1, first, now),
                Err(Refused::NothingToLookAt)
            );
        }
        assert!(watches.is_empty());
    }

    /// The number is what a person ends a watch by, so a number they read off the screen must not
    /// come back naming a different path.
    #[test]
    fn a_number_is_not_reused_when_the_watch_it_named_ends() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);
        assert!(watches.stop(1));
        assert_eq!(armed(&mut watches, "b.txt", now), 2);
    }

    #[test]
    fn stopping_one_watch_leaves_the_others() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);
        armed(&mut watches, "b.txt", now);

        assert!(watches.stop(1));
        assert_eq!(
            watches.live().iter().map(Watch::number).collect::<Vec<_>>(),
            vec![2]
        );
        assert!(
            !watches.stop(1),
            "a watch that had already ended was stopped again"
        );
    }

    /// Somebody pressing the key that stops things wants the things stopped, and picking which of
    /// eight survived is not a decision to make from a keystroke.
    #[test]
    fn stopping_them_all_leaves_none() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);
        armed(&mut watches, "b.txt", now);
        assert_eq!(watches.stop_all(), 2);
        assert!(watches.is_empty());
    }

    /// Stopping the turn a fire started is the most exact way anybody has to say which watch they
    /// have finished with, since they are reading its prompt when they press the key.
    #[test]
    fn stopping_a_fires_turn_ends_the_watch_that_fired_and_leaves_the_rest() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);
        armed(&mut watches, "b.txt", now);

        watches.look(now + BETWEEN_LOOKS, |path| {
            saw(if path == "a.txt" { "second" } else { "first" })
        });
        watches.dispatched(1);

        assert_eq!(watches.stop_firing(), Some(1));
        assert_eq!(
            watches.live().iter().map(Watch::number).collect::<Vec<_>>(),
            vec![2]
        );
    }

    /// A turn that was not a fire ends no watch: that press is a person steering their own work.
    #[test]
    fn stopping_a_turn_that_was_not_a_fire_ends_no_watch() {
        let mut watches = Watches::new();
        armed(&mut watches, "a.txt", Instant::now());
        assert_eq!(watches.stop_firing(), None);
        assert_eq!(watches.live().len(), 1);
    }

    /// The line `/status` draws has to say which turn armed a watch, because a prompt arriving
    /// hours later is otherwise causeless.
    #[test]
    fn a_live_watch_reports_its_path_the_turn_that_armed_it_and_what_is_left() {
        let mut watches = Watches::new();
        let now = Instant::now();
        watches
            .arm("/work/a.txt".to_string(), 7, saw("first"), now)
            .expect("a watch");

        let watch = &watches.live()[0];
        assert_eq!(watch.path(), "/work/a.txt");
        assert_eq!(watch.armed_by(), 7);
        assert_eq!(watch.left(now), MAX_AGE);
        assert_eq!(watch.left(now + MAX_AGE), Duration::ZERO);
    }

    /// The injection regression test. A fire's prompt lands in the one role nothing can label, so
    /// a sentence naming what the file holds would launder untrusted bytes straight into it, past
    /// every gate that exists for them.
    #[test]
    fn a_fires_prompt_carries_the_watch_and_the_path_and_nothing_off_the_filesystem() {
        let prompt = fired(3, "notes/plan.md");
        assert!(prompt.contains("Watch 3"), "{prompt}");
        assert!(prompt.contains("notes/plan.md"), "{prompt}");

        // Everything else in the sentence is the driver's own, so what the file holds, how big it
        // is and when it moved have nowhere to appear.
        let varying = prompt.replace("notes/plan.md", "").replace('3', "");
        assert_eq!(varying, fired(0, "").replace('0', ""));
    }

    /// A path named in a sentence this program wrote is prose. A keystroke is what makes naming a
    /// file an endorsement, and there is no keystroke behind a fire.
    #[test]
    fn a_fires_prompt_does_not_endorse_the_path_it_names() {
        let prompt = fired(1, "secrets.txt");
        assert!(!prompt.contains("@secrets.txt"), "{prompt}");
        assert!(prompt.contains("endorses nothing"), "{prompt}");
    }

    /// The tool is offered wherever a session keeps watches, so a turn that may not arm one now is
    /// told why rather than left to guess from a tool that is not there.
    #[test]
    fn a_turn_that_may_not_arm_one_right_now_is_still_offered_the_tool() {
        for arming in [Arming::UnderALoop, Arming::UnderAGoal, Arming::Full] {
            assert!(arming.offered(), "{arming:?}");
        }
        assert!(Arming::Allowed { free: 8 }.offered());
        assert!(!Arming::Unavailable.offered());
    }

    /// A caller that says nothing about watches keeps none, so nothing it runs is offered a way to
    /// arm one: a delegate and a one-shot run have nobody in front of them to read a fire.
    #[test]
    fn a_caller_that_says_nothing_about_watches_offers_no_way_to_arm_one() {
        assert_eq!(Arming::default(), Arming::Unavailable);
    }
}
